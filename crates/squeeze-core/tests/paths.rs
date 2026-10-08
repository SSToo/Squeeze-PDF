mod common;
use lopdf::{Document, Object};
use squeeze_core::{Options, Preset, compress};
use std::{fs, sync::atomic::AtomicBool};
use tempfile::tempdir;

#[test]
fn path_compaction_is_opt_in_and_excludes_array_graphics_state() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let mut data = String::from("q 1 0 0 1 0 0 cm 12.345678 23.456789 m ");
    for i in 0..100 {
        data.push_str(&format!(
            "{}.{:06} {}.{:06} l ",
            i,
            i * 7919 + 123456,
            i + 100,
            i * 5711 + 234567
        ));
    }
    data.push_str("S Q BT /F1 12 Tf 20 490 Td (Exact text 12.345678) Tj ET");
    for (compact, array, shared, interactive, preset) in [
        (false, false, false, false, Preset::Small),
        (true, false, false, false, Preset::Small),
        (true, true, false, false, Preset::Small),
        (true, false, true, false, Preset::Small),
        (true, false, false, false, Preset::Lossless),
        (true, false, false, false, Preset::Fidelity),
        (true, false, false, true, Preset::Small),
    ] {
        let mut f = common::fixture(true);
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(data.as_bytes().to_vec());
        if array {
            f.doc
                .get_object_mut(f.page)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Contents", Object::Array(vec![f.content.into()]));
        }
        if shared {
            f.doc
                .get_object_mut(f.resources)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("UnclassifiedResource", f.content);
        }
        if interactive {
            let annotation=f.doc.add_object(lopdf::dictionary! {"Type"=>"Annot","Subtype"=>"Text","Rect"=>vec![0.into(),0.into(),20.into(),20.into()]});
            f.doc
                .get_object_mut(f.page)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Annots", vec![Object::Reference(annotation)]);
        }
        f.doc.save(&input).unwrap();
        let before = fs::read(&input).unwrap();
        let output = dir.path().join(format!(
            "out-{compact}-{array}-{shared}-{interactive}-{preset:?}.pdf"
        ));
        let report = compress(
            &input,
            &output,
            &Options {
                preset,
                compact_paths: compact,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let d = Document::load(&output).unwrap();
        let text = String::from_utf8(d.get_page_content(d.get_pages()[&1])).unwrap();
        assert!(text.contains("Exact text 12.345678"));
        let expected = compact && !array && !shared && !interactive && preset != Preset::Lossless;
        assert_eq!(text.contains("12.346 23.457 m"), expected);
        assert_eq!(
            report.warnings.iter().any(|w| w.contains("路径精度")),
            expected
        );
        assert_eq!(fs::read(&input).unwrap(), before);
    }
}

#[test]
fn nested_forms_keep_geometry_metadata_and_physical_error_budget() {
    use lopdf::{Stream, dictionary};
    let dir = tempdir().unwrap();
    let input = dir.path().join("forms.pdf");
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let raw = (0..200)
        .map(|i| {
            format!(
                "{}.1234567 {}.7654321 {}\n",
                i,
                i + 1,
                if i == 0 { "m" } else { "l" }
            )
        })
        .collect::<String>()
        + "S";
    let leaf=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),300.into(),300.into()],"Resources"=>dictionary!{},"Group"=>dictionary!{"S"=>"Transparency","I"=>true},"Matrix"=>vec![2.into(),0.into(),0.into(),2.into(),17.into(),19.into()]},raw.as_bytes().to_vec()));
    let parent=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),300.into(),300.into()],"Resources"=>dictionary!{"XObject"=>dictionary!{"Child"=>leaf}}},b"q 3 1 2 4 0 0 cm /Child Do Q".to_vec()));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 5 0 0 5 0 0 cm /Parent Do Q".to_vec(),
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),600.into(),800.into()],"UserUnit"=>2,"Resources"=>dictionary!{"XObject"=>dictionary!{"Parent"=>parent}},"Contents"=>content});
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! {"Type"=>"Pages","Count"=>1,"Kids"=>vec![Object::Reference(page)]},
        ),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    doc.save(&input).unwrap();
    let original = fs::read(&input).unwrap();
    for preset in [
        Preset::Fidelity,
        Preset::Balanced,
        Preset::Small,
        Preset::Lossless,
    ] {
        let output = dir.path().join(format!("{preset:?}.pdf"));
        compress(
            &input,
            &output,
            &Options {
                preset,
                compact_paths: true,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let result = Document::load(output).unwrap();
        let after = result.get_object(leaf).unwrap().as_stream().unwrap();
        let bytes = after.get_plain_content().unwrap();
        for key in [b"BBox".as_slice(), b"Resources", b"Group", b"Matrix"] {
            assert_eq!(
                after.dict.get(key).unwrap(),
                doc.get_object(leaf)
                    .unwrap()
                    .as_stream()
                    .unwrap()
                    .dict
                    .get(key)
                    .unwrap()
            );
        }
        assert_eq!(
            result
                .get_object(parent)
                .unwrap()
                .as_stream()
                .unwrap()
                .get_plain_content()
                .unwrap(),
            b"q 3 1 2 4 0 0 cm /Child Do Q"
        );
        let numbers = |s: &str| {
            s.split_whitespace()
                .filter_map(|v| v.parse::<f64>().ok())
                .collect::<Vec<_>>()
        };
        let before = numbers(&raw);
        let after = numbers(std::str::from_utf8(&bytes).unwrap());
        assert_eq!(before.len(), after.len());
        // UserUnit 2 * page scale 5 * parent shear norm 5 * form scale 2 = 100.
        for (a, b) in before.iter().zip(&after) {
            assert!((a - b).abs() * 100. <= 0.00050001);
        }
        assert_eq!(before != after, preset != Preset::Lossless);
    }
    assert_eq!(fs::read(&input).unwrap(), original);
}
