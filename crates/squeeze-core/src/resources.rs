//! Conservative resource cleanup and exact decoded-sample image sharing.
use crate::{Limits, Options, Result, check_cancel, inspect, usage};
use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

const TABLES: [&[u8]; 6] = [
    b"XObject",
    b"Font",
    b"ColorSpace",
    b"ExtGState",
    b"Shading",
    b"Properties",
];
type Used = HashMap<Vec<u8>, HashSet<Vec<u8>>>;
/// Known resource entries are removed. Each owner receives its own copy so an
/// inherited/shared dictionary remains available to every other caller.
pub(crate) fn cleanup(doc: &mut Document, limits: &Limits, cancel: &AtomicBool) -> Result<usize> {
    let mut updates = Vec::new();
    // An appearance's currently unused fonts may be selected by a field's DA
    // during later editing. Leave Form dictionaries alone in that context.
    let editable_appearances = doc
        .objects
        .values()
        .filter_map(inspect::dictionary)
        .any(|d| d.has(b"AP") || d.has(b"AcroForm") || d.has(b"FT"));
    let mut budget = limits.max_operations.saturating_mul(16);
    let owners: Vec<_> = inspect::pages(doc)?
        .into_iter()
        .chain(doc.objects.iter().filter_map(|(&id, o)| {
            inspect::dictionary(o)
                .filter(|d| inspect::named(d, b"Subtype", b"Form"))
                .map(|_| id)
        }))
        .collect();
    for owner in owners {
        check_cancel(cancel)?;
        let d = doc
            .get_object(owner)?
            .as_dict()
            .ok()
            .or_else(|| {
                doc.get_object(owner)
                    .ok()?
                    .as_stream()
                    .ok()
                    .map(|s| &s.dict)
            })
            .unwrap();
        let page = inspect::named(d, b"Type", b"Page");
        // An appearance without its own resources can borrow its page's
        // dictionary. Keep page resources intact whenever annotations exist.
        if (!page && editable_appearances) || d.has(b"Annots") || d.has(b"Ref") || d.has(b"OPI") {
            continue;
        }
        let r = if page {
            inspect::inherited(doc, owner, b"Resources")?
        } else {
            d.get(b"Resources")
                .ok()
                .and_then(|o| doc.dereference(o).ok())
                .map(|(_, o)| o)
        };
        let Some(Object::Dictionary(r)) = r else {
            continue;
        };
        if !TABLES.iter().any(|key| r.has(key)) {
            continue;
        }
        let bytes = if page {
            usage::page_content(doc, d, limits.max_stream_bytes)
        } else {
            doc.get_object(owner)?
                .as_stream()?
                .get_plain_content_with_limit(limits.max_stream_bytes)
        };
        let Ok(bytes) = bytes else {
            continue;
        };
        let mut used = Used::new();
        if !names(
            doc,
            &bytes,
            r,
            &mut used,
            &mut HashSet::new(),
            &mut budget,
            limits,
            cancel,
        )? {
            continue;
        }
        let mut private = r.clone();
        let mut removed = 0;
        let mut complete = true;
        for key in TABLES {
            let Ok(o) = r.get(key) else {
                if used.get(key).is_some_and(|names| !names.is_empty()) {
                    complete = false;
                }
                continue;
            };
            let Some(table) = doc.dereference(o).ok().and_then(|(_, o)| o.as_dict().ok()) else {
                complete = false;
                break;
            };
            let entries = used.get(key);
            if entries.is_some_and(|names| names.iter().any(|name| !table.has(name))) {
                complete = false;
                break;
            }
            let mut filtered = table.clone();
            for (name, _) in table.iter() {
                // These can be implicitly selected by device-color operators.
                // Indirect color selections need their own dependency walk.
                // Retain the whole color table when they can occur.
                let indirect_color = [
                    b"XObject".as_slice(),
                    b"Shading",
                    b"ExtGState",
                    b"ColorSpace",
                ]
                .iter()
                .any(|key| used.get(*key).is_some_and(|names| !names.is_empty()));
                let implicit = key == b"ColorSpace"
                    && (indirect_color
                        || [b"DefaultRGB".as_slice(), b"DefaultGray", b"DefaultCMYK"]
                            .iter()
                            .any(|key| table.has(key)));
                if !implicit && !entries.is_some_and(|names| names.contains(name)) {
                    filtered.remove(name);
                }
            }
            removed += table.len() - filtered.len();
            private.set(key, filtered);
        }
        if complete && removed > 0 {
            updates.push((owner, private, removed));
        }
    }
    let mut count = 0;
    for (owner, r, removed) in updates {
        let o = doc.get_object_mut(owner)?;
        match o {
            Object::Dictionary(d) => d.set("Resources", r),
            Object::Stream(s) => s.dict.set("Resources", r),
            _ => unreachable!(),
        }
        count += removed;
    }
    Ok(count)
}
#[allow(clippy::too_many_arguments)]
fn names(
    doc: &Document,
    bytes: &[u8],
    r: &Dictionary,
    used: &mut Used,
    path: &mut HashSet<ObjectId>,
    budget: &mut usize,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<bool> {
    // These can invoke content with an inherited resource dictionary. Their
    // implicit resource use is intentionally outside this first cleanup pass.
    if r.has(b"Pattern") {
        return Ok(false);
    }
    for key in [b"Font".as_slice(), b"ExtGState"] {
        if let Some(d) = r
            .get(key)
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok())
        {
            for (_, v) in d.iter() {
                let Some(d) = doc
                    .dereference(v)
                    .ok()
                    .and_then(|(_, o)| inspect::dictionary(o))
                else {
                    return Ok(false);
                };
                if inspect::named(d, b"Subtype", b"Type3") || d.has(b"SMask") {
                    return Ok(false);
                }
                if key == b"ExtGState"
                    && let Ok(font) = d.get(b"Font")
                {
                    let Some(font) = doc
                        .dereference(font)
                        .ok()
                        .and_then(|(_, o)| o.as_array().ok())
                        .and_then(|a| a.first())
                    else {
                        return Ok(false);
                    };
                    let Some(font) = doc
                        .dereference(font)
                        .ok()
                        .and_then(|(_, o)| inspect::dictionary(o))
                    else {
                        return Ok(false);
                    };
                    if inspect::named(font, b"Subtype", b"Type3") {
                        return Ok(false);
                    }
                }
            }
        }
    }
    let Ok(content) = Content::decode(bytes) else {
        return Ok(false);
    };
    for op in content.operations {
        check_cancel(cancel)?;
        if *budget == 0 {
            return Ok(false);
        }
        *budget -= 1;
        // No binary inline-image parsing or unknown extension operators.
        if !matches!(
            op.operator.as_str(),
            "q" | "Q"
                | "cm"
                | "w"
                | "J"
                | "j"
                | "M"
                | "d"
                | "ri"
                | "i"
                | "gs"
                | "m"
                | "l"
                | "c"
                | "v"
                | "y"
                | "h"
                | "re"
                | "S"
                | "s"
                | "f"
                | "F"
                | "f*"
                | "B"
                | "B*"
                | "b"
                | "b*"
                | "n"
                | "W"
                | "W*"
                | "BT"
                | "ET"
                | "Tc"
                | "Tw"
                | "Tz"
                | "TL"
                | "Tf"
                | "Tr"
                | "Ts"
                | "Td"
                | "TD"
                | "Tm"
                | "T*"
                | "Tj"
                | "TJ"
                | "'"
                | "\""
                | "d0"
                | "d1"
                | "CS"
                | "cs"
                | "SC"
                | "SCN"
                | "sc"
                | "scn"
                | "G"
                | "g"
                | "RG"
                | "rg"
                | "K"
                | "k"
                | "sh"
                | "Do"
                | "MP"
                | "DP"
                | "BMC"
                | "BDC"
                | "EMC"
                | "BX"
                | "EX"
        ) {
            return Ok(false);
        }
        let resource = match op.operator.as_str() {
            "Tf" => Some((b"Font".as_slice(), 0, 2)),
            "gs" => Some((b"ExtGState".as_slice(), 0, 1)),
            "sh" => Some((b"Shading".as_slice(), 0, 1)),
            "CS" | "cs" => Some((b"ColorSpace".as_slice(), 0, 1)),
            "BDC" | "DP" if op.operands.len() == 2 && op.operands[1].as_name().is_ok() => {
                Some((b"Properties".as_slice(), 1, 2))
            }
            _ => None,
        };
        if let Some((key, index, length)) = resource {
            if op.operands.len() != length {
                return Ok(false);
            }
            let Ok(name) = op.operands[index].as_name() else {
                return Ok(false);
            };
            let builtin = key == b"ColorSpace"
                && [
                    b"DeviceRGB".as_slice(),
                    b"DeviceGray",
                    b"DeviceCMYK",
                    b"Pattern",
                ]
                .contains(&name);
            if !builtin {
                used.entry(key.to_vec()).or_default().insert(name.to_vec());
            }
        }
        if op.operator != "Do" {
            continue;
        }
        if op.operands.len() != 1 {
            return Ok(false);
        }
        let Ok(name) = op.operands[0].as_name() else {
            return Ok(false);
        };
        used.entry(b"XObject".to_vec())
            .or_default()
            .insert(name.to_vec());
        let Some(v) = r
            .get(b"XObject")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok())
            .and_then(|d| d.get(name).ok())
        else {
            return Ok(false);
        };
        let Ok((id, o)) = doc.dereference(v) else {
            return Ok(false);
        };
        let Ok(s) = o.as_stream() else {
            return Ok(false);
        };
        if inspect::named(&s.dict, b"Subtype", b"Image") {
            continue;
        }
        if !inspect::named(&s.dict, b"Subtype", b"Form") || s.dict.has(b"Ref") || s.dict.has(b"OPI")
        {
            return Ok(false);
        }
        if s.dict.has(b"Resources") {
            continue;
        }
        let Some(id) = id else {
            return Ok(false);
        };
        if path.len() >= 32 || !path.insert(id) {
            return Ok(false);
        }
        let Ok(bytes) = s.get_plain_content_with_limit(limits.max_stream_bytes) else {
            return Ok(false);
        };
        if !names(doc, &bytes, r, used, path, budget, limits, cancel)? {
            return Ok(false);
        }
        path.remove(&id);
    }
    Ok(true)
}

