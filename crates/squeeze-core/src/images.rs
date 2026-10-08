use crate::{
    ImageResult, Options, Phase, Preset, Progress, Result, check_cancel, inspect, lossless,
    usage::Usage,
};
use image::{
    DynamicImage, GrayImage, ImageDecoder, RgbImage, codecs::jpeg::JpegDecoder,
    imageops::FilterType,
};
use lopdf::{Document, Object, Stream};
use std::{collections::HashSet, io::Cursor, sync::atomic::AtomicBool};

pub(crate) fn optimize(
    doc: &mut Document,
    options: &Options,
    usage: &Usage,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
) -> Result<Vec<ImageResult>> {
    // Mask samples describe opacity, not colour. Keep them lossless even when
    // the associated colour image is recompressed.
    let mut masks = HashSet::new();
    for d in doc.objects.values().filter_map(inspect::dictionary) {
        for key in [b"SMask".as_slice(), b"Mask"] {
            if let Some(id) = d
                .get(key)
                .ok()
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(id, _)| id)
            {
                masks.insert(id);
            }
        }
    }
    let ids: Vec<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            inspect::dictionary(o)
                .filter(|d| inspect::named(d, b"Subtype", b"Image"))
                .map(|_| id)
        })
        .collect();
    let mut results = Vec::new();
    let mut encoder = crate::jpeg::Encoder::default();
    let mut native_elapsed = std::time::Duration::ZERO;
    let mut native_remaining = 16 * 1024 * 1024usize;
    let default_spaces = crate::native_samples::has_default_spaces(doc, &options.limits, cancel)?;
    for (index, id) in ids.iter().enumerate() {
        check_cancel(cancel)?;
        progress(Progress {
            phase: Phase::Images,
            completed: index,
            total: ids.len(),
        });
        let s = doc
            .objects
            .get(id)
            .and_then(|o| o.as_stream().ok())
            .expect("filtered image streams");
        let before = [
            dimension(doc, s, b"Width").unwrap_or(0),
            dimension(doc, s, b"Height").unwrap_or(0),
        ];
        let image_started = std::time::Instant::now();
        let mut report = ImageResult {
            object: format!("{} {}", id.0, id.1),
            before_bytes: s.content.len(),
            after_bytes: s.content.len(),
            before_dimensions: before,
            after_dimensions: before,
            changed: false,
            reason: String::new(),
            encoding: "original".into(),
            candidates: Vec::new(),
            elapsed_ms: 0,
            jpeg_cached: false,
        };
        let mut candidates = crate::image_encoding::Candidates::new(s.clone(), "original", true)?;
        if usage.protected.contains(id) {
            report.reason = "保留特殊调用或交互外观共享的图片".into();
            report.candidates = candidates.entries;
            results.push(report);
            continue;
        }
        if [b"F".as_slice(), b"FFilter", b"FDecodeParms"]
            .iter()
            .any(|key| s.dict.has(key))
        {
            report.reason = "保留带遮罩、特殊映射或替代图像的图片".into();
            report.candidates = candidates.entries;
            results.push(report);
            continue;
        }
        let normalized = (!default_spaces
            && before.iter().all(|&n| n > 0)
            && crate::native_samples::default_decode(doc, s))
        .then(|| {
            let mut normalized = s.clone();
            normalized.dict.remove(b"Decode");
            normalized
        });
        if let Some(stream) = &normalized {
            candidates.consider(stream.clone(), "default-decode", true)?;
        }
        let s = normalized.as_ref().unwrap_or(s);
        let is_mask = masks.contains(id);
        let decoded = if is_mask
            && (s.dict.has(b"Matte")
                || !inspect::named(&s.dict, b"ColorSpace", b"DeviceGray")
                || matches!(s.dict.get(b"Filter"),Ok(Object::Name(n)) if n==b"DCTDecode"))
        {
            Err("透明遮罩保留无损像素".into())
        } else {
            decode(doc, s, options)
        };
        // Embedded ICC blocks prohibit pixel recompression, but remain byte-for-
        // byte intact in coefficient recoding after all preceding checks pass.
        if !is_mask
            && matches!(s.dict.get(b"Filter"),Ok(Object::Name(n)) if n==b"DCTDecode")
            && (decoded.is_ok()
                || decoded
                    .as_ref()
                    .err()
                    .is_some_and(|e| e == "保留带 ICC 配置的 JPEG"))
            && let Ok(outputs) = crate::jpeg_lossless::optimize(
                &s.content,
                before,
                options.limits.max_image_pixels,
                options.limits.max_stream_bytes,
            )
        {
            for (name, bytes) in outputs {
                candidates.consider(Stream::new(s.dict.clone(), bytes), name, true)?;
            }
        }
        // Preserve colour and opacity component bytes, including 16-bit and
        // special spaces. The legacy small CMYK path remains eligible after
        // the optional new-path budget expires.
        let legacy_cmyk = !is_mask && exact_cmyk_eligible(doc, s);
        if (decoded.is_err() || options.preset == Preset::Lossless)
            && (legacy_cmyk || (native_remaining > 0 && native_elapsed.as_secs() < 4))
        {
            let attempt = std::time::Instant::now();
            if let Some(samples) = crate::native_samples::read(doc, s, options)
                && (legacy_cmyk || samples.bytes.len() <= native_remaining)
            {
                native_remaining = native_remaining.saturating_sub(samples.bytes.len());
                crate::image_encoding::add_native(
                    &mut candidates,
                    &s.dict,
                    &samples,
                    before[0],
                    cancel,
                )?;
                if candidates.encoding.starts_with("native-flate/")
                    && !crate::native_samples::read(doc, &candidates.best, options).is_some_and(
                        |round_trip| {
                            round_trip.bytes == samples.bytes
                                && round_trip.bits == samples.bits
                                && round_trip.channels == samples.channels
                        },
                    )
                {
                    return Err(crate::Error::Validation(
                        "原始颜色样本重编码校验失败".into(),
                    ));
                }
            }
            native_elapsed += attempt.elapsed();
        }
        let result = decoded.and_then(|mut image| {
            let mut dimensions = before;
            if !is_mask && !matches!(options.preset, Preset::Fidelity | Preset::Lossless)
                && !usage.unknown.contains(id) && !s.dict.has(b"SMask")
                && let Some(size) = usage.sizes.get(id)
            {
                let wanted = [(size[0]*options.dpi() as f64/72.).ceil(),(size[1]*options.dpi() as f64/72.).ceil()];
                let scale=(wanted[0]/before[0] as f64).max(wanted[1]/before[1] as f64).min(1.);
                if scale.is_finite() && scale>0. && scale<0.95 {
                    dimensions=[((before[0] as f64*scale-1e-9).ceil() as u32).max(1),((before[1] as f64*scale-1e-9).ceil() as u32).max(1)];
                    image=image.resize_exact(dimensions[0],dimensions[1],FilterType::Lanczos3);
                }
            }
            check_cancel(cancel).map_err(|_| "任务已取消".to_string())?;
            let gray=image.color()==image::ColorType::L8;
            let mut base=s.dict.clone();
            if let Some(space) = indexed_base(doc, s) { base.set("ColorSpace", space); }
            base.set("Width",dimensions[0] as i64);base.set("Height",dimensions[1] as i64);base.set("BitsPerComponent",8);
            if !is_mask && options.preset != Preset::Lossless {
                let bytes=encoder.encode(image.as_bytes(),image.width(),image.height(),gray,options.quality())?;
                report.jpeg_cached=encoder.cached;
                let mut dict=base.clone();dict.set("Filter","DCTDecode");dict.remove(b"DecodeParms");
                candidates.consider(Stream::new(dict,bytes),"mozjpeg",false).map_err(|e|e.to_string())?;
            }
            // Packing changes the sample depth/space; masks and ICC-based
            // images retain their original representation. Flate predictors can
            // still improve them without changing any color or opacity sample.
            let packed=!is_mask && matches!(base.get(b"ColorSpace"),Ok(Object::Name(n)) if n==b"DeviceRGB" || n==b"DeviceGray");
            crate::image_encoding::add_exact(&mut candidates,&base,image.as_bytes(),dimensions[0],if gray {1} else {3},packed,false).map_err(|e|e.to_string())?;
            Ok(())
        });
        check_cancel(cancel)?;
        if candidates.encoding != "original" {
            let stream = candidates.best;
            let dims = [
                dimension(doc, &stream, b"Width").unwrap(),
                dimension(doc, &stream, b"Height").unwrap(),
            ];
            report.after_bytes = stream.content.len();
            report.after_dimensions = dims;
            report.changed = true;
            report.reason = if before != dims {
                "降低分辨率并重编码"
            } else if candidates.encoding.starts_with("jpeg-lossless/") {
                "JPEG 无损优化"
            } else if candidates.encoding == "mozjpeg" {
                "图片重编码"
            } else {
                "图片无损重编码"
            }
            .into();
            doc.objects.insert(*id, Object::Stream(stream));
        } else {
            report.reason = result
                .err()
                .unwrap_or_else(|| "候选图片没有更小，保留原始数据".into());
        }
        report.encoding = candidates.encoding;
        report.candidates = candidates.entries;
        report.elapsed_ms = image_started.elapsed().as_millis() as u64;
        results.push(report);
    }
    Ok(results)
}
fn dimension(doc: &Document, s: &Stream, key: &[u8]) -> Option<u32> {
    u32::try_from(
        doc.dereference(s.dict.get(key).ok()?)
            .ok()?
            .1
            .as_i64()
            .ok()?,
    )
    .ok()
    .filter(|n| *n > 0)
}
pub(crate) fn decode(
    doc: &Document,
    s: &Stream,
    options: &Options,
) -> std::result::Result<DynamicImage, String> {
    let width = dimension(doc, s, b"Width").ok_or("无效图片宽度")?;
    let height = dimension(doc, s, b"Height").ok_or("无效图片高度")?;
    let pixels = width as u64 * height as u64;
    if pixels > options.limits.max_image_pixels
        || pixels.saturating_mul(3) > options.limits.max_stream_bytes as u64
    {
        return Err("图片像素或解压大小超过限制".into());
    }
    let bits = s
        .dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .ok()
        .unwrap_or(0);
    let indexed = indexed_base(doc, s).is_some();
    if bits != 8
        && !(bits == 1 && inspect::named(&s.dict, b"ColorSpace", b"DeviceGray"))
        && !(indexed && matches!(bits, 1 | 2 | 4))
    {
        return Err("保留非 8 位或黑白灰度图片".into());
    }
    for key in [b"Mask".as_slice(), b"Decode", b"Alternates", b"OPI"] {
        if s.dict.has(key) {
            return Err("保留带遮罩、特殊映射或替代图像的图片".into());
        }
    }
    if s.dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false)
        || s.dict
            .get(b"SMaskInData")
            .and_then(Object::as_i64)
            .unwrap_or(0)
            != 0
    {
        return Err("保留透明或模板图片".into());
    }
    if let Ok(mask) = s.dict.get(b"SMask") {
        let mask = doc
            .dereference(mask)
            .map_err(|_| "透明遮罩引用无效")?
            .1
            .as_stream()
            .map_err(|_| "透明遮罩不是图片流")?;
        if dimension(doc, mask, b"Width") != Some(width)
            || dimension(doc, mask, b"Height") != Some(height)
            || !inspect::named(&mask.dict, b"Subtype", b"Image")
            || !inspect::named(&mask.dict, b"ColorSpace", b"DeviceGray")
            || mask
                .dict
                .get(b"BitsPerComponent")
                .and_then(Object::as_i64)
                .ok()
                != Some(8)
            || [b"Matte".as_slice(), b"Decode", b"SMask", b"Mask"]
                .iter()
                .any(|k| mask.dict.has(k))
        {
            return Err("保留尚未支持的透明遮罩图片".into());
        }
    }
    let color = doc
        .dereference(s.dict.get(b"ColorSpace").map_err(|_| "缺少颜色空间")?)
        .map_err(|_| "颜色空间引用无效")?
        .1;
    let palette = if indexed {
        let a = color.as_array().map_err(|_| "调色板数据无效")?;
        let count = a[2].as_i64().map_err(|_| "调色板数据无效")?;
        if !(0..=255).contains(&count) {
            return Err("调色板数据无效".into());
        }
        let base = doc.dereference(&a[1]).map_err(|_| "调色板数据无效")?.1;
        let channels = color_channels(doc, base)?;
        let lookup = doc.dereference(&a[3]).map_err(|_| "调色板数据无效")?.1;
        let data = match lookup {
            Object::String(bytes, _) => bytes.clone(),
            Object::Stream(stream) => stream
                .get_plain_content_with_limit(4096)
                .map_err(|_| "调色板数据无效")?,
            _ => return Err("调色板数据无效".into()),
        };
        if data.len() < (count as usize + 1) * channels {
            return Err("调色板数据无效".into());
        }
        Some((data, count as usize, channels))
    } else {
        None
    };
    let channels = if indexed {
        1
    } else {
        color_channels(doc, color)?
    };
    /* Color-space component samples retain their ICC profile. */
    let filters = match s.dict.get(b"Filter") {
        Err(_) => Vec::new(),
        Ok(Object::Name(n)) => vec![n.as_slice()],
        Ok(Object::Array(a)) if a.len() == 1 => vec![a[0].as_name().map_err(|_| "未知过滤器")?],
        _ => return Err("保留多层或未知图片过滤器".into()),
    };
    if filters.as_slice() == [b"CCITTFaxDecode".as_slice()] {
        if bits != 1 || indexed || !inspect::named(&s.dict, b"ColorSpace", b"DeviceGray") {
            return Err("CCITT 图片不完整或参数未支持".into());
        }
        let params = match s.dict.get(b"DecodeParms") {
            Ok(Object::Array(a)) if a.len() == 1 => {
                doc.dereference(&a[0])
                    .map_err(|_| "CCITT 图片不完整或参数未支持")?
                    .1
            }
            Ok(o) => {
                doc.dereference(o)
                    .map_err(|_| "CCITT 图片不完整或参数未支持")?
                    .1
            }
            Err(_) => return Err("CCITT 图片不完整或参数未支持".into()),
        };
        let params = params
            .as_dict()
            .map_err(|_| "CCITT 图片不完整或参数未支持")?;
        let samples = crate::ccitt::decode(&s.content, width, height, params)
            .ok_or("CCITT 图片不完整或参数未支持")?;
        return Ok(DynamicImage::ImageLuma8(
            GrayImage::from_raw(width, height, samples).ok_or("CCITT 图片不完整或参数未支持")?,
        ));
    }
    if filters.as_slice() == [b"DCTDecode".as_slice()] {
        if bits != 8 || indexed {
            return Err("JPEG 位深或颜色空间不一致".into());
        }
        if s.dict.has(b"DecodeParms") {
            return Err("保留带 JPEG 解码参数的图片".into());
        }
        let mut decoder = JpegDecoder::new(Cursor::new(&s.content)).map_err(|e| e.to_string())?;
        if decoder.dimensions() != (width, height)
            || decoder.color_type().channel_count() as usize != channels
            || jpeg_components(&s.content).is_none_or(|n| n as usize != channels)
        {
            return Err("JPEG 颜色或尺寸与 PDF 声明不一致".into());
        }
        if decoder.icc_profile().map_err(|e| e.to_string())?.is_some() {
            return Err("保留带 ICC 配置的 JPEG".into());
        }
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(options.limits.max_stream_bytes as u64);
        decoder.set_limits(limits).map_err(|e| e.to_string())?;
        return DynamicImage::from_decoder(decoder).map_err(|e| e.to_string());
    }
    let raw = if filters.is_empty() {
        if s.dict.has(b"DecodeParms") {
            return Err("无过滤器但有解码参数".into());
        }
        s.content.clone()
    } else if filters.as_slice() == [b"FlateDecode".as_slice()] {
        let encoded = lossless::inflate(&s.content, options.limits.max_stream_bytes)
            .map_err(|e| e.to_string())?;
        unfilter(s, encoded, width, height, channels, bits as usize)?
    } else {
        return Err("保留尚未支持的图片编码".into());
    };
    if let Some((lookup, highest, components)) = palette {
        let stride = (width as usize * bits as usize).div_ceil(8);
        if raw.len() != stride * height as usize {
            return Err("图片数据长度与声明不一致".into());
        }
        let mut samples = Vec::with_capacity(pixels as usize * components);
        for row in raw.chunks_exact(stride) {
            for x in 0..width as usize {
                let offset = x * bits as usize;
                let index = ((row[offset / 8] >> (8 - bits as usize - offset % 8))
                    & ((1u16 << bits) - 1) as u8) as usize;
                if index > highest {
                    return Err("调色板索引超过范围".into());
                }
                samples.extend_from_slice(&lookup[index * components..(index + 1) * components]);
            }
        }
        return if components == 3 {
            Ok(DynamicImage::ImageRgb8(
                RgbImage::from_raw(width, height, samples).ok_or("调色板数据无效")?,
            ))
        } else {
            Ok(DynamicImage::ImageLuma8(
                GrayImage::from_raw(width, height, samples).ok_or("调色板数据无效")?,
            ))
        };
    }
    if bits == 1 {
        let stride = (width as usize).div_ceil(8);
        if raw.len() != stride * height as usize {
            return Err("黑白图片长度不一致".into());
        }
        let mut samples = Vec::with_capacity(pixels as usize);
        for row in raw.chunks_exact(stride) {
            for x in 0..width as usize {
                samples.push(((row[x / 8] >> (7 - x % 8)) & 1) * 255);
            }
        }
        return Ok(DynamicImage::ImageLuma8(
            GrayImage::from_raw(width, height, samples).ok_or("黑白数据无效")?,
        ));
    }
    if raw.len() != width as usize * height as usize * channels {
        return Err("图片数据长度与声明不一致".into());
    }
    if channels == 3 {
        Ok(DynamicImage::ImageRgb8(
            RgbImage::from_raw(width, height, raw).ok_or("RGB 数据无效")?,
        ))
    } else {
        Ok(DynamicImage::ImageLuma8(
            GrayImage::from_raw(width, height, raw).ok_or("灰度数据无效")?,
        ))
    }
}
fn indexed_base(doc: &Document, s: &Stream) -> Option<Object> {
    let a = doc
        .dereference(s.dict.get(b"ColorSpace").ok()?)
        .ok()?
        .1
        .as_array()
        .ok()?;
    (a.len() == 4 && a[0].as_name().ok() == Some(b"Indexed")).then(|| a[1].clone())
}
fn exact_cmyk_eligible(doc: &Document, s: &Stream) -> bool {
    exact_cmyk_space(doc, s).unwrap_or(false)
}
fn exact_cmyk_space(doc: &Document, s: &Stream) -> Option<bool> {
    let color = doc.dereference(s.dict.get(b"ColorSpace").ok()?).ok()?.1;
    let cmyk = match color {
        Object::Name(n) => n == b"DeviceCMYK",
        Object::Array(a) if a.len() == 2 && a[0].as_name().ok() == Some(b"ICCBased") => {
            let profile = doc.dereference(&a[1]).ok()?.1.as_stream().ok()?;
            profile.dict.get(b"N").and_then(Object::as_i64).ok() == Some(4)
                && !profile.dict.has(b"Range")
        }
        _ => false,
    };
    if !cmyk
        || s.dict
            .get(b"BitsPerComponent")
            .and_then(Object::as_i64)
            .ok()
            != Some(8)
        || [
            b"Decode".as_slice(),
            b"Mask",
            b"SMask",
            b"Alternates",
            b"OPI",
            b"F",
            b"SMaskInData",
        ]
        .iter()
        .any(|k| s.dict.has(k))
        || s.dict
            .get(b"ImageMask")
            .is_ok_and(|v| v.as_bool().unwrap_or(true))
    {
        return None;
    }
    Some(true)
}
fn color_channels(doc: &Document, color: &Object) -> std::result::Result<usize, String> {
    Ok(match color {
        Object::Name(n) if n == b"DeviceRGB" => 3usize,
        Object::Name(n) if n == b"DeviceGray" => 1,
        Object::Array(a) if a.len() == 2 && a[0].as_name().ok() == Some(b"ICCBased") => {
            let profile = doc
                .dereference(&a[1])
                .map_err(|_| "ICC 配置引用无效")?
                .1
                .as_stream()
                .map_err(|_| "ICC 配置无效")?;
            let n = profile
                .dict
                .get(b"N")
                .and_then(Object::as_i64)
                .map_err(|_| "ICC 通道数无效")?;
            if !matches!(n, 1 | 3) || profile.dict.has(b"Range") {
                return Err("保留特殊 ICC 通道或范围的图片".into());
            }
            // Recompress component samples while retaining the original PDF
            // colour-space reference and profile; no conversion to DeviceRGB.
            n as usize
        }
        _ => return Err("保留特殊颜色空间图片".into()),
    })
}
fn jpeg_components(bytes: &[u8]) -> Option<u8> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut i = 2;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xff {
            return None;
        }
        while i < bytes.len() && bytes[i] == 0xff {
            i += 1;
        }
        let marker = *bytes.get(i)?;
        i += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = u16::from_be_bytes([*bytes.get(i)?, *bytes.get(i + 1)?]) as usize;
        if length < 2 || i + length > bytes.len() {
            return None;
        }
        if [0xc0, 0xc1, 0xc2].contains(&marker) {
            if length < 8 || bytes[i + 2] != 8 {
                return None;
            }
            return bytes.get(i + 7).copied();
        }
        i += length;
    }
    None
}
fn unfilter(
    s: &Stream,
    mut bytes: Vec<u8>,
    width: u32,
    height: u32,
    channels: usize,
    bits: usize,
) -> std::result::Result<Vec<u8>, String> {
    let params = match s.dict.get(b"DecodeParms") {
        Err(_) | Ok(Object::Null) => None,
        Ok(Object::Dictionary(d)) => Some(d),
        Ok(Object::Array(a)) if a.len() == 1 => match &a[0] {
            Object::Null => None,
            Object::Dictionary(d) => Some(d),
            _ => return Err("解码参数未支持".into()),
        },
        _ => return Err("解码参数未支持".into()),
    };
    let get = |k: &[u8], default: i64| -> std::result::Result<i64, String> {
        match params.and_then(|d| d.get(k).ok()) {
            Some(o) => o.as_i64().map_err(|_| "解码参数未支持".into()),
            None => Ok(default),
        }
    };
    let predictor = get(b"Predictor", 1)?;
    if predictor == 1 {
        return Ok(bytes);
    }
    if get(b"Colors", 1)? != channels as i64
        || get(b"BitsPerComponent", 8)? != bits as i64
        || get(b"Columns", 1)? != width as i64
    {
        return Err("预测器参数与图片不一致".into());
    }
    let row = (width as usize * channels * bits).div_ceil(8);
    let bpp = (channels * bits).div_ceil(8);
    if predictor == 2 {
        if bits != 8 {
            return Err("保留非 8 位 TIFF 预测器".into());
        }
        if bytes.len() != row * height as usize {
            return Err("TIFF 预测器数据长度错误".into());
        }
        for line in bytes.chunks_exact_mut(row) {
            for i in channels..row {
                line[i] = line[i].wrapping_add(line[i - channels]);
            }
        }
        return Ok(bytes);
    }
    if !(10..=15).contains(&predictor) || bytes.len() != (row + 1) * height as usize {
        return Err("PNG 预测器或长度未支持".into());
    }
    let mut raw = vec![0u8; row * height as usize];
    for y in 0..height as usize {
        let tag = bytes[y * (row + 1)];
        if tag > 4 || (predictor != 15 && tag as i64 != predictor - 10) {
            return Err("PNG 行过滤器不一致".into());
        }
        for x in 0..row {
            let left = if x >= bpp { raw[y * row + x - bpp] } else { 0 };
            let up = if y > 0 { raw[(y - 1) * row + x] } else { 0 };
            let corner = if y > 0 && x >= bpp {
                raw[(y - 1) * row + x - bpp]
            } else {
                0
            };
            let value = match tag {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((left as u16 + up as u16) / 2) as u8,
                4 => {
                    let p = left as i32 + up as i32 - corner as i32;
                    let a = (p - left as i32).abs();
                    let b = (p - up as i32).abs();
                    let c = (p - corner as i32).abs();
                    if a <= b && a <= c {
                        left
                    } else if b <= c {
                        up
                    } else {
                        corner
                    }
                }
                _ => unreachable!(),
            };
            raw[y * row + x] = bytes[y * (row + 1) + x + 1].wrapping_add(value);
        }
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    #[test]
    fn known_png_and_tiff_predictor_vectors_decode_exactly() {
        // Independent, hand-calculated 3x2 grayscale samples.
        for (predictor, encoded) in [
            (10, vec![0, 10, 20, 30, 0, 40, 50, 60]),
            (11, vec![1, 10, 10, 10, 1, 40, 10, 10]),
            (12, vec![2, 10, 20, 30, 2, 30, 30, 30]),
            (13, vec![3, 10, 15, 20, 3, 35, 20, 20]),
            (14, vec![4, 10, 10, 10, 4, 30, 10, 10]),
            (15, vec![1, 10, 10, 10, 2, 30, 30, 30]),
            (2, vec![10, 10, 10, 40, 10, 10]),
        ] {
            let s = Stream::new(
                dictionary! {"DecodeParms"=>dictionary! {"Predictor"=>predictor,"Colors"=>1,"Columns"=>3,"BitsPerComponent"=>8}},
                vec![],
            );
            assert_eq!(
                unfilter(&s, encoded, 3, 2, 1, 8).unwrap(),
                vec![10, 20, 30, 40, 50, 60]
            );
        }
    }
}

#[cfg(test)]
mod extended_tests {
    use super::*;
    use lopdf::dictionary;
    #[test]
    fn packed_indexed_rows_lookup_streams_and_icc_bases_decode_exactly() {
        for bits in [1usize, 2, 4, 8] {
            for gray in [false, true] {
                let (w, h) = (13usize, 5usize);
                let count = 1usize << bits;
                let channels = if gray { 1 } else { 3 };
                let palette: Vec<u8> = (0..count)
                    .flat_map(|i| (0..channels).map(move |c| ((i * 31 + c * 13) % 256) as u8))
                    .collect();
                let mut doc = Document::new();
                let lookup = doc.add_object(Stream::new(dictionary! {}, palette.clone()));
                let mut packed = vec![0u8; (w * bits).div_ceil(8) * h];
                let mut expected = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        let i = (x + y) % count;
                        packed[y * (w * bits).div_ceil(8) + x * bits / 8] |=
                            (i as u8) << (8 - bits - x * bits % 8);
                        expected.extend_from_slice(&palette[i * channels..(i + 1) * channels]);
                    }
                }
                let profile = doc.add_object(Stream::new(
                    dictionary! {"N"=>channels as i64},
                    vec![0; 128],
                ));
                let base = Object::Array(vec!["ICCBased".into(), profile.into()]);
                let s = Stream::new(
                    dictionary! {"Width"=>w as i64,"Height"=>h as i64,"BitsPerComponent"=>bits as i64,"ColorSpace"=>vec!["Indexed".into(),base.clone(),(count as i64-1).into(),lookup.into()]},
                    packed,
                );
                assert_eq!(
                    decode(&doc, &s, &Options::default()).unwrap().as_bytes(),
                    expected
                );
                assert_eq!(indexed_base(&doc, &s), Some(base));
                let mut bad = s.clone();
                bad.dict.set("Decode", vec![0.into(), 1.into()]);
                assert!(decode(&doc, &bad, &Options::default()).is_err());
            }
        }
    }
    #[test]
    fn cmyk_native_path_keeps_four_components_and_special_mappings() {
        let mut doc = Document::new();
        let profile = doc.add_object(Stream::new(dictionary! {"N"=>4}, vec![0; 128]));
        for space in [
            Object::Name(b"DeviceCMYK".to_vec()),
            Object::Array(vec!["ICCBased".into(), profile.into()]),
        ] {
            let raw = vec![1, 2, 3, 4, 6, 8, 10, 12, 13, 15, 17, 19, 22, 25, 28, 31];
            let predicted = vec![1, 2, 3, 4, 5, 6, 7, 8, 13, 15, 17, 19, 9, 10, 11, 12];
            let mut s = Stream::new(
                dictionary! {"Width"=>2,"Height"=>2,"BitsPerComponent"=>8,"ColorSpace"=>space,"Filter"=>"FlateDecode","DecodeParms"=>dictionary! {"Predictor"=>2,"Colors"=>4,"Columns"=>2,"BitsPerComponent"=>8}},
                lossless::deflate(&predicted).unwrap(),
            );
            assert_eq!(
                crate::native_samples::read(&doc, &s, &Options::default())
                    .unwrap()
                    .bytes,
                raw
            );
            s.dict.set(
                "Decode",
                vec![
                    1.into(),
                    0.into(),
                    1.into(),
                    0.into(),
                    1.into(),
                    0.into(),
                    1.into(),
                    0.into(),
                ],
            );
            assert!(!exact_cmyk_eligible(&doc, &s));
            assert_eq!(
                crate::native_samples::read(&doc, &s, &Options::default())
                    .unwrap()
                    .bytes,
                raw
            );
        }
    }
}
