use crate::{Limits, Result, check_cancel, inspect, reusable_paths::flat_space};
use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Colour {
    Flat,
    Unknown,
    Inherited(ObjectId),
}

// Prove a form's incoming nonstroking colour space at every known call site.
// A form may be shared by pages, other forms, appearances and soft masks.
// Unknown painting contexts never qualify as an inherited flat colour.
pub(crate) fn flat_callers(
    doc: &Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<HashSet<ObjectId>> {
    let forms: BTreeSet<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            inspect::dictionary(o)
                .filter(|d| inspect::named(d, b"Subtype", b"Form"))
                .map(|_| id)
        })
        .collect();
    let mut tables = HashSet::new();
    fn table_ids(doc: &Document, object: &Object, tables: &mut HashSet<ObjectId>, depth: usize) {
        if depth > 64 {
            return;
        }
        if let Some(d) = inspect::dictionary(object) {
            for (key, value) in d.iter() {
                if key == b"XObject"
                    && let Ok((Some(id), _)) = doc.dereference(value)
                {
                    tables.insert(id);
                }
                table_ids(doc, value, tables, depth + 1);
            }
        } else if let Object::Array(items) = object {
            for item in items {
                table_ids(doc, item, tables, depth + 1);
            }
        }
    }
    for object in doc.objects.values() {
        table_ids(doc, object, &mut tables, 0);
    }
    let mut blocked = HashSet::new();
    fn foreign(
        object: &Object,
        table: bool,
        forms: &BTreeSet<ObjectId>,
        blocked: &mut HashSet<ObjectId>,
        depth: usize,
    ) {
        if depth > 64 {
            blocked.extend(forms);
            return;
        }
        match object {
            Object::Reference(id) if !table && forms.contains(id) => {
                blocked.insert(*id);
            }
            Object::Array(items) => {
                for item in items {
                    foreign(item, false, forms, blocked, depth + 1);
                }
            }
            _ => {
                if let Some(d) = inspect::dictionary(object) {
                    for (key, value) in d.iter() {
                        foreign(value, table || key == b"XObject", forms, blocked, depth + 1);
                    }
                }
            }
        }
    }
    let mut incoming: HashMap<ObjectId, HashSet<Colour>> = HashMap::new();
    let mut budget = limits.max_operations.saturating_mul(16);
    for (&id, object) in &doc.objects {
        check_cancel(cancel)?;
        foreign(object, tables.contains(&id), &forms, &mut blocked, 0);
        if let Some(d) = inspect::dictionary(object)
            && (inspect::named(d, b"Subtype", b"Type3") || d.has(b"PatternType"))
        {
            block_resources(doc, resources(doc, d), &forms, &mut blocked);
        }
    }
    for page in inspect::pages(doc)? {
        check_cancel(cancel)?;
        let resources = inspect::inherited(doc, page, b"Resources")?.and_then(|o| o.as_dict().ok());
        let Ok(content) =
            crate::usage::page_content(doc, doc.get_dictionary(page)?, limits.max_stream_bytes)
        else {
            block_resources(doc, resources, &forms, &mut blocked);
            continue;
        };
        calls(
            doc,
            &content,
            resources,
            Colour::Flat,
            &forms,
            &mut incoming,
            &mut blocked,
            &mut budget,
            limits,
            cancel,
        )?;
    }
    for &id in &forms {
        check_cancel(cancel)?;
        let stream = doc.get_object(id)?.as_stream()?;
        let resources = resources(doc, &stream.dict);
        if let Ok(bytes) = stream.get_plain_content_with_limit(limits.max_stream_bytes) {
            calls(
                doc,
                &bytes,
                resources,
                Colour::Inherited(id),
                &forms,
                &mut incoming,
                &mut blocked,
                &mut budget,
                limits,
                cancel,
            )?;
        } else {
            block_resources(doc, resources, &forms, &mut blocked);
        }
    }
    let mut flat = HashSet::new();
    for _ in 0..64 {
        check_cancel(cancel)?;
        let next: Vec<_> = incoming
            .iter()
            .filter_map(|(&id, callers)| {
                (!blocked.contains(&id)
                    && !flat.contains(&id)
                    && callers.iter().all(|colour| match colour {
                        Colour::Flat => true,
                        Colour::Inherited(parent) => flat.contains(parent),
                        Colour::Unknown => false,
                    }))
                .then_some(id)
            })
            .collect();
        if next.is_empty() {
            break;
        }
        flat.extend(next);
        if flat.len() > limits.max_objects {
            break;
        }
    }
    Ok(flat)
}
fn resources<'a>(doc: &'a Document, d: &'a Dictionary) -> Option<&'a Dictionary> {
    doc.dereference(d.get(b"Resources").ok()?)
        .ok()?
        .1
        .as_dict()
        .ok()
}
fn objects<'a>(doc: &'a Document, resources: Option<&'a Dictionary>) -> Option<&'a Dictionary> {
    doc.dereference(resources?.get(b"XObject").ok()?)
        .ok()?
        .1
        .as_dict()
        .ok()
}
fn block_resources(
    doc: &Document,
    resources: Option<&Dictionary>,
    forms: &BTreeSet<ObjectId>,
    blocked: &mut HashSet<ObjectId>,
) {
    if let Some(objects) = objects(doc, resources) {
        for (_, value) in objects.iter() {
            if let Ok((Some(id), _)) = doc.dereference(value)
                && forms.contains(&id)
            {
                blocked.insert(id);
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn calls(
    doc: &Document,
    bytes: &[u8],
    resources: Option<&Dictionary>,
    mut colour: Colour,
    forms: &BTreeSet<ObjectId>,
    incoming: &mut HashMap<ObjectId, HashSet<Colour>>,
    blocked: &mut HashSet<ObjectId>,
    budget: &mut usize,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<()> {
    if *budget == 0 {
        block_resources(doc, resources, forms, blocked);
        return Ok(());
    }
    let Ok(content) = Content::decode(bytes) else {
        block_resources(doc, resources, forms, blocked);
        return Ok(());
    };
    if content.operations.len() > limits.max_operations || content.operations.len() > *budget {
        block_resources(doc, resources, forms, blocked);
        return Ok(());
    }
    *budget -= content.operations.len();
    let mut stack = Vec::new();
    for op in content.operations {
        check_cancel(cancel)?;
        match op.operator.as_str() {
            "q" => {
                if stack.len() >= 64 {
                    block_resources(doc, resources, forms, blocked);
                    return Ok(());
                }
                stack.push(colour);
            }
            "Q" => colour = stack.pop().unwrap_or(Colour::Unknown),
            "cs" => {
                colour = if op.operands.len() == 1
                    && op.operands[0]
                        .as_name()
                        .ok()
                        .is_some_and(|name| resources.is_some_and(|r| flat_space(doc, r, name)))
                {
                    Colour::Flat
                } else {
                    Colour::Unknown
                }
            }
            "g" | "rg" | "k" => {
                colour = if op.operands.len()
                    == match op.operator.as_str() {
                        "g" => 1,
                        "rg" => 3,
                        _ => 4,
                    }
                    && op
                        .operands
                        .iter()
                        .all(|o| matches!(o, Object::Integer(_) | Object::Real(_)))
                {
                    Colour::Flat
                } else {
                    Colour::Unknown
                }
            }
            "scn" => {
                if op.operands.iter().any(|o| o.as_name().is_ok()) {
                    colour = Colour::Unknown;
                }
            }
            "Do" => {
                let target = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .and_then(|name| objects(doc, resources)?.get(name).ok())
                    .and_then(|o| doc.dereference(o).ok());
                if let Some((Some(id), object)) = target
                    && forms.contains(&id)
                {
                    incoming.entry(id).or_default().insert(colour);
                    // Older forms without resources use their caller's table.
                    // Their own calls cannot be resolved independently here.
                    if inspect::dictionary(object).is_none_or(|d| !d.has(b"Resources")) {
                        block_resources(doc, resources, forms, blocked);
                    }
                } else if target.is_none() {
                    block_resources(doc, resources, forms, blocked);
                }
            }
            "BI" | "ID" | "EI" => {
                block_resources(doc, resources, forms, blocked);
                return Ok(());
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        block_resources(doc, resources, forms, blocked);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};
    fn fixture() -> (Document, ObjectId, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let child=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),20.into(),20.into()],"Resources"=>dictionary!{}},b"0 0 m 10 0 l 10 10 l h f".to_vec()));
        let table = doc.add_object(dictionary! {"Child"=>child});
        let parent=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),20.into(),20.into()],"Resources"=>dictionary!{"XObject"=>table}},b"/Child Do".to_vec()));
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"/DeviceRGB cs /Parent Do".to_vec(),
        ));
        let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Resources"=>dictionary!{"XObject"=>dictionary!{"Parent"=>parent},"Pattern"=>dictionary!{"Unused"=>dictionary!{"PatternType"=>2}}},"Contents"=>content});
        doc.objects.insert(
            pages,
            Object::Dictionary(
                dictionary! {"Type"=>"Pages","Count"=>1,"Kids"=>vec![Object::Reference(page)]},
            ),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        (doc, parent, child, content)
    }
    #[test]
    fn inherited_flat_colour_crosses_nested_forms_and_indirect_tables() {
        let (doc, parent, child, _) = fixture();
        let flat = flat_callers(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(flat.contains(&parent));
        assert!(flat.contains(&child));
    }
    #[test]
    fn one_pattern_call_or_foreign_appearance_prevents_inherited_factoring() {
        let (mut doc, parent, child, content) = fixture();
        doc.get_object_mut(content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(
                b"q /DeviceRGB cs /Parent Do Q /Pattern cs /Tiles scn /Parent Do".to_vec(),
            );
        let flat = flat_callers(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(!flat.contains(&parent));
        assert!(!flat.contains(&child));
        doc.get_object_mut(content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(b"/DeviceRGB cs /Parent Do".to_vec());
        doc.add_object(dictionary! {"Subtype"=>"Widget","AP"=>dictionary!{"N"=>parent}});
        let flat = flat_callers(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(!flat.contains(&parent));
        assert!(!flat.contains(&child));
        // A child which explicitly selects its own colour remains eligible.
        doc.get_object_mut(parent)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(b"0.2 0.4 0.6 rg /Child Do".to_vec());
        let flat = flat_callers(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(flat.contains(&child));
    }
    #[test]
    fn soft_masks_type3_fonts_and_analysis_limits_remain_conservative() {
        let (mut doc, parent, child, _) = fixture();
        doc.add_object(dictionary! {"SMask"=>dictionary!{"G"=>parent,"S"=>"Alpha"}});
        let flat = flat_callers(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(!flat.contains(&parent));
        assert!(!flat.contains(&child));
        let (mut doc, parent, child, _) = fixture();
        doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type3","Resources"=>dictionary!{"XObject"=>dictionary!{"GlyphForm"=>parent}}});
        let flat = flat_callers(&doc, &Limits::default(), &AtomicBool::new(false)).unwrap();
        assert!(!flat.contains(&parent));
        assert!(!flat.contains(&child));
        let (doc, parent, child, _) = fixture();
        let limits = Limits {
            max_operations: 0,
            ..Default::default()
        };
        let flat = flat_callers(&doc, &limits, &AtomicBool::new(false)).unwrap();
        assert!(!flat.contains(&parent));
        assert!(!flat.contains(&child));
        assert!(matches!(
            flat_callers(&doc, &limits, &AtomicBool::new(true)),
            Err(crate::Error::Cancelled)
        ));
    }
}
