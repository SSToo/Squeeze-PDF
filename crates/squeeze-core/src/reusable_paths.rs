use crate::{Limits, Result, check_cancel, form_usage, inspect, lossless, paths};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

const SCALE: i64 = 1_000_000_000;
#[derive(Clone)]
struct Op {
    kind: u8,
    values: Vec<i64>,
}
type Contour = Vec<Op>;
struct Shape {
    data: Vec<u8>,
    origin: [i64; 2],
    bounds: [i64; 2],
}
struct Run {
    start: usize,
    end: usize,
    shapes: Vec<Shape>,
}
struct Cached {
    name: Vec<u8>,
    id: ObjectId,
}
struct Proposed {
    data: Vec<u8>,
    encoded: Vec<u8>,
    name: Vec<u8>,
    bounds: [i64; 2],
}
#[derive(Clone, Copy)]
enum Target {
    Page(ObjectId),
    Form(ObjectId),
}

// Only closed, constant-colour fills are factored. Intersecting control-point
// bounds stay in the same form, preserving holes and nonzero/even-odd winding.
// Pattern fills retain their original coordinate system and are never moved.
pub(crate) fn optimize(
    doc: &mut Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<(usize, usize)> {
    let eligible = paths::eligible(doc)?;
    let pages = inspect::pages(doc)?;
    let mut targets: Vec<_> = pages.iter().copied().map(Target::Page).collect();
    targets.extend(doc.objects.iter().filter_map(|(&id, object)| {
        let stream = object.as_stream().ok()?;
        (inspect::named(&stream.dict, b"Subtype", b"Form")
            && !stream.dict.has(b"Ref")
            && !stream.dict.has(b"OPI")
            && stream.dict.has(b"Resources"))
        .then_some(Target::Form(id))
    }));
    let flat_callers = form_usage::flat_callers(doc, limits, cancel)?;
    let mut names = HashSet::new();
    for &page in &pages {
        if let Some(resources) =
            inspect::inherited(doc, page, b"Resources")?.and_then(|o| o.as_dict().ok())
            && let Ok(objects) = resources.get(b"XObject")
            && let Ok((_, Object::Dictionary(objects))) = doc.dereference(objects)
        {
            names.extend(objects.iter().map(|(n, _)| n.clone()));
        }
    }
    for target in &targets {
        if let Target::Form(id) = target
            && let Ok(resources) = doc.get_object(*id)?.as_stream()?.dict.get(b"Resources")
            && let Ok((_, Object::Dictionary(resources))) = doc.dereference(resources)
            && let Ok(objects) = resources.get(b"XObject")
            && let Ok((_, Object::Dictionary(objects))) = doc.dereference(objects)
        {
            names.extend(objects.iter().map(|(name, _)| name.clone()));
        }
    }
    let mut prefix = b"SqR".to_vec();
    while names.iter().any(|n| n.starts_with(&prefix)) {
        prefix.push(b'_');
    }
    let mut cache: HashMap<Vec<u8>, Cached> = HashMap::new();
    let mut cache_bytes = 0usize;
    let (mut streams, mut uses) = (0, 0);
    // Early streams can be uneconomical before later streams populate the
    // shared contour cache. Revisit only rejected candidates, at most twice;
    // successful forms are never factored again.
    let mut pending_targets = targets;
    for _ in 0..3 {
        let previous_streams = streams;
        let mut retry = Vec::new();
        for target in std::mem::take(&mut pending_targets) {
            check_cancel(cancel)?;
            let (id, resource, initial_flat) = match target {
                Target::Page(page) => {
                    let Some(id) = doc
                        .get_dictionary(page)?
                        .get(b"Contents")
                        .ok()
                        .and_then(|o| o.as_reference().ok())
                    else {
                        continue;
                    };
                    if !eligible.contains_key(&id) {
                        continue;
                    }
                    (id, inspect::inherited(doc, page, b"Resources")?, true)
                }
                Target::Form(id) => {
                    let resource = doc.get_object(id)?.as_stream()?.dict.get(b"Resources")?;
                    (
                        id,
                        Some(doc.dereference(resource)?.1),
                        flat_callers.contains(&id),
                    )
                }
            };
            let stream = doc.get_object(id)?.as_stream()?;
            let original_size = stream.content.len();
            let Ok(raw) = crate::stream_filters::content(doc, stream, limits.max_stream_bytes)
            else {
                continue;
            };
            let mut resources = match resource {
                None => Dictionary::new(),
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let Some(runs) = runs(doc, &resources, &raw, initial_flat, limits, cancel)? else {
                continue;
            };
            if runs.is_empty() {
                continue;
            }
            let mut objects = match resources.get(b"XObject") {
                Err(_) => Dictionary::new(),
                Ok(o) => match doc.dereference(o)?.1 {
                    Object::Dictionary(d) => d.clone(),
                    _ => continue,
                },
            };
            let mut proposals = Vec::<Proposed>::new();
            let mut pending = HashMap::<Vec<u8>, usize>::new();
            let mut used = HashSet::<Vec<u8>>::new();
            let mut data = Vec::with_capacity(raw.len() / 4);
            let (mut last, mut references) = (0, 0);
            let mut candidate_bytes = 0usize;
            for run in runs {
                check_cancel(cancel)?;
                data.extend_from_slice(&raw[last..run.start]);
                for shape in run.shapes {
                    let name = if let Some(found) = cache.get(&shape.data) {
                        found.name.clone()
                    } else if let Some(&i) = pending.get(&shape.data) {
                        proposals[i].name.clone()
                    } else {
                        candidate_bytes = candidate_bytes.saturating_add(shape.data.len());
                        let mut name = prefix.clone();
                        name.extend_from_slice(
                            (cache.len() + proposals.len()).to_string().as_bytes(),
                        );
                        pending.insert(shape.data.clone(), proposals.len());
                        proposals.push(Proposed {
                            encoded: lossless::deflate(&shape.data)?,
                            data: shape.data,
                            name: name.clone(),
                            bounds: shape.bounds,
                        });
                        name
                    };
                    used.insert(name.clone());
                    data.extend_from_slice(b"q 1 0 0 1 ");
                    data.extend_from_slice(decimal(shape.origin[0]).as_bytes());
                    data.push(b' ');
                    data.extend_from_slice(decimal(shape.origin[1]).as_bytes());
                    data.extend_from_slice(b" cm /");
                    data.extend_from_slice(&name);
                    data.extend_from_slice(b" Do Q\n");
                    references += 1;
                }
                last = run.end;
            }
            data.extend_from_slice(&raw[last..]);
            if data.len() > limits.max_stream_bytes
                || cache_bytes.saturating_add(candidate_bytes) > limits.max_stream_bytes
                || doc.objects.len().saturating_add(proposals.len() + 1) > limits.max_objects
            {
                continue;
            }
            let encoded = lossless::deflate(&data)?;
            // Include conservative form/resource overhead; reject a rewrite which
            // only moves bytes from page content into a larger collection of forms.
            let Some(resource_cost) = object_cost(&Object::Dictionary(resources.clone()), 0) else {
                continue;
            };
            let cost = encoded
                .len()
                .saturating_add(resource_cost)
                .saturating_add(used.len() * 64)
                .saturating_add(
                    proposals
                        .iter()
                        .map(|p| p.encoded.len() + 384)
                        .sum::<usize>(),
                )
                .saturating_add(256);
            if cost >= original_size {
                retry.push(target);
                continue;
            }
            for proposal in proposals {
                check_cancel(cancel)?;
                let bounds = proposal.bounds.map(|n| n as f64 / SCALE as f64 + 1.);
                let form = Stream::new(
                    dictionary! {"Type"=>"XObject","Subtype"=>"Form","FormType"=>1,"BBox"=>vec![(-1).into(),(-1).into(),Object::Real(bounds[0] as f32),Object::Real(bounds[1] as f32)],"Resources"=>dictionary!{},"Filter"=>"FlateDecode"},
                    proposal.encoded,
                );
                let id = doc.add_object(form);
                cache_bytes += proposal.data.len();
                cache.insert(
                    proposal.data,
                    Cached {
                        name: proposal.name,
                        id,
                    },
                );
            }
            // HashMap iteration changes between runs. Keep resource insertion
            // order stable so the compressed object streams do not fluctuate.
            let mut shared: Vec<_> = cache.values().filter(|f| used.contains(&f.name)).collect();
            shared.sort_unstable_by_key(|f| f.id);
            for found in shared {
                objects.set(found.name.clone(), found.id);
            }
            resources.set("XObject", objects);
            match target {
                Target::Page(page) => {
                    let content =
                        doc.add_object(Stream::new(dictionary! {"Filter"=>"FlateDecode"}, encoded));
                    let page = doc.get_object_mut(page)?.as_dict_mut()?;
                    page.set("Resources", resources);
                    page.set("Contents", content);
                }
                Target::Form(id) => {
                    // Keep the original form's Matrix, BBox, transparency Group,
                    // optional-content and structure metadata in place. The child
                    // forms only paint paths and inherit the current graphics state.
                    let stream = doc.get_object_mut(id)?.as_stream_mut()?;
                    stream.dict.set("Resources", resources);
                    stream.dict.set("Filter", "FlateDecode");
                    stream.dict.remove(b"DecodeParms");
                    stream.set_content(encoded);
                }
            }
            streams += 1;
            uses += references;
        }
        if streams == previous_streams || retry.is_empty() {
            break;
        }
        pending_targets = retry;
    }
    Ok((streams, uses))
}
fn object_cost(o: &Object, depth: usize) -> Option<usize> {
    if depth > 64 {
        return None;
    }
    Some(match o {
        Object::Name(n) => n.len().checked_mul(3)?.checked_add(2)?,
        Object::String(s, _) => s.len().checked_mul(2)?.checked_add(2)?,
        Object::Array(a) => a
            .iter()
            .try_fold(2usize, |n, o| n.checked_add(object_cost(o, depth + 1)? + 1))?,
        Object::Dictionary(d) => d.iter().try_fold(4usize, |n, (k, v)| {
            n.checked_add(k.len().checked_mul(3)? + 3 + object_cost(v, depth + 1)?)
        })?,
        Object::Stream(_) => return None,
        _ => 32,
    })
}
fn fixed(token: &[u8]) -> Option<i64> {
    let s = std::str::from_utf8(token).ok()?;
    let negative = s.starts_with('-');
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let mut split = s.split('.');
    let integer = split.next()?;
    let fraction = split.next().unwrap_or("");
    if split.next().is_some()
        || integer.is_empty() && fraction.is_empty()
        || fraction.len() > 9
        || !integer
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole = if integer.is_empty() {
        0
    } else {
        integer.parse::<i64>().ok()?
    };
    if whole > 1_000_000 {
        return None;
    }
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .ok()?
            .checked_mul(10i64.pow(9 - fraction.len() as u32))?
    };
    let n = whole.checked_mul(SCALE)?.checked_add(fractional)?;
    Some(if negative { -n } else { n })
}
fn decimal(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let n = n.unsigned_abs();
    let whole = n / SCALE as u64;
    let frac = n % SCALE as u64;
    if frac == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{frac:09}")
            .trim_end_matches('0')
            .into()
    }
}
fn components(contours: Vec<Contour>, paint: &[u8], budget: &mut usize) -> Option<Vec<Shape>> {
    let boxes: Vec<[i64; 4]> = contours
        .iter()
        .map(|c| {
            let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
            for op in c {
                for xy in op.values.chunks_exact(2) {
                    x0 = x0.min(xy[0]);
                    y0 = y0.min(xy[1]);
                    x1 = x1.max(xy[0]);
                    y1 = y1.max(xy[1]);
                }
            }
            [x0, y0, x1, y1]
        })
        .collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..boxes.len()).collect();
    let mut order = parent.clone();
    order.sort_unstable_by_key(|&i| boxes[i][0]);
    let mut active: Vec<usize> = Vec::new();
    for i in order {
        active.retain(|&j| boxes[j][2] >= boxes[i][0]);
        for &j in &active {
            *budget = budget.checked_sub(1)?;
            if boxes[j][1] <= boxes[i][3] && boxes[j][3] >= boxes[i][1] {
                let a = root(&mut parent, i);
                let b = root(&mut parent, j);
                parent[a] = b;
            }
        }
        active.push(i);
    }
    let mut groups = Vec::<Vec<Contour>>::new();
    let mut indices = HashMap::new();
    for (i, contour) in contours.into_iter().enumerate() {
        let key = root(&mut parent, i);
        let at = *indices.entry(key).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[at].push(contour);
    }
    let mut shapes = Vec::new();
    for group in groups {
        let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for contour in &group {
            for op in contour {
                for xy in op.values.chunks_exact(2) {
                    x0 = x0.min(xy[0]);
                    y0 = y0.min(xy[1]);
                    x1 = x1.max(xy[0]);
                    y1 = y1.max(xy[1]);
                }
            }
        }
        // A zero-area closed segment keeps narrow rectangle fills on general
        // scan conversion. Factoring a letter's straight stem out of a compound
        // paragraph must not trigger a renderer's rectangle snapping shortcut.
        // It adds no filled area and stays within the original control bounds.
        let mut data = b"0 0 m 0 0 l h\n".to_vec();
        for contour in group {
            for op in contour {
                for (i, n) in op.values.into_iter().enumerate() {
                    data.extend_from_slice(decimal(n - [x0, y0][i % 2]).as_bytes());
                    data.push(b' ');
                }
                data.push(op.kind);
                data.push(b'\n');
            }
        }
        data.extend_from_slice(paint);
        data.push(b'\n');
        shapes.push(Shape {
            data,
            origin: [x0, y0],
            bounds: [x1 - x0, y1 - y0],
        });
    }
    Some(shapes)
}
pub(crate) fn flat_space(doc: &Document, resources: &Dictionary, name: &[u8]) -> bool {
    if [b"DeviceRGB".as_slice(), b"DeviceGray", b"DeviceCMYK"].contains(&name) {
        return true;
    }
    let Some(spaces) = resources
        .get(b"ColorSpace")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_dict().ok())
    else {
        return false;
    };
    let Some(space) = spaces
        .get(name)
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .map(|(_, o)| o)
    else {
        return false;
    };
    match space {
        Object::Name(n) => {
            [b"DeviceRGB".as_slice(), b"DeviceGray", b"DeviceCMYK"].contains(&n.as_slice())
        }
        Object::Array(a) => a.first().and_then(|o| o.as_name().ok()).is_some_and(|n| {
            [
                b"ICCBased".as_slice(),
                b"CalRGB",
                b"CalGray",
                b"Lab",
                b"Indexed",
                b"Separation",
                b"DeviceN",
            ]
            .contains(&n)
        }),
        _ => false,
    }
}
fn runs(
    doc: &Document,
    resources: &Dictionary,
    raw: &[u8],
    initial_flat: bool,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<Option<Vec<Run>>> {
    fn delimiter(c: u8) -> bool {
        matches!(c, 0 | 9 | 10 | 12 | 13 | 32) || b"()<>[]{}/%".contains(&c)
    }
    let (mut i, mut operations) = (0, 0);
    let mut arguments = Vec::<(usize, Option<i64>)>::new();
    let mut operand_name = None::<Vec<u8>>;
    let mut opaque = false;
    let (mut flat, mut text, mut clip) = (initial_flat, false, false);
    let mut stack = Vec::new();
    let mut start = None;
    let mut blocked = false;
    let mut current = Vec::new();
    let mut contours = Vec::new();
    let mut result = Vec::new();
    let mut budget = limits.max_operations.saturating_mul(16);
    while i < raw.len() {
        if i & 0x3fff == 0 {
            check_cancel(cancel)?;
        }
        let at = i;
        match raw[i] {
            0 | 9 | 10 | 12 | 13 | 32 => {
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
                                return Ok(None);
                            }
                        }
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                if depth != 0 {
                    return Ok(None);
                }
                opaque = true;
                continue;
            }
            b'<' if raw.get(i + 1) != Some(&b'<') => {
                i += 1;
                while i < raw.len() && raw[i] != b'>' {
                    i += 1;
                }
                if i == raw.len() {
                    return Ok(None);
                }
                i += 1;
                opaque = true;
                continue;
            }
            b'/' => {
                i += 1;
                let from = i;
                while i < raw.len() && !delimiter(raw[i]) {
                    i += 1;
                }
                operand_name = Some(raw[from..i].to_vec());
                opaque = true;
                continue;
            }
            c if delimiter(c) => {
                i += 1;
                opaque = true;
                continue;
            }
            _ => {
                while i < raw.len() && !delimiter(raw[i]) {
                    i += 1;
                }
            }
        }
        let token = &raw[at..i];
        if token == b"BI" {
            return Ok(None);
        }
        if matches!(token[0], b'+' | b'-' | b'.' | b'0'..=b'9') {
            if arguments.len() < 7 {
                arguments.push((at, fixed(token)));
            }
            continue;
        }
        operations += 1;
        if operations > limits.max_operations {
            return Ok(None);
        }
        match token {
            b"q" => {
                if stack.len() >= 64 {
                    return Ok(None);
                }
                stack.push(flat);
            }
            b"Q" => {
                let Some(value) = stack.pop() else {
                    return Ok(None);
                };
                flat = value;
            }
            b"cs" => {
                flat = operand_name
                    .as_deref()
                    .is_some_and(|n| flat_space(doc, resources, n));
            }
            b"scn" if operand_name.is_some() => flat = false,
            b"g" | b"rg" | b"k" => flat = true,
            b"BT" => text = true,
            b"ET" => text = false,
            b"W" | b"W*" => clip = true,
            _ => {}
        }
        if [b"m".as_slice(), b"l", b"c", b"h", b"v", b"y", b"re"].contains(&token) {
            start.get_or_insert(arguments.first().map_or(at, |a| a.0));
            let count = match token {
                b"m" | b"l" => 2,
                b"c" => 6,
                _ => 0,
            };
            let values = arguments.iter().map(|a| a.1).collect::<Option<Vec<_>>>();
            if opaque
                || arguments.len() != count
                || values.is_none()
                || [b"v".as_slice(), b"y", b"re"].contains(&token)
            {
                blocked = true;
            }
            if !blocked {
                let op = Op {
                    kind: token[0],
                    values: values.unwrap(),
                };
                if token == b"m" {
                    if !current.is_empty() {
                        blocked = true;
                    } else {
                        current.push(op);
                    }
                } else if token == b"h" {
                    if current.is_empty() {
                        blocked = true;
                    } else {
                        current.push(op);
                        if current.len() > 2 {
                            contours.push(std::mem::take(&mut current));
                        } else {
                            current.clear();
                        }
                    }
                } else if current.is_empty() {
                    blocked = true;
                } else {
                    current.push(op);
                }
            }
        } else if [
            b"f".as_slice(),
            b"f*",
            b"S",
            b"s",
            b"B",
            b"B*",
            b"b",
            b"b*",
            b"n",
        ]
        .contains(&token)
        {
            if let Some(from) = start
                && !blocked
                && flat
                && !clip
                && !text
                && !opaque
                && arguments.is_empty()
                && current.is_empty()
                && !contours.is_empty()
                && [b"f".as_slice(), b"f*"].contains(&token)
                && let Some(shapes) = components(std::mem::take(&mut contours), token, &mut budget)
            {
                result.push(Run {
                    start: from,
                    end: i,
                    shapes,
                });
            }
            start = None;
            clip = false;
            blocked = false;
            current.clear();
            contours.clear();
        } else if start.is_some() {
            blocked = true;
        }
        arguments.clear();
        operand_name = None;
        opaque = false;
    }
    if !stack.is_empty() {
        return Ok(None);
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_translation_keeps_holes_and_excludes_patterns_clips_and_open_paths() {
        assert_eq!(fixed(b"-12.000000001"), Some(-12_000_000_001));
        assert_eq!(decimal(12_000_000_001), "12.000000001");
        assert_eq!(fixed(b"1.1234567891"), None);
        let doc = Document::new();
        let resources = Dictionary::new();
        let limits = Limits::default();
        let cancel = AtomicBool::new(false);
        let raw =
            b"1 1 m 9 1 l 9 9 l 1 9 l h 3 3 m 3 7 l 7 7 l 7 3 l h 20 1 m 28 1 l 28 9 l 20 9 l h f";
        let r = runs(&doc, &resources, raw, true, &limits, &cancel)
            .unwrap()
            .unwrap();
        assert_eq!(r[0].shapes.len(), 2);
        assert_eq!(
            r[0].shapes[0].data.iter().filter(|&&b| b == b'h').count(),
            3
        );
        assert_eq!(r[0].shapes[0].origin, [SCALE, SCALE]);
        for raw in [
            b"/Pattern cs /P1 scn 1 1 m 2 1 l 2 2 l h f".as_slice(),
            b"1 1 m 2 1 l 2 2 l h W f",
            b"1 1 m 2 1 l 2 2 l f",
            b"1 1 m q 2 2 m 3 2 l 3 3 l h f Q",
            b"0 0 10 10 re 20 20 m 30 20 l 30 30 l h f",
            b"W 1 1 m 2 1 l 2 2 l h f",
        ] {
            assert!(
                runs(&doc, &resources, raw, true, &limits, &cancel)
                    .unwrap()
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            runs(&doc, &resources, b"BI ID x EI", true, &limits, &cancel)
                .unwrap()
                .is_none()
        );
    }
}
