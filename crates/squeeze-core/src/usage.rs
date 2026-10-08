use crate::{Limits, Result, check_cancel, inspect};
use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

pub(crate) struct Usage {
    pub unknown: HashSet<ObjectId>,
    pub protected: HashSet<ObjectId>,
    pub sizes: HashMap<ObjectId, [f64; 2]>,
}
#[derive(Clone, Copy)]
struct Matrix([f64; 6]);
impl Matrix {
    fn identity() -> Self {
        Self([1., 0., 0., 1., 0., 0.])
    }
    fn multiply(self, rhs: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = rhs.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }
    fn size(self) -> Option<[f64; 2]> {
        let [a, b, c, d, _, _] = self.0;
        let x = a.hypot(b);
        let y = c.hypot(d);
        if x <= 0.
            || y <= 0.
            || !x.is_finite()
            || !y.is_finite()
            || (a * c + b * d).abs() > x * y * 1e-6
        {
            return None;
        }
        Some([x, y])
    }
}
fn number(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(v) => Some(*v as f64),
        Object::Real(v) => Some(*v as f64),
        _ => None,
    }
}
fn matrix(items: &[Object]) -> Option<Matrix> {
    if items.len() != 6 {
        return None;
    }
    let mut values = [0.; 6];
    for (value, object) in values.iter_mut().zip(items) {
        *value = number(object)?;
        if !value.is_finite() {
            return None;
        }
    }
    Some(Matrix(values))
}
pub(crate) fn analyze(doc: &Document, limits: &Limits, cancel: &AtomicBool) -> Result<Usage> {
    let mut analysis = Analyzer {
        doc,
        limits,
        cancel,
        usage: Usage {
            unknown: HashSet::new(),
            protected: HashSet::new(),
            sizes: HashMap::new(),
        },
        operations: 0,
    };
    let mut tables = HashSet::new();
    for o in doc.objects.values() {
        collect_tables(doc, o, &mut tables);
    }
    for (&id, o) in &doc.objects {
        check_cancel(cancel)?;
        analysis.foreign(o, tables.contains(&id), false)?;
        if let Some(d) = inspect::dictionary(o) {
            // Glyphs, patterns, field default resources and appearance forms
            // are not ordinary page calls. Protect their transitive resources.
            if inspect::named(d, b"Subtype", b"Type3") || d.has(b"PatternType") {
                if let Ok(r) = d.get(b"Resources") {
                    analysis.block(r, false)?;
                } else {
                    analysis.unknown_all();
                }
            }
            if let Ok(form) = d.get(b"AcroForm") {
                let (_, form) = doc.dereference(form)?;
                if let Some(d) = inspect::dictionary(form)
                    && let Ok(r) = d.get(b"DR")
                {
                    analysis.block(r, true)?;
                }
            }
        }
    }
    for page in inspect::pages(doc)? {
        check_cancel(cancel)?;
        let d = doc.get_dictionary(page)?;
        let unit = d
            .get(b"UserUnit")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| number(o))
            .unwrap_or(1.);
        if !unit.is_finite() || unit <= 0. {
            analysis.block_page(page)?;
            continue;
        }
        let resources = inspect::inherited(doc, page, b"Resources")?.and_then(|o| o.as_dict().ok());
        let content = match page_content(doc, d, limits.max_stream_bytes) {
            Ok(bytes) => bytes,
            Err(_) => {
                analysis.block_page(page)?;
                continue;
            }
        };
        analysis.walk(
            &content,
            resources,
            Matrix([unit, 0., 0., unit, 0., 0.]),
            &mut HashSet::new(),
            0,
        )?;
    }
    Ok(analysis.usage)
}
fn collect_tables(doc: &Document, o: &Object, tables: &mut HashSet<ObjectId>) {
    if let Some(d) = inspect::dictionary(o) {
        for (k, v) in d.iter() {
            if k == b"XObject"
                && let Ok((Some(id), Object::Dictionary(_))) = doc.dereference(v)
            {
                tables.insert(id);
            }
            collect_tables(doc, v, tables);
        }
    } else if let Object::Array(a) = o {
        for o in a {
            collect_tables(doc, o, tables);
        }
    }
}

