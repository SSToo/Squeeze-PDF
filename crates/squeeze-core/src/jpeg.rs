use mozjpeg::{ColorSpace, Compress, ScanMode};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// Per-document cache; never shares pixels between jobs or retains source data.
// Key every encoder input, including dimensions when pixel bytes coincide.
pub(crate) struct Encoder {
    cache: HashMap<[u8; 32], Vec<u8>>,
    bytes: usize,
    budget: usize,
    pub cached: bool,
}
impl Default for Encoder {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            bytes: 0,
            budget: 32 * 1024 * 1024,
            cached: false,
        }
    }
}
impl Encoder {
    pub(crate) fn encode(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        gray: bool,
        quality: u8,
    ) -> std::result::Result<Vec<u8>, String> {
        self.cached = false;
        let mut hash = Sha256::new();
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        hash.update([u8::from(gray), quality]);
        hash.update(pixels);
        let key = hash.finalize().into();
        if let Some(data) = self.cache.get(&key) {
            self.cached = true;
            return Ok(data.clone());
        }
        let data = encode(pixels, width, height, gray, quality)?;
        if self.cache.len() < 256 && self.bytes.saturating_add(data.len()) <= self.budget {
            self.bytes += data.len();
            self.cache.insert(key, data.clone());
        }
        Ok(data)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Profile {
    pub sampling: (u8, u8),
    pub table: u8,
    pub scan_trellis: bool,
}

pub(crate) fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    gray: bool,
    quality: u8,
) -> std::result::Result<Vec<u8>, String> {
    encode_profile(
        pixels,
        width,
        height,
        gray,
        quality,
        Profile {
            sampling: if quality >= 90 { (1, 1) } else { (2, 2) },
            table: 0,
            scan_trellis: false,
        },
    )
}

pub(crate) fn encode_profile(
    pixels: &[u8],
    width: u32,
    height: u32,
    gray: bool,
    quality: u8,
    profile: Profile,
) -> std::result::Result<Vec<u8>, String> {
    // Validate before crossing the FFI boundary. MozJPEG's dimension ceiling
    // is slightly below u16::MAX, and its error handler unwinds on failure.
    if width == 0 || height == 0 || width > 65_500 || height > 65_500 {
        return Err("JPEG 图片尺寸无效或超过限制".into());
    }
    if !(1..=100).contains(&quality) {
        return Err("JPEG 图片质量应为 1–100".into());
    }
    if !matches!(profile.sampling, (1, 1) | (2, 1) | (2, 2)) || profile.table > 3 {
        return Err("JPEG 候选参数无效".into());
    }
    let channels = if gray { 1 } else { 3 };
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels));
    if expected != Some(pixels.len()) {
        return Err("JPEG 图片像素长度与尺寸不匹配".into());
    }
    std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
        let mut encoder = Compress::new(if gray {
            ColorSpace::JCS_GRAYSCALE
        } else {
            ColorSpace::JCS_RGB
        });
        // This wrapper's scan-mode setter resets libjpeg defaults, so apply it
        // before quality, dimensions, sampling and the other explicit options.
        encoder.set_scan_optimization_mode(ScanMode::Auto);
        encoder.set_size(width as usize, height as usize);
        // Keep MozJPEG's maximum-compression defaults, including trellis and
        // deringing. Optimize scans before generating the progressive script.
        encoder.set_quality(quality as f32);
        let tables = match profile.table {
            1 => Some((
                &mozjpeg::qtable::AnnexK_Luma,
                &mozjpeg::qtable::AnnexK_Chroma,
            )),
            2 => Some((
                &mozjpeg::qtable::MSSSIM_Luma,
                &mozjpeg::qtable::MSSSIM_Chroma,
            )),
            3 => Some((
                &mozjpeg::qtable::PSNRHVS_Luma,
                &mozjpeg::qtable::PSNRHVS_Chroma,
            )),
            _ => None,
        };
        if let Some((luma, chroma)) = tables {
            encoder.set_luma_qtable(&luma.scaled(quality as f32, quality as f32));
            encoder.set_chroma_qtable(&chroma.scaled(quality as f32, quality as f32));
        }
        encoder.set_use_scans_in_trellis(profile.scan_trellis);
        encoder.set_optimize_coding(true);
        encoder.set_optimize_scans(true);
        if !gray {
            let sampling = profile.sampling;
            encoder.set_chroma_sampling_pixel_sizes(sampling, sampling);
        }
        encoder.set_progressive_mode();
        let mut started = encoder.start_compress(Vec::new())?;
        started.write_scanlines(pixels)?;
        started.finish()
    })
    .map_err(|_| "MozJPEG 编码失败".to_string())?
    .map_err(|e| format!("MozJPEG 编码失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::encode;
    #[test]
    fn cache_is_bounded_and_cannot_mix_quality_geometry_or_color() {
        let mut encoder = super::Encoder::default();
        let pixels: Vec<_> = (0..32 * 32 * 3).map(|n| (n % 251) as u8).collect();
        let a = encoder.encode(&pixels, 32, 32, false, 85).unwrap();
        assert!(!encoder.cached);
        assert_eq!(a, encoder.encode(&pixels, 32, 32, false, 85).unwrap());
        assert!(encoder.cached);
        encoder.encode(&pixels, 64, 16, false, 85).unwrap();
        assert!(!encoder.cached);
        encoder.encode(&pixels, 32, 32, false, 95).unwrap();
        assert!(!encoder.cached);
        encoder.encode(&pixels, 32, 96, true, 85).unwrap();
        assert!(!encoder.cached);
        let mut encoder = super::Encoder {
            budget: 1,
            ..Default::default()
        };
        encoder.encode(&pixels, 32, 32, false, 85).unwrap();
        assert_eq!(encoder.bytes, 0);
        encoder.encode(&pixels, 32, 32, false, 85).unwrap();
        assert!(!encoder.cached);
    }

    #[test]
    fn mozjpeg_round_trips_rgb_and_gray_with_progressive_scans() {
        for gray in [false, true] {
            let channels = if gray { 1 } else { 3 };
            let mut pixels = Vec::new();
            for y in 0..33u8 {
                for x in 0..47u8 {
                    if gray {
                        pixels.push(x * 4);
                    } else {
                        pixels.extend_from_slice(&[x * 4, y * 6, 160]);
                    }
                }
            }
            for quality in [60, 95] {
                let encoded = encode(&pixels, 47, 33, gray, quality).unwrap();
                assert!(encoded.windows(2).any(|w| w == [0xff, 0xc2]));
                let decoded = image::load_from_memory(&encoded).unwrap();
                assert_eq!((decoded.width(), decoded.height()), (47, 33));
                assert_eq!(decoded.color().channel_count() as usize, channels);
                let decoded = decoded.into_bytes();
                let mse = pixels
                    .iter()
                    .zip(decoded)
                    .map(|(&a, b)| (a as f64 - b as f64).powi(2))
                    .sum::<f64>()
                    / pixels.len() as f64;
                assert!(mse.sqrt() < 8., "gray={gray}, quality={quality}, mse={mse}");
            }
        }
    }

    #[test]
    fn rejects_invalid_input_before_entering_native_encoder() {
        for (pixels, width, height, gray, quality) in [
            (&[][..], 0, 1, false, 85),
            (&[][..], 65_501, 1, false, 85),
            (&[][..], 1, 1, false, 85),
            (&[0][..], 1, 1, true, 0),
            (&[0][..], 1, 1, true, 101),
        ] {
            assert!(encode(pixels, width, height, gray, quality).is_err());
        }
    }

    #[test]
    fn quality_is_not_reset_by_scan_configuration() {
        let pixels: Vec<u8> = (0..128 * 96)
            .map(|i| ((i * 37 + (i / 128) * 19) % 256) as u8)
            .collect();
        let low = encode(&pixels, 128, 96, true, 50).unwrap();
        let high = encode(&pixels, 128, 96, true, 95).unwrap();
        assert!(high.len() > low.len());
        let mse = |bytes: &[u8]| {
            let decoded = image::load_from_memory(bytes).unwrap().to_luma8();
            pixels
                .iter()
                .zip(decoded.as_raw())
                .map(|(&a, &b)| (a as f64 - b as f64).powi(2))
                .sum::<f64>()
        };
        assert!(mse(&high) < mse(&low));
    }
}
