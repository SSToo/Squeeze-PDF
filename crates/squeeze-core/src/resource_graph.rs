//! Share fully equivalent resource graphs, retaining identity-bearing objects.
use crate::{Limits, Result, check_cancel, inspect, structure};
use lopdf::{Dictionary, Document, Object, ObjectId};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::atomic::AtomicBool,
};

fn safe(d: &Dictionary) -> bool {
    ![
        b"Parent".as_slice(),
        b"P",
        b"StructParent",
        b"StructParents",
        b"MCID",
        b"Ref",
        b"OPI",
        b"ByteRange",
        b"ID",
        b"Stm",
        b"AP",
        b"FT",
        b"MarkInfo",
        b"StructTreeRoot",
    ]
    .iter()
    .any(|key| d.has(key))
        && !matches!(
            d.get(b"Type").and_then(Object::as_name).ok(),
            Some(
                b"Page"
                    | b"Pages"
                    | b"Catalog"
                    | b"Annot"
                    | b"OCG"
                    | b"OCMD"
                    | b"StructElem"
                    | b"MCR"
                    | b"OBJR"
                    | b"Filespec"
                    | b"EmbeddedFile"
                    | b"Metadata"
                    | b"ObjStm"
                    | b"XRef"
            )
        )
}
struct Fingerprints<'a> {
    doc: &'a Document,
    memo: HashMap<ObjectId, Option<[u8; 32]>>,
    path: HashSet<ObjectId>,
    work: usize,
    bytes: usize,
    identity_bound: bool,
}
impl Fingerprints<'_> {
    fn hash(&mut self, o: &Object, depth: usize) -> Option<[u8; 32]> {
        if depth > 64 || self.work == 0 {
            return None;
        }
        self.work -= 1;
        if let Object::Reference(id) = o {
            if let Some(h) = self.memo.get(id) {
                return *h;
            }
            if !self.path.insert(*id) {
                return None;
            }
            let result = self
                .doc
                .objects
                .get(id)
                .and_then(|value| self.hash(value, depth + 1));
            self.path.remove(id);
            self.memo.insert(*id, result);
            return result;
        }
        let mut h = Sha256::new();
        match o {
            Object::Null => h.update(b"null"),
            Object::Boolean(v) => h.update([b'b', u8::from(*v)]),
            Object::Integer(v) => {
                h.update(b"int");
                h.update(v.to_be_bytes());
            }
            Object::Real(v) => {
                h.update(b"real");
                h.update(v.to_bits().to_be_bytes());
            }
            Object::Name(v) | Object::String(v, _) => {
                h.update(if matches!(o, Object::Name(_)) {
                    b"name".as_slice()
                } else {
                    b"string"
                });
                h.update((v.len() as u64).to_be_bytes());
                h.update(v);
            }
            Object::Array(a) => {
                h.update(b"array");
                h.update((a.len() as u64).to_be_bytes());
                for v in a {
                    h.update(self.hash(v, depth + 1)?);
                }
            }
            Object::Dictionary(d) => {
                h.update(b"dict");
                self.dict(d, &mut h, depth, false)?;
            }
            Object::Stream(s) => {
                // Pure resource dictionaries may still share (e.g. standard
                // fonts). Any graph reaching a stream retains its identity in
                // documents with tags, appearances or an uncertain scan.
                if self.identity_bound {
                    return None;
                }
                h.update(b"stream");
                self.dict(&s.dict, &mut h, depth, true)?;
                if s.content.len() > self.bytes {
                    return None;
                }
                self.bytes -= s.content.len();
                h.update((s.content.len() as u64).to_be_bytes());
                h.update(&s.content);
            }
            Object::Reference(_) => unreachable!(),
        }
        Some(h.finalize().into())
    }
    fn dict(&mut self, d: &Dictionary, h: &mut Sha256, depth: usize, stream: bool) -> Option<()> {
        if !safe(d) {
            return None;
        }
        let mut keys: Vec<_> = d
            .iter()
            .filter(|(k, _)| !stream || k.as_slice() != b"Length")
            .collect();
        keys.sort_by_key(|(k, _)| *k);
        h.update((keys.len() as u64).to_be_bytes());
        for (key, value) in keys {
            h.update((key.len() as u64).to_be_bytes());
            h.update(key);
            h.update(self.hash(value, depth + 1)?);
        }
        Some(())
    }
}
pub(crate) fn equivalent(
    doc: &Document,
    a: &Object,
    b: &Object,
    work: &mut usize,
    depth: usize,
) -> bool {
    if *work == 0 || depth > 64 {
        return false;
    }
    *work -= 1;
    let (Ok((_, a)), Ok((_, b))) = (doc.dereference(a), doc.dereference(b)) else {
        return false;
    };
    let equal_dict = |x: &Dictionary, y: &Dictionary, stream: bool, work: &mut usize| {
        let count = |d: &Dictionary| {
            d.iter()
                .filter(|(k, _)| !stream || k.as_slice() != b"Length")
                .count()
        };
        safe(x)
            && safe(y)
            && count(x) == count(y)
            && x.iter().all(|(k, v)| {
                (stream && k.as_slice() == b"Length")
                    || y.get(k)
                        .is_ok_and(|other| equivalent(doc, v, other, work, depth + 1))
            })
    };
    match (a, b) {
        (Object::Array(x), Object::Array(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(x, y)| equivalent(doc, x, y, work, depth + 1))
        }
        (Object::Dictionary(x), Object::Dictionary(y)) => equal_dict(x, y, false, work),
        (Object::Stream(x), Object::Stream(y)) => {
            x.content == y.content && equal_dict(&x.dict, &y.dict, true, work)
        }
        (Object::String(x, _), Object::String(y, _)) => x == y,
        _ => a == b,
    }
}

