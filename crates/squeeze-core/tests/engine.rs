mod common;
use lopdf::{Document, Object, Stream, dictionary};
use squeeze_core::{Error, Limits, Options, Outcome, Phase, Preset, compress, inspect_file};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;

#[test]
fn lossless_preserves_page_content_image_metadata_and_source() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("中文 原件.pdf");
    let output = dir.path().join("结果.pdf");
    let mut f = common::fixture(false);
    f.doc.save(&input).unwrap();
    let original = fs::read(&input).unwrap();
    let content = f.doc.get_page_content(f.page);
    let report = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::Saved);
    assert!(report.saved_bytes > 1000);
    let result = Document::load(&output).unwrap();
    assert_eq!(content, result.get_page_content(f.page));
    assert_eq!(
        f.doc
            .get_object(f.image)
            .unwrap()
            .as_stream()
            .unwrap()
            .content,
        result
            .get_object(f.image)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
    );
    assert_eq!(original, fs::read(input).unwrap());
    assert_eq!(
        result.trailer.get(b"Info").unwrap(),
        f.doc.trailer.get(b"Info").unwrap()
    );
}
#[test]
fn balanced_downsamples_rgb_and_keeps_page_dimensions() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Balanced,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let image = &report.images[0];
    assert!(image.changed);
    assert_eq!(image.after_dimensions, [834, 1112]);
    let result = Document::load(output).unwrap();
    assert_eq!(
        result
            .get_dictionary(f.page)
            .unwrap()
            .get(b"MediaBox")
            .unwrap(),
        f.doc
            .get_dictionary(f.page)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
    );
    assert!(report.reduction_percent > 80.);
}
#[test]
fn existing_jpeg_can_be_reencoded() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.images[0].changed);
    assert!(report.images[0].after_dimensions[0] < 1200);
}
#[test]
fn shared_image_uses_largest_painted_size() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    let s = f
        .doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap();
    s.set_content(b"q 100 0 0 100 0 0 cm /Photo Do Q q 600 0 0 800 0 0 cm /Photo Do Q".to_vec());
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            target_dpi: Some(150),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.images[0].after_dimensions, [1200, 1600]);
}
#[test]
fn nested_form_matrix_and_user_unit_are_accounted_for() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    let form=f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),150.into()],"Matrix"=>vec![1.5.into(),0.into(),0.into(),1.5.into(),0.into(),0.into()],"Resources"=>f.resources},b"q 100 0 0 150 0 0 cm /Photo Do Q".to_vec()));
    let resources = f
        .doc
        .add_object(dictionary! {"XObject"=>dictionary! {"Form"=>form}});
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(b"/Form Do".to_vec());
    let page = f.doc.get_object_mut(f.page).unwrap().as_dict_mut().unwrap();
    page.set("UserUnit", 2);
    page.set("Resources", resources);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            target_dpi: Some(150),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.images[0].after_dimensions, [704, 938]);
}
#[test]
fn thumbnail_reference_disables_resizing() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Thumb", f.image);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.images[0].after_dimensions, [1200, 1600]);
    assert!(!report.warnings.is_empty());
}
#[test]
fn unsupported_color_space_keeps_image_bytes() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    f.doc
        .get_object_mut(f.image)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .dict
        .set("ColorSpace", "DeviceCMYK");
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(!report.images[0].changed);
    assert!(report.images[0].reason.contains("颜色空间"));
    let result = Document::load(output).unwrap();
    assert_eq!(
        result
            .get_object(f.image)
            .unwrap()
            .as_stream()
            .unwrap()
            .content,
        f.doc
            .get_object(f.image)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
    );
}
#[test]
fn annotation_without_shared_images_allows_compression_and_is_retained() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    let annotation=f.doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Text","Rect"=>vec![1.into(),1.into(),10.into(),10.into()],"Contents"=>Object::string_literal("preserve me")});
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", vec![annotation.into()]);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.images.iter().any(|i| i.changed));
    assert_eq!(
        Document::load(output)
            .unwrap()
            .get_object(annotation)
            .unwrap(),
        f.doc.get_object(annotation).unwrap()
    );
}
#[test]
fn empty_password_encryption_is_rejected_after_automatic_decryption() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("encrypted.pdf");
    let mut f = common::fixture(true);
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
        document: &f.doc,
        owner_password: "owner",
        user_password: "",
        key_length: 128,
        permissions: lopdf::Permissions::all(),
    })
    .unwrap();
    f.doc.encrypt(&state).unwrap();
    f.doc.save(&input).unwrap();
    assert!(
        matches!(inspect_file(input,&Limits::default()),Err(Error::Unsupported(reason)) if reason.contains("加密"))
    );
}
#[test]
fn signed_and_pdfa_files_are_rejected() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("signed.pdf");
    let mut f = common::fixture(true);
    f.doc.add_object(
        dictionary! {"Type"=>"Sig","ByteRange"=>vec![0.into(),10.into(),20.into(),30.into()]},
    );
    f.doc.save(&input).unwrap();
    assert!(matches!(
        inspect_file(&input, &Limits::default()),
        Err(Error::Unsupported(_))
    ));
    let mut f = common::fixture(true);
    let meta = f.doc.add_object(Stream::new(
        dictionary! {"Type"=>"Metadata","Subtype"=>"XML"},
        b"<rdf xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"></rdf>".to_vec(),
    ));
    f.doc.catalog_mut().unwrap().set("Metadata", meta);
    f.doc.save(&input).unwrap();
    assert!(
        matches!(inspect_file(input,&Limits::default()),Err(Error::Unsupported(reason)) if reason.contains("PDF/A"))
    );
}
#[test]
fn cancellation_cleans_up_candidate() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let original = fs::read(&input).unwrap();
    let cancel = AtomicBool::new(false);
    let result = compress(&input, &output, &Options::default(), &cancel, |p| {
        if matches!(p.phase, Phase::Saving) {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!output.exists());
    assert_eq!(fs::read(input).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn existing_output_and_creation_race_never_overwrite() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    fs::write(&output, b"keep existing").unwrap();
    assert!(matches!(
        compress(
            &input,
            &output,
            &Options::default(),
            &AtomicBool::new(false),
            |_| {}
        ),
        Err(Error::OutputExists(_))
    ));
    assert_eq!(fs::read(&output).unwrap(), b"keep existing");
    fs::remove_file(&output).unwrap();
    let result = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |p| {
            if matches!(p.phase, Phase::Saving) {
                fs::write(&output, b"created during processing").unwrap();
            }
        },
    );
    assert!(matches!(result, Err(Error::OutputExists(_))));
    assert_eq!(fs::read(output).unwrap(), b"created during processing");
}
#[test]
fn file_and_pixel_limits_are_enforced() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    assert!(matches!(
        inspect_file(
            &input,
            &Limits {
                max_file_bytes: 10,
                ..Default::default()
            }
        ),
        Err(Error::Limit(_))
    ));
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Balanced,
            limits: Limits {
                max_image_pixels: 100,
                ..Default::default()
            },
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(!report.images[0].changed);
    assert!(report.images[0].reason.contains("限制"));
}
#[test]
fn dangling_references_and_page_cycles_are_rejected() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let mut f = common::fixture(true);
    f.doc
        .catalog_mut()
        .unwrap()
        .set("BadReference", Object::Reference((9999, 0)));
    f.doc.save(&input).unwrap();
    assert!(inspect_file(&input, &Limits::default()).is_err());
    let mut f = common::fixture(true);
    let root = f
        .doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    f.doc
        .get_object_mut(root)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Kids", vec![root.into()]);
    f.doc.save(&input).unwrap();
    assert!(
        matches!(inspect_file(&input,&Limits::default()),Err(Error::Invalid(reason)) if reason.contains("循环"))
    );
}

