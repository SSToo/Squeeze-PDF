use crate::{Limits, Preset, Result, check_cancel, inspect, lossless};
use lopdf::{Document, Object, ObjectId};
use std::{collections::HashMap, sync::atomic::AtomicBool};

// Standalone pages and forms whose complete invocation scale is proven are
// changed. Arrays carry state across streams and remain analysis-only.
pub(crate) fn optimize(
    doc: &mut Document,
    preset: Preset,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<usize> {
    if preset == Preset::Lossless {
        return Ok(0);
    }
    let mut ids = eligible(doc)?;
    ids.extend(crate::form_precision::scales(doc, limits, cancel)?);
    let mut changed = 0;
    for (id, unit) in ids {
        check_cancel(cancel)?;
        let stream = doc.get_object(id)?.as_stream()?;
        let Ok(raw) = stream.get_plain_content_with_limit(limits.max_stream_bytes) else {
            continue;
        };
        let Some(data) = compact(&raw, unit, limits.max_operations) else {
            continue;
        };
        let compressed = lossless::deflate(&data)?;
        if compressed.len().saturating_add(32) >= stream.content.len() {
            continue;
        }
        let stream = doc.get_object_mut(id)?.as_stream_mut()?;
        stream.dict.set("Filter", "FlateDecode");
        stream.dict.remove(b"DecodeParms");
        stream.set_content(compressed);
        changed += 1;
    }
    Ok(changed)
}
pub(crate) fn eligible(doc: &Document) -> Result<HashMap<ObjectId, f64>> {
    let mut ids: HashMap<ObjectId, f64> = HashMap::new();
    for id in inspect::pages(doc)? {
        let page = doc.get_dictionary(id)?;
        let Some(content) = page
            .get(b"Contents")
            .ok()
            .and_then(|o| o.as_reference().ok())
        else {
            continue;
        };
        let Ok(stream) = doc.get_object(content).and_then(Object::as_stream) else {
            continue;
        };
        if stream.dict.has(b"Subtype") || stream.dict.has(b"Type") {
            continue;
        }
        let unit = match page.get(b"UserUnit") {
            Err(_) => 1.,
            Ok(o) => match doc.dereference(o).ok().and_then(|(_, o)| num(o)) {
                Some(unit) => unit,
                None => continue,
            },
        };
        if unit.is_finite() && unit > 0. {
            let max = ids.entry(content).or_insert(unit);
            *max = max.max(unit);
        }
    }
    // A typeless stream can also be referenced as a glyph, form or resource.
    // Exclude such shared streams, whose other coordinate systems are unknown.
    let mut excluded = Vec::new();
    for root in doc.objects.values() {
        let is_page = root.as_dict().ok().is_some_and(|d| {
            d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Page".as_slice())
        });
        let mut pending = vec![(root, true)];
        while let Some((object, at_root)) = pending.pop() {
            match object {
                Object::Reference(id) if ids.contains_key(id) => excluded.push(*id),
                Object::Array(a) => pending.extend(a.iter().map(|v| (v, false))),
                Object::Dictionary(d) => {
                    for (key, value) in d.iter() {
                        if at_root && is_page && key == b"Contents" && value.as_reference().is_ok()
                        {
                            continue;
                        }
                        pending.push((value, false));
                    }
                }
                Object::Stream(s) => pending.extend(s.dict.iter().map(|(_, v)| (v, false))),
                _ => {}
            }
        }
    }
    for id in excluded {
        ids.remove(&id);
    }
    Ok(ids)
}
fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(n) => Some(*n as f64),
        Object::Real(n) => Some(*n as f64),
        _ => None,
    }
}
fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
    ]
}
fn compact(raw: &[u8], unit: f64, max_operations: usize) -> Option<Vec<u8>> {
    fn space(c: u8) -> bool {
        matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
    }
    fn delimiter(c: u8) -> bool {
        space(c) || b"()<>[]{}/%".contains(&c)
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut numbers: Vec<(usize, usize, f64)> = Vec::new();
    let mut ctm = [unit, 0., 0., unit];
    let mut stack = Vec::new();
    let mut operations = 0;
    let mut i = 0;
    while i < raw.len() {
        let start = i;
        match raw[i] {
            c if space(c) => {
                i += 1;
                continue;
            }
            b'%' => {
                while i < raw.len() && !matches!(raw[i], 10 | 13) {
                    i += 1;
                }
                continue;
            }
            b'(' => {
                let mut depth = 1;
                i += 1;
                while i < raw.len() && depth > 0 {
                    match raw[i] {
                        b'\\' => {
                            i += 1;
                            if i >= raw.len() {
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
                numbers.clear();
                continue;
            }
            b'<' if raw.get(i + 1) != Some(&b'<') => {
                i += 1;
                while i < raw.len() && raw[i] != b'>' {
                    i += 1;
                }
                if i == raw.len() {
                    return None;
                }
                i += 1;
                numbers.clear();
                continue;
            }
            b'/' => {
                i += 1;
                while i < raw.len() && !delimiter(raw[i]) {
                    i += 1;
                }
                numbers.clear();
                continue;
            }
            c if delimiter(c) => {
                i += 1;
                numbers.clear();
                continue;
            }
            _ => {
                while i < raw.len() && !delimiter(raw[i]) {
                    i += 1;
                }
            }
        }
        let token = &raw[start..i];
        if token == b"BI" {
            return None;
        }
        if let Ok(value) = std::str::from_utf8(token).ok()?.parse::<f64>() {
            if !value.is_finite() {
                return None;
            }
            if numbers.len() < 7 {
                numbers.push((start, i, value));
            }
            continue;
        }
        operations += 1;
        if operations > max_operations {
            return None;
        }
        match token {
            b"q" => {
                if stack.len() >= 64 {
                    return None;
                }
                stack.push(ctm);
            }
            b"Q" => {
                ctm = stack.pop()?;
            }
            b"cm" => {
                if numbers.len() != 6 {
                    return None;
                }
                ctm = multiply(
                    ctm,
                    [numbers[0].2, numbers[1].2, numbers[2].2, numbers[3].2],
                );
                if !ctm.iter().all(|v| v.is_finite()) {
                    return None;
                }
            }
            b"m" | b"l" | b"c" | b"v" | b"y" | b"re" => {
                let count = match token {
                    b"m" | b"l" => 2,
                    b"c" => 6,
                    _ => 4,
                };
                if numbers.len() != count {
                    return None;
                }
                // Bound rounding after every active page transform. Maximum
                // device-coordinate error is 0.0005 point on either axis.
                let mut norm = (ctm[0].abs() + ctm[2].abs()).max(ctm[1].abs() + ctm[3].abs());
                // Rectangle corners include x+w / y+h, so their accumulated
                // rounding error needs twice the coordinate precision.
                if token == b"re" {
                    norm *= 2.;
                }
                if norm > 0. && norm <= 10_000. {
                    let precision = (norm / 0.001).log10().ceil().max(0.) as usize;
                    if precision <= 9 {
                        for &(start, end, value) in &numbers {
                            if value.abs() > 1_000_000. {
                                continue;
                            }
                            let mut text = format!("{value:.precision$}");
                            if precision > 0 {
                                text = text.trim_end_matches('0').trim_end_matches('.').into();
                            }
                            if text == "-0" {
                                text = "0".into();
                            }
                            let rounded: f64 = text.parse().ok()?;
                            if (rounded - value).abs() * norm <= 0.00050001
                                && text.len() < end - start
                            {
                                edits.push((start, end, text));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        numbers.clear();
    }
    if edits.is_empty() || !stack.is_empty() {
        return None;
    }
    let mut result = Vec::with_capacity(raw.len());
    let mut last = 0;
    for (start, end, text) in edits {
        result.extend_from_slice(&raw[last..start]);
        result.extend_from_slice(text.as_bytes());
        last = end;
    }
    result.extend_from_slice(&raw[last..]);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::compact;
    #[test]
    fn rounding_accounts_for_scale_and_preserves_text_and_colours() {
        let raw=b"q 1000 0 0 1000 0 0 cm .123456789 .987654321 m Q q 1 0 0 1 0 0 cm 12.345678 23.456789 l Q .123456789 0 0 rg BT 12.345678 0 Td (12.345678 m) Tj ET";
        let out = compact(raw, 1., 100).unwrap();
        assert_eq!(out,b"q 1000 0 0 1000 0 0 cm 0.123457 0.987654 m Q q 1 0 0 1 0 0 cm 12.346 23.457 l Q .123456789 0 0 rg BT 12.345678 0 Td (12.345678 m) Tj ET");
        assert!(compact(b"q BI /W 1 ID x EI Q", 1., 100).is_none());
        assert!(compact(raw, 1., 1).is_none());
        assert!(compact(b"q 1.23456 2.34567 m", 1., 100).is_none());
    }
}
