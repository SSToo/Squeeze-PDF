mod common;
use lopdf::{Document, Object, content::Content, dictionary};
use squeeze_core::{Error, Options, Outcome, OutputMode, Phase, Preset, compress};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;

#[test]
fn raster_rejects_brotli_page_and_nested_form_instead_of_exporting_blank_pages() {
    use std::io::Write;
    for nested in [false, true] {
        for filter_array in [false, true] {
            let dir = tempdir().unwrap();
            let input = dir.path().join("in.pdf");
            let output = dir.path().join("out.pdf");
            let mut f = common::fixture(true);
            let mut stream = f
                .doc
                .get_object(f.content)
                .unwrap()
                .as_stream()
                .unwrap()
                .clone();
            let raw = stream.get_plain_content().unwrap();
            let mut bytes = Vec::new();
            {
                let mut writer = brotli::CompressorWriter::new(&mut bytes, 4096, 6, 22);
                writer.write_all(&raw).unwrap();
            }
            stream.set_content(bytes);
            let filter = Object::Name(b"BrotliDecode".to_vec());
            stream.dict.set(
                "Filter",
                if filter_array {
                    Object::Array(vec![filter])
                } else {
                    filter
                },
            );
            assert_eq!(stream.get_plain_content().unwrap(), raw);
            if nested {
                stream.dict.set("Type", "XObject");
                stream.dict.set("Subtype", "Form");
                stream
                    .dict
                    .set("BBox", vec![0.into(), 0.into(), 360.into(), 510.into()]);
                stream
                    .dict
                    .set("Resources", f.doc.get_object(f.resources).unwrap().clone());
                let form = f.doc.add_object(stream);
                f.doc
                    .get_object_mut(f.resources)
                    .unwrap()
                    .as_dict_mut()
                    .unwrap()
                    .get_mut(b"XObject")
                    .unwrap()
                    .as_dict_mut()
                    .unwrap()
                    .set("OnlyForm", form);
                f.doc
                    .get_object_mut(f.content)
                    .unwrap()
                    .as_stream_mut()
                    .unwrap()
                    .set_content(b"q /OnlyForm Do Q".to_vec());
            } else {
                f.doc.objects.insert(f.content, Object::Stream(stream));
            }
            f.doc.version = "2.0".into();
            f.doc.save(&input).unwrap();
            let before = fs::read(&input).unwrap();
            let options = Options {
                output_mode: OutputMode::Raster,
                preset: Preset::Lossless,
                target_dpi: Some(72),
                ..Default::default()
            };
            // The desktop uses hayro only to read page geometry; PDF.js renders
            // the content. This input must still be accepted for that path.
            let plan =
                squeeze_core::prepare_raster(&input, &options, &AtomicBool::new(false)).unwrap();
            assert_eq!(plan.pages[0].dimensions, [360, 510]);
            let error =
                compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap_err();
            assert!(
                matches!(error, Error::Unsupported(ref message) if message.contains("BrotliDecode"))
            );
            assert!(!output.exists());
            assert_eq!(fs::read(&input).unwrap(), before);
        }
    }
}

fn assert_image_only(doc: &Document) -> Vec<lopdf::ObjectId> {
    let mut images = Vec::new();
    for page in doc.get_pages().values() {
        let d = doc.get_dictionary(*page).unwrap();
        assert!(!d.has(b"Annots"));
        assert!(!d.has(b"Rotate"));
        assert!(doc.get_page_fonts(*page).unwrap().is_empty());
        let content = Content::decode(&doc.get_page_content(*page)).unwrap();
        assert_eq!(
            content
                .operations
                .iter()
                .map(|o| o.operator.as_str())
                .collect::<Vec<_>>(),
            vec!["q", "cm", "Do", "Q"]
        );
        let resources = d.get(b"Resources").unwrap().as_dict().unwrap();
        let x = resources.get(b"XObject").unwrap().as_dict().unwrap();
        assert_eq!(x.len(), 1);
        let id = x.get(b"PageImage").unwrap().as_reference().unwrap();
        assert_eq!(
            doc.get_object(id)
                .unwrap()
                .as_stream()
                .unwrap()
                .dict
                .get(b"Subtype")
                .unwrap()
                .as_name()
                .unwrap(),
            b"Image"
        );
        images.push(id);
    }
    assert_eq!(
        doc.extract_text(&doc.get_pages().keys().copied().collect::<Vec<_>>())
            .unwrap(),
        ""
    );
    images
}

