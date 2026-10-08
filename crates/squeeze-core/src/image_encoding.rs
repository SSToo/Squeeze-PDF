//! Exact sample encodings shared by original images and rendered pages.
use crate::{EncodingCandidate, Result, lossless};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};
use std::collections::HashMap;

pub(crate) struct Candidates {
    pub best: Stream,
    pub encoding: String,
    pub entries: Vec<EncodingCandidate>,
    cost: usize,
}
impl Candidates {
    pub fn new(stream: Stream, encoding: &str, lossless: bool) -> Result<Self> {
        let cost = cost(&stream)?;
        Ok(Self {
            best: stream,
            encoding: encoding.into(),
            entries: vec![EncodingCandidate {
                encoding: encoding.into(),
                bytes: cost,
                lossless,
            }],
            cost,
        })
    }
    pub fn consider(&mut self, stream: Stream, encoding: &str, lossless: bool) -> Result<()> {
        let bytes = cost(&stream)?;
        self.entries.push(EncodingCandidate {
            encoding: encoding.into(),
            bytes,
            lossless,
        });
        if bytes < self.cost {
            self.best = stream;
            self.encoding = encoding.into();
            self.cost = bytes;
        }
        Ok(())
    }
}
/// Serialize the dictionary with lopdf's actual writer and exclude the file
/// trailer/xref. Dictionary bytes, Length digits, DecodeParms and escaped palette
/// data are included; only the identical indirect-object framing is retained.
pub(crate) fn cost(stream: &Stream) -> Result<usize> {
    let mut doc = Document::with_version("1.7");
    let mut dict = stream.dict.clone();
    dict.set("Length", stream.content.len() as i64);
    doc.add_object(dict);
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    let mut bytes = Vec::new();
    doc.save_with_options(&mut bytes, lopdf::SaveOptions::default())?;
    let start = bytes
        .windows(8)
        .position(|b| b == b"1 0 obj\n")
        .ok_or_else(|| crate::Error::Validation("candidate dictionary start missing".into()))?;
    let end = bytes
        .windows(6)
        .rposition(|b| b == b"\nxref\n")
        .ok_or_else(|| crate::Error::Validation("candidate dictionary end missing".into()))?
        + 1;
    Ok(end - start + stream.content.len())
}

