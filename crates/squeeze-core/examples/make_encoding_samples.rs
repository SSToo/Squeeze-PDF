//! Self-owned, deterministic images to exercise the phase-one codec selection.
use lopdf::{Document, Object, Stream, dictionary};
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    fs::create_dir_all(&dir)?;
    for name in ["binary", "palette", "gray-gradient", "jpeg-entropy"] {
        let (width, height) = (513usize, 389usize);
        let gray = name == "binary" || name == "gray-gradient";
        let mut pixels = Vec::new();
        let mut seed = 7u32;
        for y in 0..height {
            for x in 0..width {
                match name {
                    "binary" => pixels.push(if (x / 7 + y / 11) % 3 == 0 { 0 } else { 255 }),
                    "gray-gradient" => pixels.push(((x + y * 3) % 256) as u8),
                    "palette" => {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        pixels.extend_from_slice(
                            &[
                                [255, 255, 255],
                                [20, 40, 160],
                                [253, 199, 17],
                                [150, 30, 40],
                            ][(seed >> 30) as usize],
                        );
                    }
                    _ => pixels.extend_from_slice(&[
                        (x % 256) as u8,
                        (y % 256) as u8,
                        ((x + y) % 256) as u8,
                    ]),
                }
            }
        }
        let mut doc = Document::with_version("1.7");
        let tree = doc.new_object_id();
        let mut dict = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>width as i64,"Height"=>height as i64,"BitsPerComponent"=>8,"ColorSpace"=>if gray {"DeviceGray"} else {"DeviceRGB"}};
        if name == "jpeg-entropy" {
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80).encode(
                &pixels,
                width as u32,
                height as u32,
                image::ExtendedColorType::Rgb8,
            )?;
            pixels = bytes;
            dict.set("Filter", "DCTDecode");
        }
        let image = doc.add_object(Stream::new(dict, pixels));
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"q 369.36 0 0 280.08 0 0 cm /I Do Q".to_vec(),
        ));
        let page=doc.add_object(dictionary! {"Type"=>"Page","Parent"=>tree,"MediaBox"=>vec![0.into(),0.into(),369.36.into(),280.08.into()],"Resources"=>dictionary! {"XObject"=>dictionary! {"I"=>image}},"Contents"=>content});
        doc.objects.insert(
            tree,
            Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>tree});
        doc.trailer.set("Root", root);
        doc.save(dir.join(format!("{name}.pdf")))?;
    }
    Ok(())
}
