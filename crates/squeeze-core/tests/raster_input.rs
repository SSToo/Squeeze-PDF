mod common;
use lopdf::{Document, Object, Stream, content::Content, dictionary};
use squeeze_core::{
    Error, Options, OutputMode, Phase, Preset, RasterManifest, compress, prepare_raster,
};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;

#[test]
fn rendering_plan_normalizes_crop_rotation_and_user_unit() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("rotated.pdf");
    let mut fixture = common::fixture(true);
    let page = fixture
        .doc
        .get_object_mut(fixture.page)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    page.set(
        "CropBox",
        vec![10.into(), 20.into(), 310.into(), 420.into()],
    );
    page.set("Rotate", 90);
    page.set("UserUnit", 2);
    fixture.doc.save(&input).unwrap();
    let options = Options {
        output_mode: OutputMode::Raster,
        preset: Preset::Balanced,
        ..Default::default()
    };
    let plan = prepare_raster(input, &options, &AtomicBool::new(false)).unwrap();
    assert_eq!(plan.pages[0].points, [800., 600.]);
    assert_eq!(plan.pages[0].dimensions, [1400, 1050]);
}

#[test]
fn staged_pixels_have_one_image_and_preserve_physical_dimensions() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let source = fs::read(&input).unwrap();
    let mut options = Options {
        output_mode: OutputMode::Raster,
        preset: Preset::Balanced,
        target_dpi: Some(72),
        ..Default::default()
    };
    let plan = prepare_raster(&input, &options, &AtomicBool::new(false)).unwrap();
    assert_eq!(plan.pages[0].dimensions, [360, 510]);
    assert_eq!(plan.pages[0].points, [360., 510.]);
    let manifest = dir.path().join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&RasterManifest {
            source_sha256: plan.source_sha256,
            pages: 1,
        })
        .unwrap(),
    )
    .unwrap();
    let mut pixels = vec![255; 360 * 510 * 3];
    pixels[0..3].copy_from_slice(&[255, 0, 0]);
    fs::write(dir.path().join("0.rgb"), pixels).unwrap();
    options.raster_manifest = Some(manifest);
    let report = compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.images[0].after_dimensions, [360, 510]);
    assert!(report.images[0].reason.contains("PDF.js"));
    let doc = Document::load(&output).unwrap();
    let id = doc.get_pages()[&1];
    assert!(doc.get_page_fonts(id).unwrap().is_empty());
    assert!(doc.extract_text(&[1]).unwrap().is_empty());
    let operations = Content::decode(&doc.get_page_content(id))
        .unwrap()
        .operations;
    assert_eq!(
        operations
            .iter()
            .map(|o| o.operator.as_str())
            .collect::<Vec<_>>(),
        vec!["q", "cm", "Do", "Q"]
    );
    assert_eq!(fs::read(input).unwrap(), source);
}

#[test]
fn staged_pixels_reject_changed_source_wrong_count_and_truncation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let mut options = Options {
        output_mode: OutputMode::Raster,
        target_dpi: Some(72),
        ..Default::default()
    };
    let plan = prepare_raster(&input, &options, &AtomicBool::new(false)).unwrap();
    let path = dir.path().join("manifest.json");
    options.raster_manifest = Some(path.clone());
    for (hash, count, changed) in [
        ("invalid", 1, true),
        (plan.source_sha256.as_str(), 2, false),
        (plan.source_sha256.as_str(), 1, false),
    ] {
        fs::write(
            &path,
            serde_json::to_vec(&RasterManifest {
                source_sha256: hash.into(),
                pages: count,
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(dir.path().join("0.rgb"), [255]).unwrap();
        let error =
            compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(if changed {
            matches!(error, Error::InputChanged)
        } else {
            matches!(error, Error::Validation(_))
        });
        assert!(!output.exists());
    }
}

#[test]
fn staged_pixels_cancellation_does_not_commit_output() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let mut options = Options {
        output_mode: OutputMode::Raster,
        target_dpi: Some(72),
        ..Default::default()
    };
    let plan = prepare_raster(&input, &options, &AtomicBool::new(false)).unwrap();
    let path = dir.path().join("manifest.json");
    fs::write(
        &path,
        serde_json::to_vec(&RasterManifest {
            source_sha256: plan.source_sha256,
            pages: 1,
        })
        .unwrap(),
    )
    .unwrap();
    options.raster_manifest = Some(path);
    let cancel = AtomicBool::new(false);
    let error = compress(&input, &output, &options, &cancel, |p| {
        if matches!(p.phase, Phase::Images) {
            cancel.store(true, Ordering::Relaxed)
        }
    })
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled));
    assert!(!output.exists());
}

