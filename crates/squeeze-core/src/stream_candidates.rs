//! New exact stream passes compete against the already validated complete PDF.
use crate::{
    Error, Limits, Result, WriterConfiguration, audit, check_cancel, coalesce, inspect,
    load_options, lossless, resource_graph, stream_filters, structure, usage, validate, writing,
};
use lopdf::{Document, Object, Stream};
use serde::Serialize;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, Serialize)]
pub struct StrategyCandidate {
    pub strategy: String,
    pub bytes: u64,
    pub selected: bool,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct StreamOptimization {
    pub status: String,
    pub rejection_reason: Option<String>,
    pub considered_streams: usize,
    pub rewritten_streams: usize,
    pub unwrapped_images: usize,
    pub considered_pages: usize,
    pub merged_pages: usize,
    pub merged_streams: usize,
    pub skipped_unsupported: usize,
    pub skipped_invalid: usize,
    pub skipped_budget: usize,
    pub elapsed_ms: u64,
}
fn cost(s: &Stream) -> Result<usize> {
    crate::image_encoding::cost(s)
}
fn set_filter(stream: &mut Stream, name: &[u8], params: Option<Object>, bytes: Vec<u8>) {
    stream.dict.set("Filter", Object::Name(name.to_vec()));
    stream.dict.remove(b"DecodeParms");
    if let Some(params) = params {
        stream.dict.set("DecodeParms", params);
    }
    stream.set_content(bytes);
}
fn rewrite(
    doc: &Document,
    stream: &Stream,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<(Stream, bool, usize)> {
    let filters = stream_filters::filters(doc, stream)?;
    if filters.is_empty() {
        return Err(Error::Unsupported("未编码流已由原策略处理".into()));
    }
    let image = inspect::named(&stream.dict, b"Subtype", b"Image");
    // Ordinary single Flate was already considered by the existing optimizer.
    if filters.len() == 1
        && filters[0].name == b"FlateDecode"
        && stream.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"FlateDecode")
        && !stream.dict.has(b"DecodeParms")
    {
        return Err(Error::Unsupported("单层 Flate 已由原策略处理".into()));
    }
    if image && filters.len() == 1 && filters[0].name == b"FlateDecode" {
        return Err(Error::Unsupported("原生单层 Flate 图片已处理".into()));
    }
    let mut result = stream.clone();
    let mut decoded = 0usize;
    if image
        && filters.len() > 1
        && filters[..filters.len() - 1]
            .iter()
            .all(|f| matches!(f.name.as_slice(), b"ASCII85Decode" | b"ASCIIHexDecode"))
        && matches!(
            filters.last().unwrap().name.as_slice(),
            b"DCTDecode" | b"CCITTFaxDecode" | b"FlateDecode"
        )
    {
        let mut bytes = stream.content.clone();
        for filter in &filters[..filters.len() - 1] {
            bytes = stream_filters::layer(doc, filter, &bytes, cap, cancel)?;
            decoded = decoded.saturating_add(bytes.len());
        }
        let terminal = filters.last().unwrap();
        // Do not decode/quantize JPEG, fax bits or predictor rows. Keep the
        // terminal codec's complete parameter object and encoded bytes intact.
        set_filter(&mut result, &terminal.name, terminal.params.clone(), bytes);
        return Ok((result, true, decoded));
    }
    let mut bytes = stream.content.clone();
    for filter in &filters {
        bytes = stream_filters::layer(doc, filter, &bytes, cap, cancel)?;
        decoded = decoded.saturating_add(bytes.len());
    }
    let encoded = lossless::deflate(&bytes)?;
    if !lossless::inflate(&encoded, cap).is_ok_and(|raw| raw == bytes) {
        return Err(Error::Validation("流重新编码不等价".into()));
    }
    set_filter(&mut result, b"FlateDecode", None, encoded);
    Ok((result, false, decoded))
}
fn optimize_filters(
    doc: &mut Document,
    limits: &Limits,
    cancel: &AtomicBool,
    stats: &mut StreamOptimization,
) -> Result<usize> {
    let protected = usage::analyze(doc, limits, cancel)?.protected;
    let started = Instant::now();
    let mut remaining = 16 * 1024 * 1024usize;
    let cap = limits.max_stream_bytes.min(4 * 1024 * 1024);
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    let mut changed = 0;
    for id in ids {
        check_cancel(cancel)?;
        let object = doc.get_object(id)?;
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        if inspect::is_container(object)
            || protected.contains(&id)
            || inspect::named(&stream.dict, b"Type", b"Metadata")
            || inspect::named(&stream.dict, b"Type", b"EmbeddedFile")
            || !stream.dict.has(b"Filter")
        {
            continue;
        }
        if stream.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"FlateDecode")
            && !stream.dict.has(b"DecodeParms")
        {
            continue;
        }
        stats.considered_streams += 1;
        // Charge four layers conservatively, including failed attempts. The
        // actual temporary footprint is two bounded byte buffers per layer.
        let local_cap = cap.min(remaining / 4);
        if local_cap == 0
            || stream.content.len() > local_cap
            || started.elapsed() > Duration::from_secs(4)
        {
            stats.skipped_budget += 1;
            continue;
        }
        match rewrite(doc, stream, local_cap, cancel) {
            Ok((replacement, unwrapped, work)) => {
                remaining = remaining.saturating_sub(work.saturating_add(stream.content.len()));
                if cost(&replacement)? < cost(stream)? {
                    doc.objects.insert(id, Object::Stream(replacement));
                    stats.rewritten_streams += 1;
                    stats.unwrapped_images += usize::from(unwrapped);
                    changed += 1;
                }
            }
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(Error::Limit(_)) => {
                remaining = remaining.saturating_sub(local_cap * 4);
                stats.skipped_budget += 1;
            }
            Err(Error::Unsupported(_)) => stats.skipped_unsupported += 1,
            Err(_) => {
                remaining = remaining.saturating_sub(local_cap * 4);
                stats.skipped_invalid += 1;
            }
        }
    }
    Ok(changed)
}

