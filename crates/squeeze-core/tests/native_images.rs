use flate2::{Compression, write::ZlibEncoder};
use lopdf::{Document, Object, Stream, dictionary};
use squeeze_core::{Options, Preset, compress};
use std::{io::Write, sync::atomic::AtomicBool};
use tempfile::tempdir;

fn source(
    space: Object,
    bits: usize,
    channels: usize,
    decode: Option<Object>,
) -> (Document, Vec<u8>) {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let (w, h) = (97usize, 61usize);
    let mut raw = Vec::new();
    for y in 0..h {
        for x in 0..w {
            for c in 0..channels {
                let n = (x * 173 + y * 89 + c * 37) as u16;
                if bits == 16 {
                    raw.extend(n.to_be_bytes());
                } else {
                    raw.push(n as u8);
                }
            }
        }
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw).unwrap();
    let mut dict = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64,"BitsPerComponent"=>bits as i64,"ColorSpace"=>space,"Filter"=>"FlateDecode"};
    if let Some(decode) = decode {
        dict.set("Decode", decode);
    }
    let image = doc.add_object(Stream::new(dict, encoder.finish().unwrap()));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 97 0 0 61 0 0 cm /Image Do Q\n".repeat(30),
    ));
    let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),97.into(),61.into()],"Contents"=>content,"Resources"=>dictionary! {"XObject"=>dictionary! {"Image"=>image}}});
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    (doc, raw)
}
fn image(doc: &Document) -> &Stream {
    let page = *doc.get_pages().values().next().unwrap();
    let resources = doc
        .dereference(doc.get_dictionary(page).unwrap().get(b"Resources").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    let images = doc
        .dereference(resources.get(b"XObject").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    doc.dereference(images.get(b"Image").unwrap())
        .unwrap()
        .1
        .as_stream()
        .unwrap()
}
#[test]
fn device_defaults_in_inline_or_indirect_resources_keep_explicit_decode() {
    for preset in [Preset::Lossless, Preset::Fidelity] {
        for indirect in [false, true] {
            let dir = tempdir().unwrap();
            let input = dir.path().join("in.pdf");
            let output = dir.path().join("out.pdf");
            let decode: Object =
                vec![0.into(), 1.into(), 0.into(), 1.into(), 0.into(), 1.into()].into();
            let (mut doc, raw) = source("DeviceRGB".into(), 16, 3, Some(decode.clone()));
            let page = *doc.get_pages().values().next().unwrap();
            let defaults: Object = dictionary! {"DefaultRGB"=>vec![Object::Name(b"CalRGB".to_vec()),dictionary! {"WhitePoint"=>vec![1.into(),1.into(),1.into()]}.into()]}.into();
            let defaults = if indirect {
                doc.add_object(defaults).into()
            } else {
                defaults
            };
            let resources = doc
                .get_dictionary_mut(page)
                .unwrap()
                .get_mut(b"Resources")
                .unwrap()
                .as_dict_mut()
                .unwrap();
            resources.set("ColorSpace", defaults);
            doc.save(&input).unwrap();
            let original = std::fs::read(&input).unwrap();
            let report = compress(
                &input,
                &output,
                &Options {
                    preset,
                    ..Default::default()
                },
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            assert!(report.images[0].encoding.starts_with("native-flate/"));
            assert!(
                !report.images[0]
                    .candidates
                    .iter()
                    .any(|c| c.encoding == "default-decode")
            );
            let result = Document::load(output).unwrap();
            let result_image = image(&result);
            assert_eq!(result_image.dict.get(b"Decode").unwrap(), &decode);
            assert_eq!(result_image.get_plain_content().unwrap(), raw);
            assert_eq!(std::fs::read(input).unwrap(), original);
        }
    }
}
#[test]
fn native_spaces_and_16_bit_data_keep_components_mapping_and_dimensions_end_to_end() {
    for (space, channels, bits) in [
        (Object::Name(b"DeviceRGB".to_vec()), 3, 16),
        (Object::Name(b"DeviceGray".to_vec()), 1, 16),
        (Object::Name(b"DeviceCMYK".to_vec()), 4, 8),
        (
            Object::Array(vec![
                "Lab".into(),
                dictionary! {"WhitePoint"=>vec![Object::Real(0.9505),1.into(),Object::Real(1.089)]}
                    .into(),
            ]),
            3,
            8,
        ),
        (
            Object::Array(vec![
                "CalRGB".into(),
                dictionary! {"WhitePoint"=>vec![1.into(),1.into(),1.into()]}.into(),
            ]),
            3,
            8,
        ),
        (
            Object::Array(vec![
                "CalGray".into(),
                dictionary! {"WhitePoint"=>vec![1.into(),1.into(),1.into()]}.into(),
            ]),
            1,
            8,
        ),
    ] {
        for preset in [Preset::Lossless, Preset::Fidelity] {
            let dir = tempdir().unwrap();
            let input = dir.path().join("in.pdf");
            let output = dir.path().join("out.pdf");
            let decode = Object::Array((0..channels).flat_map(|_| [1.into(), 0.into()]).collect());
            let (mut doc, raw) = source(space.clone(), bits, channels, Some(decode.clone()));
            doc.save(&input).unwrap();
            let original = std::fs::read(&input).unwrap();
            let report = compress(
                &input,
                &output,
                &Options {
                    preset,
                    ..Default::default()
                },
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            assert!(report.result_bytes < report.original_bytes);
            assert!(report.images[0].encoding.starts_with("native-flate/"));
            assert!(report.images[0].candidates.iter().all(|c| c.lossless));
            let result = Document::load(&output).unwrap();
            let s = image(&result);
            assert_eq!(s.dict.get(b"ColorSpace").unwrap(), &space);
            assert_eq!(s.dict.get(b"Decode").unwrap(), &decode);
            assert_eq!(
                s.dict.get(b"BitsPerComponent").unwrap().as_i64().unwrap(),
                bits as i64
            );
            assert_eq!(s.decompressed_content().unwrap(), raw);
            assert_eq!(
                report.images[0].before_dimensions,
                report.images[0].after_dimensions
            );
            assert_eq!(std::fs::read(&input).unwrap(), original);
        }
    }
}
#[test]
fn indirect_dimensions_and_default_decode_unlock_exact_lossless_candidates() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let (mut doc, raw) = source("DeviceRGB".into(), 16, 3, None);
    let width = doc.add_object(97);
    let height = doc.add_object(61);
    let decode = doc.add_object(vec![
        0.into(),
        1.into(),
        0.into(),
        1.into(),
        0.into(),
        1.into(),
    ]);
    let id = doc
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_stream().is_ok_and(|s| {
                s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
            })
        })
        .map(|(&id, _)| id)
        .unwrap();
    let s = doc.get_object_mut(id).unwrap().as_stream_mut().unwrap();
    s.dict.set("Width", width);
    s.dict.set("Height", height);
    s.dict.set("Decode", decode);
    doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.images[0].after_dimensions, [97, 61]);
    let result = Document::load(output).unwrap();
    let s = image(&result);
    assert!(!s.dict.has(b"Decode"));
    assert_eq!(s.decompressed_content().unwrap(), raw);
}

