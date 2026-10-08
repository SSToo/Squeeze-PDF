//! Development-only PDF 2.0 Brotli compatibility probe. No production flag.
use lopdf::{Document, Object, SaveOptions, Stream, dictionary};
use std::{
    env, fs,
    io::{Read, Write},
    path::Path,
    time::Instant,
};
#[derive(serde::Serialize)]
struct Row {
    object: String,
    raw_bytes: usize,
    flate_bytes: usize,
    brotli_bytes: usize,
    quality: u32,
    elapsed_ms: u128,
    adopted: bool,
}
fn container(o: &Object) -> bool {
    matches!(o.type_name(), Ok(b"ObjStm" | b"XRef"))
        || o.as_dict().is_ok_and(|d| d.has(b"Linearized"))
}
fn save(doc: &mut Document, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let many = doc.objects.len() >= 1000;
    let mut f = std::io::BufWriter::new(fs::File::create(path)?);
    doc.save_with_options(
        &mut f,
        SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .max_objects_per_stream(if many { 1000 } else { 100 })
            .compression_level(if many { 9 } else { 6 })
            .build(),
    )?;
    f.flush()?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = env::args().collect();
    if a.len() != 3 {
        return Err("input.pdf output-directory".into());
    }
    let root = Path::new(&a[2]);
    fs::create_dir_all(root)?;
    squeeze_core::inspect_file(&a[1], &squeeze_core::Limits::default())?;
    let mut doc = Document::load(&a[1])?;
    if doc.is_encrypted() {
        return Err("encrypted document".into());
    }
    doc.objects.retain(|_, o| !container(o));
    doc.prune_objects();
    // Serialize a matched baseline. Re-reading and saving an object-stream PDF
    // changes container IDs, so compare these variants to each other.
    let mut baseline = doc.clone();
    save(&mut baseline, &root.join("baseline.pdf"))?;
    let mut ids: Vec<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            let s = o.as_stream().ok()?;
            (s.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"FlateDecode")
                && !s.dict.has(b"DecodeParms")
                && !s.dict.has(b"F")
                && !matches!(
                    s.dict.get(b"Type").and_then(Object::as_name).ok(),
                    Some(b"EmbeddedFile" | b"Metadata")
                ))
            .then_some((id, s.content.len()))
        })
        .collect();
    ids.sort_by_key(|&(id, n)| (std::cmp::Reverse(n), id));
    let mut budget = 16 * 1024 * 1024usize;
    let mut rows = Vec::new();
    let mut adopted = 0;
    let mut adopted_samples = Vec::new();
    for (id, _) in ids.into_iter().take(64) {
        let s = doc.get_object(id)?.as_stream()?;
        let Ok(raw) = s.get_plain_content_with_limit(4 * 1024 * 1024) else {
            continue;
        };
        if raw.len() > budget {
            continue;
        }
        budget -= raw.len();
        let mut best = None;
        for quality in [6, 9, 11] {
            let started = Instant::now();
            let mut encoded = Vec::new();
            {
                let mut w = brotli::CompressorWriter::new(&mut encoded, 4096, quality, 22);
                w.write_all(&raw)?;
            }
            let mut recovered = Vec::new();
            brotli::Decompressor::new(encoded.as_slice(), 4096).read_to_end(&mut recovered)?;
            if recovered != raw {
                return Err("Brotli sample mismatch".into());
            }
            let smaller = encoded.len() + 16 < s.content.len();
            rows.push(Row {
                object: format!("{} {}", id.0, id.1),
                raw_bytes: raw.len(),
                flate_bytes: s.content.len(),
                brotli_bytes: encoded.len(),
                quality,
                elapsed_ms: started.elapsed().as_millis(),
                adopted: false,
            });
            if smaller
                && best
                    .as_ref()
                    .is_none_or(|(bytes, _): &(Vec<u8>, usize)| encoded.len() < bytes.len())
            {
                best = Some((encoded, rows.len() - 1));
            }
        }
        if let Some((bytes, index)) = best {
            rows[index].adopted = true;
            let mut d = s.dict.clone();
            d.set("Filter", "BrotliDecode");
            doc.objects
                .insert(id, Object::Stream(Stream::new(d, bytes)));
            adopted_samples.push((id, raw));
            adopted += 1;
        }
    }
    // Registered PDF Association extension. Keep the catalog, object streams,
    // xref and all discovery entries encoded with ordinary PDF filters.
    doc.version = "2.0".into();
    doc.catalog_mut()?
        .set("Version", Object::Name(b"2.0".to_vec()));
    let mut extensions = match doc.catalog()?.get(b"Extensions") {
        Ok(o) => doc.dereference(o)?.1.as_dict()?.clone(),
        Err(lopdf::Error::DictKey(_)) => lopdf::Dictionary::new(),
        Err(e) => return Err(e.into()),
    };
    if extensions.has(b"PDFa") {
        return Err("existing PDFa extension needs explicit merge".into());
    }
    extensions.set("PDFa",dictionary!{"BaseVersion"=>Object::Name(b"2.0".to_vec()),"ExtensionLevel"=>1,"ExtensionRevision"=>Object::string_literal("2026"),"URL"=>Object::string_literal("https://pdfa.org/resource/extension-brotli")});
    doc.catalog_mut()?.set("Extensions", extensions);
    save(&mut doc, &root.join("brotli.pdf"))?;
    let decoded = Document::load(root.join("brotli.pdf"))?;
    for (id, expected) in &adopted_samples {
        let actual = decoded
            .get_object(*id)?
            .as_stream()?
            .get_plain_content_with_limit(4 * 1024 * 1024)?;
        if &actual != expected {
            return Err("adopted PDF stream samples differ".into());
        }
    }
    for (&id, o) in &doc.objects {
        if let Ok(s) = o.as_stream() {
            let expected = s.get_plain_content_with_limit(64 * 1024 * 1024);
            let actual = decoded
                .get_object(id)?
                .as_stream()?
                .get_plain_content_with_limit(64 * 1024 * 1024);
            if let (Ok(x), Ok(y)) = (expected, actual)
                && x != y
            {
                return Err("PDF round-trip samples differ".into());
            }
        }
    }
    let pages = doc.get_pages();
    if pages != decoded.get_pages() {
        return Err("page identity changed".into());
    }
    let report = serde_json::json!({"input":a[1],"baseline_bytes":fs::metadata(root.join("baseline.pdf"))?.len(),"brotli_bytes":fs::metadata(root.join("brotli.pdf"))?.len(),"adopted_streams":adopted,"decoded_streams_verified":adopted_samples.len(),"pages":pages.len(),"rows":rows,"limits":{"streams":64,"per_stream":4*1024*1024,"total":16*1024*1024,"window":22},"development_only":true});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", report);
    Ok(())
}
