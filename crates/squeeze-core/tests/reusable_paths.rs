mod common;
use lopdf::{Document, Object, Stream, content::Content, dictionary};
use squeeze_core::{Options, Preset, compress};
use std::{fs, sync::atomic::AtomicBool};
use tempfile::tempdir;

#[test]
fn fidelity_combines_vector_compaction_and_reuse_with_full_resolution_images() {
    for jpeg in [false, true] {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        let mut f = common::fixture(jpeg);
        let mut body = String::from(
            "q 300 0 0 400 30 30 cm /Photo Do Q\nBT /F1 12 Tf 20 490 Td (Selectable text) Tj ET\n",
        );
        for n in 0..300 {
            let x = n % 20 * 14 + 30;
            let y = n / 20 * 18 + 30;
            body.push_str(&format!(
                "{x}.123456789 {y}.987654321 m {}.123456792 {y}.987654321 l {}.123456792 {}.987654321 l {x}.123456789 {}.987654321 l h f\n",
                x + 8, x + 8, y + 8, y + 8
            ));
        }
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(body.into_bytes());
        f.doc.save(&input).unwrap();
        let source = fs::read(&input).unwrap();
        let original_image = f.doc.get_object(f.image).unwrap().as_stream().unwrap();
        let original_pixels = if jpeg {
            image::load_from_memory(&original_image.content)
                .unwrap()
                .to_rgb8()
                .into_raw()
        } else {
            original_image.content.clone()
        };
        let report = compress(
            &input,
            &output,
            &Options {
                preset: Preset::Fidelity,
                reuse_paths: true,
                compact_paths: true,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(report.images[0].changed);
        assert_eq!(report.images[0].before_dimensions, [1200, 1600]);
        assert_eq!(report.images[0].after_dimensions, [1200, 1600]);
        assert!(report.warnings.iter().any(|w| w.contains("矢量轮廓")));
        assert!(report.warnings.iter().any(|w| w.contains("路径精度")));
        let result = Document::load(&output).unwrap();
        let page = String::from_utf8(result.get_page_content(f.page)).unwrap();
        assert!(page.contains(" cm /SqR"));
        assert!(
            result
                .objects
                .values()
                .any(|o| o.as_stream().ok().is_some_and(|s| s
                    .dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .ok()
                    == Some(b"Form")))
        );
        assert!(
            result
                .extract_text(&[1])
                .unwrap()
                .contains("Selectable text")
        );
        let image = result.get_object(f.image).unwrap().as_stream().unwrap();
        let pixels = image::load_from_memory(&image.content).unwrap().to_rgb8();
        assert_eq!(pixels.dimensions(), (1200, 1600));
        let mean_squared_error = original_pixels
            .iter()
            .zip(pixels.as_raw())
            .map(|(&a, &b)| {
                let delta = f64::from(a) - f64::from(b);
                delta * delta
            })
            .sum::<f64>()
            / original_pixels.len() as f64;
        assert!(
            mean_squared_error < 9.,
            "JPEG 95 fixture error: {mean_squared_error}"
        );
        assert_eq!(source, fs::read(input).unwrap());
    }
}

#[test]
fn reusable_vector_forms_keep_holes_text_shared_pages_and_resource_names() {
    let directory = tempdir().unwrap();
    let input = directory.path().join("in.pdf");
    for (enabled, preset, interactive, links) in [
        (false, Preset::Small, false, false),
        (true, Preset::Small, false, false),
        (true, Preset::Lossless, false, false),
        (true, Preset::Fidelity, false, false),
        (true, Preset::Small, true, false),
        (true, Preset::Small, false, true),
    ] {
        let mut f = common::fixture(true);
        f.doc.objects.remove(&f.image);
        f.doc
            .get_object_mut(f.resources)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .remove(b"XObject");
        let shape=f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),2.into(),2.into()],"Resources"=>dictionary!{}},b"0 0 m 1 0 l 1 1 l h f".to_vec()));
        f.doc
            .get_object_mut(f.resources)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("XObject", dictionary! {"SqR0"=>shape});
        f.doc
            .get_object_mut(f.resources)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "ExtGState",
                dictionary! {"Half"=>dictionary!{"ca"=>Object::Real(0.5)}},
            );
        let mut body = String::from("0.2 0.4 0.6 rg /Half gs\n");
        for n in 0..200 {
            let x = (n % 20 * 14 + 30) as f64;
            let y = (n / 20 * 18 + 30) as f64;
            let scale = (100 + n % 8) as f64 / 100.;
            let contours = [
                (
                    10.,
                    5.,
                    [
                        (10., 7.761, 7.761, 10., 5., 10.),
                        (2.239, 10., 0., 7.761, 0., 5.),
                        (0., 2.239, 2.239, 0., 5., 0.),
                        (7.761, 0., 10., 2.239, 10., 5.),
                    ],
                ),
                (
                    7.5,
                    5.,
                    [
                        (7.5, 3.62, 6.38, 2.5, 5., 2.5),
                        (3.62, 2.5, 2.5, 3.62, 2.5, 5.),
                        (2.5, 6.38, 3.62, 7.5, 5., 7.5),
                        (6.38, 7.5, 7.5, 6.38, 7.5, 5.),
                    ],
                ),
            ];
            for (a, b, curves) in contours {
                body.push_str(&format!("{:.3} {:.3} m\n", x + a * scale, y + b * scale));
                for (a, b, c, d, e, g) in curves {
                    body.push_str(&format!(
                        "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n",
                        x + a * scale,
                        y + b * scale,
                        x + c * scale,
                        y + d * scale,
                        x + e * scale,
                        y + g * scale
                    ));
                }
                body.push_str("h\n");
            }
        }
        body.push_str("f\nBT /F1 12 Tf 20 490 Td (Exact selectable text) Tj ET\nq 1 0 0 1 10 10 cm /SqR0 Do Q");
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(body.into_bytes());
        if interactive {
            let annotation=f.doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Text","Rect"=>vec![0.into(),0.into(),10.into(),10.into()]});
            f.doc
                .get_object_mut(f.page)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Annots", vec![Object::Reference(annotation)]);
        }
        if links {
            let annotation = f.doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Link", "Rect"=>vec![20.into(),480.into(),220.into(),500.into()], "A"=>dictionary! {"S"=>"URI", "URI"=>Object::string_literal("https://example.invalid/")}});
            f.doc
                .get_object_mut(f.page)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Annots", vec![annotation.into()]);
        }
        let mut other = f.doc.get_dictionary(f.page).unwrap().clone();
        other.set("UserUnit", 2);
        let second = f.doc.add_object(other);
        let parent = f
            .doc
            .get_dictionary(f.page)
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap();
        let tree = f.doc.get_object_mut(parent).unwrap().as_dict_mut().unwrap();
        tree.set(
            "Kids",
            vec![Object::Reference(f.page), Object::Reference(second)],
        );
        tree.set("Count", 2);
        f.doc.save(&input).unwrap();
        let before = fs::read(&input).unwrap();
        let output = directory.path().join(format!(
            "out-{enabled}-{preset:?}-{interactive}-{links}.pdf"
        ));
        let report = compress(
            &input,
            &output,
            &Options {
                preset,
                reuse_paths: enabled,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let rewritten = enabled && preset != Preset::Lossless && !interactive;
        if rewritten {
            // Each optimization creates fresh, randomly seeded shape caches.
            // Repeated exports must still serialize the same resource order.
            let expected = fs::read(&output).unwrap();
            for run in 0..4 {
                let repeated = directory
                    .path()
                    .join(format!("repeat-{preset:?}-{links}-{run}.pdf"));
                compress(
                    &input,
                    &repeated,
                    &Options {
                        preset,
                        reuse_paths: enabled,
                        ..Default::default()
                    },
                    &AtomicBool::new(false),
                    |_| {},
                )
                .unwrap();
                assert_eq!(fs::read(repeated).unwrap(), expected);
            }
        }
        assert_eq!(
            report.warnings.iter().any(|w| w.contains("矢量轮廓")),
            rewritten
        );
        let doc = Document::load(output).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
        for page in doc.get_pages().values() {
            if links {
                let source = f
                    .doc
                    .get_dictionary(f.page)
                    .unwrap()
                    .get(b"Annots")
                    .unwrap();
                assert_eq!(
                    doc.get_dictionary(*page).unwrap().get(b"Annots").unwrap(),
                    source
                );
                for link in source.as_array().unwrap() {
                    assert_eq!(
                        doc.dereference(link).unwrap().1,
                        f.doc.dereference(link).unwrap().1
                    );
                }
            }
            assert!(!doc.get_page_fonts(*page).unwrap().is_empty());
            let ops = Content::decode(&doc.get_page_content(*page))
                .unwrap()
                .operations;
            assert_eq!(
                ops.iter().filter(|o| o.operator == "Do").count(),
                if rewritten { 201 } else { 1 }
            );
            assert!(
                doc.extract_text(&[if page == &f.page { 1 } else { 2 }])
                    .unwrap()
                    .contains("Exact selectable text")
            );
            let resources = doc.get_page_resources(*page).unwrap();
            let resources = resources
                .0
                .unwrap_or_else(|| doc.get_dictionary(f.resources).unwrap());
            let objects = doc
                .dereference(resources.get(b"XObject").unwrap())
                .unwrap()
                .1
                .as_dict()
                .unwrap();
            assert_eq!(objects.get(b"SqR0").unwrap().as_reference().unwrap(), shape);
            if rewritten {
                assert_eq!(
                    objects
                        .iter()
                        .filter(|(name, _)| name.starts_with(b"SqR_"))
                        .count(),
                    8
                );
                let new = objects
                    .iter()
                    .find(|(name, _)| name.starts_with(b"SqR_"))
                    .unwrap()
                    .1
                    .as_reference()
                    .unwrap();
                let data = doc
                    .get_object(new)
                    .unwrap()
                    .as_stream()
                    .unwrap()
                    .get_plain_content()
                    .unwrap();
                assert_eq!(
                    Content::decode(&data)
                        .unwrap()
                        .operations
                        .iter()
                        .filter(|op| op.operator == "h")
                        .count(),
                    3
                );
            }
        }
        assert_eq!(fs::read(&input).unwrap(), before);
    }
}

