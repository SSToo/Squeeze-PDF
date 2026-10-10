//! Decode-equivalent, explicitly typed leaves. Whole dictionaries still agree.
use crate::{Options, Result, check_cancel, inspect, stream_filters, structure};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::atomic::AtomicBool,
};

fn semantics(d: &Dictionary) -> Dictionary {
    let mut d = d.clone();
    for key in [b"Length".as_slice(), b"Filter", b"DecodeParms"] {
        d.remove(key);
    }
    d
}
pub(crate) fn deduplicate(
    doc: &mut Document,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<usize> {
    if crate::identity::bound(doc, &options.limits) {
        return Ok(0);
    }
    let mut leaves = BTreeMap::<ObjectId, u8>::new();
    // Do not turn an arbitrary stream with equal bytes into a font or profile.
    fn add(doc: &Document, o: &Object, kind: u8, leaves: &mut BTreeMap<ObjectId, u8>) {
        if let Ok((Some(id), Object::Stream(_))) = doc.dereference(o) {
            leaves
                .entry(id)
                .and_modify(|old| {
                    if *old != kind {
                        *old = 0;
                    }
                })
                .or_insert(kind);
        }
    }
    let mut work = options.limits.max_operations;
    fn visit(
        doc: &Document,
        o: &Object,
        leaves: &mut BTreeMap<ObjectId, u8>,
        work: &mut usize,
        depth: usize,
    ) {
        if *work == 0 || depth > 64 {
            return;
        }
        *work -= 1;
        if let Some(d) = inspect::dictionary(o) {
            if let Ok(o) = d.get(b"FontFile2") {
                add(doc, o, 1, leaves);
            }
            if let Ok(o) = d.get(b"ToUnicode") {
                add(doc, o, 2, leaves);
            }
            for (_, o) in d.iter() {
                visit(doc, o, leaves, work, depth + 1);
            }
        } else if let Object::Array(a) = o {
            if a.len() == 2 && a[0].as_name().ok() == Some(b"ICCBased") {
                add(doc, &a[1], 3, leaves);
            }
            for o in a {
                visit(doc, o, leaves, work, depth + 1);
            }
        }
    }
    for o in doc.objects.values() {
        check_cancel(cancel)?;
        visit(doc, o, &mut leaves, &mut work, 0);
    }
    if work == 0 {
        return Ok(0);
    }
    let mut groups = HashMap::<(u8, [u8; 32]), Vec<(ObjectId, Vec<u8>)>>::new();
    let mut mapping = HashMap::new();
    let mut replacements = HashMap::<ObjectId, Stream>::new();
    let mut remaining = 32 * 1024 * 1024usize;
    for (id, kind) in leaves {
        check_cancel(cancel)?;
        if kind == 0 {
            continue;
        }
        let s = doc.get_object(id)?.as_stream()?;
        let Ok(raw) = stream_filters::content(
            doc,
            s,
            options
                .limits
                .max_stream_bytes
                .min(4 * 1024 * 1024)
                .min(remaining),
        ) else {
            continue;
        };
        if raw.len() > remaining {
            continue;
        }
        remaining -= raw.len();
        let group = groups
            .entry((kind, Sha256::digest(&raw).into()))
            .or_default();
        let other = group.iter().find(|(other, bytes)| {
            bytes == &raw
                && semantics(&doc.get_object(*other).unwrap().as_stream().unwrap().dict)
                    == semantics(&s.dict)
        });
        if let Some((other, _)) = other {
            let keeper = replacements
                .get(other)
                .unwrap_or(doc.get_object(*other)?.as_stream()?);
            if crate::image_encoding::cost(s)? < crate::image_encoding::cost(keeper)? {
                replacements.insert(*other, s.clone());
            }
            mapping.insert(id, *other);
        } else {
            group.push((id, raw));
        }
    }
    for (id, s) in replacements {
        doc.objects.insert(id, Object::Stream(s));
    }
    structure::rewrite_references(doc, &mapping, cancel)?;
    Ok(mapping.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    #[test]
    fn typed_leaves_merge_decoded_bytes_but_preserve_kind_and_semantics() {
        let mut d = Document::new();
        let raw = b"font-or-cmap-data".repeat(100);
        let a = d.add_object(Stream::new(dictionary! {}, raw.clone()));
        let b = d.add_object(Stream::new(
            dictionary! {"Filter"=>"FlateDecode"},
            crate::lossless::deflate(&raw).unwrap(),
        ));
        let c = d.add_object(Stream::new(dictionary! {}, raw.clone()));
        let e = d.add_object(Stream::new(dictionary! {"N"=>4}, raw));
        let x = d.add_object(dictionary! {"FontFile2"=>a});
        let y=d.add_object(dictionary!{"FontFile2"=>b,"ToUnicode"=>c,"CS"=>vec![Object::Name(b"ICCBased".to_vec()), e.into()]});
        assert_eq!(
            deduplicate(&mut d, &Options::default(), &AtomicBool::new(false)).unwrap(),
            1
        );
        assert_eq!(
            d.get_dictionary(x).unwrap().get(b"FontFile2").unwrap(),
            d.get_dictionary(y).unwrap().get(b"FontFile2").unwrap()
        );
        assert_eq!(
            d.get_dictionary(y)
                .unwrap()
                .get(b"ToUnicode")
                .unwrap()
                .as_reference()
                .unwrap(),
            c
        );
    }
}
