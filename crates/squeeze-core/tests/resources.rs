mod common;
use lopdf::{Document, Stream, dictionary};
use squeeze_core::{Limits, Options, Preset, compress};
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;

fn run(f: &mut common::Fixture, options: Options) -> (Document, squeeze_core::Report) {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    f.doc.save(&input).unwrap();
    let source = std::fs::read(&input).unwrap();
    let report = compress(&input, &output, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(source, std::fs::read(&input).unwrap());
    (Document::load(output).unwrap(), report)
}
fn objects(doc: &Document, page: lopdf::ObjectId) -> &lopdf::Dictionary {
    let r = doc
        .dereference(doc.get_dictionary(page).unwrap().get(b"Resources").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    doc.dereference(r.get(b"XObject").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap()
}
#[test]
fn unused_entries_are_removed_per_owner_with_inherited_resources_and_content_arrays() {
    let mut f = common::fixture(true);
    let second = f.doc.add_object(f.doc.get_object(f.image).unwrap().clone());
    f.doc
        .get_object_mut(second)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .dict
        .set("Interpolate", true);
    f.doc
        .get_dictionary_mut(f.resources)
        .unwrap()
        .get_mut(b"XObject")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Second", second);
    let first = f.doc.add_object(Stream::new(
        dictionary! {},
        b"q 300 0 0 400 0 0 cm".to_vec(),
    ));
    let last = f
        .doc
        .add_object(Stream::new(dictionary! {}, b"/Photo Do Q".to_vec()));
    let tree = f
        .doc
        .get_dictionary(f.page)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    f.doc
        .get_dictionary_mut(f.page)
        .unwrap()
        .remove(b"Resources");
    f.doc
        .get_dictionary_mut(tree)
        .unwrap()
        .set("Resources", f.resources);
    f.doc
        .get_dictionary_mut(f.page)
        .unwrap()
        .set("Contents", vec![first.into(), last.into()]);
    let body = f.doc.add_object(Stream::new(
        dictionary! {},
        b"q 300 0 0 400 0 0 cm /Second Do Q".to_vec(),
    ));
    let page2=f.doc.add_object(dictionary!{"Type"=>"Page","Parent"=>tree,"MediaBox"=>vec![0.into(),0.into(),360.into(),510.into()],"Contents"=>body});
    f.doc
        .get_dictionary_mut(tree)
        .unwrap()
        .set("Kids", vec![f.page.into(), page2.into()]);
    f.doc.get_dictionary_mut(tree).unwrap().set("Count", 2);
    let (out, report) = run(&mut f, Options::default());
    assert!(report.warnings.iter().any(|w| w.contains("未使用")));
    assert!(objects(&out, f.page).has(b"Photo"));
    assert!(!objects(&out, f.page).has(b"Second"));
    assert!(objects(&out, page2).has(b"Second"));
    assert!(!objects(&out, page2).has(b"Photo"));
    assert_eq!(out.get_page_content(f.page), f.doc.get_page_content(f.page));
}
#[test]
fn forms_without_resources_keep_their_inherited_image_names() {
    let mut f = common::fixture(true);
    let form=f.doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),1.into(),1.into()]},b"/Photo Do".to_vec()));
    f.doc
        .get_dictionary_mut(f.resources)
        .unwrap()
        .get_mut(b"XObject")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("F", form);
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(b"q 300 0 0 400 0 0 cm /F Do Q".to_vec());
    let (out, _) = run(&mut f, Options::default());
    assert!(objects(&out, f.page).has(b"F"));
    assert!(objects(&out, f.page).has(b"Photo"));
}
#[test]
fn opaque_inline_content_and_budget_failure_do_not_remove_resources() {
    for body in [
        b"q BI /W 1 /H 1 /BPC 8 /CS /RGB ID abc EI Q".as_slice(),
        b"q /Photo Do Q unsupported",
    ] {
        let mut f = common::fixture(true);
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(body.to_vec());
        let (out, _) = run(
            &mut f,
            Options {
                limits: Limits {
                    max_operations: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(objects(&out, f.page).has(b"Photo"));
    }
}
#[test]
fn decoded_image_sharing_keeps_distinct_color_transparency_and_tagged_identity() {
    for distinct in ["same", "interpolate", "color", "tag", "decode", "mask"] {
        let mut f = common::fixture(false);
        let mut s = f
            .doc
            .get_object(f.image)
            .unwrap()
            .as_stream()
            .unwrap()
            .clone();
        let encoded = s.content.clone();
        s.dict.set("Filter", "FlateDecode");
        s.set_content({
            use std::io::Write;
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            z.write_all(&encoded).unwrap();
            z.finish().unwrap()
        });
        match distinct {
            "interpolate" => s.dict.set("Interpolate", true),
            "color" => s.dict.set("ColorSpace", "DeviceGray"),
            "tag" => s.dict.set("StructParent", 2),
            "decode" => s.dict.set(
                "Decode",
                vec![0.into(), 1.into(), 0.into(), 1.into(), 0.into(), 1.into()],
            ),
            "mask" => s.dict.set(
                "Mask",
                vec![0.into(), 0.into(), 0.into(), 0.into(), 0.into(), 0.into()],
            ),
            _ => {}
        }
        let second = f.doc.add_object(s);
        f.doc
            .get_dictionary_mut(f.resources)
            .unwrap()
            .get_mut(b"XObject")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Second", second);
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(
                b"q 300 0 0 400 0 0 cm /Photo Do Q q 300 0 0 400 0 0 cm /Second Do Q".to_vec(),
            );
        let (out, report) = run(&mut f, Options::default());
        let x = objects(&out, f.page);
        assert_eq!(
            x.get(b"Photo").unwrap() == x.get(b"Second").unwrap(),
            distinct == "same",
            "{distinct}"
        );
        if distinct == "same" {
            assert_eq!(x.get(b"Photo").unwrap().as_reference().unwrap(), f.image);
            assert_eq!(
                out.get_object(f.image)
                    .unwrap()
                    .as_stream()
                    .unwrap()
                    .content,
                f.doc
                    .get_object(second)
                    .unwrap()
                    .as_stream()
                    .unwrap()
                    .content
            );
            assert!(
                report
                    .stages
                    .iter()
                    .any(|s| s.stage == "decoded_image_sharing"
                        && s.after_objects < s.before_objects)
            );
        }
    }
}
#[test]
fn shared_appearance_images_are_preserved_but_unrelated_page_images_compress() {
    let mut f = common::fixture(false);
    let protected = f.doc.add_object(f.doc.get_object(f.image).unwrap().clone());
    // Different original pixels prevent the exact sharing pass from combining them.
    f.doc
        .get_object_mut(protected)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .content[0] = 255;
    f.doc
        .get_dictionary_mut(f.resources)
        .unwrap()
        .get_mut(b"XObject")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Shared", protected);
    let appearance=f.doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),10.into(),10.into()],"Resources"=>dictionary!{"XObject"=>dictionary!{"I"=>protected}}},b"q 10 0 0 10 0 0 cm /I Do Q".to_vec()));
    let ann=f.doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Stamp","Rect"=>vec![0.into(),0.into(),10.into(),10.into()],"AP"=>dictionary!{"N"=>appearance}});
    f.doc
        .get_dictionary_mut(f.page)
        .unwrap()
        .set("Annots", vec![ann.into()]);
    let (out, report) = run(
        &mut f,
        Options {
            preset: Preset::Small,
            ..Default::default()
        },
    );
    assert!(
        report
            .images
            .iter()
            .find(|i| i.object == format!("{} 0", f.image.0))
            .unwrap()
            .after_dimensions[0]
            < 1200
    );
    let p = report
        .images
        .iter()
        .find(|i| i.object == format!("{} 0", protected.0))
        .unwrap();
    assert!(!p.changed);
    assert_eq!(
        out.get_object(protected)
            .unwrap()
            .as_stream()
            .unwrap()
            .content,
        f.doc
            .get_object(protected)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
    );
    assert_eq!(
        out.get_dictionary(ann).unwrap(),
        f.doc.get_dictionary(ann).unwrap()
    );
}
#[test]
fn unrelated_pattern_no_longer_disables_page_image_downsampling() {
    let mut f = common::fixture(false);
    let other = f.doc.add_object(f.doc.get_object(f.image).unwrap().clone());
    f.doc
        .get_object_mut(other)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .content[0] = 255;
    let pattern=f.doc.add_object(Stream::new(dictionary!{"Type"=>"Pattern","PatternType"=>1,"PaintType"=>1,"TilingType"=>1,"BBox"=>vec![0.into(),0.into(),10.into(),10.into()],"XStep"=>10,"YStep"=>10,"Resources"=>dictionary!{"XObject"=>dictionary!{"I"=>other}}},b"/I Do".to_vec()));
    f.doc
        .get_dictionary_mut(f.resources)
        .unwrap()
        .set("Pattern", dictionary! {"P"=>pattern});
    let (out, report) = run(
        &mut f,
        Options {
            preset: Preset::Small,
            ..Default::default()
        },
    );
    let photo = report
        .images
        .iter()
        .find(|i| i.object == format!("{} 0", f.image.0))
        .unwrap();
    assert!(photo.after_dimensions[0] < 1200);
    let image = out.get_object(other).unwrap().as_stream().unwrap();
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 1200);
    assert_eq!(image.dict.get(b"Height").unwrap().as_i64().unwrap(), 1600);
    assert!(
        report
            .images
            .iter()
            .find(|i| i.object == format!("{} 0", other.0))
            .unwrap()
            .changed
    );
}