#[test]
fn raster_replaces_text_and_vectors_with_one_image_per_page() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let s = f
        .doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap();
    s.set_content([s.content.as_slice(), b"\nq 1 0 0 rg 40 40 40 40 re f Q"].concat());
    f.doc.save(&input).unwrap();
    let before = fs::read(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            preset: Preset::Balanced,
            target_dpi: Some(72),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::Saved);
    assert_eq!(report.output_mode, OutputMode::Raster);
    assert_eq!(report.images.len(), 1);
    assert_eq!(report.images[0].after_dimensions, [360, 510]);
    assert_image_only(&Document::load(&output).unwrap());
    assert_eq!(fs::read(&input).unwrap(), before);
}

#[test]
fn raster_normalizes_rotation_crop_box_and_user_unit_and_keeps_page_order() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let original = f.doc.get_dictionary(f.page).unwrap().clone();
    let second = f.doc.add_object(original);
    let page = f.doc.get_object_mut(f.page).unwrap().as_dict_mut().unwrap();
    page.set(
        "CropBox",
        vec![30.into(), 30.into(), 330.into(), 430.into()],
    );
    page.set("Rotate", 90);
    page.set("UserUnit", 2);
    let root = f
        .doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let tree = f.doc.get_object_mut(root).unwrap().as_dict_mut().unwrap();
    tree.set("Kids", vec![f.page.into(), second.into()]);
    tree.set("Count", 2);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            preset: Preset::Small,
            target_dpi: Some(72),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.pages, 2);
    assert_eq!(report.images[0].after_dimensions, [800, 600]);
    assert_eq!(report.images[1].after_dimensions, [360, 510]);
    let doc = Document::load(&output).unwrap();
    let ids = assert_image_only(&doc);
    assert_ne!(ids[0], ids[1]);
    let pages = doc.get_pages();
    assert_eq!(
        doc.get_dictionary(pages[&1])
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap(),
        &vec![0.into(), 0.into(), 800.into(), 600.into()]
    );
}

#[test]
fn raster_exports_requested_content_mode_even_when_larger() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    f.doc.objects.remove(&f.image);
    f.doc
        .get_object_mut(f.resources)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .remove(b"XObject");
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(
            b"BT /F1 18 Tf 30 470 Td (Vector text) Tj ET q 1 0 0 rg 40 40 100 100 re f Q".to_vec(),
        );
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            preset: Preset::Lossless,
            target_dpi: Some(144),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::Saved);
    assert!(report.reduction_percent < 0.);
    assert_eq!(report.saved_bytes, 0);
    assert!(report.warnings.iter().any(|w| w.contains("比原件大")));
    let doc = Document::load(&output).unwrap();
    let images = assert_image_only(&doc);
    let image = doc.get_object(images[0]).unwrap().as_stream().unwrap();
    assert_eq!(
        image.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"FlateDecode"
    );
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 720);
    assert_eq!(image.dict.get(b"Height").unwrap().as_i64().unwrap(), 1020);
    assert!(!image.decompressed_content().unwrap().is_empty());
}

#[test]
fn raster_rejects_interactive_content_and_excessive_render_size() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let widget = f.doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Widget", "FT"=>"Tx", "V"=>Object::string_literal("visible value"), "Rect"=>vec![0.into(), 0.into(), 100.into(), 30.into()]});
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", vec![widget.into()]);
    f.doc.save(&input).unwrap();
    let options = Options {
        output_mode: OutputMode::Raster,
        preset: Preset::Balanced,
        target_dpi: Some(200),
        ..Default::default()
    };
    assert!(matches!(
        compress(&input, &output, &options, &AtomicBool::new(false), |_| {}),
        Err(Error::Unsupported(_))
    ));
    let mut f = common::fixture(true);
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set(
            "MediaBox",
            vec![0.into(), 0.into(), 10000.into(), 10000.into()],
        );
    f.doc.save(&input).unwrap();
    assert!(matches!(
        compress(&input, &output, &options, &AtomicBool::new(false), |_| {}),
        Err(Error::Limit(_))
    ));
    assert!(!output.exists());
}