#[test]
fn changed_input_during_processing_is_not_exported() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    common::fixture(true).doc.save(&input).unwrap();
    let result = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |p| {
            if matches!(p.phase, Phase::Validating) {
                use std::io::Write;
                fs::OpenOptions::new()
                    .append(true)
                    .open(&input)
                    .unwrap()
                    .write_all(b"\n% external edit")
                    .unwrap();
            }
        },
    );
    assert!(matches!(result, Err(Error::InputChanged)));
    assert!(!output.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn larger_candidate_is_not_exported() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let page=doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()]});
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::Unchanged);
    assert!(report.candidate_bytes >= report.original_bytes);
    assert_eq!(report.result_bytes, report.original_bytes);
    assert_eq!(report.saved_bytes, 0);
    assert!(!output.exists());
}

#[test]
fn inherited_resources_and_rotation_preserve_effective_dpi() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    let pages = f
        .doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let page = f.doc.get_object_mut(f.page).unwrap().as_dict_mut().unwrap();
    page.remove(b"Resources");
    page.set("Rotate", 90);
    f.doc
        .get_object_mut(pages)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Resources", f.resources);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Balanced,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.images[0].after_dimensions, [834, 1112]);
    assert!(report.warnings.is_empty());
    assert_eq!(
        Document::load(&output)
            .unwrap()
            .get_dictionary(f.page)
            .unwrap()
            .get(b"Rotate")
            .unwrap()
            .as_i64()
            .unwrap(),
        90
    );
}

