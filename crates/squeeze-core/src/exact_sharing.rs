use crate::{Options, Result, check_cancel, inspect, large_images, resource_graph, structure};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;
use std::{collections::HashMap, sync::atomic::AtomicBool, time::Instant};
#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub considered: usize,
    pub merged: usize,
    pub decoded_work_bytes: usize,
    pub skipped_budget: usize,
}
fn semantics(d: &Dictionary) -> Object {
    let mut d = d.clone();
    for k in [b"Length".as_slice(), b"Filter", b"DecodeParms"] {
        d.remove(k);
    }
    Object::Dictionary(d)
}
pub(crate) fn optimize(
    doc: &mut Document,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<Stats> {
    let mut stats = Stats::default();
    if crate::identity::bound(doc, &options.limits) {
        return Ok(stats);
    }
    let protected = crate::usage::analyze(doc, &options.limits, cancel)?.protected;
    let started = Instant::now();
    let mut remaining = 256 * 1024 * 1024usize;
    let mut comparisons = options.limits.max_operations;
    let mut groups = HashMap::<[u8; 32], Vec<ObjectId>>::new();
    let mut map = HashMap::new();
    let mut replacements = HashMap::new();
    for (&id, o) in &doc.objects {
        check_cancel(cancel)?;
        let Ok(s) = o.as_stream() else { continue };
        if !inspect::named(&s.dict, b"Subtype", b"Image") || protected.contains(&id) {
            continue;
        }
        if stats.considered >= 4096 || remaining == 0 || started.elapsed().as_secs() >= 8 {
            stats.skipped_budget += 1;
            continue;
        }
        stats.considered += 1;
        // Row work is bounded by caller limits. Reject before consuming the document budget.
        let Some(estimate) = large_images::decoded_size(doc, s, options) else {
            continue;
        };
        if estimate > remaining {
            stats.skipped_budget += 1;
            continue;
        }
        let Some((hash, bytes)) = large_images::fingerprint(doc, s, options, cancel)? else {
            continue;
        };
        if bytes > remaining {
            stats.skipped_budget += 1;
            continue;
        }
        remaining -= bytes;
        stats.decoded_work_bytes += bytes;
        let group = groups.entry(hash).or_default();
        let mut duplicate = None;
        for &other in group.iter() {
            let old = doc.get_object(other)?.as_stream()?;
            if bytes.saturating_mul(2) > remaining {
                stats.skipped_budget += 1;
                break;
            }
            if !resource_graph::equivalent(
                doc,
                &semantics(&s.dict),
                &semantics(&old.dict),
                &mut comparisons,
                0,
            ) {
                continue;
            }
            remaining -= bytes * 2;
            stats.decoded_work_bytes += bytes * 2;
            if large_images::same_samples(doc, s, old, options, cancel)? {
                duplicate = Some(other);
                break;
            }
        }
        if let Some(other) = duplicate {
            let keeper = replacements
                .get(&other)
                .unwrap_or(doc.get_object(other)?.as_stream()?);
            if crate::image_encoding::cost(s)? < crate::image_encoding::cost(keeper)? {
                replacements.insert(other, s.clone());
            }
            map.insert(id, other);
            stats.merged += 1;
        } else {
            group.push(id);
        }
    }
    for (id, s) in replacements {
        doc.objects.insert(id, Object::Stream(s));
    }
    structure::rewrite_references(doc, &map, cancel)?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};
    #[test]
    fn indirect_16bit_samples_share_only_with_matching_color_and_decode_semantics() {
        let mut d = Document::new();
        let w = d.add_object(2i64);
        let h = d.add_object(1i64);
        let profile_a = d.add_object(Stream::new(dictionary! {"N"=>3}, b"profile".to_vec()));
        let profile_b = d.add_object(Stream::new(dictionary! {"N"=>3}, b"profile".to_vec()));
        let bytes = vec![0, 0, 0, 10, 0, 90, 255, 255, 0, 80, 0, 70];
        let a = d.add_object(Stream::new(dictionary! {"Subtype"=>"Image", "Width"=>w, "Height"=>h, "BitsPerComponent"=>16, "ColorSpace"=>vec![Object::Name(b"ICCBased".to_vec()), profile_a.into()]}, bytes.clone()));
        let mut s = d.get_object(a).unwrap().as_stream().unwrap().clone();
        s.dict.set(
            "ColorSpace",
            vec![Object::Name(b"ICCBased".to_vec()), profile_b.into()],
        );
        s.dict.set("Filter", "FlateDecode");
        s.set_content(crate::lossless::deflate(&bytes).unwrap());
        let b = d.add_object(s.clone());
        s.dict.set(
            "Decode",
            vec![1.into(), 0.into(), 1.into(), 0.into(), 1.into(), 0.into()],
        );
        let c = d.add_object(s);
        let holder = d.add_object(dictionary! {"Images"=>vec![a.into(), b.into(), c.into()]});
        // Empty page trees are valid for this local resource operation.
        let pages =
            d.add_object(dictionary! {"Type"=>"Pages", "Kids"=>Vec::<Object>::new(), "Count"=>0});
        let root = d.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages, "Holder"=>holder});
        d.trailer.set("Root", root);
        let r = optimize(&mut d, &Options::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(r.merged, 1);
        let refs = d
            .get_dictionary(holder)
            .unwrap()
            .get(b"Images")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(refs[0], refs[1]);
        assert_ne!(refs[1], refs[2]);
    }
}