pub(crate) fn deduplicate(
    doc: &mut Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<usize> {
    check_cancel(cancel)?;
    let identity_bound = crate::identity::bound(doc, limits);
    // Roots come only from resource dictionaries. Follow their child objects,
    // but not page trees, annotation identity or document-level relationships.
    let mut pending = Vec::new();
    for d in doc.objects.values().filter_map(inspect::dictionary) {
        if let Ok(r) = d.get(b"Resources") {
            pending.push(r);
        }
    }
    let mut ids = BTreeSet::new();
    let mut walk = limits.max_operations;
    while let Some(o) = pending.pop() {
        check_cancel(cancel)?;
        if walk == 0 {
            break;
        }
        walk -= 1;
        match o {
            Object::Reference(id) if ids.insert(*id) => {
                if let Some(o) = doc.objects.get(id) {
                    pending.push(o);
                }
            }
            Object::Array(a) => pending.extend(a),
            Object::Dictionary(d) if safe(d) => pending.extend(d.iter().map(|(_, o)| o)),
            Object::Stream(s) if safe(&s.dict) => pending.extend(s.dict.iter().map(|(_, o)| o)),
            _ => {}
        }
    }
    let mut hashes = Fingerprints {
        doc,
        memo: HashMap::new(),
        path: HashSet::new(),
        work: limits.max_operations.saturating_mul(4),
        bytes: limits.max_stream_bytes.saturating_mul(8),
        identity_bound,
    };
    let mut buckets: HashMap<[u8; 32], Vec<ObjectId>> = HashMap::new();
    let mut map = HashMap::new();
    let mut compare_work = limits.max_operations.saturating_mul(4);
    for id in ids {
        check_cancel(cancel)?;
        // Do not alias scalar values; their object framing is tiny and causes
        // surprising parent/Length changes with no useful resource saving.
        if !matches!(
            doc.objects.get(&id),
            Some(Object::Dictionary(_) | Object::Stream(_) | Object::Array(_))
        ) {
            continue;
        }
        let reference = Object::Reference(id);
        let Some(hash) = hashes.hash(&reference, 0) else {
            continue;
        };
        let group = buckets.entry(hash).or_default();
        if let Some(&other) = group.iter().find(|&&other| {
            equivalent(
                doc,
                &reference,
                &Object::Reference(other),
                &mut compare_work,
                0,
            )
        }) {
            map.insert(id, other);
        } else {
            group.push(id);
        }
    }
    structure::rewrite_references(doc, &map, cancel)?;
    Ok(map.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};
    fn pair(tag: bool, different: bool) -> (Document, ObjectId, ObjectId, ObjectId) {
        let mut doc = Document::new();
        let mut forms = Vec::new();
        for i in 0..2 {
            let font = doc.add_object(
                dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"},
            );
            let resources = doc.add_object(dictionary! {"Font"=>dictionary! {"F"=>font}});
            let mut d = dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),100.into(),100.into()], "Resources"=>resources};
            if tag {
                d.set("StructParent", i);
            }
            if different && i == 1 {
                d.set(
                    "Matrix",
                    vec![1.into(), 0.into(), 0.into(), 1.into(), 1.into(), 0.into()],
                );
            }
            forms.push(doc.add_object(Stream::new(d, b"BT /F 12 Tf (Hello) Tj ET".to_vec())));
        }
        let root = doc.add_object(dictionary! {"Resources"=>dictionary! {"XObject"=>dictionary! {"A"=>forms[0],"B"=>forms[1]}}});
        (doc, forms[0], forms[1], root)
    }
    #[test]
    fn equivalent_form_graphs_share_despite_distinct_child_reference_numbers() {
        let (mut doc, first, second, root) = pair(false, false);
        assert_eq!(
            deduplicate(&mut doc, &Limits::default(), &AtomicBool::new(false)).unwrap(),
            3
        );
        assert!(doc.objects.contains_key(&first));
        assert!(!doc.objects.contains_key(&second));
        let x = doc
            .get_dictionary(root)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(x.get(b"A").unwrap(), x.get(b"B").unwrap());
    }
    #[test]
    fn tagged_forms_distinct_transforms_cycles_and_tight_budgets_are_not_aliased() {
        for (tag, different) in [(true, false), (false, true)] {
            let (mut doc, first, second, _) = pair(tag, different);
            deduplicate(&mut doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
            assert!(doc.objects.contains_key(&first) && doc.objects.contains_key(&second));
        }
        let (mut doc, first, second, _) = pair(false, false);
        for id in [first, second] {
            doc.get_object_mut(id)
                .unwrap()
                .as_stream_mut()
                .unwrap()
                .dict
                .set("Cycle", id);
        }
        deduplicate(&mut doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(doc.objects.contains_key(&first) && doc.objects.contains_key(&second));
        let (mut doc, first, second, _) = pair(false, false);
        assert_eq!(
            deduplicate(
                &mut doc,
                &Limits {
                    max_operations: 0,
                    ..Default::default()
                },
                &AtomicBool::new(false)
            )
            .unwrap(),
            0
        );
        assert!(doc.objects.contains_key(&first) && doc.objects.contains_key(&second));
        assert!(matches!(
            deduplicate(&mut doc, &Limits::default(), &AtomicBool::new(true)),
            Err(crate::Error::Cancelled)
        ));
    }
}