#[test]
fn flate_gray_with_png_predictor_becomes_valid_gray_jpeg() {
    use std::io::Write;
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    let mut pixels = Vec::new();
    let mut seed = 123456u32;
    for _ in 0..128 * 128 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        pixels.push((seed >> 24) as u8);
    }
    let mut encoded = Vec::new();
    for row in pixels.chunks_exact(128) {
        encoded.push(1);
        let mut previous = 0;
        for &value in row {
            encoded.push(value.wrapping_sub(previous));
            previous = value;
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&encoded).unwrap();
    f.doc.objects.insert(f.image,Object::Stream(Stream::new(dictionary! {
        "Type"=>"XObject","Subtype"=>"Image","Width"=>128,"Height"=>128,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8,
        "Filter"=>"FlateDecode","DecodeParms"=>dictionary! {"Predictor"=>11,"Colors"=>1,"Columns"=>128,"BitsPerComponent"=>8}
    },z.finish().unwrap())));
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            // Verify predictor interpretation independently of the codec's
            // low-quality preset and its aggressive high-frequency tables.
            jpeg_quality: Some(95),
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.images[0].changed);
    assert_eq!(report.images[0].after_dimensions, [128, 128]);
    let doc = Document::load(&output).unwrap();
    let image = doc.get_object(f.image).unwrap().as_stream().unwrap();
    assert!(!image.dict.has(b"DecodeParms"));
    let decoded =
        image::load_from_memory_with_format(&image.content, image::ImageFormat::Jpeg).unwrap();
    assert_eq!(decoded.color(), image::ColorType::L8);
    let error = decoded
        .to_luma8()
        .as_raw()
        .iter()
        .zip(&pixels)
        .map(|(&a, &b)| (a as f64 - b as f64).powi(2))
        .sum::<f64>()
        / (128. * 128.);
    assert!(
        error.sqrt() < 20.,
        "unexpected predictor interpretation or excessive error: {error}"
    );
}

#[test]
fn masks_form_values_bookmarks_and_attachments_are_retained() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let mask=f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1200,"Height"=>1600,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},vec![255;1200*1600]));
    f.doc
        .get_object_mut(f.image)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .dict
        .set("SMask", mask);
    let field=f.doc.add_object(dictionary! {"FT"=>"Tx","T"=>Object::string_literal("customer"),"V"=>Object::string_literal("Ada")});
    let form = f.doc.add_object(dictionary! {"Fields"=>vec![field.into()]});
    let attachment = f.doc.add_object(Stream::new(
        dictionary! {"Type"=>"EmbeddedFile"},
        b"attachment content".repeat(100),
    ));
    let file=f.doc.add_object(dictionary! {"Type"=>"Filespec","F"=>Object::string_literal("notes.txt"),"EF"=>dictionary! {"F"=>attachment}});
    let outline=f.doc.add_object(dictionary! {"Title"=>Object::string_literal("First page"),"Dest"=>vec![f.page.into(),Object::Name(b"Fit".to_vec())]});
    f.doc.catalog_mut().unwrap().set("AcroForm", form);
    f.doc.catalog_mut().unwrap().set("Names",dictionary! {"EmbeddedFiles"=>dictionary! {"Names"=>vec![Object::string_literal("notes.txt"),file.into()]}});
    f.doc.catalog_mut().unwrap().set(
        "Outlines",
        dictionary! {"First"=>outline,"Last"=>outline,"Count"=>1},
    );
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.images.iter().any(|i| i.changed));
    let result = Document::load(&output).unwrap();
    for id in [field, file, outline] {
        assert_eq!(
            result.get_object(id).unwrap(),
            f.doc.get_object(id).unwrap()
        );
    }
    let image = result.get_object(f.image).unwrap().as_stream().unwrap();
    assert_eq!(image.dict.get(b"SMask").unwrap(), &Object::Reference(mask));
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 1200);
    assert_eq!(image.dict.get(b"Height").unwrap().as_i64().unwrap(), 1600);
    let mask = result.get_object(mask).unwrap().as_stream().unwrap();
    assert_eq!(mask.get_plain_content().unwrap(), vec![255; 1200 * 1600]);
    assert_eq!(
        mask.dict
            .get(b"BitsPerComponent")
            .unwrap()
            .as_i64()
            .unwrap(),
        8
    );
    assert_eq!(
        result
            .get_object(attachment)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap(),
        b"attachment content".repeat(100)
    );
}