#[test]
fn nested_transparency_forms_reuse_paths_without_changing_geometry_or_callers() {
    for preset in [Preset::Balanced, Preset::Fidelity] {
        check_nested_transparency_forms(preset);
    }
}

fn check_nested_transparency_forms(preset: Preset) {
    let dir = tempdir().unwrap();
    let input = dir.path().join("forms.pdf");
    let output = dir.path().join("out.pdf");
    let mut f = common::fixture(true);
    f.doc.objects.remove(&f.image);
    f.doc
        .get_object_mut(f.resources)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .remove(b"XObject");
    let mut body = String::from("/DeviceRGB cs 0.2 0.4 0.6 scn\n");
    for i in 0..400 {
        let x = i % 20 * 20;
        let y = i / 20 * 20;
        // Outer curved contour and oppositely wound inner hole are one fill.
        body.push_str(&format!(
            "{x} {y} m {} {y} {} {} {} {} c {x} {} l h\n",
            x + 8,
            x + 16,
            y + 8,
            x + 16,
            y + 16,
            y + 16
        ));
        body.push_str(&format!(
            "{} {} m {} {} l {} {} l h\n",
            x + 4,
            y + 4,
            x + 4,
            y + 8,
            x + 8,
            y + 8
        ));
    }
    body.push_str("f\n");
    let collision = f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),1.into(),1.into()], "Resources"=>dictionary!{}}, b"0 g 0 0 m 1 0 l 1 1 l h f".to_vec()));
    let group = dictionary! {"Type"=>"Group", "S"=>"Transparency", "I"=>true, "K"=>false};
    let matrix = vec![1.into(), 0.into(), 0.into(), 1.into(), 3.into(), 4.into()];
    let bbox = vec![0.into(), 0.into(), 400.into(), 400.into()];
    let form_resources = dictionary! {"XObject"=>dictionary!{"SqR0"=>collision}};
    body.insert_str(0, "q /SqR0 Do Q\n");
    let inner = f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "FormType"=>1, "BBox"=>bbox.clone(), "Matrix"=>matrix.clone(), "Group"=>group.clone(), "Resources"=>form_resources.clone()}, body.clone().into_bytes()));
    // The same form is used both normally and as a soft-mask group's content.
    let mask = f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>bbox.clone(), "Group"=>dictionary!{"S"=>"Transparency", "CS"=>"DeviceGray"}, "Resources"=>dictionary!{"XObject"=>dictionary!{"Inner"=>inner}}}, b"/Inner Do".to_vec()));
    let ext = dictionary! {"SMask"=>dictionary!{"S"=>"Alpha", "G"=>mask}, "ca"=>Object::Real(0.7)};
    let outer_resources =
        dictionary! {"XObject"=>dictionary!{"Inner"=>inner}, "ExtGState"=>dictionary!{"Fade"=>ext}};
    let outer_body = b"q /DeviceRGB cs /Fade gs /Inner Do Q q 1 0 0 1 5 7 cm /Inner Do Q";
    let outer = f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>bbox.clone(), "Resources"=>outer_resources.clone()}, outer_body.to_vec()));
    f.doc
        .get_object_mut(f.resources)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("XObject", dictionary! {"Outer"=>outer});
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(b"/Outer Do".to_vec());
    // Array contents were previously excluded, even though the nested form is self-contained.
    f.doc
        .get_object_mut(f.page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Contents", vec![Object::Reference(f.content)]);
    f.doc.save(&input).unwrap();
    let source = fs::read(&input).unwrap();
    let report = compress(
        &input,
        &output,
        &Options {
            preset,
            reuse_paths: true,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("矢量轮廓")));
    let result = Document::load(output).unwrap();
    assert_eq!(result.get_page_content(f.page), b"/Outer Do\n");
    assert_eq!(
        result
            .get_object(outer)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap(),
        outer_body
    );
    assert_eq!(
        result
            .get_object(outer)
            .unwrap()
            .as_stream()
            .unwrap()
            .dict
            .get(b"Resources")
            .unwrap(),
        &Object::Dictionary(outer_resources)
    );
    let rewritten = result.get_object(inner).unwrap().as_stream().unwrap();
    for (key, value) in [
        (b"Group".as_slice(), Object::Dictionary(group)),
        (b"Matrix", Object::Array(matrix)),
        (b"BBox", Object::Array(bbox)),
    ] {
        assert_eq!(rewritten.dict.get(key).unwrap(), &value);
    }
    let resources = result
        .dereference(rewritten.dict.get(b"Resources").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    let objects = result
        .dereference(resources.get(b"XObject").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    assert_eq!(objects.get(b"SqR0").unwrap(), &Object::Reference(collision));
    assert!(objects.iter().any(|(name, _)| name.starts_with(b"SqR_")));
    // Expand inserted forms and compare every original non-degenerate contour
    // in the original coordinate space, including control points and holes.
    fn contours(
        doc: &Document,
        bytes: &[u8],
        resources: &lopdf::Dictionary,
        offset: [i64; 2],
    ) -> Vec<Vec<(String, Vec<i64>)>> {
        let mut result = Vec::new();
        let mut contour = Vec::new();
        let mut shift = offset;
        let mut stack = Vec::new();
        for op in Content::decode(bytes).unwrap().operations {
            match op.operator.as_str() {
                "q" => stack.push(shift),
                "Q" => shift = stack.pop().unwrap(),
                "cm" => {
                    assert_eq!(&op.operands[..4], &[1.into(), 0.into(), 0.into(), 1.into()]);
                    shift[0] += op.operands[4].as_i64().unwrap();
                    shift[1] += op.operands[5].as_i64().unwrap();
                }
                "Do" => {
                    let objects = doc
                        .dereference(resources.get(b"XObject").unwrap())
                        .unwrap()
                        .1
                        .as_dict()
                        .unwrap();
                    let form = doc
                        .dereference(objects.get(op.operands[0].as_name().unwrap()).unwrap())
                        .unwrap()
                        .1
                        .as_stream()
                        .unwrap();
                    let resources = doc
                        .dereference(form.dict.get(b"Resources").unwrap())
                        .unwrap()
                        .1
                        .as_dict()
                        .unwrap();
                    result.extend(contours(
                        doc,
                        &form.get_plain_content().unwrap(),
                        resources,
                        shift,
                    ));
                }
                "m" | "l" | "c" | "h" => {
                    let values: Vec<_> = op
                        .operands
                        .iter()
                        .enumerate()
                        .map(|(i, n)| n.as_i64().unwrap() + shift[i % 2])
                        .collect();
                    contour.push((op.operator.clone(), values));
                    if op.operator == "h" {
                        let degenerate = contour.len() == 3 && contour[0].1 == contour[1].1;
                        if !degenerate {
                            result.push(std::mem::take(&mut contour));
                        } else {
                            contour.clear();
                        }
                    }
                }
                _ => {}
            }
        }
        result
    }
    assert_eq!(
        contours(&f.doc, body.as_bytes(), &form_resources, [0, 0]),
        contours(
            &result,
            &rewritten.get_plain_content().unwrap(),
            resources,
            [0, 0]
        )
    );
    assert_eq!(source, fs::read(input).unwrap());
}

#[test]
fn form_with_inherited_pattern_colour_is_not_factored() {
    let dir = tempdir().unwrap();
    for explicit in [false, true] {
        let mut f = common::fixture(true);
        let pattern=f.doc.add_object(Stream::new(dictionary!{"Type"=>"Pattern", "PatternType"=>1, "PaintType"=>1,"TilingType"=>1,"BBox"=>vec![0.into(),0.into(),10.into(),10.into()],"XStep"=>10,"YStep"=>10,"Resources"=>dictionary!{}},b"1 0 0 rg 0 0 10 10 re f".to_vec()));
        let mut body = if explicit {
            String::from("0 g\n")
        } else {
            String::new()
        };
        for i in 0..400 {
            let x = i % 20 * 12;
            let y = i / 20 * 12;
            body.push_str(&format!(
                "{x} {y} m {} {y} l {} {} l h\n",
                x + 8,
                x + 8,
                y + 8
            ));
        }
        body.push('f');
        let form=f.doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),360.into(),510.into()],"Resources"=>dictionary!{}},body.into_bytes()));
        let resources = f
            .doc
            .get_object_mut(f.resources)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        resources.set("Pattern", dictionary! {"Tiles"=>pattern});
        resources.set("XObject", dictionary! {"Form"=>form});
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(b"/Pattern cs /Tiles scn /Form Do".to_vec());
        let input = dir.path().join(format!("in-{explicit}.pdf"));
        let output = dir.path().join(format!("out-{explicit}.pdf"));
        f.doc.save(&input).unwrap();
        let report = compress(
            input,
            &output,
            &Options {
                preset: Preset::Balanced,
                reuse_paths: true,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            report.warnings.iter().any(|w| w.contains("矢量轮廓")),
            explicit
        );
        let result = Document::load(output).unwrap();
        let content = result
            .get_object(form)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        assert_eq!(
            Content::decode(&content)
                .unwrap()
                .operations
                .iter()
                .filter(|op| op.operator == "Do")
                .count(),
            if explicit { 400 } else { 0 }
        );
    }
}

