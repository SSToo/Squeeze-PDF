use crate::{Limits, Result, check_cancel, inspect, lossless};
use lopdf::{Dictionary, Document, Object, ObjectId};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

pub(crate) fn optimize_content(
    doc: &mut Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<usize> {
    let mut ids = HashSet::new();
    fn collect(
        doc: &Document,
        o: &Object,
        ids: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> lopdf::Result<()> {
        if depth > 64 {
            return Err(lopdf::Error::InvalidStream("内容引用嵌套过深或循环".into()));
        }
        match o {
            Object::Reference(id) => {
                if doc.get_object(*id)?.as_stream().is_ok() {
                    ids.insert(*id);
                } else if let Object::Array(a) = doc.get_object(*id)? {
                    for o in a {
                        collect(doc, o, ids, depth + 1)?;
                    }
                }
            }
            Object::Array(a) => {
                for o in a {
                    collect(doc, o, ids, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for page in inspect::pages(doc)? {
        if let Ok(o) = doc.get_dictionary(page)?.get(b"Contents") {
            collect(doc, o, &mut ids, 0)?;
        }
    }
    for (&id, o) in &doc.objects {
        if let Some(d) = inspect::dictionary(o) {
            if (inspect::named(d, b"Subtype", b"Form")
                || d.get(b"PatternType").and_then(Object::as_i64).ok() == Some(1))
                && o.as_stream().is_ok()
            {
                ids.insert(id);
            }
            if inspect::named(d, b"Subtype", b"Type3")
                && let Ok(procs) = d.get(b"CharProcs")
            {
                for (_, o) in doc.dereference(procs)?.1.as_dict()?.iter() {
                    collect(doc, o, &mut ids, 0)?;
                }
            }
        }
    }
    let mut changed = 0;
    for id in ids {
        check_cancel(cancel)?;
        let s = doc.get_object(id)?.as_stream()?;
        let Ok(raw) = s.get_plain_content_with_limit(limits.max_stream_bytes) else {
            continue;
        };
        let Some(normalized) = normalize_numbers(&raw) else {
            continue;
        };
        if normalized.len() >= raw.len() {
            continue;
        }
        let encoded = lossless::deflate(&normalized)?;
        if encoded.len().saturating_add(24) >= s.content.len() {
            continue;
        }
        let stream = doc.get_object_mut(id)?.as_stream_mut()?;
        stream.dict.set("Filter", "FlateDecode");
        stream.dict.remove(b"DecodeParms");
        stream.set_content(encoded);
        changed += 1;
    }
    Ok(changed)
}

// Rewrite exact decimal spellings only. Strings, names and comments are copied
// byte-for-byte. Inline image streams are skipped because their binary data may
// contain bytes that look like ordinary PDF tokens.
fn normalize_numbers(data: &[u8]) -> Option<Vec<u8>> {
    fn space(c: u8) -> bool {
        matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
    }
    fn delimiter(c: u8) -> bool {
        space(c) || b"()<>[]{}/%".contains(&c)
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let start = i;
        match data[i] {
            b'(' => {
                let mut depth = 1;
                i += 1;
                while i < data.len() && depth > 0 {
                    match data[i] {
                        b'\\' => {
                            i += 1;
                            if i >= data.len() {
                                return None;
                            }
                        }
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                if depth != 0 {
                    return None;
                }
            }
            b'<' if data.get(i + 1) == Some(&b'<') => i += 2,
            b'<' => {
                i += 1;
                while i < data.len() && data[i] != b'>' {
                    i += 1;
                }
                if i == data.len() {
                    return None;
                }
                i += 1;
            }
            b'%' => {
                while i < data.len() && !matches!(data[i], 10 | 13) {
                    i += 1;
                }
            }
            b'/' => {
                i += 1;
                while i < data.len() && !delimiter(data[i]) {
                    i += 1;
                }
            }
            c if delimiter(c) => {
                i += 1;
            }
            _ => {
                while i < data.len() && !delimiter(data[i]) {
                    i += 1;
                }
                let token = &data[start..i];
                if token == b"BI" {
                    return None;
                }
                let unsigned = if matches!(token[0], b'+' | b'-') {
                    &token[1..]
                } else {
                    token
                };
                if !unsigned.is_empty()
                    && unsigned.iter().any(u8::is_ascii_digit)
                    && unsigned.iter().all(|c| c.is_ascii_digit() || *c == b'.')
                    && unsigned.iter().filter(|c| **c == b'.').count() <= 1
                {
                    let dot = unsigned
                        .iter()
                        .position(|c| *c == b'.')
                        .unwrap_or(unsigned.len());
                    let integer = &unsigned[..dot];
                    let integer = &integer[integer
                        .iter()
                        .position(|c| *c != b'0')
                        .unwrap_or(integer.len())..];
                    let fraction = if dot < unsigned.len() {
                        &unsigned[dot + 1..]
                    } else {
                        &[]
                    };
                    let fraction = &fraction[..fraction
                        .iter()
                        .rposition(|c| *c != b'0')
                        .map_or(0, |n| n + 1)];
                    if integer.is_empty() && fraction.is_empty() {
                        out.push(b'0');
                    } else {
                        if token[0] == b'-' {
                            out.push(b'-');
                        }
                        out.extend(integer);
                        if !fraction.is_empty() {
                            out.push(b'.');
                            out.extend(fraction);
                        }
                    }
                    continue;
                }
            }
        }
        out.extend_from_slice(&data[start..i]);
    }
    Some(out)
}

pub(crate) fn deduplicate(doc: &mut Document, cancel: &AtomicBool) -> Result<usize> {
    fn same_dict(a: &Dictionary, b: &Dictionary) -> bool {
        a.iter().filter(|(k, _)| k.as_slice() != b"Length").count()
            == b.iter().filter(|(k, _)| k.as_slice() != b"Length").count()
            && a.iter()
                .all(|(k, v)| k.as_slice() == b"Length" || b.get(k).is_ok_and(|o| o == v))
    }
    let mut buckets: HashMap<[u8; 32], Vec<ObjectId>> = HashMap::new();
    let mut map = HashMap::new();
    for (&id, o) in &doc.objects {
        check_cancel(cancel)?;
        let Ok(stream) = o.as_stream() else {
            continue;
        };
        if inspect::is_container(o) {
            continue;
        }
        let key: [u8; 32] = Sha256::digest(&stream.content).into();
        let candidates = buckets.entry(key).or_default();
        if let Some(&other) = candidates.iter().find(|&&other| {
            let s = doc.get_object(other).unwrap().as_stream().unwrap();
            same_dict(&stream.dict, &s.dict) && s.content == stream.content
        }) {
            map.insert(id, other);
        } else {
            candidates.push(id);
        }
    }
    rewrite_references(doc, &map, cancel)?;
    Ok(map.len())
}

pub(crate) fn rewrite_references(
    doc: &mut Document,
    map: &HashMap<ObjectId, ObjectId>,
    cancel: &AtomicBool,
) -> Result<()> {
    fn rewrite(o: &mut Object, map: &HashMap<ObjectId, ObjectId>) {
        match o {
            Object::Reference(id) => {
                if let Some(new) = map.get(id) {
                    *id = *new;
                }
            }
            Object::Array(a) => {
                for o in a {
                    rewrite(o, map);
                }
            }
            Object::Dictionary(d) => {
                for (_, o) in d.iter_mut() {
                    rewrite(o, map);
                }
            }
            Object::Stream(s) => {
                for (_, o) in s.dict.iter_mut() {
                    rewrite(o, map);
                }
            }
            _ => {}
        }
    }
    for object in doc.objects.values_mut() {
        check_cancel(cancel)?;
        rewrite(object, map);
    }
    for (_, o) in doc.trailer.iter_mut() {
        rewrite(o, map);
    }
    doc.objects.retain(|id, _| !map.contains_key(id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_numbers;
    #[test]
    fn exact_number_normalization_preserves_opaque_pdf_tokens() {
        let data=b"/12.000 /Name#201.000 % 1.000\n(1.000 (2.000) \\) 3.000) <312e303030> [ +001.000 -0.000 0.25000 -.001200 ] TJ";
        assert_eq!(normalize_numbers(data).unwrap(), b"/12.000 /Name#201.000 % 1.000\n(1.000 (2.000) \\) 3.000) <312e303030> [ 1 0 .25 -.0012 ] TJ");
        assert!(normalize_numbers(b"q BI /W 1 /H 1 ID 1.000 EI Q").is_none());
        assert_eq!(
            normalize_numbers(b"/Span << /ActualText (text > 1.000) /Value +001.000 >> BDC")
                .unwrap(),
            b"/Span << /ActualText (text > 1.000) /Value 1 >> BDC"
        );
    }
}
