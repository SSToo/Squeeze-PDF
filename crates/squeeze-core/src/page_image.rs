use crate::{Preset, Result, image_encoding::Candidates};
use lopdf::{Stream, dictionary};

/// Exactly-gray RGB may use DeviceGray. All other colors remain RGB, including
/// a one-level channel difference; palette compression never approximates.
#[cfg(test)]
pub(crate) fn encode(
    rgb: Vec<u8>,
    width: u32,
    height: u32,
    preset: Preset,
    quality: u8,
    encoder: &mut crate::jpeg::Encoder,
) -> Result<EncodedPage> {
    let mut budget = crate::image_encoding::RgbBudget::new(&crate::Limits::default());
    Ok(encode_variants(
        rgb,
        width,
        height,
        preset,
        quality,
        encoder,
        &mut budget,
        &std::sync::atomic::AtomicBool::new(false),
    )?
    .0)
}
pub(crate) type EncodedPage = (Stream, &'static str, String, Vec<crate::EncodingCandidate>);
pub(crate) type PageReplacement = (lopdf::ObjectId, Stream, crate::ImageResult);
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_variants(
    rgb: Vec<u8>,
    width: u32,
    height: u32,
    preset: Preset,
    quality: u8,
    encoder: &mut crate::jpeg::Encoder,
    budget: &mut crate::image_encoding::RgbBudget,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(EncodedPage, Option<EncodedPage>)> {
    encoder.cached = false;
    let gray = rgb.chunks_exact(3).all(|p| p[0] == p[1] && p[1] == p[2]);
    let pixels = if gray {
        rgb.chunks_exact(3).map(|p| p[0]).collect::<Vec<_>>()
    } else {
        rgb
    };
    let channels = if gray { 1 } else { 3 };
    let base = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>width as i64,"Height"=>height as i64,"BitsPerComponent"=>8,"ColorSpace"=>if gray {"DeviceGray"} else {"DeviceRGB"}};
    let mut candidates = if preset == Preset::Lossless {
        let mut dict = base.clone();
        dict.set("Filter", "FlateDecode");
        Candidates::new(
            Stream::new(dict, crate::lossless::deflate(&pixels)?),
            "flate/plain",
            true,
        )?
    } else {
        let bytes = encoder
            .encode(&pixels, width, height, gray, quality)
            .map_err(|e| crate::Error::Invalid(format!("页面图片编码失败：{e}")))?;
        let mut dict = base.clone();
        dict.set("Filter", "DCTDecode");
        Candidates::new(Stream::new(dict, bytes), "mozjpeg", false)?
    };
    crate::image_encoding::add_exact(
        &mut candidates,
        &base,
        &pixels,
        width,
        channels,
        true,
        preset == Preset::Lossless,
    )?;
    let mut enhanced = candidates.clone();
    if preset != Preset::Lossless {
        crate::image_encoding::add_rgb_extra(
            &mut enhanced,
            &base,
            &pixels,
            width,
            channels,
            budget,
            cancel,
        )?;
    }
    fn finish(c: Candidates) -> EncodedPage {
        let label = if c.encoding == "mozjpeg" {
            "MozJPEG"
        } else if c.encoding == "ccitt-g4" {
            "无损 CCITT G4"
        } else {
            "无损 Flate"
        };
        (c.best, label, c.encoding, c.entries)
    }
    let alternate = (enhanced.best != candidates.best).then(|| finish(enhanced));
    Ok((finish(candidates), alternate))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_text_is_packed_and_faint_color_is_never_grayscaled() {
        let mut rgb = vec![255; 400 * 300 * 3];
        for y in 40..60 {
            for x in 30..350 {
                rgb[(y * 400 + x) * 3..(y * 400 + x) * 3 + 3].fill(0);
            }
        }
        let mut encoder = crate::jpeg::Encoder::default();
        let (stream, _, encoding, _) =
            encode(rgb.clone(), 400, 300, Preset::Small, 60, &mut encoder).unwrap();
        assert!(encoding.starts_with("binary-1bit"));
        assert_eq!(
            stream
                .dict
                .get(b"BitsPerComponent")
                .unwrap()
                .as_i64()
                .unwrap(),
            1
        );
        let packed = stream.decompressed_content().unwrap();
        for y in 0..300 {
            for x in 0..400 {
                let bit = (packed[y * 50 + x / 8] >> (7 - x % 8)) & 1;
                assert_eq!(bit * 255, rgb[(y * 400 + x) * 3]);
            }
        }
        rgb[0] = 254;
        let (stream, _, _, _) = encode(rgb, 400, 300, Preset::Lossless, 72, &mut encoder).unwrap();
        let space = stream.dict.get(b"ColorSpace").unwrap();
        assert!(
            space.as_name().ok() == Some(b"DeviceRGB")
                || space
                    .as_array()
                    .is_ok_and(|a| a[1].as_name().ok() == Some(b"DeviceRGB"))
        );
    }
}