#[test]
fn appearance_shared_native_images_keep_original_bytes_and_default_mapping() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let (mut doc, _) = source(
        "DeviceRGB".into(),
        16,
        3,
        Some(Object::Array(vec![
            0.into(),
            1.into(),
            0.into(),
            1.into(),
            0.into(),
            1.into(),
        ])),
    );
    let page = *doc.get_pages().values().next().unwrap();
    let image_id = doc
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_stream().is_ok_and(|s| {
                s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
            })
        })
        .map(|(&id, _)| id)
        .unwrap();
    let original_stream = doc
        .get_object(image_id)
        .unwrap()
        .as_stream()
        .unwrap()
        .clone();
    let ap=doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),97.into(),61.into()],"Resources"=>dictionary! {"XObject"=>dictionary! {"Image"=>image_id}}},b"q 97 0 0 61 0 0 cm /Image Do Q".to_vec()));
    let field=doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx","T"=>Object::string_literal("Editable"),"V"=>Object::string_literal("preserve me"),"Rect"=>vec![0.into(),0.into(),97.into(),61.into()],"AP"=>dictionary! {"N"=>ap}});
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Annots", vec![field.into()]);
    doc.catalog_mut()
        .unwrap()
        .set("AcroForm", dictionary! {"Fields"=>vec![field.into()]});
    doc.save(&input).unwrap();
    let source_bytes = std::fs::read(&input).unwrap();
    for preset in [Preset::Lossless, Preset::Fidelity] {
        let out = output.with_file_name(format!("{preset:?}.pdf"));
        let report = compress(
            &input,
            &out,
            &Options {
                preset,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(!report.images[0].changed);
        let result = Document::load(report.output.as_deref().unwrap_or(&input)).unwrap();
        assert_eq!(image(&result).content, original_stream.content);
        assert_eq!(
            image(&result).dict.get(b"Decode").unwrap(),
            original_stream.dict.get(b"Decode").unwrap()
        );
        let form = result
            .catalog()
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_dict()
            .unwrap();
        let fields = form.get(b"Fields").unwrap().as_array().unwrap();
        let field = result.dereference(&fields[0]).unwrap().1.as_dict().unwrap();
        assert_eq!(field.get(b"V").unwrap().as_str().unwrap(), b"preserve me");
        assert_eq!(std::fs::read(&input).unwrap(), source_bytes);
    }
}
