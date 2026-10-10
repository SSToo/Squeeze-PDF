//! Paired, premultiplied resampling from the original colour/opacity samples.
use crate::{Options, Preset, Result, check_cancel, inspect, native_samples, usage::Usage};
use image::{Rgba32FImage, imageops::FilterType};
use lopdf::{Document, Object, ObjectId, Stream};
use serde::Serialize;
use std::{collections::HashSet, sync::atomic::AtomicBool};
#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub considered: usize,
    pub changed: usize,
    pub skipped_unsupported: usize,
}
pub(crate) fn optimize(
    doc: &mut Document,
    options: &Options,
    usage: &Usage,
    cancel: &AtomicBool,
) -> Result<(Stats, HashSet<ObjectId>)> {
    let mut stats = Stats::default();
    let mut protected = HashSet::new();
    if !matches!(options.preset, Preset::Balanced | Preset::Small)
        || native_samples::has_default_spaces(doc, &options.limits, cancel)?
    {
        return Ok((stats, protected));
    }
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    let mut remaining = 16 * 1024 * 1024usize;
    for id in ids {
        check_cancel(cancel)?;
        let Ok(s) = doc.get_object(id)?.as_stream() else {
            continue;
        };
        if !inspect::named(&s.dict, b"Subtype", b"Image")
            || usage.protected.contains(&id)
            || usage.unknown.contains(&id)
        {
            continue;
        }
        let Some(mask_id) = s
            .dict
            .get(b"SMask")
            .ok()
            .and_then(|o| o.as_reference().ok())
        else {
            continue;
        };
        stats.considered += 1;
        let Ok(mask) = doc.get_object(mask_id)?.as_stream() else {
            stats.skipped_unsupported += 1;
            continue;
        };
        if [
            b"Decode".as_slice(),
            b"Mask",
            b"ImageMask",
            b"Alternates",
            b"OPI",
            b"SMaskInData",
        ]
        .iter()
        .any(|k| s.dict.has(k))
            || [
                b"Decode".as_slice(),
                b"Mask",
                b"SMask",
                b"ImageMask",
                b"SMaskInData",
            ]
            .iter()
            .any(|k| mask.dict.has(k))
            || usage.protected.contains(&mask_id)
        {
            stats.skipped_unsupported += 1;
            continue;
        }
        let channels = match s.dict.get(b"ColorSpace").and_then(Object::as_name).ok() {
            Some(b"DeviceRGB") => 3,
            Some(b"DeviceGray") => 1,
            _ => {
                stats.skipped_unsupported += 1;
                continue;
            }
        };
        let n = |s: &Stream, k: &[u8]| {
            s.dict
                .get(k)
                .ok()
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(_, o)| o.as_i64().ok())
                .and_then(|n| u32::try_from(n).ok())
        };
        let (Some(w), Some(h)) = (n(s, b"Width"), n(s, b"Height")) else {
            continue;
        };
        if w == 0
            || h == 0
            || n(s, b"BitsPerComponent") != Some(8)
            || n(mask, b"BitsPerComponent") != Some(8)
            || n(mask, b"Width") != Some(w)
            || n(mask, b"Height") != Some(h)
            || !inspect::named(&mask.dict, b"ColorSpace", b"DeviceGray")
        {
            stats.skipped_unsupported += 1;
            continue;
        }
        let Some(size) = usage.sizes.get(&id) else {
            continue;
        };
        let target = [
            (size[0] * options.dpi() as f64 / 72.).ceil(),
            (size[1] * options.dpi() as f64 / 72.).ceil(),
        ];
        let scale = (target[0] / f64::from(w))
            .max(target[1] / f64::from(h))
            .min(1.);
        if !scale.is_finite() || scale <= 0. || scale >= 0.95 {
            continue;
        }
        let dims = [
            ((f64::from(w) * scale - 1e-9).ceil() as u32).max(1),
            ((f64::from(h) * scale - 1e-9).ceil() as u32).max(1),
        ];
        let required = (w as usize)
            .saturating_mul(h as usize)
            .saturating_mul(channels + 1);
        if required > remaining || required > 4 * 1024 * 1024 {
            continue;
        }
        remaining -= required;
        let matte = match mask.dict.get(b"Matte") {
            Err(_) => None,
            Ok(o) => {
                let Some(values) = doc.dereference(o).ok().and_then(|(_, o)| o.as_array().ok())
                else {
                    continue;
                };
                if values.len() != channels {
                    continue;
                }
                let Some(values) = values
                    .iter()
                    .map(|o| match o {
                        Object::Integer(n) => Some(*n as f32),
                        Object::Real(n) => Some(*n),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                if values
                    .iter()
                    .any(|&n| !n.is_finite() || !(0. ..=1.).contains(&n))
                {
                    continue;
                }
                Some(values)
            }
        };
        let Some(alpha) = native_samples::read(doc, mask, options) else {
            continue;
        };
        if alpha.bits != 8 || alpha.channels != 1 {
            continue;
        }
        let mut source = s.clone();
        source.dict.remove(b"SMask");
        let Ok(colour) = crate::images::decode(doc, &source, options) else {
            continue;
        };
        if colour.as_bytes().len() != w as usize * h as usize * channels {
            continue;
        }
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for (i, &a) in alpha.bytes.iter().enumerate() {
            if i % 16384 == 0 {
                check_cancel(cancel)?;
            }
            let a = f32::from(a) / 255.;
            for c in 0..3 {
                let c = if channels == 1 { 0 } else { c };
                let value = f32::from(colour.as_bytes()[i * channels + c]) / 255.;
                rgba.push(if let Some(m) = &matte {
                    (value - (1. - a) * m[c]).clamp(0., a)
                } else {
                    value * a
                });
            }
            rgba.push(a);
        }
        let Some(rgba) = Rgba32FImage::from_raw(w, h, rgba) else {
            continue;
        };
        let resized = image::imageops::resize(&rgba, dims[0], dims[1], FilterType::Lanczos3);
        check_cancel(cancel)?;
        let mut colour = Vec::with_capacity(dims[0] as usize * dims[1] as usize * channels);
        let mut alpha = Vec::with_capacity(dims[0] as usize * dims[1] as usize);
        for pixel in resized.pixels() {
            let a = pixel[3].clamp(0., 1.);
            alpha.push((a * 255.).round() as u8);
            for c in 0..channels {
                let p = pixel[c].clamp(0., a);
                let value = if let Some(m) = &matte {
                    p + (1. - a) * m[c]
                } else if a > 0. {
                    p / a
                } else {
                    0.
                };
                colour.push((value.clamp(0., 1.) * 255.).round() as u8);
            }
        }
        let mut md = mask.dict.clone();
        md.set("Width", dims[0] as i64);
        md.set("Height", dims[1] as i64);
        md.set("Filter", "FlateDecode");
        md.remove(b"DecodeParms");
        let mut mc = crate::image_encoding::Candidates::new(
            Stream::new(md.clone(), crate::lossless::deflate(&alpha)?),
            "paired-mask/flate",
            true,
        )?;
        crate::image_encoding::add_exact(&mut mc, &md, &alpha, dims[0], 1, false, true)?;
        let mut cd = s.dict.clone();
        cd.remove(b"SMask");
        cd.set("Width", dims[0] as i64);
        cd.set("Height", dims[1] as i64);
        cd.set("Filter", "FlateDecode");
        cd.remove(b"DecodeParms");
        let mut cc = crate::image_encoding::Candidates::new(
            Stream::new(cd.clone(), crate::lossless::deflate(&colour)?),
            "paired-colour/flate",
            true,
        )?;
        crate::image_encoding::add_exact(&mut cc, &cd, &colour, dims[0], channels, false, true)?;
        if crate::image_encoding::cost(&cc.best)? + crate::image_encoding::cost(&mc.best)? + 32
            >= crate::image_encoding::cost(s)? + crate::image_encoding::cost(mask)?
        {
            continue;
        }
        let mask = doc.add_object(mc.best);
        let mut colour = cc.best;
        colour.dict.set("SMask", mask);
        doc.objects.insert(id, Object::Stream(colour));
        protected.insert(id);
        protected.insert(mask);
        stats.changed += 1;
    }
    Ok((stats, protected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    fn fixture() -> (Document, ObjectId, ObjectId, Usage) {
        let mut d = Document::new();
        let mask=d.add_object(Stream::new(dictionary!{"Subtype"=>"Image","Width"=>256,"Height"=>256,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceGray","Matte"=>vec![1.into(),1.into(),1.into()]},vec![128;256*256]));
        let image=d.add_object(Stream::new(dictionary!{"Subtype"=>"Image","Width"=>256,"Height"=>256,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB","SMask"=>mask},vec![190;256*256*3]));
        let usage = Usage {
            unknown: HashSet::new(),
            protected: HashSet::new(),
            sizes: std::collections::HashMap::from([(image, [20., 20.])]),
        };
        (d, image, mask, usage)
    }
    #[test]
    fn paired_matte_resampling_keeps_shared_source_mask_and_matches_new_grid() {
        let (mut d, image, mask, u) = fixture();
        let old = d.get_object(mask).unwrap().clone();
        let (stats, protected) = optimize(
            &mut d,
            &Options {
                preset: Preset::Balanced,
                ..Default::default()
            },
            &u,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(stats.changed, 1);
        assert!(protected.contains(&image));
        assert_eq!(d.get_object(mask).unwrap(), &old);
        let c = d.get_object(image).unwrap().as_stream().unwrap();
        let next = c.dict.get(b"SMask").unwrap().as_reference().unwrap();
        assert_ne!(next, mask);
        let a = d.get_object(next).unwrap().as_stream().unwrap();
        assert_eq!(c.dict.get(b"Width").unwrap(), a.dict.get(b"Width").unwrap());
        assert_eq!(
            a.dict.get(b"Matte").unwrap(),
            old.as_stream().unwrap().dict.get(b"Matte").unwrap()
        );
        let values = native_samples::read(&d, c, &Options::default()).unwrap();
        assert!(values.bytes.iter().all(|&n| n == 190));
        let values = native_samples::read(&d, a, &Options::default()).unwrap();
        assert!(values.bytes.iter().all(|&n| n == 128));
    }
    #[test]
    fn fidelity_default_spaces_unknown_use_and_decode_never_resize_pairs() {
        for case in 0..4 {
            let (mut d, image, _, mut u) = fixture();
            let mut options = Options {
                preset: Preset::Balanced,
                ..Default::default()
            };
            match case {
                0 => options.preset = Preset::Fidelity,
                1 => {
                    d.add_object(dictionary! {"DefaultRGB"=>"DeviceRGB"});
                }
                2 => {
                    u.unknown.insert(image);
                }
                _ => d
                    .get_object_mut(image)
                    .unwrap()
                    .as_stream_mut()
                    .unwrap()
                    .dict
                    .set(
                        "Decode",
                        vec![1.into(), 0.into(), 1.into(), 0.into(), 1.into(), 0.into()],
                    ),
            };
            let before = d.get_object(image).unwrap().clone();
            assert_eq!(
                optimize(&mut d, &options, &u, &AtomicBool::new(false))
                    .unwrap()
                    .0
                    .changed,
                0
            );
            assert_eq!(d.get_object(image).unwrap(), &before);
        }
    }
}