pub(crate) fn add_exact(
    candidates: &mut Candidates,
    base: &Dictionary,
    pixels: &[u8],
    width: u32,
    channels: usize,
    packed: bool,
    force_flate: bool,
) -> Result<()> {
    let palette = if packed {
        palette(pixels, channels)
    } else {
        None
    };
    // Avoid repeated multi-megabyte Flate attempts on continuous-tone RGB.
    // Gray/opacity is always evaluated, and an exact small palette proves that
    // RGB samples may benefit. Classification never changes sample values.
    if force_flate || channels == 1 || palette.is_some() {
        add_flate(
            candidates,
            base,
            pixels,
            width as usize * channels,
            channels,
            8,
            channels,
            width,
            "flate",
            None,
        )?;
    }
    if !packed {
        return Ok(());
    }
    if channels == 1 && pixels.iter().all(|&v| v == 0 || v == 255) {
        if let Some(bytes) = crate::ccitt::encode(pixels, width) {
            let mut dict = base.clone();
            dict.set("BitsPerComponent", 1);
            dict.set("Filter", "CCITTFaxDecode");
            dict.set("DecodeParms", dictionary! {"K"=>-1,"Columns"=>width as i64,"Rows"=>(pixels.len()/width as usize) as i64,"BlackIs1"=>false});
            candidates.consider(Stream::new(dict, bytes), "ccitt-g4", true)?;
        }
        let indices: Vec<_> = pixels.iter().map(|&v| u8::from(v == 255)).collect();
        let raw = pack(&indices, width as usize, 1);
        let mut dict = base.clone();
        dict.set("BitsPerComponent", 1);
        add_flate(
            candidates,
            &dict,
            &raw,
            (width as usize).div_ceil(8),
            1,
            1,
            1,
            width,
            "binary-1bit",
            None,
        )?;
    }
    if let Some((lookup, indices)) = palette {
        let count = lookup.len() / channels;
        let bits = match count {
            0..=2 => 1,
            3..=4 => 2,
            5..=16 => 4,
            _ => 8,
        };
        let raw = pack(&indices, width as usize, bits);
        let mut dict = base.clone();
        let original_space = base.get(b"ColorSpace")?.clone();
        dict.set(
            "ColorSpace",
            vec![
                Object::Name(b"Indexed".to_vec()),
                original_space,
                (count as i64 - 1).into(),
                Object::string_literal(lookup),
            ],
        );
        dict.set("BitsPerComponent", bits as i64);
        add_flate(
            candidates,
            &dict,
            &raw,
            (width as usize * bits).div_ceil(8),
            1,
            bits,
            1,
            width,
            "indexed",
            None,
        )?;
    }
    Ok(())
}
pub(crate) fn add_native(
    candidates: &mut Candidates,
    base: &Dictionary,
    samples: &crate::native_samples::Samples,
    width: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    add_flate(
        candidates,
        base,
        &samples.bytes,
        (width as usize * samples.channels * samples.bits).div_ceil(8),
        (samples.channels * samples.bits).div_ceil(8),
        samples.bits,
        samples.channels,
        width,
        "native-flate",
        Some(cancel),
    )
}
fn palette(pixels: &[u8], channels: usize) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut colors: HashMap<&[u8], u8> = HashMap::new();
    let mut lookup = Vec::new();
    let mut indices = Vec::with_capacity(pixels.len() / channels);
    for sample in pixels.chunks_exact(channels) {
        let index = match colors.get(sample) {
            Some(&index) => index,
            None => {
                if colors.len() == 256 {
                    return None;
                }
                let index = colors.len() as u8;
                colors.insert(sample, index);
                lookup.extend_from_slice(sample);
                index
            }
        };
        indices.push(index);
    }
    Some((lookup, indices))
}
fn pack(indices: &[u8], width: usize, bits: usize) -> Vec<u8> {
    let stride = (width * bits).div_ceil(8);
    let mut out = vec![0; stride * (indices.len() / width)];
    for (y, row) in indices.chunks_exact(width).enumerate() {
        for (x, &value) in row.iter().enumerate() {
            out[y * stride + x * bits / 8] |= value << (8 - bits - x * bits % 8);
        }
    }
    out
}
#[allow(clippy::too_many_arguments)]
fn add_flate(
    candidates: &mut Candidates,
    base: &Dictionary,
    raw: &[u8],
    stride: usize,
    bpp: usize,
    bits: usize,
    colors: usize,
    width: u32,
    label: &str,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let uniform = raw
        .first()
        .is_some_and(|first| raw.iter().all(|v| v == first));
    // A constant byte run is already ideal for Deflate. PNG tags/prediction
    // add rows and dictionary cost; avoid eight full passes on opaque masks.
    let strategies = [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(3),
        Some(4),
        Some(5),
        Some(6),
    ];
    for &strategy in if uniform {
        &strategies[..1]
    } else {
        &strategies
    } {
        if let Some(cancel) = cancel {
            crate::check_cancel(cancel)?;
        }
        let mut dict = base.clone();
        dict.set("Filter", "FlateDecode");
        dict.remove(b"DecodeParms");
        let bytes = if let Some(strategy) = strategy {
            dict.set("DecodeParms", dictionary! {"Predictor"=>15,"Columns"=>width as i64,"Colors"=>colors as i64,"BitsPerComponent"=>bits as i64});
            lossless::deflate(&predict(raw, stride, bpp, strategy))?
        } else {
            lossless::deflate(raw)?
        };
        let name = format!(
            "{label}/{}",
            match strategy {
                None => "plain",
                Some(0) => "none",
                Some(1) => "sub",
                Some(2) => "up",
                Some(3) => "average",
                Some(4) => "paeth",
                Some(5) => "adaptive-3",
                _ => "adaptive-5",
            }
        );
        candidates.consider(Stream::new(dict, bytes), &name, true)?;
    }
    Ok(())
}
pub(crate) fn paeth(left: u8, up: u8, corner: u8) -> u8 {
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
fn predict(raw: &[u8], stride: usize, bpp: usize, strategy: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / stride);
    let mut previous: Option<&[u8]> = None;
    for row in raw.chunks_exact(stride) {
        let residual = |filter: u8, i: usize| {
            let left = if i >= bpp { row[i - bpp] } else { 0 };
            let up = previous.map_or(0, |r| r[i]);
            let corner = if i >= bpp {
                previous.map_or(0, |r| r[i - bpp])
            } else {
                0
            };
            let predicted = match filter {
                1 => left,
                2 => up,
                3 => ((left as u16 + up as u16) / 2) as u8,
                4 => paeth(left, up, corner),
                _ => 0,
            };
            row[i].wrapping_sub(predicted)
        };
        let filter = if strategy < 5 {
            strategy
        } else {
            (0..=if strategy == 5 { 2 } else { 4 })
                .min_by_key(|&f| {
                    (0..stride)
                        .map(|i| (residual(f, i) as i8 as i16).unsigned_abs() as u64)
                        .sum::<u64>()
                })
                .unwrap()
        };
        out.push(filter);
        out.extend((0..stride).map(|i| residual(filter, i)));
        previous = Some(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_five_filters_and_adaptive_decode_exactly() {
        for channels in [1, 3] {
            let raw: Vec<_> = (0..31 * 19 * channels)
                .map(|i| ((i * 7 + i / (31 * channels) * 17) % 256) as u8)
                .collect();
            for strategy in 0..=6 {
                let stream = Stream::new(
                    dictionary! {"Filter"=>"FlateDecode","DecodeParms"=>dictionary! {"Predictor"=>15,"Columns"=>31,"Colors"=>channels as i64,"BitsPerComponent"=>8}},
                    lossless::deflate(&predict(&raw, 31 * channels, channels, strategy)).unwrap(),
                );
                assert_eq!(
                    stream.decompressed_content().unwrap(),
                    raw,
                    "channels={channels},strategy={strategy}"
                );
            }
        }
    }
    #[test]
    fn uniform_opacity_uses_a_single_flate_attempt_and_keeps_8_bit_samples() {
        for value in [0, 128, 255] {
            let pixels = vec![value; 513 * 389];
            let base = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>513,"Height"=>389,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceGray"};
            let mut candidates =
                Candidates::new(Stream::new(base.clone(), pixels.clone()), "original", true)
                    .unwrap();
            add_exact(&mut candidates, &base, &pixels, 513, 1, false, false).unwrap();
            assert_eq!(candidates.entries.len(), 2);
            assert_eq!(candidates.encoding, "flate/plain");
            assert_eq!(
                candidates
                    .best
                    .dict
                    .get(b"BitsPerComponent")
                    .unwrap()
                    .as_i64()
                    .unwrap(),
                8
            );
            assert_eq!(candidates.best.decompressed_content().unwrap(), pixels);
        }
    }
    #[test]
    fn packed_rows_are_padded_and_palette_is_exact_and_deterministic() {
        for (count, bits) in [(2, 1), (4, 2), (16, 4), (256, 8)] {
            let width = 13;
            let pixels: Vec<_> = (0..width * 17)
                .flat_map(|i| {
                    let v = (i % count) as u8;
                    [v, 255 - v, v / 2]
                })
                .collect();
            let (lookup, indices) = palette(&pixels, 3).unwrap();
            let raw = pack(&indices, width, bits);
            for y in 0..17 {
                for x in 0..width {
                    let index = (raw[y * (width * bits).div_ceil(8) + x * bits / 8]
                        >> (8 - bits - x * bits % 8))
                        & (255 >> (8 - bits));
                    assert_eq!(
                        &lookup[index as usize * 3..index as usize * 3 + 3],
                        &pixels[(y * width + x) * 3..(y * width + x) * 3 + 3]
                    );
                }
            }
            assert_eq!(palette(&pixels, 3).unwrap(), (lookup, indices));
        }
    }
}