/// Walk resource references, stopping at page/catalog back pointers. This
/// deliberately overprotects an unknown context rather than guessing its calls.
pub(crate) fn resource_images(
    doc: &Document,
    root: &Object,
    cancel: &AtomicBool,
) -> Result<HashSet<ObjectId>> {
    let mut images = HashSet::new();
    let mut seen = HashSet::new();
    let mut pending = vec![root];
    while let Some(o) = pending.pop() {
        check_cancel(cancel)?;
        if let Object::Reference(id) = o {
            if seen.insert(*id) {
                pending.push(doc.get_object(*id)?);
                if doc
                    .get_object(*id)
                    .ok()
                    .and_then(inspect::dictionary)
                    .is_some_and(|d| inspect::named(d, b"Subtype", b"Image"))
                {
                    images.insert(*id);
                }
            }
        } else if let Some(d) = inspect::dictionary(o) {
            if [b"Page".as_slice(), b"Pages", b"Catalog"]
                .iter()
                .any(|name| inspect::named(d, b"Type", name))
            {
                continue;
            }
            pending.extend(d.iter().map(|(_, v)| v));
        } else if let Object::Array(a) = o {
            pending.extend(a);
        }
    }
    Ok(images)
}
pub(crate) fn page_content(doc: &Document, d: &Dictionary, limit: usize) -> lopdf::Result<Vec<u8>> {
    let Ok(contents) = d.get(b"Contents") else {
        return Ok(Vec::new());
    };
    let (_, contents) = doc.dereference(contents)?;
    let items = match contents {
        Object::Array(a) => a.as_slice(),
        _ => std::slice::from_ref(contents),
    };
    let mut bytes = Vec::new();
    for o in items {
        let (_, o) = doc.dereference(o)?;
        let data = o
            .as_stream()?
            .get_plain_content_with_limit(limit.saturating_sub(bytes.len() + 1))?;
        bytes.extend(data);
        bytes.push(b'\n');
        if bytes.len() > limit {
            return Err(lopdf::Error::InvalidStream("页面内容过大".into()));
        }
    }
    Ok(bytes)
}
struct Analyzer<'a> {
    doc: &'a Document,
    limits: &'a Limits,
    cancel: &'a AtomicBool,
    usage: Usage,
    operations: usize,
}
impl Analyzer<'_> {
    fn unknown_all(&mut self) {
        self.usage
            .unknown
            .extend(self.doc.objects.iter().filter_map(|(&id, o)| {
                inspect::dictionary(o)
                    .filter(|d| inspect::named(d, b"Subtype", b"Image"))
                    .map(|_| id)
            }));
    }
    fn protect_all(&mut self) {
        self.usage
            .protected
            .extend(self.doc.objects.iter().filter_map(|(&id, o)| {
                inspect::dictionary(o)
                    .filter(|d| inspect::named(d, b"Subtype", b"Image"))
                    .map(|_| id)
            }));
    }
    fn block(&mut self, root: &Object, protect: bool) -> Result<()> {
        let images = resource_images(self.doc, root, self.cancel)?;
        self.usage.unknown.extend(&images);
        if protect {
            self.usage.protected.extend(images);
        }
        Ok(())
    }
    fn block_page(&mut self, page: ObjectId) -> Result<()> {
        if let Some(r) = inspect::inherited(self.doc, page, b"Resources")? {
            self.block(r, false)?;
        }
        Ok(())
    }
    fn uncertain(&mut self, resources: Option<&Dictionary>) -> Result<()> {
        if let Some(r) = resources {
            self.block(&Object::Dictionary(r.clone()), false)?;
        }
        Ok(())
    }
    fn foreign(&mut self, o: &Object, table: bool, _mask: bool) -> Result<()> {
        match o {
            Object::Reference(id) if !table => {
                if let Some(d) = self.doc.get_object(*id).ok().and_then(inspect::dictionary) {
                    if inspect::named(d, b"Subtype", b"Image") {
                        self.usage.unknown.insert(*id);
                    } else if inspect::named(d, b"Subtype", b"Form") {
                        if let Ok(r) = d.get(b"Resources") {
                            self.block(r, true)?;
                        } else {
                            // A foreign Form without resources may borrow an
                            // unknown caller's dictionary. No safe local bound.
                            let s = self.doc.get_object(*id)?.as_stream()?;
                            if s.get_plain_content_with_limit(self.limits.max_stream_bytes)
                                .ok()
                                .and_then(|v| Content::decode(&v).ok())
                                .is_none_or(|c| {
                                    c.operations.iter().any(|o| {
                                        matches!(
                                            o.operator.as_str(),
                                            "Do" | "Tf" | "gs" | "CS" | "cs" | "SCN" | "scn" | "sh"
                                        )
                                    })
                                })
                            {
                                self.protect_all();
                            }
                        }
                    }
                }
            }
            Object::Array(a) => {
                for o in a {
                    self.foreign(o, false, false)?;
                }
            }
            _ => {
                if let Some(d) = inspect::dictionary(o) {
                    for (k, v) in d.iter() {
                        let child_table =
                            k == b"XObject" || (table && matches!(v, Object::Reference(_)));
                        self.foreign(v, child_table, k == b"SMask" || k == b"Mask")?;
                    }
                }
            }
        }
        Ok(())
    }

    fn walk(
        &mut self,
        bytes: &[u8],
        resources: Option<&Dictionary>,
        mut ctm: Matrix,
        path: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Result<()> {
        if depth > 32 {
            self.uncertain(resources)?;
            return Ok(());
        }
        let content = match Content::decode(bytes) {
            Ok(c) => c,
            Err(_) => {
                self.uncertain(resources)?;
                return Ok(());
            }
        };
        let mut stack = Vec::new();
        for op in content.operations {
            check_cancel(self.cancel)?;
            self.operations += 1;
            if self.operations > self.limits.max_operations {
                self.uncertain(resources)?;
                return Ok(());
            }
            match op.operator.as_str() {
                "q" => {
                    if stack.len() >= 64 {
                        self.uncertain(resources)?;
                        return Ok(());
                    }
                    stack.push(ctm);
                }
                "Q" => match stack.pop() {
                    Some(m) => ctm = m,
                    None => {
                        self.uncertain(resources)?;
                        return Ok(());
                    }
                },
                "cm" => match matrix(&op.operands) {
                    Some(m) => ctm = ctm.multiply(m),
                    None => {
                        self.uncertain(resources)?;
                        return Ok(());
                    }
                },
                "BI" | "ID" | "EI" => {
                    self.uncertain(resources)?;
                    return Ok(());
                }
                "Do" => {
                    let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else {
                        self.uncertain(resources)?;
                        continue;
                    };
                    let resource = resources
                        .and_then(|d| d.get(b"XObject").ok())
                        .and_then(|o| self.doc.dereference(o).ok())
                        .and_then(|(_, o)| o.as_dict().ok())
                        .and_then(|d| d.get(name).ok());
                    let Some(Object::Reference(id)) = resource else {
                        self.uncertain(resources)?;
                        continue;
                    };
                    let stream = match self.doc.get_object(*id).and_then(Object::as_stream) {
                        Ok(s) => s,
                        Err(_) => {
                            self.uncertain(resources)?;
                            continue;
                        }
                    };
                    if inspect::named(&stream.dict, b"Subtype", b"Image") {
                        if let Some(size) = ctm.size() {
                            let entry = self.usage.sizes.entry(*id).or_insert([0., 0.]);
                            entry[0] = entry[0].max(size[0]);
                            entry[1] = entry[1].max(size[1]);
                        } else {
                            self.usage.unknown.insert(*id);
                        }
                    } else if inspect::named(&stream.dict, b"Subtype", b"Form") {
                        if !path.insert(*id) {
                            self.uncertain(resources)?;
                            continue;
                        }
                        let form_matrix = match stream.dict.get(b"Matrix") {
                            Ok(o) => o.as_array().ok().and_then(|a| matrix(a)),
                            Err(_) => Some(Matrix::identity()),
                        };
                        let own_resources = stream
                            .dict
                            .get(b"Resources")
                            .ok()
                            .and_then(|o| self.doc.dereference(o).ok())
                            .and_then(|(_, o)| o.as_dict().ok());
                        match (
                            form_matrix,
                            stream.get_plain_content_with_limit(self.limits.max_stream_bytes),
                        ) {
                            (Some(m), Ok(data)) => self.walk(
                                &data,
                                own_resources.or(resources),
                                ctm.multiply(m),
                                path,
                                depth + 1,
                            )?,
                            _ => {
                                self.uncertain(resources)?;
                            }
                        }
                        path.remove(id);
                    } else {
                        self.uncertain(resources)?;
                    }
                }
                _ => {}
            }
        }
        if !stack.is_empty() {
            self.uncertain(resources)?;
        }
        Ok(())
    }
}