#[test]
fn staged_interactive_pages_keep_all_attachment_data_and_associations() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("interactive.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let mut specs = Vec::new();
    for i in 0..3 {
        let bytes = format!("attachment {i} data").repeat(100).into_bytes();
        let data = f.doc.add_object(Stream::new(dictionary! {"Type"=>"EmbeddedFile", "Params"=>dictionary! {"Size"=>bytes.len() as i64}}, bytes));
        specs.push(f.doc.add_object(dictionary! {"Type"=>"Filespec", "F"=>Object::string_literal(format!("file-{i}.txt")), "EF"=>dictionary! {"F"=>data}, "AFRelationship"=>"Data"}));
    }
    let leaf = f.doc.add_object(
        dictionary! {"Names"=>vec![Object::string_literal("file-0.txt"), specs[0].into()]},
    );
    f.doc.catalog_mut().unwrap().set(
        "Names",
        dictionary! {"EmbeddedFiles"=>dictionary! {"Kids"=>vec![leaf.into()]}},
    );
    f.doc
        .catalog_mut()
        .unwrap()
        .set("AF", vec![specs[0].into()]);
    let note = f.doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"FileAttachment", "FS"=>specs[2], "F"=>2, "Rect"=>vec![0.into(),0.into(),20.into(),20.into()]});
    let widget = f.doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Widget", "FT"=>"Tx", "V"=>Object::string_literal("visible value"), "Rect"=>vec![0.into(),0.into(),100.into(),30.into()]});
    f.doc
        .catalog_mut()
        .unwrap()
        .set("AcroForm", dictionary! {"Fields"=>vec![widget.into()]});
    let page = f.doc.get_object_mut(f.page).unwrap().as_dict_mut().unwrap();
    page.set("AF", vec![specs[1].into()]);
    page.set("Annots", vec![note.into(), widget.into()]);
    f.doc.save(&input).unwrap();
    let source = fs::read(&input).unwrap();
    let mut options = Options {
        output_mode: OutputMode::Raster,
        target_dpi: Some(72),
        ..Default::default()
    };
    let plan = prepare_raster(&input, &options, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        plan.pages[0].annotation_ids,
        vec![format!("{}R", note.0), format!("{}R", widget.0)]
    );
    let manifest = dir.path().join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&RasterManifest {
            source_sha256: plan.source_sha256,
            pages: 1,
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(dir.path().join("0.rgb"), vec![255; 360 * 510 * 3]).unwrap();
    options.raster_manifest = Some(manifest);
    let report = compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("保留 3 个嵌入附件"))
    );
    let doc = Document::load(output).unwrap();
    let catalog = doc.catalog().unwrap();
    assert!(!catalog.has(b"AcroForm"));
    let names = catalog
        .get(b"Names")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"EmbeddedFiles")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Names")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(names.len(), 6);
    for (i, pair) in names.chunks_exact(2).enumerate() {
        assert_eq!(
            pair[0].as_str().unwrap(),
            format!("file-{i}.txt").as_bytes()
        );
        let spec = doc.dereference(&pair[1]).unwrap().1.as_dict().unwrap();
        let data = spec
            .get(b"EF")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"F")
            .unwrap();
        let data = doc
            .dereference(data)
            .unwrap()
            .1
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        assert_eq!(data, format!("attachment {i} data").repeat(100).as_bytes());
    }
    assert_eq!(catalog.get(b"AF").unwrap().as_array().unwrap()[0], names[1]);
    let page = doc.get_dictionary(doc.get_pages()[&1]).unwrap();
    assert!(!page.has(b"Annots"));
    assert_eq!(page.get(b"AF").unwrap().as_array().unwrap()[0], names[3]);
    assert_eq!(fs::read(input).unwrap(), source);
}

#[test]
fn attachment_tree_cycles_do_not_export_a_partial_pdf() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("cyclic-attachments.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let node = f.doc.new_object_id();
    f.doc.objects.insert(
        node,
        Object::Dictionary(dictionary! {"Kids"=>vec![node.into()]}),
    );
    f.doc
        .catalog_mut()
        .unwrap()
        .set("Names", dictionary! {"EmbeddedFiles"=>node});
    f.doc.save(&input).unwrap();
    let error = compress(
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
    .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert!(!output.exists());
}

#[test]
fn binary_staged_page_offers_ccitt_with_exact_pixels_and_one_image() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let mut options = Options {
        output_mode: OutputMode::Raster,
        preset: Preset::Lossless,
        target_dpi: Some(72),
        ..Default::default()
    };
    let plan = prepare_raster(&input, &options, &AtomicBool::new(false)).unwrap();
    let [w, h] = plan.pages[0].dimensions;
    let mut pixels = vec![255; (w * h * 3) as usize];
    let mut seed = 7u32;
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            if y % 25 < 10 && x % 12 < 8 && seed >> 29 < 6 {
                pixels[((y * w + x) * 3) as usize..((y * w + x) * 3 + 3) as usize].fill(0);
            }
        }
    }
    let manifest = dir.path().join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&RasterManifest {
            source_sha256: plan.source_sha256,
            pages: 1,
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(dir.path().join("0.rgb"), &pixels).unwrap();
    options.raster_manifest = Some(manifest);
    let report = compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(
        report.images[0]
            .candidates
            .iter()
            .any(|c| c.encoding == "ccitt-g4" && c.lossless)
    );
    let doc = Document::load(output).unwrap();
    let page = doc.get_pages()[&1];
    let resources = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap();
    let objects = resources.get(b"XObject").unwrap().as_dict().unwrap();
    assert_eq!(objects.len(), 1);
    if report.images[0].encoding == "ccitt-g4" {
        let id = objects.iter().next().unwrap().1.as_reference().unwrap();
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let mut decoded = Vec::new();
        fax::decoder::decode_g4(stream.content.iter().copied(), w, Some(h), |line| {
            for color in fax::decoder::pels(line, w) {
                decoded.extend([if color == fax::Color::Black { 0 } else { 255 }; 3]);
            }
        })
        .unwrap();
        assert_eq!(decoded, pixels);
    }
}
