//! Exact state reduction; inline only a private page's sole simple vector call.
use crate::{Options, Result, check_cancel, inspect, stream_filters};
use lopdf::{
    Document, Object, ObjectId, Stream,
    content::{Content, Operation},
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};
#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub redundant_operations: usize,
    pub inlined_forms: usize,
    pub changed_streams: usize,
}
fn finite(o: &Object) -> bool {
    matches!(o, Object::Integer(_)) || matches!(o, Object::Real(n) if n.is_finite())
}
fn assignment(op: &Operation) -> bool {
    let a = &op.operands;
    match op.operator.as_str() {
        "ri" => {
            a.len() == 1
                && a[0].as_name().is_ok_and(|n| {
                    matches!(
                        n,
                        b"AbsoluteColorimetric"
                            | b"RelativeColorimetric"
                            | b"Saturation"
                            | b"Perceptual"
                    )
                })
        }
        "d" => {
            a.len() == 2
                && a[1].as_float().is_ok_and(|n| n.is_finite() && n >= 0.)
                && a[0].as_array().is_ok_and(|values| {
                    values
                        .iter()
                        .all(|o| o.as_float().is_ok_and(|n| n.is_finite() && n >= 0.))
                        && (values.is_empty()
                            || values.iter().any(|o| o.as_float().unwrap_or(0.) > 0.))
                })
        }
        "J" | "j" => a.len() == 1 && a[0].as_i64().is_ok_and(|n| (0..=2).contains(&n)),
        "w" | "M" | "i" => a.len() == 1 && a[0].as_float().is_ok_and(|n| n.is_finite() && n >= 0.),
        "G" | "g" | "RG" | "rg" | "K" | "k" => {
            let n = match op.operator.as_str() {
                "RG" | "rg" => 3,
                "K" | "k" => 4,
                _ => 1,
            };
            a.len() == n && a.iter().all(finite)
        }
        _ => false,
    }
}
fn reduce(ops: Vec<Operation>) -> Option<(Vec<Operation>, usize)> {
    let mut state = HashMap::<String, (String, Vec<Object>)>::new();
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut removed = 0;
    for op in ops {
        match op.operator.as_str() {
            "q" if op.operands.is_empty() => stack.push(state.clone()),
            "Q" if op.operands.is_empty() => state = stack.pop()?,
            "w" | "J" | "j" | "M" | "d" | "ri" | "i" | "G" | "RG" | "K" | "g" | "rg" | "k" => {
                if !assignment(&op) {
                    return None;
                }
                let key = match op.operator.as_str() {
                    "G" | "RG" | "K" => "stroke",
                    "g" | "rg" | "k" => "fill",
                    x => x,
                }
                .to_string();
                let next = (op.operator.clone(), op.operands.clone());
                if state.get(&key) == Some(&next) {
                    removed += 1;
                    continue;
                }
                state.insert(key, next);
            }
            "gs" => state.clear(),
            "CS" | "SC" | "SCN" => {
                state.remove("stroke");
            }
            "cs" | "sc" | "scn" => {
                state.remove("fill");
            }
            "cm" | "m" | "l" | "c" | "v" | "y" | "h" | "re" | "S" | "s" | "f" | "F" | "f*"
            | "B" | "B*" | "b" | "b*" | "n" | "W" | "W*" | "BT" | "ET" | "Tc" | "Tw" | "Tz"
            | "TL" | "Tf" | "Tr" | "Ts" | "Td" | "TD" | "Tm" | "T*" | "Tj" | "TJ" | "'" | "\""
            | "Do" | "sh" | "MP" | "DP" | "BMC" | "BDC" | "EMC" => {}
            _ => state.clear(),
        }
        out.push(op);
    }
    stack.is_empty().then_some((out, removed))
}
fn contents(doc: &Document) -> Result<HashSet<ObjectId>> {
    let mut ids = HashSet::new();
    fn add(doc: &Document, o: &Object, ids: &mut HashSet<ObjectId>, depth: usize) {
        if depth > 32 {
            return;
        }
        match o {
            Object::Reference(id) => {
                if let Ok(Object::Stream(_)) = doc.get_object(*id) {
                    ids.insert(*id);
                } else if let Ok(Object::Array(a)) = doc.get_object(*id) {
                    for o in a {
                        add(doc, o, ids, depth + 1);
                    }
                }
            }
            Object::Array(a) => {
                for o in a {
                    add(doc, o, ids, depth + 1);
                }
            }
            _ => {}
        }
    }
    for page in inspect::pages(doc)? {
        if let Ok(o) = doc.get_dictionary(page)?.get(b"Contents") {
            add(doc, o, &mut ids, 0);
        }
    }
    for (&id, o) in &doc.objects {
        if inspect::dictionary(o).is_some_and(|d| inspect::named(d, b"Subtype", b"Form")) {
            ids.insert(id);
        }
    }
    Ok(ids)
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
    let mut remaining = options.limits.max_operations;
    for id in contents(doc)? {
        check_cancel(cancel)?;
        let s = doc.get_object(id)?.as_stream()?;
        let Ok(raw) =
            stream_filters::content(doc, s, options.limits.max_stream_bytes.min(4 * 1024 * 1024))
        else {
            continue;
        };
        if crate::structure::normalize_numbers(&raw).is_none() {
            continue;
        }
        let Ok(content) = Content::decode(&raw) else {
            continue;
        };
        if content.operations.len() > remaining {
            continue;
        }
        remaining -= content.operations.len();
        let Some((operations, removed)) = reduce(content.operations) else {
            continue;
        };
        if removed == 0 {
            continue;
        }
        let data = Content { operations }.encode()?;
        let mut d = s.dict.clone();
        d.set("Filter", "FlateDecode");
        d.remove(b"DecodeParms");
        let candidate = Stream::new(d, crate::lossless::deflate(&data)?);
        if crate::image_encoding::cost(&candidate)? < crate::image_encoding::cost(s)? {
            doc.objects.insert(id, Object::Stream(candidate));
            stats.changed_streams += 1;
            stats.redundant_operations += removed;
        }
    }
    // Inlining changes the active resource scope. A form's empty resources
    // must not accidentally inherit a page's DefaultRGB/Gray/CMYK mapping.
    if crate::native_samples::has_default_spaces(doc, &options.limits, cancel)? {
        return Ok(stats);
    }
    let pages = inspect::pages(doc)?;
    for &page in &pages {
        check_cancel(cancel)?;
        let d = doc.get_dictionary(page)?;
        let Some(content_id) = d.get(b"Contents").ok().and_then(|o| o.as_reference().ok()) else {
            continue;
        };
        if pages
            .iter()
            .filter(|&&p| {
                doc.get_dictionary(p)
                    .ok()
                    .and_then(|d| d.get(b"Contents").ok())
                    .and_then(|o| o.as_reference().ok())
                    == Some(content_id)
            })
            .count()
            != 1
        {
            continue;
        }
        let Ok(s) = doc.get_object(content_id)?.as_stream() else {
            continue;
        };
        if s.dict.has(b"Subtype") {
            continue;
        }
        let Ok(raw) = stream_filters::content(doc, s, 65536) else {
            continue;
        };
        let Ok(content) = Content::decode(&raw) else {
            continue;
        };
        if content.operations.len() != 1
            || content.operations[0].operator != "Do"
            || content.operations[0].operands.len() != 1
        {
            continue;
        }
        let Ok(name) = content.operations[0].operands[0].as_name() else {
            continue;
        };
        let Some(resources) =
            inspect::inherited(doc, page, b"Resources")?.and_then(|r| r.as_dict().ok())
        else {
            continue;
        };
        let Some(objects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok())
        else {
            continue;
        };
        let Some(form) = objects.get(name).ok().and_then(|o| o.as_reference().ok()) else {
            continue;
        };
        let Ok(form_stream) = doc.get_object(form)?.as_stream() else {
            continue;
        };
        let fd = &form_stream.dict;
        if !inspect::named(fd, b"Subtype", b"Form")
            || fd
                .get(b"FormType")
                .is_ok_and(|o| o.as_i64().ok() != Some(1))
            || fd.iter().any(|(k, _)| {
                !matches!(
                    k.as_slice(),
                    b"Type"
                        | b"Subtype"
                        | b"FormType"
                        | b"BBox"
                        | b"Matrix"
                        | b"Length"
                        | b"Filter"
                        | b"DecodeParms"
                        | b"Resources"
                )
            })
        {
            continue;
        }
        if fd.get(b"Resources").is_ok_and(|r| {
            !doc.dereference(r)
                .is_ok_and(|(_, r)| r.as_dict().is_ok_and(|r| r.is_empty()))
        }) {
            continue;
        }
        let Ok(body) = stream_filters::content(doc, form_stream, 65536) else {
            continue;
        };
        let Ok(body) = Content::decode(&body) else {
            continue;
        };
        if body.operations.len() > 128
            || body.operations.iter().any(|o| {
                !matches!(
                    o.operator.as_str(),
                    "q" | "Q"
                        | "cm"
                        | "w"
                        | "J"
                        | "j"
                        | "M"
                        | "d"
                        | "ri"
                        | "i"
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
                        | "G"
                        | "g"
                        | "RG"
                        | "rg"
                        | "K"
                        | "k"
                )
            })
        {
            continue;
        }
        if reduce(body.operations.clone()).is_none() {
            continue;
        }
        let Ok(bbox) = fd.get(b"BBox").and_then(Object::as_array) else {
            continue;
        };
        if bbox.len() != 4 {
            continue;
        }
        let Some(b) = bbox
            .iter()
            .map(|o| o.as_i64().ok())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let (Some(w), Some(h)) = (b[2].checked_sub(b[0]), b[3].checked_sub(b[1])) else {
            continue;
        };
        if w <= 0 || h <= 0 {
            continue;
        }
        let matrix = match fd.get(b"Matrix") {
            Ok(o) => {
                let Ok(a) = o.as_array() else { continue };
                if a.len() != 6
                    || a.iter().any(|o| !finite(o))
                    || (a[0].as_float().unwrap_or(0.) * a[3].as_float().unwrap_or(0.)
                        - a[1].as_float().unwrap_or(0.) * a[2].as_float().unwrap_or(0.))
                    .abs()
                        < 1e-12
                {
                    continue;
                }
                a.clone()
            }
            Err(_) => vec![1.into(), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()],
        };
        let mut ops = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix),
            Operation::new("re", vec![b[0].into(), b[1].into(), w.into(), h.into()]),
            Operation::new("W", vec![]),
            Operation::new("n", vec![]),
        ];
        ops.extend(body.operations);
        ops.push(Operation::new("Q", vec![]));
        let mut stream = s.clone();
        stream.dict.set("Filter", "FlateDecode");
        stream.dict.remove(b"DecodeParms");
        stream.set_content(crate::lossless::deflate(
            &Content { operations: ops }.encode()?,
        )?);
        let mut r = resources.clone();
        let mut x = objects.clone();
        x.remove(name);
        r.set("XObject", x);
        doc.objects.insert(content_id, Object::Stream(stream));
        doc.get_object_mut(page)?.as_dict_mut()?.set("Resources", r);
        stats.inlined_forms += 1;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &[u8]) -> Option<(Vec<Operation>, usize)> {
        reduce(Content::decode(s).unwrap().operations)
    }
    #[test]
    fn repeats_respect_saved_state_extgstate_and_color_space() {
        assert_eq!(run(b"2 w 2 w q 3 w 3 w Q 2 w").unwrap().1, 3);
        assert_eq!(run(b"1 0 0 rg 1 0 0 rg /S gs 1 0 0 rg").unwrap().1, 1);
        assert_eq!(run(b"1 0 0 rg /DeviceRGB cs 1 0 0 rg").unwrap().1, 0);
        assert_eq!(run(b"1 0 0 RG /P SCN 1 0 0 RG").unwrap().1, 0);
        assert_eq!(run(b"2 w 1 0 0 1 0 0 cm 2 w").unwrap().1, 1);
        assert_eq!(run(b"1 0 0 1 0 0 cm 1 0 0 1 0 0 cm").unwrap().1, 0);
        assert_eq!(run(b"2 w Unrecognized 2 w").unwrap().1, 0);
    }
    #[test]
    fn malformed_state_and_unbalanced_saves_are_retained() {
        for s in [
            b"Q".as_slice(),
            b"q 2 w",
            b"3 J 3 J",
            b"1 2 w 1 2 w",
            b"[0 0] 0 d",
            b"/Unknown ri /Unknown ri",
        ] {
            assert!(run(s).is_none());
        }
        assert_eq!(run(b"[] 0 d [] 0 d").unwrap().1, 1);
    }
}
