//! Private, bounded page Contents candidates. Drawing bytes and state are kept.
use crate::{Limits, Result, check_cancel, inspect, lossless, stream_filters, structure};
use lopdf::{Document, Stream, content::Content, dictionary};
use serde::Serialize;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    pub considered_pages: usize,
    pub merged_pages: usize,
    pub merged_streams: usize,
    pub skipped_unsupported: usize,
    pub skipped_budget: usize,
}
fn same_operations(a: &Content, b: &Content) -> bool {
    a.operations.len() == b.operations.len()
        && a.operations
            .iter()
            .zip(&b.operations)
            .all(|(a, b)| a.operator == b.operator && a.operands == b.operands)
}
/// Replacing a Contents array can invalidate MCR /Stm identities even when
/// page pixels and MCIDs are unchanged. Unknown/deep structures are protected.
pub(crate) fn identity_bound(doc: &Document, limits: &Limits) -> bool {
    crate::identity::bound(doc, limits)
}
pub(crate) fn optimize(doc: &mut Document, limits: &Limits, cancel: &AtomicBool) -> Result<Stats> {
    let mut stats = Stats::default();
    check_cancel(cancel)?;
    if identity_bound(doc, limits) {
        stats.skipped_unsupported = 1;
        return Ok(stats);
    }
    let started = Instant::now();
    let mut remaining = 16 * 1024 * 1024usize;
    let mut operations = limits.max_operations;
    let cap = limits.max_stream_bytes.min(256 * 1024);
    for page in inspect::pages(doc)? {
        check_cancel(cancel)?;
        let Some(contents) = doc
            .get_dictionary(page)?
            .get(b"Contents")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_array().ok())
        else {
            continue;
        };
        if contents.len() < 2 {
            continue;
        }
        stats.considered_pages += 1;
        if contents.len() > 128
            || remaining == 0
            || operations == 0
            || started.elapsed() > Duration::from_secs(4)
            || stats.merged_pages >= 128
        {
            stats.skipped_budget += 1;
            continue;
        }
        let mut joined = Vec::new();
        let mut separated = Vec::new();
        let mut old_bytes = 0usize;
        let mut valid = true;
        for o in contents {
            check_cancel(cancel)?;
            let Some(stream) = o
                .as_reference()
                .ok()
                .and_then(|id| doc.get_object(id).ok())
                .and_then(|o| o.as_stream().ok())
            else {
                valid = false;
                break;
            };
            // Keep semantic/identity dictionaries, external streams and special
            // content owners intact. Shared source streams are never mutated.
            if stream.dict.iter().any(|(k, _)| {
                ![b"Length".as_slice(), b"Filter", b"DecodeParms"].contains(&k.as_slice())
            }) {
                valid = false;
                break;
            }
            let stream_cap = cap
                .saturating_sub(joined.len())
                .min(64 * 1024)
                .min(remaining);
            let bytes = match stream_filters::decode(doc, stream, stream_cap, cancel) {
                Ok(bytes) => bytes,
                Err(crate::Error::Cancelled) => return Err(crate::Error::Cancelled),
                Err(crate::Error::Limit(_)) => {
                    stats.skipped_budget += 1;
                    valid = false;
                    break;
                }
                Err(_) => {
                    valid = false;
                    break;
                }
            };
            remaining = remaining.saturating_sub(bytes.len().saturating_add(stream.content.len()));
            // This is a lexical safety check, not a numeric rewrite. Binary
            // inline images and incomplete literal strings stay in their array.
            if structure::normalize_numbers(&bytes).is_none() {
                valid = false;
                break;
            }
            old_bytes = old_bytes.saturating_add(stream.content.len());
            joined.extend_from_slice(&bytes);
            separated.extend_from_slice(&bytes);
            separated.push(b'\n');
        }
        if !valid {
            stats.skipped_unsupported += 1;
            continue;
        }
        let (Ok(exact), Ok(with_separators)) =
            (Content::decode(&joined), Content::decode(&separated))
        else {
            stats.skipped_unsupported += 1;
            continue;
        };
        if exact.operations.len() > operations {
            stats.skipped_budget += 1;
            continue;
        }
        operations -= exact.operations.len();
        // A missing separator, split token, or unterminated end comment must
        // not change its meaning across readers. Preserve exact concatenation.
        if !same_operations(&exact, &with_separators) {
            stats.skipped_unsupported += 1;
            continue;
        }
        let encoded = lossless::deflate(&joined)?;
        if !lossless::inflate(&encoded, cap).is_ok_and(|raw| raw == joined) {
            continue;
        }
        // A local estimate only admits promising candidates; the complete-file
        // comparison decides whether any merge is committed.
        if encoded.len().saturating_add(32) >= old_bytes.saturating_add((contents.len() - 1) * 64) {
            continue;
        }
        let count = contents.len();
        let merged = doc.add_object(Stream::new(dictionary! {"Filter"=>"FlateDecode"}, encoded));
        doc.get_dictionary_mut(page)?.set("Contents", merged);
        stats.merged_pages += 1;
        stats.merged_streams += count;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Object;
    fn page(doc: &mut Document, data: &[&[u8]]) -> (lopdf::ObjectId, Vec<lopdf::ObjectId>) {
        let streams: Vec<_> = data
            .iter()
            .map(|raw| doc.add_object(Stream::new(dictionary! {}, raw.to_vec())))
            .collect();
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Contents"=>streams.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>()});
        doc.objects.insert(
            pages,
            Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        (page, streams)
    }
    #[test]
    fn state_operands_and_shared_streams_are_preserved() {
        let mut doc = Document::new();
        let data: &[&[u8]] = &[
            b"q\n0.3 0.2 0.1 rg\n0 0 ",
            b"80 80 re f\nQ\n% terminal comment\n",
        ];
        let (p, ids) = page(&mut doc, data);
        let originals: Vec<_> = ids.iter().map(|id| doc.objects[id].clone()).collect();
        let holder = doc.add_object(dictionary! {"Shared"=>ids[0]});
        doc.catalog_mut().unwrap().set("TestHolder", holder);
        let stats = optimize(&mut doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(stats.merged_pages, 1);
        let merged = doc
            .get_dictionary(p)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap();
        assert_eq!(
            stream_filters::content(
                &doc,
                doc.get_object(merged).unwrap().as_stream().unwrap(),
                1000
            )
            .unwrap(),
            data.concat()
        );
        for (id, original) in ids.iter().zip(&originals) {
            assert_eq!(&doc.objects[id], original);
        }
    }
    #[test]
    fn ambiguous_comments_split_tokens_inline_images_and_special_dicts_are_skipped() {
        for data in [
            vec![b"q % no newline".as_slice(), b"Q\n"],
            vec![b"1".as_slice(), b"0 0 m\n"],
            vec![b"BI /W 1 /H 1 /BPC 8 /CS /G ID ".as_slice(), b"\0 EI\n"],
            vec![b"BT (split".as_slice(), b" string) Tj ET\n"],
        ] {
            let mut doc = Document::new();
            let (p, _) = page(&mut doc, &data);
            let before = doc.get_dictionary(p).unwrap().clone();
            assert_eq!(
                optimize(&mut doc, &Limits::default(), &AtomicBool::new(false))
                    .unwrap()
                    .merged_pages,
                0
            );
            assert_eq!(doc.get_dictionary(p).unwrap(), &before);
        }
        let mut doc = Document::new();
        let (p, ids) = page(&mut doc, &[b"q\n", b"Q\n"]);
        doc.get_object_mut(ids[0])
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .dict
            .set("PieceInfo", dictionary! {});
        assert_eq!(
            optimize(&mut doc, &Limits::default(), &AtomicBool::new(false))
                .unwrap()
                .merged_pages,
            0
        );
        assert!(
            doc.get_dictionary(p)
                .unwrap()
                .get(b"Contents")
                .unwrap()
                .as_array()
                .is_ok()
        );
        assert!(matches!(
            optimize(&mut doc, &Limits::default(), &AtomicBool::new(true)),
            Err(crate::Error::Cancelled)
        ));
    }
}