#[test]
fn early_forms_are_revisited_after_later_forms_populate_the_shared_cache() {
    let dir = tempdir().unwrap();
    let mut f = common::fixture(true);
    fn body(repetitions: usize) -> Vec<u8> {
        let mut text = String::from("0 g\n");
        for i in 0..50 * repetitions {
            let x = i % 25 * 20;
            let y = i / 25 * 20;
            text.push_str(&format!("{x} {y} m\n"));
            let mut rng = (i % 50 + 1) as u32;
            for _ in 0..24 {
                for n in 0..6 {
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let v = rng % 10000;
                    let offset = if n % 2 == 0 { x } else { y };
                    text.push_str(&format!("{}.{:03} ", offset + v as usize / 1000, v % 1000));
                }
                text.push_str("c\n");
            }
            text.push_str("h\n");
        }
        text.push_str("f\n");
        text.into_bytes()
    }
    let mut first = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),600.into(),1000.into()],"Resources"=>dictionary!{}},
        body(1),
    );
    first.compress().unwrap();
    let early = f.doc.add_object(first);
    let mut later = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),600.into(),1000.into()],"Resources"=>dictionary!{}},
        body(20),
    );
    later.compress().unwrap();
    let later = f.doc.add_object(later);
    f.doc
        .get_object_mut(f.resources)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("XObject", dictionary! {"Early"=>early,"Later"=>later});
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(b"/Early Do /Later Do".to_vec());
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    f.doc.save(&input).unwrap();
    compress(
        input,
        &output,
        &Options {
            preset: Preset::Balanced,
            reuse_paths: true,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let result = Document::load(output).unwrap();
    for (id, count) in [(early, 50), (later, 1000)] {
        let data = result
            .get_object(id)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        assert_eq!(
            Content::decode(&data)
                .unwrap()
                .operations
                .iter()
                .filter(|op| op.operator == "Do")
                .count(),
            count
        );
    }
}
