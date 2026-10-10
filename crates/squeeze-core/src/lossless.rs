use crate::{Limits, Result, check_cancel, inspect};
use flate2::{Compression, write::ZlibEncoder};
use lopdf::{Document, Object, ObjectId};
use std::{
    collections::HashSet,
    io::Write,
    num::NonZeroU64,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
pub(crate) fn inflate(bytes: &[u8], limit: usize) -> std::io::Result<Vec<u8>> {
    crate::stream_filters::flate(bytes, limit, &AtomicBool::new(false))
        .map_err(|e| std::io::Error::other(e.to_string()))
}

pub(crate) fn deflate(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(bytes)?;
    encoder.finish()
}
pub(crate) fn optimize(doc: &mut Document, limits: &Limits, cancel: &AtomicBool) -> Result<usize> {
    let mut changed = 0;
    for object in doc.objects.values_mut() {
        check_cancel(cancel)?;
        if inspect::is_container(object) {
            continue;
        }
        let Object::Stream(s) = object else {
            continue;
        };
        if inspect::named(&s.dict, b"Subtype", b"Image") {
            continue;
        }
        let raw = match s.dict.get(b"Filter") {
            Err(_) if !s.dict.has(b"DecodeParms") && s.content.len() <= limits.max_stream_bytes => {
                s.content.clone()
            }
            Ok(Object::Name(n)) if n == b"FlateDecode" && !s.dict.has(b"DecodeParms") => {
                match inflate(&s.content, limits.max_stream_bytes) {
                    Ok(raw) => raw,
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        let compressed = deflate(&raw)?;
        if compressed.len().saturating_add(24) < s.content.len() {
            s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
            s.set_content(compressed);
            changed += 1;
        }
    }
    Ok(changed)
}

/// Bounded exact re-deflation, including predictor bytes without modifying
/// DecodeParms. Only one-name Flate streams participate. Special shared images
/// retain their original encoding; JPEG/G4 and opaque filter chains are skipped.
pub(crate) fn zopfli(
    doc: &mut Document,
    limits: &Limits,
    protected: &HashSet<ObjectId>,
    cancel: &AtomicBool,
) -> Result<usize> {
    let mut ids: Vec<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            let s = o.as_stream().ok()?;
            (s.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"FlateDecode")
                && s.content.len() >= 2048
                && !protected.contains(&id)
                && !inspect::is_container(o)
                && !s.dict.has(b"F")
                && !inspect::named(&s.dict, b"Type", b"EmbeddedFile")
                && !inspect::named(&s.dict, b"Type", b"Metadata"))
            .then_some((id, s.content.len()))
        })
        .collect();
    ids.sort_by_key(|&(id, bytes)| (std::cmp::Reverse(bytes), id));
    let started = Instant::now();
    let mut remaining = 1024 * 1024usize;
    let mut attempts = 0;
    let mut changed = 0;
    let cap = limits.max_stream_bytes.min(256 * 1024);
    for (id, _) in ids.into_iter().take(48) {
        check_cancel(cancel)?;
        if attempts >= 6 || started.elapsed() > Duration::from_secs(4) {
            break;
        }
        let s = doc.get_object(id)?.as_stream()?;
        let Ok(raw) = inflate(&s.content, cap) else {
            continue;
        };
        if raw.len() > remaining {
            continue;
        }
        remaining -= raw.len();
        attempts += 1;
        let mut output = Vec::new();
        let settings = zopfli::Options {
            iteration_count: NonZeroU64::new(2).unwrap(),
            iterations_without_improvement: NonZeroU64::new(1).unwrap(),
            maximum_block_splits: 3,
        };
        // One attempt has a bounded decoded size/iteration count. The deadline
        // is checked between attempts; it is not a hard preemptive timeout.
        zopfli::compress(settings, zopfli::Format::Zlib, raw.as_slice(), &mut output)?;
        check_cancel(cancel)?;
        if output.len().saturating_add(24) < s.content.len()
            && inflate(&output, cap).is_ok_and(|decoded| decoded == raw)
        {
            doc.get_object_mut(id)?.as_stream_mut()?.set_content(output);
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};
    #[test]
    fn zopfli_keeps_predictor_bytes_parameters_and_protected_encodings_exact() {
        let mut doc = Document::new();
        let raw: Vec<u8> = (0u32..40000)
            .flat_map(|n| (n % 12345).to_le_bytes())
            .collect();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&raw).unwrap();
        let encoded = encoder.finish().unwrap();
        let params = dictionary! {"Predictor"=>15,"Colors"=>1,"Columns"=>399,"BitsPerComponent"=>8};
        let stream = Stream::new(
            dictionary! {"Subtype"=>"Image","Filter"=>"FlateDecode","DecodeParms"=>params.clone()},
            encoded.clone(),
        );
        let first = doc.add_object(stream.clone());
        let protected = doc.add_object(stream);
        let over_cap = doc.add_object(Stream::new(
            dictionary! {"Filter"=>"FlateDecode"},
            deflate(&vec![42; 300000]).unwrap(),
        ));
        let large_before = doc.get_object(over_cap).unwrap().clone();
        zopfli(
            &mut doc,
            &Limits::default(),
            &HashSet::from([protected]),
            &AtomicBool::new(false),
        )
        .unwrap();
        let output = doc.get_object(first).unwrap().as_stream().unwrap();
        assert_eq!(inflate(&output.content, raw.len()).unwrap(), raw);
        assert_eq!(
            output.dict.get(b"DecodeParms").unwrap().as_dict().unwrap(),
            &params
        );
        assert_eq!(
            doc.get_object(protected)
                .unwrap()
                .as_stream()
                .unwrap()
                .content,
            encoded
        );
        assert_eq!(doc.get_object(over_cap).unwrap(), &large_before);
        assert!(output.content.len() < encoded.len());
        assert!(matches!(
            zopfli(
                &mut doc,
                &Limits::default(),
                &HashSet::new(),
                &AtomicBool::new(true)
            ),
            Err(crate::Error::Cancelled)
        ));
    }
}
