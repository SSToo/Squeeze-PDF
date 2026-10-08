mod common;
use lopdf::{Document, Object, Stream, dictionary};
use squeeze_core::{Options, Preset, compress};
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;
fn rgb(stream: &Stream, width: usize, height: usize) -> Vec<u8> {
    let raw = stream.decompressed_content().unwrap();
    let bits = stream
        .dict
        .get(b"BitsPerComponent")
        .unwrap()
        .as_i64()
        .unwrap() as usize;
    let space = stream.dict.get(b"ColorSpace").unwrap();
    let stride = (width * bits).div_ceil(8);
    let mut result = Vec::new();
    if let Ok(a) = space.as_array() {
        assert_eq!(a[0].as_name().unwrap(), b"Indexed");
        let lookup = a[3].as_str().unwrap();
        let channels = if a[1].as_name().unwrap() == b"DeviceRGB" {
            3
        } else {
            1
        };
        for y in 0..height {
            for x in 0..width {
                let i = ((raw[y * stride + x * bits / 8] >> (8 - bits - x * bits % 8))
                    & (255 >> (8 - bits))) as usize;
                if channels == 3 {
                    result.extend_from_slice(&lookup[i * 3..i * 3 + 3]);
                } else {
                    result.extend([lookup[i]; 3]);
                }
            }
        }
    } else if space.as_name().unwrap() == b"DeviceGray" {
        if bits == 8 {
            for p in raw {
                result.extend([p; 3]);
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let p = ((raw[y * stride + x / 8] >> (7 - x % 8)) & 1) * 255;
                    result.extend([p; 3]);
                }
            }
        }
    } else {
        result = raw;
    }
    result
}
#[test]
fn fidelity_palette_and_binary_keep_original_samples_dimensions_and_drawing() {
    for gray in [false, true] {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        let (w, h) = (513usize, 389usize);
        let mut seed = 7u32;
        let mut samples = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if gray {
                    samples.push(if (x / 7 + y / 11) % 3 == 0 { 0 } else { 255 });
                } else {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    samples.extend_from_slice(
                        &[
                            [255, 255, 255],
                            [20, 40, 160],
                            [253, 199, 17],
                            [150, 30, 40],
                        ][(seed >> 30) as usize],
                    );
                }
            }
        }
        let mut f = common::fixture(false);
        f.doc.objects.insert(f.image,Object::Stream(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64,"BitsPerComponent"=>8,"ColorSpace"=>if gray {"DeviceGray"} else {"DeviceRGB"}},samples.clone())));
        f.doc.save(&input).unwrap();
        let source = std::fs::read(&input).unwrap();
        let report = compress(
            &input,
            &output,
            &Options {
                preset: Preset::Fidelity,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.images[0].after_dimensions, [w as u32, h as u32]);
        assert!(report.images[0].encoding.starts_with(if gray {
            "binary-1bit/"
        } else {
            "indexed/"
        }));
        let selected = report.images[0]
            .candidates
            .iter()
            .find(|c| c.encoding == report.images[0].encoding)
            .unwrap();
        assert!(selected.lossless);
        assert_eq!(
            selected.bytes,
            report.images[0]
                .candidates
                .iter()
                .map(|c| c.bytes)
                .min()
                .unwrap()
        );
        let out = Document::load(&output).unwrap();
        let expected = if gray {
            samples.iter().flat_map(|&v| [v; 3]).collect::<Vec<_>>()
        } else {
            samples
        };
        assert_eq!(
            rgb(out.get_object(f.image).unwrap().as_stream().unwrap(), w, h),
            expected
        );
        assert_eq!(out.get_page_content(f.page), f.doc.get_page_content(f.page));
        assert_eq!(std::fs::read(input).unwrap(), source);
        assert_eq!(report.size_after.file_bytes, report.candidate_bytes);
        let a = &report.size_after;
        assert_eq!(
            a.images
                + a.masks
                + a.drawing
                + a.fonts
                + a.attachments
                + a.metadata
                + a.containers
                + a.other_streams
                + a.syntax_and_dictionaries,
            a.file_bytes
        );
    }
}
