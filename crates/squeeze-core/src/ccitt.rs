//! Lossless T.6 candidate for proven binary samples, never thresholded text.
use fax::{Color, VecWriter, encoder::Encoder};
use lopdf::Dictionary;

pub(crate) fn decode(
    bytes: &[u8],
    width: u32,
    height: u32,
    params: &Dictionary,
) -> Option<Vec<u8>> {
    use hayro_ccitt::{DecodeSettings, Decoder, DecoderContext, EncodingMode};
    let number = |key: &[u8], default| match params.get(key) {
        Ok(o) => o.as_i64().ok(),
        Err(_) => Some(default),
    };
    let flag = |key: &[u8], default| match params.get(key) {
        Ok(o) => o.as_bool().ok(),
        Err(_) => Some(default),
    };
    let rows = number(b"Rows", 0)?;
    if width == 0
        || width > 16384
        || height == 0
        || number(b"K", 0)? >= 0
        || number(b"Columns", 1728)? != width as i64
        || (rows != 0 && rows != height as i64)
        || number(b"DamagedRowsBeforeError", 0)? != 0
        || flag(b"EndOfLine", false)?
    {
        return None;
    }
    struct Samples {
        bytes: Vec<u8>,
        width: usize,
        height: usize,
        rows: usize,
        valid: bool,
    }
    impl Decoder for Samples {
        fn push_pixel(&mut self, white: bool) {
            if self.bytes.len() < self.width * self.height {
                self.bytes.push(if white { 255 } else { 0 });
            } else {
                self.valid = false;
            }
        }
        fn push_pixel_chunk(&mut self, white: bool, count: u32) {
            let count = count as usize * 8;
            let end = self.bytes.len().saturating_add(count);
            if end > self.width * self.height {
                self.valid = false;
                return;
            }
            self.bytes.resize(end, if white { 255 } else { 0 });
        }
        fn next_line(&mut self) {
            self.rows += 1;
            self.valid &= self.bytes.len() == self.rows * self.width;
        }
    }
    let mut target = Samples {
        bytes: Vec::with_capacity(width as usize * height as usize),
        width: width as usize,
        height: height as usize,
        rows: 0,
        valid: true,
    };
    let mut context = DecoderContext::new(DecodeSettings {
        columns: width,
        rows: height,
        end_of_block: flag(b"EndOfBlock", true)?,
        end_of_line: false,
        rows_are_byte_aligned: flag(b"EncodedByteAlign", false)?,
        encoding: EncodingMode::Group4,
        invert_black: flag(b"BlackIs1", false)?,
    });
    hayro_ccitt::decode(bytes, &mut target, &mut context).ok()?;
    (target.valid
        && target.rows == height as usize
        && target.bytes.len() == width as usize * height as usize)
        .then_some(target.bytes)
}

pub(crate) fn encode(pixels: &[u8], width: u32) -> Option<Vec<u8>> {
    // fax uses signed 16-bit differences internally. Keep all coordinates
    // below that boundary and bound output by the already limited input.
    if width == 0
        || width > 16_384
        || !pixels.len().is_multiple_of(width as usize)
        || pixels.is_empty()
        || !pixels.iter().all(|&v| v == 0 || v == 255)
    {
        return None;
    }
    let mut encoder = Encoder::new(VecWriter::new());
    for row in pixels.chunks_exact(width as usize) {
        encoder
            .encode_line(
                row.iter()
                    .map(|&v| if v == 0 { Color::Black } else { Color::White }),
                width,
            )
            .ok()?;
    }
    let encoded = encoder.finish().ok()?.finish();
    // Independently run the decoder state machine before considering output.
    let mut offset = 0;
    let mut same = true;
    fax::decoder::decode_g4(
        encoded.iter().copied(),
        width,
        Some((pixels.len() / width as usize) as u32),
        |line| {
            for color in fax::decoder::pels(line, width) {
                let value = if color == Color::Black { 0 } else { 255 };
                same &= pixels.get(offset) == Some(&value);
                offset += 1;
            }
        },
    )?;
    (same && offset == pixels.len()).then_some(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn odd_width_black_first_noise_and_long_runs_roundtrip() {
        for width in [1, 7, 513, 3000, 16384] {
            let mut seed = 13u32;
            let mut pixels = vec![255; width * 12];
            for y in 0..12 {
                for x in 0..width {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    pixels[y * width + x] = match y {
                        0 => 0,
                        1 => 255,
                        2 => {
                            if x % 2 == 0 {
                                0
                            } else {
                                255
                            }
                        }
                        _ => {
                            if seed >> 31 == 0 {
                                0
                            } else {
                                255
                            }
                        }
                    };
                }
            }
            assert!(encode(&pixels, width as u32).is_some());
        }
        assert!(encode(&[0, 254, 255], 3).is_none());
        assert!(encode(&[255; 17], 7).is_none());
        assert!(encode(&[0], 0).is_none());
    }
    #[test]
    fn independent_g4_decoder_preserves_odd_rows_black_polarity_and_rejects_truncation() {
        use lopdf::dictionary;
        let width = 513;
        let pixels: Vec<_> = (0..width * 37)
            .map(|i| {
                if (i / width + i % width / 7) % 3 == 0 {
                    0
                } else {
                    255
                }
            })
            .collect();
        let bytes = encode(&pixels, width as u32).unwrap();
        for invert in [false, true] {
            let params =
                dictionary! {"K"=>-1,"Columns"=>width as i64,"Rows"=>37,"BlackIs1"=>invert};
            let expected: Vec<_> = pixels
                .iter()
                .map(|v| if invert { 255 - v } else { *v })
                .collect();
            assert_eq!(decode(&bytes, width as u32, 37, &params), Some(expected));
            assert!(decode(&bytes[..bytes.len() / 2], width as u32, 37, &params).is_none());
            assert!(decode(&bytes, width as u32 + 1, 37, &params).is_none());
        }
        assert!(
            decode(
                &bytes,
                width as u32,
                37,
                &dictionary! {"K"=>0,"Columns"=>width as i64}
            )
            .is_none()
        );
    }
}