#[test]
fn empty_annotation_arrays_allow_image_optimization_and_raster_export() {
    for indirect in [false, true] {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let mut f = common::fixture(true);
        let empty = Object::Array(Vec::new());
        let annots = if indirect {
            Object::Reference(f.doc.add_object(empty))
        } else {
            empty
        };
        f.doc
            .get_object_mut(f.page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", annots);
        f.doc.save(&input).unwrap();
        assert!(
            !inspect_file(&input, &Limits::default())
                .unwrap()
                .has_interactive_content
        );
        for mode in [
            squeeze_core::OutputMode::Preserve,
            squeeze_core::OutputMode::Raster,
        ] {
            let out = dir.path().join(format!("{mode:?}.pdf"));
            let report = compress(
                &input,
                &out,
                &Options {
                    preset: Preset::Small,
                    output_mode: mode,
                    target_dpi: Some(72),
                    ..Default::default()
                },
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            assert_eq!(report.outcome, Outcome::Saved);
            assert!(report.images.iter().any(|i| i.changed));
            assert_eq!(Document::load(out).unwrap().get_pages().len(), 1);
        }
    }
}

#[test]
fn ordinary_links_allow_image_optimization_and_keep_targets_and_rectangles() {
    for appearance in [false, true] {
        let dir = tempdir().unwrap();
        let input = dir.path().join("links.pdf");
        let output = dir.path().join("compressed.pdf");
        let mut f = common::fixture(true);
        let mut annotation = dictionary! {"Type"=>"Annot", "Subtype"=>"Link", "Rect"=>vec![30.into(),470.into(),320.into(),490.into()], "A"=>dictionary! {"S"=>"URI", "URI"=>Object::string_literal("https://example.invalid/original-target")}};
        if appearance {
            let ap = f.doc.add_object(lopdf::Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),10.into(),10.into()]}, b"q Q".to_vec()));
            annotation.set("AP", dictionary! {"N"=>ap});
        }
        let link = f.doc.add_object(annotation.clone());
        f.doc
            .get_object_mut(f.page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", vec![link.into()]);
        f.doc.save(&input).unwrap();
        let source = fs::read(&input).unwrap();
        let report = compress(
            &input,
            &output,
            &Options {
                preset: Preset::Small,
                compact_paths: true,
                reuse_paths: true,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(report.images.iter().any(|image| image.changed));
        let doc = Document::load(output).unwrap();
        assert_eq!(doc.get_dictionary(link).unwrap(), &annotation);
        assert_eq!(fs::read(input).unwrap(), source);
    }
}

#[test]
fn icc_colour_with_soft_mask_keeps_profile_opacity_and_vector_content() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    let profile_bytes =
        b"profile bytes retained without interpreting or converting components".to_vec();
    let profile = f.doc.add_object(Stream::new(
        dictionary! {"N"=>3,"Alternate"=>"DeviceRGB"},
        profile_bytes.clone(),
    ));
    let colour = f
        .doc
        .add_object(vec![Object::Name(b"ICCBased".to_vec()), profile.into()]);
    let alpha: Vec<u8> = (0..1200 * 1600).map(|i| (i % 256) as u8).collect();
    let mask = f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1200,"Height"=>1600,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceGray"}, alpha.clone()));
    let s = f
        .doc
        .get_object_mut(f.image)
        .unwrap()
        .as_stream_mut()
        .unwrap();
    s.dict.set("ColorSpace", colour);
    s.dict.set("SMask", mask);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let img = report
        .images
        .iter()
        .find(|i| i.object == format!("{} 0", f.image.0))
        .unwrap();
    assert!(img.changed);
    assert_eq!(img.before_dimensions, img.after_dimensions);
    let out = Document::load(output).unwrap();
    let s = out.get_object(f.image).unwrap().as_stream().unwrap();
    assert_eq!(
        s.dict.get(b"ColorSpace").unwrap(),
        &Object::Reference(colour)
    );
    assert_eq!(s.dict.get(b"SMask").unwrap(), &Object::Reference(mask));
    assert_eq!(
        out.get_object(mask)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap(),
        alpha
    );
    assert_eq!(
        out.get_object(profile)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap(),
        profile_bytes
    );
    assert_eq!(out.get_page_content(f.page), f.doc.get_page_content(f.page));
    assert!(!out.get_page_fonts(f.page).unwrap().is_empty());
}

