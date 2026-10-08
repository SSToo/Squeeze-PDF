use crate::{Error, Limits, Result, check_cancel, inspect};
use lopdf::{Document, Object, ObjectId, dictionary};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

// Rebuild only the attachment name tree and associated-file references. Do not
// import the source catalog, page tree, actions or interactive form fields.
pub(crate) fn copy(
    source: &Document,
    output: &mut Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<usize> {
    let mut entries = Vec::<(Object, Object)>::new();
    let mut seen_specs = HashSet::new();
    fn add(
        source: &Document,
        entries: &mut Vec<(Object, Object)>,
        seen: &mut HashSet<ObjectId>,
        key: Option<&Object>,
        spec: &Object,
    ) -> Result<()> {
        let (id, value) = source.dereference(spec)?;
        let d = value.as_dict()?;
        if !d.has(b"EF") {
            return Ok(());
        }
        if let Some(id) = id
            && !seen.insert(id)
        {
            return Ok(());
        }
        let name = key
            .or_else(|| d.get(b"UF").ok())
            .or_else(|| d.get(b"F").ok())
            .filter(|o| matches!(o, Object::String(..)))
            .cloned()
            .unwrap_or_else(|| Object::string_literal(format!("Attachment {}", entries.len() + 1)));
        entries.push((name, spec.clone()));
        Ok(())
    }
    fn tree(
        source: &Document,
        node: &Object,
        entries: &mut Vec<(Object, Object)>,
        specs: &mut HashSet<ObjectId>,
        visited: &mut HashSet<ObjectId>,
        depth: usize,
        cancel: &AtomicBool,
    ) -> Result<()> {
        check_cancel(cancel)?;
        let (id, node) = source.dereference(node)?;
        if depth > 64 || id.is_some_and(|id| !visited.insert(id)) {
            return Err(Error::Invalid("嵌入附件结构无效".into()));
        }
        let d = node.as_dict()?;
        if let Ok(names) = d.get(b"Names") {
            let names = source.dereference(names)?.1.as_array()?;
            if names.len() % 2 != 0 {
                return Err(Error::Invalid("嵌入附件结构无效".into()));
            }
            for pair in names.chunks_exact(2) {
                check_cancel(cancel)?;
                add(source, entries, specs, Some(&pair[0]), &pair[1])?;
            }
        }
        if let Ok(kids) = d.get(b"Kids") {
            for kid in source.dereference(kids)?.1.as_array()? {
                tree(source, kid, entries, specs, visited, depth + 1, cancel)?;
            }
        }
        Ok(())
    }
    let catalog = source.catalog()?;
    if let Ok(names) = catalog.get(b"Names") {
        let names = source.dereference(names)?.1.as_dict()?;
        if let Ok(root) = names.get(b"EmbeddedFiles") {
            tree(
                source,
                root,
                &mut entries,
                &mut seen_specs,
                &mut HashSet::new(),
                0,
                cancel,
            )?;
        }
    }
    // Also retain files attached to annotations or PDF associated-file lists,
    // even when they are not registered in the catalog's EmbeddedFiles tree.
    for object in source.objects.values() {
        check_cancel(cancel)?;
        if let Some(d) = inspect::dictionary(object) {
            if inspect::named(d, b"Subtype", b"FileAttachment")
                && let Ok(spec) = d.get(b"FS")
            {
                add(source, &mut entries, &mut seen_specs, None, spec)?;
            }
            if let Ok(files) = d.get(b"AF") {
                for spec in source.dereference(files)?.1.as_array()? {
                    add(source, &mut entries, &mut seen_specs, None, spec)?;
                }
            }
        }
    }
    if entries.is_empty() {
        return Ok(0);
    }
    struct Import<'a> {
        source: &'a Document,
        output: &'a mut Document,
        ids: HashMap<ObjectId, ObjectId>,
        limits: &'a Limits,
        cancel: &'a AtomicBool,
    }
    impl Import<'_> {
        fn object(&mut self, object: &Object, depth: usize) -> Result<Object> {
            check_cancel(self.cancel)?;
            if depth > 64 {
                return Err(Error::Limit("PDF 对象嵌套过深".into()));
            }
            Ok(match object {
                Object::Reference(id) => {
                    if let Some(mapped) = self.ids.get(id) {
                        return Ok(Object::Reference(*mapped));
                    }
                    if self.output.objects.len() >= self.limits.max_objects {
                        return Err(Error::Limit("PDF 对象数量过多".into()));
                    }
                    let new_id = self.output.new_object_id();
                    self.ids.insert(*id, new_id);
                    let value = self.object(self.source.get_object(*id)?, depth + 1)?;
                    self.output.objects.insert(new_id, value);
                    Object::Reference(new_id)
                }
                Object::Array(items) => Object::Array(
                    items
                        .iter()
                        .map(|o| self.object(o, depth + 1))
                        .collect::<Result<_>>()?,
                ),
                Object::Dictionary(d) => {
                    let mut copy = dictionary! {};
                    for (key, value) in d.iter() {
                        copy.set(key.clone(), self.object(value, depth + 1)?);
                    }
                    Object::Dictionary(copy)
                }
                Object::Stream(stream) => {
                    let mut copy = stream.clone();
                    copy.dict = self
                        .object(&Object::Dictionary(stream.dict.clone()), depth + 1)?
                        .as_dict()?
                        .clone();
                    Object::Stream(copy)
                }
                _ => object.clone(),
            })
        }
    }
    let count = entries.len();
    let mut import = Import {
        source,
        output,
        ids: HashMap::new(),
        limits,
        cancel,
    };
    let mut names = Vec::new();
    let mut keys = HashSet::new();
    for (mut key, spec) in entries {
        let mut suffix = 1;
        while !keys.insert(key.as_str()?.to_vec()) {
            key = Object::string_literal(format!("Attachment {} ({suffix})", names.len() / 2 + 1));
            suffix += 1;
        }
        names.push(key);
        names.push(import.object(&spec, 0)?);
    }
    let mut pairs = names
        .chunks_exact(2)
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| a.0.as_str().unwrap().cmp(b.0.as_str().unwrap()));
    let names = pairs
        .into_iter()
        .flat_map(|(key, value)| [key, value])
        .collect::<Vec<_>>();
    import.output.catalog_mut()?.set(
        "Names",
        dictionary! {"EmbeddedFiles" => dictionary! {"Names"=>names}},
    );
    if let Ok(files) = catalog.get(b"AF") {
        let files = import.object(files, 0)?;
        import.output.catalog_mut()?.set("AF", files);
    }
    let source_pages = inspect::pages(source)?;
    let output_pages = inspect::pages(import.output)?;
    for (before, after) in source_pages.into_iter().zip(output_pages) {
        if let Ok(files) = source.get_dictionary(before)?.get(b"AF") {
            let files = import.object(files, 0)?;
            import
                .output
                .get_object_mut(after)?
                .as_dict_mut()?
                .set("AF", files);
        }
    }
    if import.output.objects.len() > limits.max_objects {
        return Err(Error::Limit("PDF 对象数量过多".into()));
    }
    Ok(count)
}