fn semantics(d: &Dictionary) -> Dictionary {
    let mut d = d.clone();
    for key in [b"Length".as_slice(), b"Filter", b"DecodeParms"] {
        d.remove(key);
    }
    d
}
/// Equality is on original component samples and the complete rendering/metadata
/// dictionary, not RGB appearance. Tagged image identities remain separate.
pub(crate) fn deduplicate_images(
    doc: &mut Document,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<usize> {
    let protected = usage::analyze(doc, &options.limits, cancel)?.protected;
    let mut buckets: HashMap<[u8; 32], Vec<ObjectId>> = HashMap::new();
    let mut map = HashMap::new();
    let mut replacements = HashMap::new();
    let mut budget = options.limits.max_stream_bytes.saturating_mul(16);
    for (&id, o) in &doc.objects {
        check_cancel(cancel)?;
        let Ok(s) = o.as_stream() else {
            continue;
        };
        if !inspect::named(&s.dict, b"Subtype", b"Image")
            || protected.contains(&id)
            || s.dict.has(b"StructParent")
            || s.dict.has(b"StructParents")
        {
            continue;
        }
        let Ok(image) = crate::images::decode(doc, s, options) else {
            continue;
        };
        let raw = image.as_bytes();
        if raw.len() > budget {
            break;
        }
        budget -= raw.len();
        let key: [u8; 32] = Sha256::digest(raw).into();
        let group = buckets.entry(key).or_default();
        let mut duplicate = None;
        for (i, &other) in group.iter().enumerate() {
            let stream = doc.get_object(other)?.as_stream()?;
            if semantics(&s.dict) != semantics(&stream.dict) {
                continue;
            }
            let Ok(decoded) = crate::images::decode(doc, stream, options) else {
                continue;
            };
            if decoded.as_bytes().len() > budget {
                break;
            }
            budget -= decoded.as_bytes().len();
            if decoded.as_bytes() == raw {
                duplicate = Some((i, other));
                break;
            }
        }
        if let Some((_i, other)) = duplicate {
            let keeper = replacements
                .get(&other)
                .unwrap_or(doc.get_object(other)?.as_stream()?);
            if crate::image_encoding::cost(s)? < crate::image_encoding::cost(keeper)? {
                replacements.insert(other, s.clone());
            }
            // Keep the earliest object ID even when a later source encoding is
            // smaller. This avoids needless xref/object-stream growth.
            map.insert(id, other);
        } else {
            group.push(id);
        }
    }
    for (id, stream) in replacements {
        doc.objects.insert(id, Object::Stream(stream));
    }
    crate::structure::rewrite_references(doc, &map, cancel)?;
    Ok(map.len())
}
