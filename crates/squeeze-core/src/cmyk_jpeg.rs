//! Keep PDF colour/decode semantics and APP14/ICC, without RGB conversion.
use crate::{Options, Result, check_cancel, inspect, stream_filters};
use lopdf::{Document, Object, Stream};
use std::sync::atomic::AtomicBool;
pub(crate) fn optimize(
    doc: &mut Document,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<usize> {
    let protected = crate::usage::analyze(doc, &options.limits, cancel)?.protected;
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    let mut changed = 0;
    let mut work = 0;
    for id in ids {
        check_cancel(cancel)?;
        let Ok(s) = doc.get_object(id)?.as_stream() else {
            continue;
        };
        if !inspect::named(&s.dict, b"Subtype", b"Image") || protected.contains(&id) {
            continue;
        }
        let Ok(filters) = stream_filters::filters(doc, s) else {
            continue;
        };
        if filters.len() != 1 || filters[0].name != b"DCTDecode" {
            continue;
        }
        let Some(space) = s
            .dict
            .get(b"ColorSpace")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .map(|(_, o)| o)
        else {
            continue;
        };
        let cmyk = space.as_name().ok() == Some(b"DeviceCMYK")
            || space.as_array().is_ok_and(|a| {
                a.first().and_then(|o| o.as_name().ok()) == Some(b"ICCBased")
                    && crate::native_samples::channels(doc, space) == Some(4)
            });
        let integer = |k: &[u8]| {
            s.dict
                .get(k)
                .ok()
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(_, o)| o.as_i64().ok())
        };
        if !cmyk
            || crate::jpeg_lossless::components(&s.content) != Some(4)
            || integer(b"BitsPerComponent") != Some(8)
            || [
                b"SMaskInData".as_slice(),
                b"Alternates",
                b"OPI",
                b"ImageMask",
            ]
            .iter()
            .any(|k| s.dict.has(k))
        {
            continue;
        }
        if let Some(p) = &filters[0].params {
            let Ok((_, Object::Dictionary(p))) = doc.dereference(p) else {
                continue;
            };
            if p.iter().any(|(k, v)| {
                k.as_slice() != b"ColorTransform"
                    || !doc
                        .dereference(v)
                        .is_ok_and(|(_, o)| matches!(o.as_i64(), Ok(0 | 1)))
            }) {
                continue;
            }
        }
        let (Some(w), Some(h)) = (
            integer(b"Width").and_then(|n| u32::try_from(n).ok()),
            integer(b"Height").and_then(|n| u32::try_from(n).ok()),
        ) else {
            continue;
        };
        if work >= 16 {
            break;
        }
        work += 1;
        let Ok(outputs) = crate::jpeg_lossless::optimize(
            &s.content,
            [w, h],
            options.limits.max_image_pixels,
            options.limits.max_stream_bytes,
            true,
        ) else {
            continue;
        };
        let mut best_cost = crate::image_encoding::cost(s)?;
        let mut best = None;
        for (_, bytes) in outputs {
            check_cancel(cancel)?;
            let candidate = Stream::new(s.dict.clone(), bytes);
            let cost = crate::image_encoding::cost(&candidate)?;
            if cost < best_cost {
                best_cost = cost;
                best = Some(candidate);
            }
        }
        if let Some(s) = best {
            doc.objects.insert(id, Object::Stream(s));
            changed += 1;
        }
    }
    Ok(changed)
}
