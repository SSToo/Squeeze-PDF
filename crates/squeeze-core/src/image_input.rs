//! Exact, bounded image representation normalization before image strategies.
use crate::{Error, Limits, Result, check_cancel, inspect, lossless, stream_filters, usage};
use lopdf::{Document, Object, Stream};
use serde::Serialize;
use std::{sync::atomic::AtomicBool, time::Instant};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Preparation {
    pub status: String,
    pub considered: usize,
    pub changed: usize,
    pub skipped_protected: usize,
    pub skipped_budget: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported: usize,
    pub elapsed_ms: u64,
}
fn scalar(doc: &Document, o: &Object) -> Result<Object> {
    Ok(doc.dereference(o)?.1.clone())
}

/// A normalized view retains sample/colour semantics and terminal JPEG/fax
/// bytes. None means it is already in the supported canonical representation.
pub(crate) fn normalize(
    doc: &Document,
    s: &Stream,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<Option<Stream>> {
    check_cancel(cancel)?;
    if [b"F".as_slice(), b"FFilter", b"FDecodeParms"]
        .iter()
        .any(|k| s.dict.has(k))
    {
        return Err(Error::Unsupported("外部图片数据".into()));
    }
    let filters = stream_filters::filters(doc, s)?;
    let mut out = s.clone();
    for key in [
        b"Width".as_slice(),
        b"Height",
        b"BitsPerComponent",
        b"ImageMask",
        b"SMaskInData",
    ] {
        if let Ok(o) = s.dict.get(key) {
            out.dict.set(key, scalar(doc, o)?);
        }
    }
    if let Ok(o) = s.dict.get(b"ColorSpace") {
        let mut space = scalar(doc, o)?;
        if let Object::Array(a) = &mut space {
            if let Some(first) = a.first_mut() {
                *first = scalar(doc, first)?;
            }
            if a.first().and_then(|o| o.as_name().ok()) == Some(b"Indexed") && a.len() == 4 {
                a[1] = scalar(doc, &a[1])?;
                a[2] = scalar(doc, &a[2])?;
            }
        }
        out.dict.set("ColorSpace", space);
    }
    if let Some(terminal) = filters.last() {
        if !matches!(
            terminal.name.as_slice(),
            b"DCTDecode" | b"CCITTFaxDecode" | b"FlateDecode" | b"LZWDecode" | b"RunLengthDecode"
        ) {
            return Err(Error::Unsupported("未知图片编码".into()));
        }
        let mut params = terminal
            .params
            .as_ref()
            .map(|o| scalar(doc, o))
            .transpose()?;
        if let Some(Object::Dictionary(d)) = &mut params {
            for (_, value) in d.iter_mut() {
                *value = scalar(doc, value)?;
            }
        }
        if let Some(Object::Dictionary(d)) = &params {
            if matches!(terminal.name.as_slice(), b"FlateDecode" | b"LZWDecode") {
                if d.iter().any(|(k, _)| {
                    !matches!(
                        k.as_slice(),
                        b"Predictor"
                            | b"Colors"
                            | b"Columns"
                            | b"BitsPerComponent"
                            | b"EarlyChange"
                    )
                }) || (terminal.name == b"FlateDecode" && d.has(b"EarlyChange"))
                {
                    return Err(Error::Unsupported("未知图片预测参数".into()));
                }
                for (key, value) in d.iter() {
                    let n = value
                        .as_i64()
                        .map_err(|_| Error::Invalid("图片预测参数类型无效".into()))?;
                    let valid = match key.as_slice() {
                        b"Predictor" => n == 1 || n == 2 || (10..=15).contains(&n),
                        b"Colors" => (1..=32).contains(&n),
                        b"Columns" => n > 0,
                        b"BitsPerComponent" => matches!(n, 1 | 2 | 4 | 8 | 16),
                        b"EarlyChange" => matches!(n, 0 | 1),
                        _ => false,
                    };
                    if !valid {
                        return Err(Error::Invalid("图片预测参数无效".into()));
                    }
                }
            } else if terminal.name == b"RunLengthDecode" && !d.is_empty() {
                return Err(Error::Unsupported("RunLength 图片不接受预测参数".into()));
            }
            if d.is_empty() {
                params = None;
            }
        }
        let mut payload: Option<Vec<u8>> = None;
        for prefix in &filters[..filters.len() - 1] {
            payload = Some(stream_filters::layer(
                doc,
                prefix,
                payload.as_deref().unwrap_or(&s.content),
                cap,
                cancel,
            )?);
        }
        let mut name = terminal.name.clone();
        if matches!(name.as_slice(), b"LZWDecode" | b"RunLengthDecode") {
            let raw = stream_filters::layer_bytes(
                doc,
                terminal,
                payload.as_deref().unwrap_or(&s.content),
                cap,
                cancel,
            )?;
            let encoded = lossless::deflate(&raw)?;
            if lossless::inflate(&encoded, cap)? != raw {
                return Err(Error::Validation("图片编码规范化校验失败".into()));
            }
            payload = Some(encoded);
            if let Some(Object::Dictionary(d)) = &mut params {
                d.remove(b"EarlyChange");
            }
            name = b"FlateDecode".to_vec();
        }
        out.dict.set("Filter", Object::Name(name));
        out.dict.remove(b"DecodeParms");
        if let Some(params) = params {
            out.dict.set("DecodeParms", params);
        }
        if let Some(bytes) = payload {
            out.set_content(bytes);
        }
    }
    Ok((out != *s).then_some(out))
}

pub(crate) fn prepare(
    doc: &mut Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<Preparation> {
    let mut stats = Preparation::default();
    let started = Instant::now();
    let protected = usage::analyze(doc, limits, cancel)?.protected;
    let ids: Vec<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            inspect::dictionary(o)
                .filter(|d| inspect::named(d, b"Subtype", b"Image"))
                .map(|_| id)
        })
        .collect();
    let mut remaining = 16 * 1024 * 1024usize;
    for id in ids {
        check_cancel(cancel)?;
        stats.considered += 1;
        if protected.contains(&id) {
            stats.skipped_protected += 1;
            continue;
        }
        let s = doc.get_object(id)?.as_stream()?;
        let cap = limits
            .max_stream_bytes
            .min(4 * 1024 * 1024)
            .min(remaining / 4);
        if cap == 0 || s.content.len() > cap || started.elapsed().as_secs() >= 4 {
            stats.skipped_budget += 1;
            continue;
        }
        match normalize(doc, s, cap, cancel) {
            Ok(Some(out)) => {
                remaining = remaining.saturating_sub(
                    s.content
                        .len()
                        .saturating_add(out.content.len())
                        .saturating_mul(2),
                );
                doc.objects.insert(id, Object::Stream(out));
                stats.changed += 1;
            }
            Ok(None) => (),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(Error::Limit(_)) => {
                remaining = remaining.saturating_sub(cap * 4);
                stats.skipped_budget += 1;
            }
            Err(Error::Unsupported(_)) => stats.skipped_unsupported += 1,
            Err(_) => {
                remaining = remaining.saturating_sub(cap * 4);
                stats.skipped_invalid += 1;
            }
        }
    }
    stats.status = if stats.changed > 0 {
        "prepared"
    } else {
        "not_applicable"
    }
    .into();
    stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    use weezl::BitOrder;
    fn image() -> Stream {
        Stream::new(
            dictionary! {"Subtype"=>"Image","Width"=>3,"Height"=>2,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceGray"},
            vec![],
        )
    }
    #[test]
    fn indirect_wrapped_jpeg_resolves_properties_and_keeps_payload_and_semantics() {
        let mut doc = Document::new();
        let mut s = image();
        let bits = doc.add_object(8i64);
        let width = doc.add_object(3i64);
        let color = doc.add_object(Object::Name(b"DeviceGray".to_vec()));
        s.dict.set("BitsPerComponent", bits);
        s.dict.set("Width", width);
        s.dict.set("ColorSpace", color);
        let terminal = doc.add_object(Object::Name(b"DCTDecode".to_vec()));
        let parms = doc.add_object(dictionary! {"ColorTransform"=>0});
        let filters = doc.add_object(vec![
            Object::Name(b"ASCIIHexDecode".to_vec()),
            terminal.into(),
        ]);
        s.dict.set("Filter", filters);
        s.dict.set("DecodeParms", vec![Object::Null, parms.into()]);
        s.dict.set("Decode", vec![1.into(), 0.into()]);
        s.set_content(b"FF D8 00 FF D9>".to_vec());
        let out = normalize(&doc, &s, 1024, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        assert_eq!(out.content, [255, 216, 0, 255, 217]);
        assert_eq!(
            out.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"DCTDecode"
        );
        assert_eq!(
            out.dict.get(b"Decode").unwrap(),
            s.dict.get(b"Decode").unwrap()
        );
        assert_eq!(
            out.dict
                .get(b"DecodeParms")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"ColorTransform")
                .unwrap()
                .as_i64()
                .unwrap(),
            0
        );
        assert_eq!(
            out.dict.get(b"BitsPerComponent").unwrap().as_i64().unwrap(),
            8
        );
        assert!(
            normalize(&doc, &out, 1024, &AtomicBool::new(false))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn lzw_png_and_tiff_predictions_keep_exact_original_components() {
        for early in [0, 1] {
            for (predictor, data) in [
                (2, vec![10, 10, 10, 40, 10, 10]),
                (15, vec![1, 10, 10, 10, 2, 30, 30, 30]),
            ] {
                let mut encoder = if early == 1 {
                    weezl::encode::Encoder::with_tiff_size_switch(BitOrder::Msb, 8)
                } else {
                    weezl::encode::Encoder::new(BitOrder::Msb, 8)
                };
                let mut s = image();
                s.dict.set("Filter", "LZWDecode");
                s.dict.set("DecodeParms",dictionary! {"EarlyChange"=>early,"Predictor"=>predictor,"Colors"=>1,"Columns"=>3,"BitsPerComponent"=>8});
                s.set_content(encoder.encode(&data).unwrap());
                let doc = Document::new();
                let out = normalize(&doc, &s, 1024, &AtomicBool::new(false))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    crate::native_samples::read(&doc, &out, &crate::Options::default())
                        .unwrap()
                        .bytes,
                    vec![10, 20, 30, 40, 50, 60]
                );
                assert!(
                    !out.dict
                        .get(b"DecodeParms")
                        .unwrap()
                        .as_dict()
                        .unwrap()
                        .has(b"EarlyChange")
                );
            }
        }
    }
    #[test]
    fn run_length_null_arrays_and_malformed_or_unknown_parameters_are_bounded() {
        let doc = Document::new();
        let mut s = image();
        s.dict
            .set("Filter", vec![Object::Name(b"RunLengthDecode".to_vec())]);
        s.dict.set("DecodeParms", vec![Object::Null]);
        s.set_content(vec![5, 10, 20, 30, 40, 50, 60, 128]);
        let out = normalize(&doc, &s, 1024, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::native_samples::read(&doc, &out, &crate::Options::default())
                .unwrap()
                .bytes,
            vec![10, 20, 30, 40, 50, 60]
        );
        assert!(matches!(
            normalize(&doc, &s, 2, &AtomicBool::new(false)),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            normalize(&doc, &s, 1024, &AtomicBool::new(true)),
            Err(Error::Cancelled)
        ));
        s.set_content(vec![5, 10, 20]);
        assert!(normalize(&doc, &s, 1024, &AtomicBool::new(false)).is_err());
        s.dict.set("Filter", "FlateDecode");
        s.dict
            .set("DecodeParms", dictionary! {"PrivatePredictor"=>1});
        assert!(matches!(
            normalize(&doc, &s, 1024, &AtomicBool::new(false)),
            Err(Error::Unsupported(_))
        ));
    }
}
