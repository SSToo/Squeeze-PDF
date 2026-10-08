use crate::{Limits, Result, check_cancel, inspect};
use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    sync::atomic::AtomicBool,
};

type Linear = [f64; 4];
fn number(o: &Object) -> Option<f64> {
    let n = match o {
        Object::Integer(n) => *n as f64,
        Object::Real(n) => *n as f64,
        _ => return None,
    };
    n.is_finite().then_some(n)
}
fn linear(items: &[Object]) -> Option<Linear> {
    if items.len() != 6 {
        return None;
    }
    // Translations do not amplify coordinate error, but must still be valid.
    let values = items.iter().map(number).collect::<Option<Vec<_>>>()?;
    Some([values[0], values[1], values[2], values[3]])
}
fn multiply(a: Linear, b: Linear) -> Linear {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
    ]
}
fn norm(m: Linear) -> f64 {
    (m[0].abs() + m[2].abs()).max(m[1].abs() + m[3].abs())
}
fn resources<'a>(doc: &'a Document, d: &'a Dictionary) -> Option<&'a Dictionary> {
    doc.dereference(d.get(b"Resources").ok()?)
        .ok()?
        .1
        .as_dict()
        .ok()
}
fn objects<'a>(doc: &'a Document, r: Option<&'a Dictionary>) -> Option<&'a Dictionary> {
    doc.dereference(r?.get(b"XObject").ok()?)
        .ok()?
        .1
        .as_dict()
        .ok()
}
fn block_resources(
    doc: &Document,
    r: Option<&Dictionary>,
    forms: &BTreeSet<ObjectId>,
    blocked: &mut HashSet<ObjectId>,
) {
    if let Some(table) = objects(doc, r) {
        for (_, value) in table.iter() {
            if let Ok((Some(id), _)) = doc.dereference(value)
                && forms.contains(&id)
            {
                blocked.insert(id);
            }
        }
    }
}

// Accumulate a conservative physical-point scale over every known invocation,
// without expanding repeated form instances. Infinity matrix norms are
// submultiplicative, so nested rotations/shears cannot understate the bound.
pub(crate) fn scales(
    doc: &Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<HashMap<ObjectId, f64>> {
    let forms: BTreeSet<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            o.as_stream()
                .ok()
                .filter(|s| inspect::named(&s.dict, b"Subtype", b"Form"))
                .map(|_| id)
        })
        .collect();
    let mut tables = HashSet::new();
    fn table_ids(doc: &Document, o: &Object, tables: &mut HashSet<ObjectId>) {
        if let Some(d) = inspect::dictionary(o) {
            for (key, value) in d.iter() {
                if key == b"XObject"
                    && let Object::Reference(id) = value
                    && doc.get_object(*id).is_ok_and(|o| o.as_dict().is_ok())
                {
                    tables.insert(*id);
                }
                table_ids(doc, value, tables);
            }
        } else if let Object::Array(a) = o {
            for v in a {
                table_ids(doc, v, tables);
            }
        }
    }
    for o in doc.objects.values() {
        table_ids(doc, o, &mut tables);
    }
    fn foreign(
        o: &Object,
        table: bool,
        forms: &BTreeSet<ObjectId>,
        blocked: &mut HashSet<ObjectId>,
    ) {
        match o {
            Object::Reference(id) if !table && forms.contains(id) => {
                blocked.insert(*id);
            }
            Object::Array(a) => {
                for v in a {
                    foreign(v, false, forms, blocked);
                }
            }
            _ => {
                if let Some(d) = inspect::dictionary(o) {
                    for (key, v) in d.iter() {
                        // Only direct entries of an XObject table are calls.
                        // Nested dictionaries cannot hide appearance/mask uses.
                        foreign(
                            v,
                            (table && matches!(v, Object::Reference(_))) || key == b"XObject",
                            forms,
                            blocked,
                        );
                    }
                }
            }
        }
    }
    let mut blocked = HashSet::new();
    for (&id, o) in &doc.objects {
        check_cancel(cancel)?;
        foreign(o, tables.contains(&id), &forms, &mut blocked);
        if let Some(d) = inspect::dictionary(o) {
            if inspect::named(d, b"Subtype", b"Type3") || d.has(b"PatternType") {
                block_resources(doc, resources(doc, d), &forms, &mut blocked);
            }
            if forms.contains(&id) && (!d.has(b"Resources") || d.has(b"Ref") || d.has(b"OPI")) {
                blocked.insert(id);
            }
        }
    }
    let mut budget = limits.max_operations.saturating_mul(16);
    let mut incoming = HashMap::<ObjectId, f64>::new();
    let mut edges = BTreeMap::<ObjectId, HashMap<ObjectId, f64>>::new();
    for page in inspect::pages(doc)? {
        check_cancel(cancel)?;
        let d = doc.get_dictionary(page)?;
        let r = inspect::inherited(doc, page, b"Resources")?.and_then(|o| o.as_dict().ok());
        let unit = match d.get(b"UserUnit") {
            Err(_) => Some(1.),
            Ok(o) => doc
                .dereference(o)
                .ok()
                .and_then(|(_, o)| number(o))
                .filter(|&n| n > 0.),
        };
        let bytes = crate::usage::page_content(doc, d, limits.max_stream_bytes);
        let calls = match (bytes, unit) {
            (Ok(bytes), Some(unit)) => calls(
                doc,
                &bytes,
                r,
                &forms,
                &mut blocked,
                &mut budget,
                limits,
                cancel,
            )?
            .map(|c| (c, unit)),
            _ => None,
        };
        if let Some((calls, unit)) = calls {
            for (id, n) in calls {
                let best = incoming.entry(id).or_default();
                *best = best.max(n * unit);
            }
        } else {
            block_resources(doc, r, &forms, &mut blocked);
        }
    }
    for &id in &forms {
        check_cancel(cancel)?;
        let s = doc.get_object(id)?.as_stream()?;
        let r = resources(doc, &s.dict);
        let found = s
            .get_plain_content_with_limit(limits.max_stream_bytes)
            .ok()
            .map(|raw| {
                calls(
                    doc,
                    &raw,
                    r,
                    &forms,
                    &mut blocked,
                    &mut budget,
                    limits,
                    cancel,
                )
            })
            .transpose()?
            .flatten();
        if let Some(c) = found {
            edges.insert(id, c);
        } else {
            blocked.insert(id);
            block_resources(doc, r, &forms, &mut blocked);
        }
    }
    let mut degrees: BTreeMap<_, usize> = forms.iter().map(|&id| (id, 0)).collect();
    for children in edges.values() {
        for child in children.keys() {
            *degrees.get_mut(child).unwrap() += 1;
        }
    }
    let mut queue: VecDeque<_> = degrees
        .iter()
        .filter_map(|(&id, &n)| (n == 0).then_some(id))
        .collect();
    let mut visited = HashSet::new();
    while let Some(id) = queue.pop_front() {
        check_cancel(cancel)?;
        visited.insert(id);
        if let Some(children) = edges.get(&id) {
            for (&child, &scale) in children {
                if blocked.contains(&id) {
                    blocked.insert(child);
                }
                if let Some(&n) = incoming.get(&id) {
                    let best = incoming.entry(child).or_default();
                    *best = best.max(n * scale);
                }
                let degree = degrees.get_mut(&child).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(child);
                }
            }
        }
    }
    incoming.retain(|id, n| {
        visited.contains(id) && !blocked.contains(id) && n.is_finite() && *n > 0. && *n <= 10_000.
    });
    Ok(incoming)
}