#[test]
fn complex_usage_budget_keeps_dimensions_and_still_recompresses() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(false);
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset: Preset::Small,
            limits: Limits {
                max_operations: 2,
                ..Default::default()
            },
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.images[0].changed);
    assert_eq!(report.images[0].after_dimensions, [1200, 1600]);
    assert!(report.warnings.iter().any(|w| w.contains("禁用降采样")));
}

#[test]
fn identical_streams_share_resources_without_changing_drawing_operations() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let duplicate = f.doc.add_object(f.doc.get_object(f.image).unwrap().clone());
    f.doc
        .get_object_mut(f.resources)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"XObject")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Second", duplicate);
    let mut body = b"q +0300.000000 0.000000 -0.000000 400.000000 30.000000 30.000000 cm /Photo Do Q q 30 0 0 40 0 0 cm /Second Do Q\n".to_vec();
    for _ in 0..200 {
        body.extend_from_slice(b"% 1.000000 string and comment data remains opaque\n");
    }
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(body.clone());
    f.doc.save(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("合并")));
    let out = Document::load(output).unwrap();
    let resources = out
        .get_dictionary(f.page)
        .unwrap()
        .get(b"Resources")
        .unwrap();
    let x = out
        .dereference(resources)
        .unwrap()
        .1
        .as_dict()
        .unwrap()
        .get(b"XObject")
        .unwrap()
        .as_dict()
        .unwrap();
    assert_eq!(x.get(b"Photo").unwrap(), x.get(b"Second").unwrap());
    let before = lopdf::content::Content::decode(&body).unwrap();
    let after = lopdf::content::Content::decode(&out.get_page_content(f.page)).unwrap();
    assert_eq!(before.operations.len(), after.operations.len());
    for (a, b) in before.operations.iter().zip(&after.operations) {
        assert_eq!(a.operator, b.operator);
        assert_eq!(a.operands.len(), b.operands.len());
        for (a, b) in a.operands.iter().zip(&b.operands) {
            let numeric = |o: &Object| match o {
                Object::Integer(n) => Some(*n as f64),
                Object::Real(n) => Some(*n as f64),
                _ => None,
            };
            if let Some(n) = numeric(a) {
                assert_eq!(Some(n), numeric(b));
            } else {
                assert_eq!(a, b);
            }
        }
    }
    assert!(!out.objects.contains_key(&duplicate));
}

#[test]
fn cyclic_content_arrays_fail_without_exporting() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    let cycle = f.doc.new_object_id();
    f.doc
        .objects
        .insert(cycle, Object::Array(vec![cycle.into()]));
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Contents", cycle);
    f.doc.save(&input).unwrap();
    assert!(
        compress(
            &input,
            &output,
            &Options::default(),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert!(!output.exists());
}
