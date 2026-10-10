use lopdf::{Document, Object, Stream, dictionary};
use squeeze_core::{Error, Options, Phase, Preset, compress};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;
fn doc(pages: usize) -> Document {
    let mut d = Document::with_version("1.7");
    let tree = d.new_object_id();
    let font =
        d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let mut kids = Vec::new();
    for n in 0..pages {
        let content = d.add_object(Stream::new(
            dictionary! {},
            format!(
                "BT /F 9 Tf 5 15 Td (Page {n}) Tj ET\n{}",
                "% repeated safe comment\n".repeat(200)
            )
            .into_bytes(),
        ));
        kids.push(Object::Reference(d.add_object(dictionary! {"Type"=>"Page","Parent"=>tree,"MediaBox"=>vec![0.into(),0.into(),100.into(),70.into()],"Resources"=>dictionary! {"Font"=>dictionary! {"F"=>font}},"Contents"=>content})));
    }
    d.objects.insert(
        tree,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages as i64}),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>tree});
    d.trailer.set("Root", root);
    d
}
#[test]
fn equal_tagged_streams_keep_distinct_mcr_identity_on_the_same_page() {
    let mut d = doc(1);
    let page = d.get_pages()[&1];
    let mut streams = Vec::new();
    for _ in 0..2 {
        streams.push(d.add_object(Stream::new(
            dictionary! {},
            b"/P << /MCID 0 >> BDC BT /F 9 Tf 5 15 Td (Tagged) Tj ET EMC\n".to_vec(),
        )));
    }
    let array: Vec<Object> = streams.iter().copied().map(Object::Reference).collect();
    let p = d.get_object_mut(page).unwrap().as_dict_mut().unwrap();
    p.set("Contents", array);
    p.set("StructParents", 0);
    let tree = d.new_object_id();
    let mut elements = Vec::new();
    for stream in &streams {
        elements.push(Object::Reference(d.add_object(dictionary! {"Type"=>"StructElem","S"=>"P","P"=>tree,"Pg"=>page,"K"=>dictionary! {"Type"=>"MCR","Pg"=>page,"Stm"=>*stream,"MCID"=>0}})));
    }
    d.objects.insert(
        tree,
        Object::Dictionary(dictionary! {"Type"=>"StructTreeRoot","K"=>elements}),
    );
    d.catalog_mut().unwrap().set("StructTreeRoot", tree);
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    d.save(&input).unwrap();
    let r = compress(
        &input,
        dir.path().join("out.pdf"),
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let out = Document::load(r.output.as_deref().unwrap_or(&input)).unwrap();
    let contents = out
        .get_dictionary(out.get_pages()[&1])
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(contents.len(), 2);
    assert_ne!(contents[0], contents[1]);
    let tree = out
        .dereference(out.catalog().unwrap().get(b"StructTreeRoot").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    for (i, element) in tree
        .get(b"K")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mcr = out
            .dereference(element)
            .unwrap()
            .1
            .as_dict()
            .unwrap()
            .get(b"K")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(mcr.get(b"Stm").unwrap(), &contents[i]);
        assert_eq!(mcr.get(b"MCID").unwrap().as_i64().unwrap(), 0);
    }
}
#[test]
fn independent_appearances_are_not_aliased_and_field_values_survive() {
    let mut d = doc(1);
    let page = d.get_pages()[&1];
    let mut fields = Vec::new();
    for name in ["First", "Second"] {
        let appearance=d.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),20.into(),10.into()]},b"0.2 0.4 0.6 rg 0 0 20 10 re f".to_vec()));
        fields.push(Object::Reference(d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx","T"=>Object::string_literal(name),"V"=>Object::string_literal("Value"),"Rect"=>vec![0.into(),0.into(),20.into(),10.into()],"P"=>page,"AP"=>dictionary! {"N"=>appearance}})));
    }
    d.get_object_mut(page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", fields.clone());
    d.catalog_mut()
        .unwrap()
        .set("AcroForm", dictionary! {"Fields"=>fields});
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    d.save(&input).unwrap();
    let r = compress(
        &input,
        dir.path().join("out.pdf"),
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let out = Document::load(r.output.as_deref().unwrap_or(&input)).unwrap();
    let acro = out
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_dict()
        .unwrap();
    let fields = acro.get(b"Fields").unwrap().as_array().unwrap();
    let mut identities = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let field = out.dereference(field).unwrap().1.as_dict().unwrap();
        assert_eq!(field.get(b"V").unwrap().as_str().unwrap(), b"Value");
        assert_eq!(
            field.get(b"T").unwrap().as_str().unwrap(),
            if i == 0 {
                b"First".as_slice()
            } else {
                b"Second".as_slice()
            }
        );
        identities.push(
            field
                .get(b"AP")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"N")
                .unwrap(),
        );
    }
    assert_ne!(identities[0], identities[1]);
}
#[test]
fn all_one_hundred_pages_and_final_image_diagnostics_are_verified() {
    let mut d = doc(100);
    let rgb: Vec<u8> = (0..200)
        .flat_map(|y| (0..300).flat_map(move |x| [(x % 256) as u8, y as u8, ((x + y) % 256) as u8]))
        .collect();
    let image=d.add_object(Stream::new(dictionary! {"Subtype"=>"Image","Width"=>300,"Height"=>200,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB"},rgb.clone()));
    for page in d.get_pages().values() {
        let p = d.get_dictionary(*page).unwrap();
        let contents = p.get(b"Contents").unwrap().as_reference().unwrap();
        let s = d.get_object_mut(contents).unwrap().as_stream_mut().unwrap();
        let mut content = s.content.clone();
        content.extend_from_slice(b"\nq 90 0 0 40 5 25 cm /I Do Q");
        s.set_content(content);
        d.get_object_mut(*page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .get_mut(b"Resources")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("XObject", dictionary! {"I"=>image});
    }
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    d.save(&input).unwrap();
    let original = fs::read(&input).unwrap();
    let r = compress(
        &input,
        dir.path().join("out.pdf"),
        &Options {
            preset: Preset::Fidelity,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.pages, 100);
    assert!(r.image_optimization.selected);
    assert!(r.result_bytes <= r.image_optimization.baseline_bytes);
    let out = Document::load(r.output.unwrap()).unwrap();
    for page in 1..=100 {
        assert_eq!(
            out.extract_text(&[page]).unwrap(),
            d.extract_text(&[page]).unwrap()
        );
        let p = out.get_dictionary(out.get_pages()[&page]).unwrap();
        let image = out
            .dereference(p.get(b"Resources").unwrap())
            .unwrap()
            .1
            .as_dict()
            .unwrap()
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"I")
            .unwrap();
        let stream = out.dereference(image).unwrap().1.as_stream().unwrap();
        assert_eq!(stream.decompressed_content().unwrap(), rgb);
    }
    assert_eq!(r.output_images.len(), 1);
    assert_eq!(r.output_images[0].dimensions, [300, 200]);
    assert_eq!(
        r.output_images[0].bytes as u64,
        r.size_after.images + r.size_after.masks
    );
    assert_eq!(fs::read(input).unwrap(), original);
}
#[test]
fn cancellation_in_the_second_pipeline_cleans_up_both_candidates() {
    let mut d = doc(1);
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    d.save(&input).unwrap();
    let cancel = AtomicBool::new(false);
    let mut saves = 0;
    let result = compress(
        &input,
        dir.path().join("out.pdf"),
        &Options::default(),
        &cancel,
        |p| {
            if matches!(p.phase, Phase::Saving) {
                saves += 1;
                if saves == 2 {
                    cancel.store(true, Ordering::Relaxed)
                }
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn strict_lossless_retains_jpeg_coefficients_instead_of_freezing_decoder_pixels() {
    use image::{DynamicImage, GrayImage, codecs::jpeg::JpegEncoder};
    let pixels = GrayImage::from_fn(160, 120, |x, y| {
        image::Luma([((x * 7 + y * 13) % 256) as u8])
    });
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 92)
        .encode_image(&DynamicImage::ImageLuma8(pixels))
        .unwrap();
    for wrapped in [false, true] {
        let mut d = doc(1);
        let page = d.get_pages()[&1];
        let mut dict = dictionary! {"Subtype"=>"Image","Width"=>160,"Height"=>120,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8};
        let (filters, bytes) = if wrapped {
            (
                vec![
                    Object::Name(b"ASCIIHexDecode".to_vec()),
                    Object::Name(b"DCTDecode".to_vec()),
                ],
                jpeg.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
                    .into_bytes()
                    .into_iter()
                    .chain([b'>'])
                    .collect::<Vec<u8>>(),
            )
        } else {
            (vec![Object::Name(b"DCTDecode".to_vec())], jpeg.clone())
        };
        dict.set("Filter", filters);
        let image = d.add_object(Stream::new(dict, bytes));
        let p = d.get_object_mut(page).unwrap().as_dict_mut().unwrap();
        p.get_mut(b"Resources")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("XObject", dictionary! {"I"=>image});
        let contents = p.get(b"Contents").unwrap().as_reference().unwrap();
        let s = d.get_object_mut(contents).unwrap().as_stream_mut().unwrap();
        let mut data = s.content.clone();
        data.extend_from_slice(b"\nq 90 0 0 40 5 25 cm /I Do Q");
        s.set_content(data);
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        d.save(&input).unwrap();
        let r = compress(
            &input,
            dir.path().join("out.pdf"),
            &Options::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(
            r.output_images
                .iter()
                .all(|i| i.filters.last().map(String::as_str) == Some("DCTDecode"))
        );
        assert!(
            r.images
                .iter()
                .flat_map(|i| &i.candidates)
                .all(|c| c.encoding == "original" || c.encoding.starts_with("jpeg-lossless/"))
        );
        assert_eq!(r.rgb_flate.attempted, 0);
    }
}