#[test]
fn all_known_resource_categories_are_removed_only_when_unreferenced() {
    let mut f = common::fixture(true);
    let font = f
        .doc
        .add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Courier"});
    let state = f
        .doc
        .add_object(dictionary! {"Type"=>"ExtGState","CA"=>1,"ca"=>1});
    let shading = f
        .doc
        .add_object(dictionary! {"ShadingType"=>2,"ColorSpace"=>"DeviceRGB"});
    let r = f.doc.get_dictionary_mut(f.resources).unwrap();
    r.get_mut(b"Font")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Unused", font);
    r.set("ExtGState", dictionary! {"Used"=>state,"Unused"=>state});
    r.set("Shading", dictionary! {"Used"=>shading,"Unused"=>shading});
    r.set(
        "Properties",
        dictionary! {"Used"=>dictionary! {"Lang"=>"en"},"Unused"=>dictionary! {"Lang"=>"fr"}},
    );
    r.set(
        "ColorSpace",
        dictionary! {"DefaultRGB"=>"DeviceRGB","Unused"=>"DeviceGray"},
    );
    let mut body = f
        .doc
        .get_object(f.content)
        .unwrap()
        .as_stream()
        .unwrap()
        .content
        .clone();
    body.extend_from_slice(b" /Used gs /Span /Used BDC EMC /Used sh");
    f.doc
        .get_object_mut(f.content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(body);
    let (out, _) = run(&mut f, Options::default());
    let r = out
        .dereference(
            out.get_dictionary(f.page)
                .unwrap()
                .get(b"Resources")
                .unwrap(),
        )
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    for key in [b"Font".as_slice(), b"ExtGState", b"Shading", b"Properties"] {
        assert!(
            !out.dereference(r.get(key).unwrap())
                .unwrap()
                .1
                .as_dict()
                .unwrap()
                .has(b"Unused")
        );
    }
    // XObjects and shading may indirectly select a named colour space.
    assert!(
        r.get(b"ColorSpace")
            .unwrap()
            .as_dict()
            .unwrap()
            .has(b"DefaultRGB")
    );
    assert!(
        r.get(b"ColorSpace")
            .unwrap()
            .as_dict()
            .unwrap()
            .has(b"Unused")
    );
}

#[test]
fn unused_colors_drop_only_without_direct_or_implicit_defaults() {
    for defaults in [false, true] {
        let mut f = common::fixture(true);
        let mut colors = dictionary! {"Unused"=>"DeviceGray"};
        if defaults {
            colors.set("DefaultRGB", "Indirect");
            colors.set("Indirect", "DeviceRGB");
        }
        f.doc
            .get_dictionary_mut(f.resources)
            .unwrap()
            .set("ColorSpace", colors);
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(b"BT /F1 12 Tf (Text) Tj ET".to_vec());
        let (out, _) = run(&mut f, Options::default());
        let r = out
            .dereference(
                out.get_dictionary(f.page)
                    .unwrap()
                    .get(b"Resources")
                    .unwrap(),
            )
            .unwrap()
            .1
            .as_dict()
            .unwrap();
        let c = r.get(b"ColorSpace").unwrap().as_dict().unwrap();
        assert_eq!(c.has(b"Unused"), defaults);
        if defaults {
            assert!(c.has(b"DefaultRGB") && c.has(b"Indirect"));
        }
    }
}

#[test]
fn state_selected_type3_and_named_colors_keep_indirect_resources() {
    for kind in ["type3", "color"] {
        let mut f = common::fixture(true);
        if kind == "type3" {
            let font = f.doc.add_object(
                dictionary! {"Type"=>"Font","Subtype"=>"Type3","CharProcs"=>dictionary! {}},
            );
            let state = f
                .doc
                .add_object(dictionary! {"Type"=>"ExtGState","Font"=>vec![font.into(),12.into()]});
            f.doc
                .get_dictionary_mut(f.resources)
                .unwrap()
                .set("ExtGState", dictionary! {"G"=>state});
            f.doc
                .get_object_mut(f.content)
                .unwrap()
                .as_stream_mut()
                .unwrap()
                .set_content(b"/G gs BT (A) Tj ET".to_vec());
        } else {
            f.doc.get_dictionary_mut(f.resources).unwrap().set(
                "ColorSpace",
                dictionary! {"Named"=>"DeviceRGB","Indirect"=>"DeviceGray"},
            );
            f.doc
                .get_object_mut(f.content)
                .unwrap()
                .as_stream_mut()
                .unwrap()
                .set_content(b"/Named cs 0.2 0.3 0.4 sc 0 0 100 100 re f".to_vec());
        }
        let (out, _) = run(&mut f, Options::default());
        let r = out
            .dereference(
                out.get_dictionary(f.page)
                    .unwrap()
                    .get(b"Resources")
                    .unwrap(),
            )
            .unwrap()
            .1
            .as_dict()
            .unwrap();
        if kind == "type3" {
            assert!(r.get(b"XObject").unwrap().as_dict().unwrap().has(b"Photo"));
        } else {
            assert!(
                r.get(b"ColorSpace")
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .has(b"Indirect")
            );
        }
    }
}

#[test]
fn editable_appearance_unused_fonts_and_original_image_encodings_are_retained() {
    use lopdf::Object;
    use std::io::Write;
    let mut f = common::fixture(false);
    let original = f
        .doc
        .get_object(f.image)
        .unwrap()
        .as_stream()
        .unwrap()
        .content
        .clone();
    let mut second = f
        .doc
        .get_object(f.image)
        .unwrap()
        .as_stream()
        .unwrap()
        .clone();
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(&original).unwrap();
    second.dict.set("Filter", "FlateDecode");
    second.set_content(e.finish().unwrap());
    let second = f.doc.add_object(second);
    f.doc
        .get_dictionary_mut(f.resources)
        .unwrap()
        .get_mut(b"XObject")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Photo", second);
    let future = f
        .doc
        .add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Courier"});
    let appearance=f.doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),80.into(),80.into()],"Resources"=>dictionary! {"Font"=>dictionary! {"Future"=>future},"XObject"=>dictionary! {"I"=>f.image}}},b"q 80 0 0 80 0 0 cm /I Do Q".to_vec()));
    let annot=f.doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Stamp","Rect"=>vec![0.into(),0.into(),80.into(),80.into()],"AP"=>dictionary! {"N"=>appearance}});
    f.doc
        .get_dictionary_mut(f.page)
        .unwrap()
        .set("Annots", vec![annot.into()]);
    let (out, report) = run(
        &mut f,
        Options {
            preset: Preset::Fidelity,
            ..Default::default()
        },
    );
    assert!(
        report
            .images
            .iter()
            .any(|i| i.object == format!("{} {}", second.0, second.1) && i.changed)
    );
    assert_eq!(
        out.get_object(f.image)
            .unwrap()
            .as_stream()
            .unwrap()
            .content,
        original
    );
    let r = out
        .get_object(appearance)
        .unwrap()
        .as_stream()
        .unwrap()
        .dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap();
    assert!(r.get(b"Font").unwrap().as_dict().unwrap().has(b"Future"));
    assert!(
        matches!(r.get(b"XObject").unwrap().as_dict().unwrap().get(b"I"),Ok(Object::Reference(id)) if *id==f.image)
    );
}