#[test]
fn raster_allows_links_and_empty_form_containers() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("links.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let link = f.doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Link", "Rect"=>vec![0.into(),0.into(),100.into(),30.into()], "Border"=>vec![0.into(),0.into(),0.into()], "A"=>dictionary! {"S"=>"URI", "URI"=>Object::string_literal("https://example.invalid/")}});
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", vec![link.into()]);
    f.doc
        .catalog_mut()
        .unwrap()
        .set("AcroForm", dictionary! {"Fields"=>Vec::<Object>::new()});
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            target_dpi: Some(72),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.pages, 1);
    let doc = Document::load(output).unwrap();
    assert!(!doc.catalog().unwrap().has(b"AcroForm"));
    assert!(
        !doc.get_dictionary(doc.get_pages()[&1])
            .unwrap()
            .has(b"Annots")
    );
}

#[test]
fn raster_cancels_before_rendering_without_exporting() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let before = fs::read(&input).unwrap();
    let cancel = AtomicBool::new(false);
    let result = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            ..Default::default()
        },
        &cancel,
        |p| {
            if matches!(p.phase, Phase::Rasterizing) {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!output.exists());
    assert_eq!(fs::read(&input).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn raster_rejects_missing_nonstandard_fonts_instead_of_omitting_text() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    for object in f.doc.objects.values_mut() {
        if let Object::Dictionary(d) = object
            && d.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Font")
        {
            d.set("BaseFont", Object::Name(b"SqueezeMissingFont".to_vec()));
        }
    }
    f.doc.save(&input).unwrap();
    let result = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            target_dpi: Some(72),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(matches!(result, Err(Error::Unsupported(_))), "{result:?}");
    assert!(!output.exists());
}

#[test]
fn raster_rejects_image_decode_failure_instead_of_exporting_incomplete_page() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("corrupt-image.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    f.doc
        .get_object_mut(f.image)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(b"invalid JPEG bytes".to_vec());
    f.doc.save(&input).unwrap();
    let error = compress(
        &input,
        &output,
        &Options {
            output_mode: OutputMode::Raster,
            preset: Preset::Small,
            target_dpi: Some(72),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap_err();
    assert!(error.to_string().contains("无法完整渲染"));
    assert!(!output.exists());
}

#[test]
fn raster_presets_keep_text_pages_lossless_at_final_pixel_width() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("text.pdf");
    let mut f = common::fixture(false);
    f.doc.objects.remove(&f.image);
    f.doc
        .get_object_mut(f.resources)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .remove(b"XObject");
    f.doc.get_object_mut(f.content).unwrap().as_stream_mut().unwrap().set_content(
        b"BT /F1 9 Tf 30 470 Td (Small text 0123456789) Tj ET q 0 G 0.5 w 30 440 m 300 440 l S Q".to_vec()
    );
    f.doc.save(&input).unwrap();
    for (preset, dimensions) in [
        (Preset::Small, [1000, 1417]),
        (Preset::Balanced, [1400, 1983]),
    ] {
        let output = dir.path().join(format!("{preset:?}.pdf"));
        let report = compress(
            &input,
            &output,
            &Options {
                output_mode: OutputMode::Raster,
                preset,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.images[0].after_dimensions, dimensions);
        let doc = Document::load(output).unwrap();
        let image = doc
            .get_object(assert_image_only(&doc)[0])
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(
            image.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
        assert_eq!(
            image.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        let pixels = image.decompressed_content().unwrap();
        assert_eq!(
            pixels.len(),
            dimensions[0] as usize * dimensions[1] as usize
        );
        assert!(pixels.iter().filter(|&&v| v < 100).count() > 500);
        assert!(report.images[0].reason.contains("超采样"));
    }
}

#[test]
fn raster_supersampling_falls_back_without_changing_requested_dimensions() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let mut options = Options {
        output_mode: OutputMode::Raster,
        preset: Preset::Small,
        target_dpi: Some(72),
        ..Default::default()
    };
    options.limits.max_image_pixels = 360 * 510;
    let report = compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.images[0].after_dimensions, [360, 510]);
    assert!(!report.images[0].reason.contains("超采样"));
    assert_image_only(&Document::load(output).unwrap());
}
