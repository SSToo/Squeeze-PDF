//! Development-only object identity compaction experiment; no product caller.
use lopdf::{Dictionary, Document, Object, ObjectId, SaveOptions};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::PathBuf,
    time::Instant,
};

fn dict_eq(
    a: &Dictionary,
    b: &Dictionary,
    left: &Document,
    right: &Document,
    forward: &mut HashMap<ObjectId, ObjectId>,
    reverse: &mut HashMap<ObjectId, ObjectId>,
    visited: &mut HashSet<(ObjectId, ObjectId)>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            b.get(key)
                .is_ok_and(|other| equivalent(value, other, left, right, forward, reverse, visited))
        })
}
fn equivalent(
    a: &Object,
    b: &Object,
    left: &Document,
    right: &Document,
    forward: &mut HashMap<ObjectId, ObjectId>,
    reverse: &mut HashMap<ObjectId, ObjectId>,
    visited: &mut HashSet<(ObjectId, ObjectId)>,
) -> bool {
    match (a, b) {
        (Object::Reference(x), Object::Reference(y)) => {
            if forward.get(x).is_some_and(|found| found != y)
                || reverse.get(y).is_some_and(|found| found != x)
            {
                return false;
            }
            forward.insert(*x, *y);
            reverse.insert(*y, *x);
            if !visited.insert((*x, *y)) {
                return true;
            }
            match (left.get_object(*x), right.get_object(*y)) {
                (Ok(x), Ok(y)) => equivalent(x, y, left, right, forward, reverse, visited),
                _ => false,
            }
        }
        (Object::Dictionary(x), Object::Dictionary(y)) => {
            dict_eq(x, y, left, right, forward, reverse, visited)
        }
        (Object::Stream(x), Object::Stream(y)) => {
            x.content == y.content
                && dict_eq(&x.dict, &y.dict, left, right, forward, reverse, visited)
        }
        (Object::Array(x), Object::Array(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(x, y)| equivalent(x, y, left, right, forward, reverse, visited))
        }
        _ => a == b,
    }
}
fn graph_eq(left: &Document, right: &Document) -> bool {
    let (mut forward, mut reverse, mut visited) = (HashMap::new(), HashMap::new(), HashSet::new());
    for key in [b"Root".as_slice(), b"Info", b"ID", b"Encrypt"] {
        match (left.trailer.get(key), right.trailer.get(key)) {
            (Err(_), Err(_)) => {}
            (Ok(a), Ok(b))
                if equivalent(a, b, left, right, &mut forward, &mut reverse, &mut visited) => {}
            _ => return false,
        }
    }
    forward.len() == left.objects.len() && reverse.len() == right.objects.len()
}
fn map_refs(object: &mut Object, mapping: &HashMap<ObjectId, ObjectId>) {
    match object {
        Object::Reference(id) => {
            if let Some(new) = mapping.get(id) {
                *id = *new;
            }
        }
        Object::Array(a) => {
            for o in a {
                map_refs(o, mapping);
            }
        }
        Object::Dictionary(d) => {
            for (_, o) in d.iter_mut() {
                map_refs(o, mapping);
            }
        }
        Object::Stream(s) => {
            for (_, o) in s.dict.iter_mut() {
                map_refs(o, mapping);
            }
        }
        _ => {}
    }
}
fn remap(doc: &mut Document, mapping: &HashMap<ObjectId, ObjectId>) {
    let old = std::mem::take(&mut doc.objects);
    doc.objects = old
        .into_iter()
        .map(|(id, mut o)| {
            map_refs(&mut o, mapping);
            (mapping.get(&id).copied().unwrap_or(id), o)
        })
        .collect::<BTreeMap<_, _>>();
    for (_, o) in doc.trailer.iter_mut() {
        map_refs(o, mapping);
    }
    doc.max_id = doc.objects.keys().map(|(id, _)| *id).max().unwrap_or(0);
}
fn schema(object: &Object) -> Vec<u8> {
    let (mut key, dict) = match object {
        Object::Dictionary(d) => (vec![b'd'], Some(d)),
        Object::Stream(s) => (vec![b's'], Some(&s.dict)),
        Object::Array(_) => (vec![b'a'], None),
        _ => (vec![b'x'], None),
    };
    if let Some(dict) = dict {
        for name in [b"Type".as_slice(), b"Subtype"] {
            if let Ok(Object::Name(n)) = dict.get(name) {
                key.extend(n);
            }
            key.push(0);
        }
        let mut keys: Vec<_> = dict.iter().map(|(k, _)| k).collect();
        keys.sort();
        for k in keys {
            key.extend(k);
            key.push(0);
        }
    }
    key
}
fn ref_counts(object: &Object, counts: &mut HashMap<ObjectId, usize>) {
    match object {
        Object::Reference(id) => *counts.entry(*id).or_default() += 1,
        Object::Array(a) => {
            for o in a {
                ref_counts(o, counts);
            }
        }
        Object::Dictionary(d) => {
            for (_, o) in d.iter() {
                ref_counts(o, counts);
            }
        }
        Object::Stream(s) => {
            for (_, o) in s.dict.iter() {
                ref_counts(o, counts);
            }
        }
        _ => {}
    }
}
fn strip(doc: &mut Document) {
    doc.objects.retain(|_, o| {
        !o.as_stream().is_ok_and(|s| {
            matches!(
                s.dict.get(b"Type").and_then(Object::as_name),
                Ok(b"ObjStm" | b"XRef")
            )
        })
    });
    doc.prune_objects();
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let input = PathBuf::from(args.get(1).ok_or("input required")?);
    let directory = PathBuf::from(args.get(2).ok_or("output directory required")?);
    fs::create_dir_all(&directory)?;
    let source_bytes = fs::metadata(&input)?.len();
    let mut base = Document::load(&input)?;
    if base.trailer.has(b"Encrypt")
        || base.objects.values().any(|o| {
            o.as_dict().is_ok_and(|d| d.has(b"ByteRange"))
                || o.as_stream().is_ok_and(|s| s.dict.has(b"ByteRange"))
        })
    {
        return Err("probe skips encrypted/signed documents".into());
    }
    strip(&mut base);
    if args.get(3).is_some_and(|a| a == "--sparse-fixture") {
        let mapping = base
            .objects
            .keys()
            .enumerate()
            .map(|(n, &id)| (id, ((n as u32 + 1) * 200, 7)))
            .collect();
        remap(&mut base, &mapping);
    }
    let group = if base.objects.len() >= 1000 {
        1000
    } else {
        100
    };
    let level = if base.objects.len() >= 1000 { 9 } else { 6 };
    let mut rows = Vec::new();
    for variant in [
        "rewrite",
        "dense",
        "dense-zero-generation",
        "schema",
        "frequency",
    ] {
        let mut doc = base.clone();
        let started = Instant::now();
        if variant == "schema" || variant == "frequency" {
            let mut counts = HashMap::new();
            for o in doc.objects.values() {
                ref_counts(o, &mut counts);
            }
            let mut sorted: Vec<_> = doc
                .objects
                .iter()
                .map(|(&id, o)| {
                    (
                        if variant == "frequency" {
                            std::cmp::Reverse(counts.get(&id).copied().unwrap_or(0))
                        } else {
                            std::cmp::Reverse(0)
                        },
                        schema(o),
                        id,
                    )
                })
                .collect();
            sorted.sort();
            let mapping = sorted
                .into_iter()
                .enumerate()
                .map(|(n, (_, _, id))| (id, (n as u32 + 1, id.1)))
                .collect();
            remap(&mut doc, &mapping);
        } else if variant != "rewrite" {
            doc.renumber_objects();
        }
        if variant == "dense-zero-generation" {
            let mapping = doc.objects.keys().map(|&id| (id, (id.0, 0))).collect();
            remap(&mut doc, &mapping);
        }
        if !graph_eq(&base, &doc) {
            return Err("object/reference/stream semantics changed".into());
        }
        let path = directory.join(format!("{variant}.pdf"));
        let mut file = fs::File::create_new(&path)?;
        doc.save_with_options(
            &mut file,
            SaveOptions::builder()
                .use_object_streams(true)
                .use_xref_streams(true)
                .max_objects_per_stream(group)
                .compression_level(level)
                .build(),
        )?;
        let mut reread = Document::load(&path)?;
        strip(&mut reread);
        if !graph_eq(&base, &reread) {
            return Err("serialized object/reference/stream semantics changed".into());
        }
        rows.push(serde_json::json!({"variant":variant,"output":path,"bytes":file.metadata()?.len(),"elapsed_ms":started.elapsed().as_millis(),"source_bytes":source_bytes,"source_live_objects":base.objects.len(),"source_max_id":base.max_id,"source_nonzero_generations":base.objects.keys().filter(|(_, g)|*g!=0).count(),"semantic_graph_equal":true}));
    }
    fs::write(
        directory.join("results.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}
