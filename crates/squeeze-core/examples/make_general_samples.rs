//! General development corpus, independent of user-provided PDFs.
use lopdf::{Document, Object, Stream, dictionary};
use std::{env, fs, io::Write, path::Path};
fn zip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}
fn page(
    mut doc: Document,
    resources: Object,
    body: Vec<u8>,
    target: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let tree = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, body));
    let p=doc.add_object(dictionary! {"Type"=>"Page","Parent"=>tree,"MediaBox"=>vec![0.into(),0.into(),360.into(),300.into()],"Resources"=>resources,"Contents"=>content});
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![p.into()],"Count"=>1}),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>tree});
    doc.trailer.set("Root", root);
    doc.save(target)?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = env::args().nth(1).ok_or("output directory")?;
    let dir = Path::new(&dir);
    fs::create_dir_all(dir)?;
    let profile = env::args().nth(2).map(fs::read).transpose()?;
    for name in [
        "indexed-1",
        "indexed-2",
        "indexed-4",
        "indexed-8",
        "indexed-gray",
        "g4",
        "g4-inverse",
        "cmyk-raw",
        "cmyk-tiff",
        "cmyk-png",
        "cmyk-icc",
    ] {
        let mut doc = Document::with_version("1.7");
        let (w, h) = (513usize, 389usize);
        let mut d =
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64};
        let bytes = if name.starts_with("indexed") {
            let gray = name == "indexed-gray";
            let bits = if gray {
                4usize
            } else {
                name.rsplit('-').next().unwrap().parse::<usize>()?
            };
            let count = 1usize << bits;
            let mut lookup = Vec::new();
            for i in 0..count {
                if gray {
                    lookup.push((i * 255 / (count - 1)) as u8);
                } else {
                    lookup.extend([
                        (i * 255 / (count - 1)) as u8,
                        ((i * 47) % 256) as u8,
                        ((i * 91) % 256) as u8,
                    ]);
                }
            }
            let table = doc.add_object(Stream::new(
                dictionary! {"Filter"=>"FlateDecode"},
                zip(&lookup),
            ));
            d.set(
                "ColorSpace",
                vec![
                    "Indexed".into(),
                    if gray {
                        "DeviceGray".into()
                    } else {
                        "DeviceRGB".into()
                    },
                    (count as i64 - 1).into(),
                    table.into(),
                ],
            );
            d.set("BitsPerComponent", bits as i64);
            let mut raw = vec![0; (w * bits).div_ceil(8) * h];
            for y in 0..h {
                for x in 0..w {
                    let i = ((x / 9) + (y / 7)) % count;
                    raw[y * (w * bits).div_ceil(8) + x * bits / 8] |=
                        (i as u8) << (8 - bits - x * bits % 8);
                }
            }
            raw
        } else if name.starts_with("g4") {
            use fax::{Color, VecWriter, encoder::Encoder};
            let mut e = Encoder::new(VecWriter::new());
            for y in 0..h {
                e.encode_line(
                    (0..w).map(|x| {
                        if (x / 7 + y / 11) % 3 == 0 {
                            Color::Black
                        } else {
                            Color::White
                        }
                    }),
                    w as u32,
                )
                .unwrap();
            }
            d.set("ColorSpace", "DeviceGray");
            d.set("BitsPerComponent", 1);
            d.set("Filter", "CCITTFaxDecode");
            d.set("DecodeParms",dictionary! {"K"=>-1,"Columns"=>w as i64,"Rows"=>h as i64,"BlackIs1"=>name=="g4-inverse"});
            e.finish().unwrap().finish()
        } else {
            let space = if name == "cmyk-icc" {
                let p = doc.add_object(Stream::new(
                    dictionary! {"N"=>4,"Alternate"=>"DeviceCMYK"},
                    profile
                        .clone()
                        .ok_or("pass CMYK ICC profile as second argument")?,
                ));
                Object::Array(vec!["ICCBased".into(), p.into()])
            } else {
                "DeviceCMYK".into()
            };
            d.set("ColorSpace", space);
            d.set("BitsPerComponent", 8);
            let mut raw = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    raw.extend([
                        (x * 200 / w) as u8,
                        (y * 220 / h) as u8,
                        70,
                        if (x / 30 + y / 30) % 2 == 0 { 20 } else { 50 },
                    ]);
                }
            }
            if name == "cmyk-tiff" || name == "cmyk-png" {
                let mut predicted = Vec::new();
                for row in raw.chunks_exact(w * 4) {
                    if name == "cmyk-png" {
                        predicted.push(1);
                    }
                    for (i, &v) in row.iter().enumerate() {
                        predicted.push(v.wrapping_sub(if i >= 4 { row[i - 4] } else { 0 }));
                    }
                }
                d.set("Filter", "FlateDecode");
                d.set("DecodeParms",dictionary! {"Predictor"=>if name=="cmyk-png" {11} else {2},"Colors"=>4,"Columns"=>w as i64,"BitsPerComponent"=>8});
                zip(&predicted)
            } else {
                raw
            }
        };
        let im = doc.add_object(Stream::new(d, bytes));
        page(
            doc,
            dictionary! {"XObject"=>dictionary! {"I"=>im}}.into(),
            b"q 324 0 0 246 18 27 cm /I Do Q".to_vec(),
            &dir.join(format!("{name}.pdf")),
        )?;
    }
    // Referenced Form graphs have equal content but different font/resource IDs.
    // Additional unused entries span every supported cleanup category.
    for name in [
        "resource-graphs",
        "resource-cleanup",
        "resource-default-colors",
        "resource-tagged",
        "resource-inherited-form",
        "zopfli-content",
    ] {
        let mut doc = Document::with_version("1.7");
        let mut r = dictionary! {};
        let mut body = Vec::new();
        let mut fonts = dictionary! {};
        for i in 0..150 {
            let f=doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica","UnusedMetadata"=>Object::string_literal(format!("unused font resource {i}"))});
            fonts.set(format!("U{i}"), f);
        }
        let f =
            doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        fonts.set("F", f);
        r.set("Font", fonts);
        let mut states = dictionary! {};
        for i in 0..80 {
            let s = doc.add_object(dictionary! {"Type"=>"ExtGState","CA"=>1,"ca"=>1,"Unused"=>i});
            states.set(format!("G{i}"), s);
        }
        r.set("ExtGState", states);
        r.set("ColorSpace", dictionary! {"Unused"=>"DeviceRGB"});
        r.set(
            "Properties",
            dictionary! {"Unused"=>dictionary! {"Lang"=>Object::string_literal("en-US")}},
        );
        let function=doc.add_object(dictionary! {"FunctionType"=>2,"Domain"=>vec![0.into(),1.into()],"C0"=>vec![0.into(),0.into(),1.into()],"C1"=>vec![1.into(),0.into(),0.into()],"N"=>1});
        let shading=doc.add_object(dictionary! {"ShadingType"=>2,"ColorSpace"=>"DeviceRGB","Coords"=>vec![0.into(),0.into(),100.into(),0.into()],"Function"=>function,"Extend"=>vec![true.into(),true.into()]});
        r.set("Shading", dictionary! {"Unused"=>shading});
        if name == "resource-graphs" || name == "resource-tagged" {
            let mut objects = dictionary! {};
            for i in 0..20 {
                let font = doc.add_object(
                    dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"},
                );
                let resources = doc.add_object(dictionary! {"Font"=>dictionary! {"F"=>font}});
                let mut d = dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),120.into(),20.into()],"Resources"=>resources};
                if name == "resource-tagged" {
                    d.set("StructParent", i);
                }
                let form = doc.add_object(Stream::new(
                    d,
                    b"BT /F 10 Tf (Reusable vector and text) Tj ET".to_vec(),
                ));
                objects.set(format!("X{i}"), form);
                body.extend(format!("q 1 0 0 1 30 {} cm /X{i} Do Q\n", 15 + i * 13).as_bytes());
            }
            r.set("XObject", objects);
        } else if name == "resource-inherited-form" {
            let form=doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),280.into(),20.into()]},b"BT /F 14 Tf (Inherited resource font) Tj ET".to_vec()));
            r.set("XObject", dictionary! {"I"=>form});
            body.extend_from_slice(b"q 1 0 0 1 30 150 cm /I Do Q");
        } else {
            body.extend_from_slice(b"BT /F 20 Tf 30 150 Td (General PDF compression) Tj ET");
        }
        if name == "resource-default-colors" {
            r.set("ColorSpace",dictionary! {"DefaultRGB"=>vec!["CalRGB".into(),dictionary! {"WhitePoint"=>vec![0.9505.into(),1.into(),1.089.into()]}.into()],"Unused"=>"DeviceRGB"});
            body.extend_from_slice(b" 0.3 0.4 0.8 rg 30 30 200 60 re f");
        }
        if name == "zopfli-content" {
            for i in 0..3000 {
                body.extend(
                    format!(
                        "\n% source object {} / repeated chunk {} and auxiliary {}",
                        i % 3451,
                        i % 123,
                        i % 851
                    )
                    .as_bytes(),
                );
            }
        }
        page(doc, r.into(), body, &dir.join(format!("{name}.pdf")))?;
    }
    Ok(())
}
