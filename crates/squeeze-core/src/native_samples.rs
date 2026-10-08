//! Exact image component bytes, without interpreting or converting colours.
use crate::{Limits, Options, Result, check_cancel, lossless};
use lopdf::{Document, Object, Stream};
use std::sync::atomic::AtomicBool;

pub(crate) struct Samples {
    pub bytes: Vec<u8>,
    pub channels: usize,
    pub bits: usize,
}

/// Device spaces can be replaced by DefaultRGB/Gray/CMYK in an owner's
/// resources. Until all owners are resolved, retain explicit Decode arrays in
/// any such document. Indirect dictionaries are visited in the outer pass;
/// only inline objects recurse, with a shared operation/depth budget.
pub(crate) fn has_default_spaces(
    doc: &Document,
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<bool> {
    fn visit(o: &Object, remaining: &mut usize, depth: usize) -> bool {
        if *remaining == 0 || depth > 64 {
            return true;
        }
        *remaining -= 1;
        let dict = match o {
            Object::Dictionary(d) => Some(d),
            Object::Stream(s) => Some(&s.dict),
            Object::Array(a) => {
                return a.iter().any(|o| visit(o, remaining, depth + 1));
            }
            _ => None,
        };
        dict.is_some_and(|d| {
            [b"DefaultRGB".as_slice(), b"DefaultGray", b"DefaultCMYK"]
                .iter()
                .any(|k| d.has(k))
                || d.iter().any(|(_, o)| visit(o, remaining, depth + 1))
        })
    }
    let mut remaining = limits.max_operations;
    for object in doc.objects.values() {
        check_cancel(cancel)?;
        if visit(object, &mut remaining, 0) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn integer(doc: &Document, object: &Object) -> Option<i64> {
    doc.dereference(object).ok()?.1.as_i64().ok()
}
fn channels(doc: &Document, space: &Object) -> Option<usize> {
    match doc.dereference(space).ok()?.1 {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" => Some(1),
            b"DeviceRGB" => Some(3),
            b"DeviceCMYK" => Some(4),
            _ => None,
        },
        Object::Array(a) => {
            let name = doc.dereference(a.first()?).ok()?.1.as_name().ok()?;
            match name {
                b"CalGray" | b"CalRGB" | b"Lab" if a.len() == 2 => {
                    doc.dereference(&a[1]).ok()?.1.as_dict().ok()?;
                    Some(if name == b"CalGray" { 1 } else { 3 })
                }
                b"ICCBased" if a.len() == 2 => {
                    let profile = doc.dereference(&a[1]).ok()?.1.as_stream().ok()?;
                    let n = integer(doc, profile.dict.get(b"N").ok()?)?;
                    matches!(n, 1 | 3 | 4).then_some(n as usize)
                }
                b"Indexed" if a.len() == 4 => Some(1),
                b"Separation" if a.len() == 4 => {
                    doc.dereference(&a[1]).ok()?.1.as_name().ok()?;
                    Some(1)
                }
                b"DeviceN" if matches!(a.len(), 4 | 5) => {
                    let names = doc.dereference(&a[1]).ok()?.1.as_array().ok()?;
                    if names.is_empty() || names.len() > 32 {
                        return None;
                    }
                    for name in names {
                        doc.dereference(name).ok()?.1.as_name().ok()?;
                    }
                    Some(names.len())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Only explicit defaults of device spaces qualify. ICC Range, Lab, Indexed,
/// reversed mappings and malformed arrays retain their Decode entry.
pub(crate) fn default_decode(doc: &Document, stream: &Stream) -> bool {
    let Some(space) = stream
        .dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .map(|(_, o)| o)
    else {
        return false;
    };
    let count = match space.as_name().ok() {
        Some(b"DeviceGray") => 1,
        Some(b"DeviceRGB") => 3,
        Some(b"DeviceCMYK") => 4,
        _ => return false,
    };
    let Some(values) = stream
        .dict
        .get(b"Decode")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_array().ok())
    else {
        return false;
    };
    values.len() == count * 2
        && values.iter().enumerate().all(|(i, o)| {
            let wanted = (i % 2) as i64;
            match doc.dereference(o).map(|(_, o)| o) {
                Ok(Object::Integer(n)) => *n == wanted,
                Ok(Object::Real(n)) => *n as f64 == wanted as f64,
                _ => false,
            }
        })
}

/// Optional work is capped at 4 MiB including PNG row tags. Colour/opacity
/// dictionaries remain intact; external data and non-Flate codecs are skipped.
pub(crate) fn read(doc: &Document, stream: &Stream, options: &Options) -> Option<Samples> {
    if [b"F".as_slice(), b"FFilter", b"FDecodeParms"]
        .iter()
        .any(|k| stream.dict.has(k))
        || stream
            .dict
            .get(b"SMaskInData")
            .is_ok_and(|o| integer(doc, o) != Some(0))
    {
        return None;
    }
    let width = usize::try_from(integer(doc, stream.dict.get(b"Width").ok()?)?).ok()?;
    let height = usize::try_from(integer(doc, stream.dict.get(b"Height").ok()?)?).ok()?;
    let bits = usize::try_from(integer(doc, stream.dict.get(b"BitsPerComponent").ok()?)?).ok()?;
    let mask = stream
        .dict
        .get(b"ImageMask")
        .ok()
        .map(|o| doc.dereference(o).ok()?.1.as_bool().ok())
        .unwrap_or(Some(false))?;
    let count = if mask {
        1
    } else {
        channels(doc, stream.dict.get(b"ColorSpace").ok()?)?
    };
    if width == 0
        || height == 0
        || !matches!(bits, 1 | 2 | 4 | 8 | 16)
        || (mask && bits != 1)
        || (width as u64).checked_mul(height as u64)? > options.limits.max_image_pixels
    {
        return None;
    }
    let stride = width.checked_mul(count)?.checked_mul(bits)?.div_ceil(8);
    let expected = stride.checked_mul(height)?;
    let cap = options.limits.max_stream_bytes.min(4 * 1024 * 1024);
    if expected.checked_add(height)? > cap {
        return None;
    }
    let filtered = match stream.dict.get(b"Filter") {
        Err(_) => false,
        Ok(o) => match doc.dereference(o).ok()?.1 {
            Object::Name(n) if n == b"FlateDecode" => true,
            Object::Array(a)
                if a.len() == 1
                    && doc.dereference(&a[0]).ok()?.1.as_name().ok()? == b"FlateDecode" =>
            {
                true
            }
            _ => return None,
        },
    };
    let params = match stream.dict.get(b"DecodeParms") {
        Err(_) => None,
        Ok(o) => match doc.dereference(o).ok()?.1 {
            Object::Null => None,
            Object::Dictionary(d) if filtered => Some(d),
            Object::Array(a) if filtered && a.len() == 1 => match doc.dereference(&a[0]).ok()?.1 {
                Object::Null => None,
                Object::Dictionary(d) => Some(d),
                _ => return None,
            },
            _ => return None,
        },
    };
    if params.is_some_and(|d| {
        d.iter().any(|(k, _)| {
            !matches!(
                k.as_slice(),
                b"Predictor" | b"Colors" | b"BitsPerComponent" | b"Columns"
            )
        })
    }) {
        return None;
    }
    let get = |key: &[u8], default: i64| -> Option<i64> {
        match params.and_then(|d| d.get(key).ok()) {
            None => Some(default),
            Some(o) => integer(doc, o),
        }
    };
    let predictor = get(b"Predictor", 1)?;
    let mut bytes = if filtered {
        lossless::inflate(&stream.content, expected + height).ok()?
    } else {
        if stream.content.len() != expected {
            return None;
        }
        stream.content.clone()
    };
    if predictor != 1
        && (get(b"Colors", 1)? != count as i64
            || get(b"Columns", 1)? != width as i64
            || get(b"BitsPerComponent", 8)? != bits as i64)
    {
        return None;
    }
    if predictor == 2 && matches!(bits, 8 | 16) {
        if bytes.len() != expected {
            return None;
        }
        let sample = bits / 8;
        for row in bytes.chunks_exact_mut(stride) {
            for i in (count * sample..stride).step_by(sample) {
                if bits == 8 {
                    row[i] = row[i].wrapping_add(row[i - count]);
                } else {
                    let previous = i - count * 2;
                    let value = u16::from_be_bytes([row[i], row[i + 1]])
                        .wrapping_add(u16::from_be_bytes([row[previous], row[previous + 1]]));
                    row[i..i + 2].copy_from_slice(&value.to_be_bytes());
                }
            }
        }
    } else if (10..=15).contains(&predictor) {
        if bytes.len() != expected + height {
            return None;
        }
        let bpp = (count * bits).div_ceil(8);
        let mut raw = vec![0; expected];
        for y in 0..height {
            let tag = bytes[y * (stride + 1)];
            if tag > 4 || (predictor != 15 && tag as i64 != predictor - 10) {
                return None;
            }
            for x in 0..stride {
                let left = if x >= bpp {
                    raw[y * stride + x - bpp]
                } else {
                    0
                };
                let up = if y > 0 { raw[(y - 1) * stride + x] } else { 0 };
                let corner = if y > 0 && x >= bpp {
                    raw[(y - 1) * stride + x - bpp]
                } else {
                    0
                };
                let value = match tag {
                    0 => 0,
                    1 => left,
                    2 => up,
                    3 => ((left as u16 + up as u16) / 2) as u8,
                    _ => crate::image_encoding::paeth(left, up, corner),
                };
                raw[y * stride + x] = bytes[y * (stride + 1) + x + 1].wrapping_add(value);
            }
        }
        bytes = raw;
    } else if predictor != 1 {
        return None;
    }
    (bytes.len() == expected).then_some(Samples {
        bytes,
        channels: count,
        bits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, dictionary};
    #[test]
    fn identity_decode_references_are_exact_and_special_spaces_stay_mapped() {
        let mut doc = Document::new();
        let decode = doc.add_object(vec![
            0.into(),
            1.into(),
            0.into(),
            1.into(),
            0.into(),
            1.into(),
        ]);
        let mut s = Stream::new(
            dictionary! {"ColorSpace"=>"DeviceRGB","Decode"=>decode},
            vec![],
        );
        assert!(default_decode(&doc, &s));
        s.dict.set(
            "Decode",
            vec![
                0.into(),
                Object::Real(0.999),
                0.into(),
                1.into(),
                0.into(),
                1.into(),
            ],
        );
        assert!(!default_decode(&doc, &s));
        s.dict.set(
            "ColorSpace",
            vec![
                Object::Name(b"Lab".to_vec()),
                Object::Dictionary(Dictionary::new()),
            ],
        );
        s.dict.set("Decode", decode);
        assert!(!default_decode(&doc, &s));
        let cancel = AtomicBool::new(false);
        assert!(!has_default_spaces(&doc, &Limits::default(), &cancel).unwrap());
        assert!(
            has_default_spaces(
                &doc,
                &Limits {
                    max_operations: 0,
                    ..Default::default()
                },
                &cancel
            )
            .unwrap()
        );
        doc.add_object(dictionary! {"Resources"=>dictionary! {"ColorSpace"=>dictionary! {"DefaultRGB"=>"DeviceRGB"}}});
        assert!(has_default_spaces(&doc, &Limits::default(), &cancel).unwrap());
        assert!(has_default_spaces(&doc, &Limits::default(), &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn tiff16_wraps_samples_not_individual_bytes_and_retains_decode_and_matte() {
        let mut doc = Document::new();
        let parms = doc.add_object(
            dictionary! {"Predictor"=>2,"Colors"=>1,"Columns"=>3,"BitsPerComponent"=>16},
        );
        let raw = [65530u16, 3, 510, 1000, 2, 65535];
        let residual = [65530u16, 9, 507, 1000, 64538, 65533];
        let s = Stream::new(
            dictionary! {"Subtype"=>"Image","Width"=>3,"Height"=>2,"BitsPerComponent"=>16,"ColorSpace"=>"DeviceGray","Decode"=>vec![1.into(),0.into()],"Matte"=>vec![0.into()],"Filter"=>vec![Object::Name(b"FlateDecode".to_vec())],"DecodeParms"=>vec![parms.into()]},
            lossless::deflate(
                &residual
                    .iter()
                    .flat_map(|n| n.to_be_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
        assert_eq!(
            read(&doc, &s, &Options::default()).unwrap().bytes,
            raw.iter().flat_map(|n| n.to_be_bytes()).collect::<Vec<_>>()
        );
        let mut bad = s.clone();
        bad.dict.set("Width", 4);
        assert!(read(&doc, &bad, &Options::default()).is_none());
        bad = s.clone();
        bad.dict.set("F", Object::string_literal("external"));
        assert!(read(&doc, &bad, &Options::default()).is_none());
        assert!(
            read(
                &doc,
                &s,
                &Options {
                    limits: crate::Limits {
                        max_stream_bytes: 12,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            )
            .is_none()
        );
    }
    #[test]
    fn png16_up_is_byte_prediction_and_malformed_tags_are_rejected() {
        let doc = Document::new();
        let raw = vec![255, 250, 0, 3, 1, 254, 3, 232, 0, 2, 255, 255];
        let encoded = vec![2, 255, 250, 0, 3, 1, 254, 2, 4, 238, 0, 255, 254, 1];
        let mut s = Stream::new(
            dictionary! {"Width"=>3,"Height"=>2,"BitsPerComponent"=>16,"ColorSpace"=>"DeviceGray","Filter"=>"FlateDecode","DecodeParms"=>dictionary! {"Predictor"=>12,"Colors"=>1,"Columns"=>3,"BitsPerComponent"=>16}},
            lossless::deflate(&encoded).unwrap(),
        );
        assert_eq!(read(&doc, &s, &Options::default()).unwrap().bytes, raw);
        let mut bad = encoded.clone();
        bad[7] = 1;
        s.set_content(lossless::deflate(&bad).unwrap());
        assert!(read(&doc, &s, &Options::default()).is_none());
        s.set_content(lossless::deflate(&encoded[..13]).unwrap());
        assert!(read(&doc, &s, &Options::default()).is_none());
    }
}