pub(crate) struct Choice {
    pub file: NamedTempFile,
    pub strategies: Vec<StrategyCandidate>,
    pub diagnostics: StreamOptimization,
    pub stages: Vec<crate::StageResult>,
    pub added_streams: usize,
    pub size_after: Option<crate::SizeAudit>,
}
/// The baseline file has already passed round-trip validation. On rejection,
/// reload it instead of attempting to undo deduplication or writer remapping.
/// Images and lossy drawing passes are never executed a second time.
pub(crate) fn choose(
    doc: &mut Document,
    baseline: NamedTempFile,
    writer: &mut WriterConfiguration,
    limits: &Limits,
    cancel: &AtomicBool,
    interactive: bool,
) -> Result<Choice> {
    let before = baseline.as_file().metadata()?.len();
    let started = Instant::now();
    let mut stats = StreamOptimization::default();
    let mut strategies = vec![StrategyCandidate {
        strategy: "current".into(),
        bytes: before,
        selected: true,
    }];
    if doc.objects.len() > limits.max_objects.min(50_000)
        || audit::payload(doc) > limits.max_stream_bytes.min(64 * 1024 * 1024) as u64
    {
        stats.status = "skipped_budget".into();
        stats.skipped_budget = 1;
        return Ok(Choice {
            file: baseline,
            strategies,
            diagnostics: stats,
            stages: Vec::new(),
            added_streams: 0,
            size_after: None,
        });
    }
    let mut stages = Vec::new();
    let attempt =
        (|| -> Result<Option<(NamedTempFile, WriterConfiguration, usize, crate::SizeAudit)>> {
            let stage = audit::Stage::start("filter_chains", doc);
            let rewritten = optimize_filters(doc, limits, cancel, &mut stats)?;
            stages.push(stage.finish(doc));
            let stage = audit::Stage::start("coalesce_contents", doc);
            let merged = coalesce::optimize(doc, limits, cancel)?;
            stats.considered_pages = merged.considered_pages;
            stats.merged_pages = merged.merged_pages;
            stats.merged_streams = merged.merged_streams;
            stats.skipped_unsupported += merged.skipped_unsupported;
            stats.skipped_budget += merged.skipped_budget;
            stages.push(stage.finish(doc));
            if rewritten + merged.merged_pages == 0 {
                return Ok(None);
            }
            let stage = audit::Stage::start("candidate_deduplication", doc);
            doc.objects.retain(|_, o| !inspect::is_container(o));
            let bound = coalesce::identity_bound(doc, limits);
            if !interactive && !bound {
                resource_graph::deduplicate(doc, limits, cancel)?;
            }
            if !bound {
                structure::deduplicate(doc, limits, cancel)?;
            }
            doc.prune_objects();
            stages.push(stage.finish(doc));
            validate::references(doc)?;
            let alternate =
                NamedTempFile::new_in(baseline.path().parent().expect("temporary file parent"))?;
            let mut config = writer.clone();
            let stage = audit::Stage::start("candidate_writing", doc);
            let alternate = writing::best(doc, alternate, &mut config, limits, cancel)?;
            stages.push(stage.finish(doc));
            let output = Document::load_with_options(alternate.path(), load_options(limits))?;
            validate::round_trip(doc, &output)?;
            check_cancel(cancel)?;
            let size_after = audit::size(&output, alternate.as_file().metadata()?.len());
            Ok(Some((
                alternate,
                config,
                rewritten + merged.merged_pages,
                size_after,
            )))
        })();
    let (file, added_streams, size_after) = match attempt {
        Ok(Some((alternate, config, added, size_after))) => {
            let size = alternate.as_file().metadata()?.len();
            let selected = size < before;
            strategies.push(StrategyCandidate {
                strategy: "exact_streams".into(),
                bytes: size,
                selected,
            });
            if selected {
                strategies[0].selected = false;
                *writer = config;
                stats.status = "selected".into();
                (alternate, added, Some(size_after))
            } else {
                *doc = Document::load_with_options(baseline.path(), load_options(limits))?;
                stats.status = "kept_baseline".into();
                (baseline, 0, None)
            }
        }
        Ok(None) => {
            stats.status = "not_applicable".into();
            (baseline, 0, None)
        }
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(error) => {
            stats.rejection_reason = Some(error.to_string());
            *doc = Document::load_with_options(baseline.path(), load_options(limits))?;
            stats.status = "rejected".into();
            stats.skipped_invalid += 1;
            (baseline, 0, None)
        }
    };
    stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(Choice {
        file,
        strategies,
        diagnostics: stats,
        stages,
        added_streams,
        size_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    #[test]
    fn opaque_image_codec_and_final_parameters_survive_unwrapping() {
        let doc = Document::new();
        for terminal in ["DCTDecode", "CCITTFaxDecode", "FlateDecode"] {
            let params = dictionary! {"Columns"=>8,"K"=>-1};
            let input = Stream::new(
                dictionary! {"Subtype"=>"Image","ColorSpace"=>"DeviceGray","Width"=>8,"Height"=>1,"BitsPerComponent"=>1,"Filter"=>vec![Object::Name(b"ASCIIHexDecode".to_vec()),Object::Name(terminal.as_bytes().to_vec())],"DecodeParms"=>vec![Object::Null,Object::Dictionary(params.clone())],"Decode"=>vec![1.into(),0.into()]},
                b"01020304>".to_vec(),
            );
            let (out, unwrapped, _) = rewrite(&doc, &input, 100, &AtomicBool::new(false)).unwrap();
            assert!(unwrapped);
            assert_eq!(out.content, [1, 2, 3, 4]);
            assert_eq!(
                out.dict.get(b"DecodeParms").unwrap().as_dict().unwrap(),
                &params
            );
            for key in [
                b"ColorSpace".as_slice(),
                b"Width",
                b"Height",
                b"BitsPerComponent",
                b"Decode",
            ] {
                assert_eq!(out.dict.get(key).unwrap(), input.dict.get(key).unwrap());
            }
        }
    }
}
