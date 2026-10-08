//! Bounded complete-file layout candidates; encoded streams never change.
use crate::{
    Limits, Result, WriterCandidate, WriterConfiguration, check_cancel, load_options, validate,
};
use lopdf::{Document, Object, ObjectId, SaveOptions};
use std::{
    collections::{BTreeMap, HashMap},
    io::Write,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

fn walk(
    object: &Object,
    budget: &mut usize,
    counts: &mut HashMap<ObjectId, usize>,
    depth: usize,
    payload: &mut usize,
    cap: usize,
) -> bool {
    if *budget == 0 || depth > 64 || *payload > cap {
        return false;
    }
    *budget -= 1;
    match object {
        Object::Reference(id) => *counts.entry(*id).or_default() += 1,
        Object::Name(n) | Object::String(n, _) => *payload = payload.saturating_add(n.len()),
        Object::Array(a) => {
            return a
                .iter()
                .all(|o| walk(o, budget, counts, depth + 1, payload, cap));
        }
        Object::Dictionary(d) => {
            return d.iter().all(|(k, o)| {
                *payload = payload.saturating_add(k.len());
                walk(o, budget, counts, depth + 1, payload, cap)
            });
        }
        Object::Stream(s) => {
            return s.dict.iter().all(|(k, o)| {
                *payload = payload.saturating_add(k.len());
                walk(o, budget, counts, depth + 1, payload, cap)
            });
        }
        _ => {}
    }
    *payload <= cap
}
fn schema(object: &Object) -> Vec<u8> {
    let (mut key, dict) = match object {
        Object::Dictionary(d) => (vec![b'd'], Some(d)),
        Object::Stream(s) => (vec![b's'], Some(&s.dict)),
        Object::Array(_) => (vec![b'a'], None),
        _ => (vec![b'x'], None),
    };
    if let Some(d) = dict {
        for k in [b"Type".as_slice(), b"Subtype"] {
            if let Ok(Object::Name(n)) = d.get(k) {
                key.extend(n)
            }
            key.push(0);
        }
        let mut names: Vec<_> = d.iter().map(|(k, _)| k).collect();
        names.sort();
        for name in names {
            key.extend(name);
            key.push(0);
        }
    }
    key
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
    doc.objects = std::mem::take(&mut doc.objects)
        .into_iter()
        .map(|(id, mut object)| {
            map_refs(&mut object, mapping);
            (mapping[&id], object)
        })
        .collect::<BTreeMap<_, _>>();
    for (_, object) in doc.trailer.iter_mut() {
        map_refs(object, mapping);
    }
    doc.max_id = doc.objects.keys().map(|(id, _)| *id).max().unwrap_or(0);
}
fn save(doc: &mut Document, file: &mut NamedTempFile, config: &WriterConfiguration) -> Result<u64> {
    let mut writer = std::io::BufWriter::with_capacity(256 * 1024, &mut *file);
    doc.save_with_options(
        &mut writer,
        SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .max_objects_per_stream(config.max_objects_per_stream)
            .compression_level(config.compression_level)
            .build(),
    )?;
    writer.flush()?;
    drop(writer);
    file.flush()?;
    Ok(file.as_file().metadata()?.len())
}

/// Baseline serialization always participates. Optional candidates are cloned
/// one at a time within encoded-byte/object/operation limits; each is read back
/// and checked before it can replace the winning temporary file.
pub(crate) fn best(
    doc: &mut Document,
    mut file: NamedTempFile,
    config: &mut WriterConfiguration,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<NamedTempFile> {
    check_cancel(cancel)?;
    let baseline = save(doc, &mut file, config)?;
    config.layout = "original".into();
    config.candidates = vec![WriterCandidate {
        layout: "original".into(),
        bytes: baseline,
    }];
    let encoded: usize = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .map(|s| s.content.len())
        .sum();
    if encoded > limits.max_stream_bytes.min(64 * 1024 * 1024)
        || doc.objects.len() > limits.max_objects.min(50_000)
    {
        return Ok(file);
    }
    let mut counts = HashMap::new();
    let mut budget = limits.max_operations;
    let mut payload = encoded;
    let cap = limits.max_stream_bytes.min(64 * 1024 * 1024);
    for o in doc.objects.values() {
        check_cancel(cancel)?;
        if !walk(o, &mut budget, &mut counts, 0, &mut payload, cap) {
            return Ok(file);
        }
    }
    for (_, o) in doc.trailer.iter() {
        if !walk(o, &mut budget, &mut counts, 0, &mut payload, cap) {
            return Ok(file);
        }
    }
    let started = Instant::now();
    let mut bytes = baseline;
    let mut winning_mapping = None;
    for layout in ["dense", "frequency"] {
        check_cancel(cancel)?;
        if started.elapsed() > Duration::from_secs(8) {
            break;
        }
        let mut ids: Vec<_> = doc.objects.keys().copied().collect();
        if layout == "frequency" {
            ids.sort_by_cached_key(|id| {
                (
                    std::cmp::Reverse(counts.get(id).copied().unwrap_or(0)),
                    schema(&doc.objects[id]),
                    *id,
                )
            });
        }
        let mapping: HashMap<_, _> = ids
            .into_iter()
            .enumerate()
            .map(|(n, id)| (id, ((n + 1) as u32, 0)))
            .collect();
        if mapping.iter().all(|(a, b)| a == b) {
            continue;
        }
        let mut expected = doc.clone();
        remap(&mut expected, &mapping);
        // Apply the inverse mapping and require exact object/trailer identity.
        // The source document's stream payloads, tags and interactive identities
        // are preserved, even when object numbers or generations are changed.
        let inverse: HashMap<_, _> = mapping.iter().map(|(&a, &b)| (b, a)).collect();
        remap(&mut expected, &inverse);
        if expected.objects != doc.objects || expected.trailer != doc.trailer {
            return Err(crate::Error::Validation("对象重编号改变了文档数据".into()));
        }
        remap(&mut expected, &mapping);
        validate::references(&expected)?;
        check_cancel(cancel)?;
        let mut alternate =
            NamedTempFile::new_in(file.path().parent().expect("temporary file parent"))?;
        let size = save(&mut expected, &mut alternate, config)?;
        let Ok(output) = Document::load_with_options(alternate.path(), load_options(limits)) else {
            continue;
        };
        if validate::round_trip(&expected, &output).is_err() {
            continue;
        }
        drop(output);
        check_cancel(cancel)?;
        config.candidates.push(WriterCandidate {
            layout: layout.into(),
            bytes: size,
        });
        // Tiny layout gains do not justify changing object identities. Every
        // accepted candidate still competes by complete-file bytes.
        if size.saturating_add(64) < bytes {
            bytes = size;
            file = alternate;
            config.layout = layout.into();
            winning_mapping = Some(mapping);
        }
    }
    if let Some(mapping) = winning_mapping {
        remap(doc, &mapping);
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};
    #[test]
    fn sparse_generations_cycles_and_opaque_bytes_survive_the_smallest_layout() {
        let mut doc = Document::with_version("1.7");
        doc.objects.insert(
            (1000, 7),
            Object::Dictionary(
                dictionary! {"Type"=>"Catalog","Pages"=>(2000,7),"Custom"=>(4000,7)},
            ),
        );
        doc.objects.insert(
            (2000, 7),
            Object::Dictionary(
                dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference((3000,7))],"Count"=>1},
            ),
        );
        doc.objects.insert((3000, 7), Object::Dictionary(dictionary! {"Type"=>"Page","Parent"=>(2000,7),"MediaBox"=>vec![0.into(),0.into(),10.into(),10.into()]}));
        let opaque = b"1000 7 R is payload, not a typed reference".to_vec();
        doc.objects.insert(
            (4000, 7),
            Object::Stream(Stream::new(
                dictionary! {"Cycle"=>(4000,7),"Items"=>vec![Object::Reference((1000,7));100]},
                opaque.clone(),
            )),
        );
        doc.max_id = 4000;
        doc.trailer.set("Root", (1000, 7));
        let original = doc.clone();
        let mut config = WriterConfiguration {
            max_objects_per_stream: 100,
            compression_level: 6,
            layout: String::new(),
            candidates: vec![],
        };
        let file = best(
            &mut doc,
            NamedTempFile::new().unwrap(),
            &mut config,
            &Limits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_ne!(config.layout, "original");
        assert!(file.as_file().metadata().unwrap().len() < config.candidates[0].bytes);
        let result = Document::load(file.path()).unwrap();
        validate::round_trip(&doc, &result).unwrap();
        assert!(doc.objects.keys().all(|(_, generation)| *generation == 0));
        let stream = doc
            .objects
            .values()
            .find_map(|o| o.as_stream().ok())
            .unwrap();
        assert_eq!(stream.content, opaque);
        assert!(original.objects.contains_key(&(1000, 7)));
        assert!(matches!(
            best(
                &mut original.clone(),
                NamedTempFile::new().unwrap(),
                &mut config,
                &Limits::default(),
                &AtomicBool::new(true)
            ),
            Err(crate::Error::Cancelled)
        ));
    }
}
