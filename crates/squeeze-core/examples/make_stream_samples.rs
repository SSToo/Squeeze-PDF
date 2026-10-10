//! Development corpus for exact stream strategies, independent of user PDFs.
use lopdf::{Document, Object, Stream, dictionary};
use std::{env, fs, io::Write, path::Path};
fn zip(raw: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(raw).unwrap();
    e.finish().unwrap()
}
fn hex(raw: &[u8]) -> Vec<u8> {
    let mut s = String::new();
    for b in raw {
        use std::fmt::Write;
        write!(&mut s, "{b:02x}").unwrap();
    }
    s.push('>');
    s.into_bytes()
}
fn a85(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in raw.chunks(4) {
        let mut b = [0; 4];
        b[..chunk.len()].copy_from_slice(chunk);
        let mut n = u32::from_be_bytes(b);
        let mut text = [0; 5];
        for c in text.iter_mut().rev() {
            *c = (n % 85) as u8 + b'!';
            n /= 85;
        }
        out.extend_from_slice(&text[..chunk.len() + 1]);
    }
    out.extend(b"~>");
    out
}
fn literal_rle(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for c in raw.chunks(128) {
        out.push((c.len() - 1) as u8);
        out.extend(c);
    }
    out.push(128);
    out
}
fn save(
    mut doc: Document,
    chunks: Vec<Stream>,
    resources: Object,
    name: &str,
    dir: &Path,
    shared: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let tree = doc.new_object_id();
    let contents: Vec<_> = chunks
        .into_iter()
        .map(|s| Object::Reference(doc.add_object(s)))
        .collect();
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>tree,"MediaBox"=>vec![0.into(),0.into(),320.into(),240.into()],"Resources"=>resources,"Contents"=>contents.clone()});
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let mut root = dictionary! {"Type"=>"Catalog","Pages"=>tree};
    if shared {
        root.set("KeepStreams", contents);
    }
    let root = doc.add_object(root);
    doc.trailer.set("Root", root);
    doc.save(dir.join(format!("{name}.pdf")))?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = env::args().nth(1).ok_or("output directory")?;
    let dir = Path::new(&dir);
    fs::create_dir_all(dir)?;
    let text=b"q\n0.1 0.3 0.7 rg 10 20 130 60 re f\n0 0 0 rg BT /F 12 Tf 15 170 Td (Stream filters - text and vectors) Tj ET\nQ\n";
    let mut body = text.to_vec();
    let mut seed = 731u32;
    for _ in 0..400 {
        body.push(b'%');
        for _ in 0..80 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            body.push(b"0123456789abcdef"[(seed >> 28) as usize]);
        }
        body.push(b'\n');
    }
    for name in [
        "ascii85-flate-content",
        "hex-rle-flate-content",
        "lzw-early0-content",
        "lzw-early1-content",
        "indirect-filter-params-content",
    ] {
        let mut doc = Document::with_version("1.7");
        let font =
            doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        let resources = dictionary! {"Font"=>dictionary!{"F"=>font}};
        let (mut d, encoded) = if name.starts_with("lzw") {
            let early = i64::from(name.contains("early1"));
            let mut e = if early == 1 {
                weezl::encode::Encoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8)
            } else {
                weezl::encode::Encoder::new(weezl::BitOrder::Msb, 8)
            };
            (
                dictionary! {"Filter"=>"LZWDecode","DecodeParms"=>dictionary!{"EarlyChange"=>early}},
                e.encode(&body)?,
            )
        } else if name.starts_with("hex") {
            (
                dictionary! {"Filter"=>vec![Object::Name(b"ASCIIHexDecode".to_vec()),Object::Name(b"RunLengthDecode".to_vec()),Object::Name(b"FlateDecode".to_vec())],"DecodeParms"=>vec![Object::Null,Object::Null,Object::Dictionary(dictionary!{"Predictor"=>1})]},
                hex(&literal_rle(&zip(&body))),
            )
        } else {
            (
                dictionary! {"Filter"=>vec![Object::Name(b"ASCII85Decode".to_vec()),Object::Name(b"FlateDecode".to_vec())],"DecodeParms"=>vec![Object::Null,Object::Dictionary(dictionary!{"Predictor"=>1})]},
                a85(&zip(&body)),
            )
        };
        if name.starts_with("indirect") {
            let names = d.remove(b"Filter").unwrap();
            let params = d.remove(b"DecodeParms").unwrap();
            d.set("Filter", doc.add_object(names));
            d.set("DecodeParms", doc.add_object(params));
        }
        save(
            doc,
            vec![Stream::new(d, encoded)],
            resources.into(),
            name,
            dir,
            false,
        )?;
    }
    for name in [
        "short-contents-state",
        "shared-contents-fallback",
        "comment-safe-boundary",
        "inline-binary-contents",
    ] {
        let mut doc = Document::with_version("1.7");
        let font =
            doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        let resources = dictionary! {"Font"=>dictionary!{"F"=>font}};
        let parts = match name {
            "shared-contents-fallback" => vec![b"q\n".to_vec(), b"Q\n".to_vec()],
            "comment-safe-boundary" => vec![
                b"q\n% comment at boundary".to_vec(),
                b"\n0.2 0.3 0.5 rg 10 10 80 80 re f Q\n".to_vec(),
            ],
            "inline-binary-contents" => vec![
                b"q\n".to_vec(),
                b"10 0 0 10 20 20 cm BI /W 1 /H 1 /BPC 8 /CS /RGB ID \x45\x49\x8f EI Q\n".to_vec(),
            ],
            _ => {
                let mut p = vec![b"q 0.1 0.4 0.7 rg\nBT /F 12 Tf 10 230 Td\n".to_vec()];
                for i in 0..80 {
                    p.push(format!("(Row {i:02}) Tj 0 -2 Td\n").into_bytes());
                }
                p.push(b"ET Q\n".to_vec());
                p
            }
        };
        save(
            doc,
            parts
                .into_iter()
                .map(|p| Stream::new(dictionary! {}, p))
                .collect(),
            resources.into(),
            name,
            dir,
            name == "shared-contents-fallback",
        )?;
    }
    let (w, h) = (320u32, 240u32);
    let mut samples = Vec::new();
    for y in 0..h {
        for x in 0..w {
            samples.extend([
                (x * 3 + y) as u8,
                (y * 3 + x) as u8,
                if (x / 8 + y / 8) % 2 == 0 { 40 } else { 190 },
            ]);
        }
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode(
        &samples,
        w,
        h,
        image::ExtendedColorType::Rgb8,
    )?;
    for name in [
        "ascii85-jpeg-image",
        "hex-jpeg-image",
        "hex-flate-predictor-image",
        "appearance-protected-wrapper",
    ] {
        let mut doc = Document::with_version("1.7");
        let mut d = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8};
        let terminal = if name.contains("predictor") {
            "FlateDecode"
        } else {
            "DCTDecode"
        };
        let wrapper = if name.starts_with("hex") {
            "ASCIIHexDecode"
        } else {
            "ASCII85Decode"
        };
        let mut payload = jpeg.clone();
        let mut params = Object::Null;
        if name.contains("predictor") {
            let mut rows = Vec::new();
            let mut prev = vec![0u8; w as usize * 3];
            for row in samples.chunks(w as usize * 3) {
                rows.push(2);
                rows.extend(row.iter().zip(&prev).map(|(a, b)| a.wrapping_sub(*b)));
                prev.copy_from_slice(row);
            }
            payload = zip(&rows);
            params = Object::Dictionary(
                dictionary! {"Predictor"=>15,"Columns"=>w as i64,"Colors"=>3,"BitsPerComponent"=>8},
            );
        }
        d.set(
            "Filter",
            vec![
                Object::Name(wrapper.as_bytes().to_vec()),
                Object::Name(terminal.as_bytes().to_vec()),
            ],
        );
        d.set("DecodeParms", vec![Object::Null, params]);
        let encoded = if wrapper == "ASCIIHexDecode" {
            hex(&payload)
        } else {
            a85(&payload)
        };
        let image = doc.add_object(Stream::new(d, encoded));
        let resources = dictionary! {"XObject"=>dictionary!{"I"=>image}};
        let parts = vec![Stream::new(
            dictionary! {},
            b"q 320 0 0 240 0 0 cm /I Do Q\n".to_vec(),
        )];
        save(doc, parts, resources.into(), name, dir, false)?;
        if name.starts_with("appearance") {
            let path = dir.join(format!("{name}.pdf"));
            let mut doc = Document::load(&path)?;
            let appearance=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),320.into(),240.into()],"Resources"=>dictionary!{"XObject"=>dictionary!{"I"=>image}}},b"q 320 0 0 240 0 0 cm /I Do Q".to_vec()));
            let annotation=doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Stamp","Rect"=>vec![0.into(),0.into(),320.into(),240.into()],"AP"=>dictionary!{"N"=>appearance}});
            let page = doc.get_pages()[&1];
            doc.get_dictionary_mut(page)?
                .set("Annots", vec![annotation.into()]);
            doc.save(path)?;
        }
    }
    Ok(())
}