#[allow(clippy::too_many_arguments)]
fn calls(
    doc: &Document,
    bytes: &[u8],
    r: Option<&Dictionary>,
    forms: &BTreeSet<ObjectId>,
    blocked: &mut HashSet<ObjectId>,
    budget: &mut usize,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<Option<HashMap<ObjectId, f64>>> {
    let Ok(content) = Content::decode(bytes) else {
        return Ok(None);
    };
    if content.operations.len() > limits.max_operations || content.operations.len() > *budget {
        return Ok(None);
    }
    *budget -= content.operations.len();
    let mut ctm = [1., 0., 0., 1.];
    let mut stack = Vec::new();
    let mut calls = HashMap::<ObjectId, f64>::new();
    for op in content.operations {
        check_cancel(cancel)?;
        match op.operator.as_str() {
            "q" if op.operands.is_empty() && stack.len() < 64 => stack.push(ctm),
            "Q" if op.operands.is_empty() => {
                let Some(m) = stack.pop() else {
                    return Ok(None);
                };
                ctm = m;
            }
            "cm" => {
                let Some(m) = linear(&op.operands) else {
                    return Ok(None);
                };
                ctm = multiply(ctm, m);
                if !norm(ctm).is_finite() {
                    return Ok(None);
                }
            }
            "Do" => {
                if op.operands.len() != 1 {
                    return Ok(None);
                }
                let target = op.operands[0]
                    .as_name()
                    .ok()
                    .and_then(|n| objects(doc, r)?.get(n).ok())
                    .and_then(|o| doc.dereference(o).ok());
                let Some((Some(id), object)) = target else {
                    return Ok(None);
                };
                if forms.contains(&id) {
                    let d = object.as_stream()?.dict.clone();
                    if !d.has(b"Resources") {
                        block_resources(doc, r, forms, blocked);
                        blocked.insert(id);
                    }
                    let m = match d.get(b"Matrix") {
                        Err(_) => Some([1., 0., 0., 1.]),
                        Ok(o) => doc
                            .dereference(o)
                            .ok()
                            .and_then(|(_, o)| o.as_array().ok())
                            .and_then(|a| linear(a)),
                    };
                    let Some(m) = m else {
                        blocked.insert(id);
                        continue;
                    };
                    let best = calls.entry(id).or_default();
                    *best = best.max(norm(multiply(ctm, m)));
                }
            }
            "q" | "Q" | "BI" | "ID" | "EI" => return Ok(None),
            _ => {}
        }
    }
    Ok(stack.is_empty().then_some(calls))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};
    fn fixture() -> (Document, ObjectId, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let leaf=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Matrix"=>vec![2.into(),0.into(),0.into(),2.into(),0.into(),0.into()],"Resources"=>dictionary!{}},b"12.345678 23.456789 m 80.765432 90.654321 l S".to_vec()));
        let table = doc.add_object(dictionary! {"Leaf"=>leaf});
        let parent=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Resources"=>dictionary!{"XObject"=>table}},b"q 3 1 2 4 0 0 cm /Leaf Do Q".to_vec()));
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"q 10 0 0 10 0 0 cm /Parent Do Q q 500 0 0 500 0 0 cm /Leaf Do Q".to_vec(),
        ));
        let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),600.into(),800.into()],"UserUnit"=>2,"Resources"=>dictionary!{"XObject"=>dictionary!{"Parent"=>parent,"Leaf"=>leaf}},"Contents"=>content});
        doc.objects.insert(
            pages,
            Object::Dictionary(
                dictionary! {"Type"=>"Pages","Count"=>1,"Kids"=>vec![Object::Reference(page)]},
            ),
        );
        let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", catalog);
        (doc, parent, leaf, page)
    }
    #[test]
    fn nested_shear_form_matrix_user_unit_and_largest_shared_call_are_bounded() {
        let (doc, parent, leaf, _) = fixture();
        let result = scales(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(result[&parent], 20.);
        assert_eq!(result[&leaf], 2000.);
        // A submultiplicative bound covers all orientations, including a shear.
        let a = [3., 1., 2., 4.];
        let b = [-2., 1., 3., -4.];
        assert!(norm(multiply(a, b)) <= norm(a) * norm(b));
    }
    #[test]
    fn arrays_share_state_during_analysis_but_are_never_compacted_individually() {
        let (mut doc, parent, leaf, page) = fixture();
        let first = doc.add_object(Stream::new(dictionary! {}, b"q 7 0 0 7 0 0 cm".to_vec()));
        let second = doc.add_object(Stream::new(dictionary! {}, b"/Parent Do Q".to_vec()));
        doc.get_dictionary_mut(page).unwrap().set(
            "Contents",
            vec![Object::Reference(first), Object::Reference(second)],
        );
        let result = scales(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(result[&parent], 14.);
        assert_eq!(result[&leaf], 140.);
        assert!(crate::paths::eligible(&doc).unwrap().is_empty());
    }
    #[test]
    fn unknown_invocations_cycles_and_limits_protect_all_descendants() {
        for kind in [
            "soft-mask",
            "appearance",
            "type3",
            "cycle",
            "malformed",
            "missing-resources",
            "budget",
            "nested-foreign",
        ] {
            let (mut doc, parent, leaf, page) = fixture();
            // Remove the direct leaf call: every invocation now inherits parent.
            let id = doc
                .get_dictionary(page)
                .unwrap()
                .get(b"Contents")
                .unwrap()
                .as_reference()
                .unwrap();
            doc.get_object_mut(id)
                .unwrap()
                .as_stream_mut()
                .unwrap()
                .set_content(b"/Parent Do".to_vec());
            match kind {
                "soft-mask" => {
                    doc.add_object(dictionary! {"SMask"=>dictionary!{"G"=>parent,"S"=>"Alpha"}});
                }
                "appearance" => {
                    doc.add_object(dictionary! {"AP"=>dictionary!{"N"=>parent}});
                }
                "type3" => {
                    doc.add_object(dictionary!{"Subtype"=>"Type3","Resources"=>dictionary!{"XObject"=>dictionary!{"Glyph"=>parent}}});
                }
                "cycle" => {
                    let s = doc.get_object_mut(leaf).unwrap().as_stream_mut().unwrap();
                    s.dict.set(
                        "Resources",
                        dictionary! {"XObject"=>dictionary!{"Parent"=>parent}},
                    );
                    s.set_content(b"/Parent Do".to_vec());
                }
                "malformed" => {
                    doc.get_object_mut(parent)
                        .unwrap()
                        .as_stream_mut()
                        .unwrap()
                        .set_content(b"Q /Leaf Do".to_vec());
                }
                "missing-resources" => {
                    doc.get_object_mut(parent)
                        .unwrap()
                        .as_stream_mut()
                        .unwrap()
                        .dict
                        .remove(b"Resources");
                }
                "nested-foreign" => {
                    doc.add_object(dictionary!{"XObject"=>dictionary!{"Malformed"=>dictionary!{"Appearance"=>parent}}});
                }
                _ => {}
            }
            let limits = Limits {
                max_operations: if kind == "budget" { 0 } else { 500_000 },
                ..Default::default()
            };
            let result = scales(&doc, &limits, &AtomicBool::new(false)).unwrap();
            assert!(
                !result.contains_key(&parent) && !result.contains_key(&leaf),
                "{kind}"
            );
        }
        let (doc, _, _, _) = fixture();
        assert!(matches!(
            scales(&doc, &Limits::default(), &AtomicBool::new(true)),
            Err(crate::Error::Cancelled)
        ));
    }
}
