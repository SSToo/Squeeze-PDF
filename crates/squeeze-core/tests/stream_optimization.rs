use lopdf::{Document, Object, Stream, content::Content, dictionary};
use squeeze_core::{Error, Options, Outcome, Preset, compress};
use std::{fs, sync::atomic::AtomicBool};
use tempfile::tempdir;

fn document(chunks: Vec<Stream>) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let streams: Vec<_> = chunks
        .into_iter()
        .map(|s| Object::Reference(doc.add_object(s)))
        .collect();
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()],"Resources"=>dictionary!{"Font"=>dictionary!{"F"=>font}},"Contents"=>streams});
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    doc
}
fn raw(bytes: &[u8]) -> Stream {
    Stream::new(dictionary! {}, bytes.to_vec())
}
fn ops(doc: &Document) -> Vec<(String, Vec<Object>)> {
    let p = doc.get_pages()[&1];
    Content::decode(&doc.get_page_content(p))
        .unwrap()
        .operations
        .into_iter()
        .map(|o| (o.operator, o.operands))
        .collect()
}
#[test]
fn fragmented_pages_win_as_complete_files_and_keep_text_state_and_shared_sources() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut chunks = vec![raw(b"q\n0.2 0.3 0.4 rg\nBT /F 12 Tf 10 170 Td\n")];
    for i in 0..80 {
        chunks.push(raw(format!("(Row {i}) Tj 0 -1 Td\n").as_bytes()));
    }
    chunks.push(raw(b"ET Q\n"));
    let mut doc = document(chunks);
    let source_contents = doc
        .get_dictionary(doc.get_pages()[&1])
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .clone();
    let holder = doc.add_object(dictionary! {"SharedContent"=>source_contents});
    doc.catalog_mut().unwrap().set("CustomData", holder);
    doc.save(&input).unwrap();
    let source = fs::read(&input).unwrap();
    let expected = ops(&doc);
    let r = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.outcome, Outcome::Saved);
    assert_eq!(r.stream_optimization.status, "selected");
    assert_eq!(r.stream_optimization.merged_pages, 1);
    let old = &r.strategy_candidates[0];
    let new = &r.strategy_candidates[1];
    assert!(new.selected && !old.selected);
    assert!(new.bytes < old.bytes);
    assert_eq!(r.result_bytes, new.bytes);
    let actual_audit = squeeze_core::audit_file(&output, &squeeze_core::Limits::default()).unwrap();
    assert_eq!(r.size_after.objects, actual_audit.objects);
    assert_eq!(r.size_after.streams, actual_audit.streams);
    assert_eq!(r.size_after.containers, actual_audit.containers);
    assert!(r.size_after.containers > 0);
    let actual = Document::load(&output).unwrap();
    assert_eq!(ops(&actual), expected);
    assert_eq!(
        actual.extract_text(&[1]).unwrap(),
        doc.extract_text(&[1]).unwrap()
    );
    let custom = actual
        .catalog()
        .unwrap()
        .get(b"CustomData")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = actual
        .get_dictionary(custom)
        .unwrap()
        .get(b"SharedContent")
        .unwrap()
        .as_reference()
        .unwrap();
    assert_eq!(
        actual
            .get_object(stream)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap(),
        b"q\n0.2 0.3 0.4 rg\nBT /F 12 Tf 10 170 Td\n"
    );
    assert_eq!(fs::read(input).unwrap(), source);
}
#[test]
fn retaining_all_shared_streams_rejects_a_larger_complete_candidate() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    let mut doc = document(vec![raw(b"q\n"), raw(b"Q\n")]);
    let page = doc.get_pages()[&1];
    let shared = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    doc.catalog_mut().unwrap().set("KeepStreams", shared);
    doc.save(&input).unwrap();
    let r = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.stream_optimization.status, "kept_baseline");
    assert!(r.strategy_candidates[0].selected);
    assert!(!r.strategy_candidates[1].selected);
    assert!(r.strategy_candidates[1].bytes >= r.strategy_candidates[0].bytes);
    let actual = Document::load(r.output.as_deref().unwrap_or(&input)).unwrap();
    assert!(
        actual
            .get_dictionary(actual.get_pages()[&1])
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_array()
            .is_ok()
    );
}
#[test]
fn malformed_and_unknown_filters_never_drive_resource_removal_or_rewrites() {
    for (filter, bytes) in [
        (
            "ASCIIHexDecode",
            b"4254202f46203132205466202848656c6c6f2920546a204554".as_slice(),
        ),
        ("RunLengthDecode", &[2, b'q']),
        ("Crypt", b"q Q"),
    ] {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        let mut doc = document(vec![Stream::new(
            dictionary! {"Filter"=>filter},
            bytes.to_vec(),
        )]);
        doc.save(&input).unwrap();
        let r = compress(
            &input,
            &output,
            &Options {
                preset: Preset::Balanced,
                compact_paths: true,
                reuse_paths: true,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let out = Document::load(r.output.as_deref().unwrap_or(&input)).unwrap();
        let p = out.get_pages()[&1];
        let id = out
            .get_dictionary(p)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        let stream = out.get_object(id).unwrap().as_stream().unwrap();
        assert_eq!(stream.content, bytes);
        assert_eq!(
            stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
            filter.as_bytes()
        );
        assert!(!out.get_page_fonts(p).unwrap().is_empty());
    }
}
#[test]
fn cancellation_during_candidate_validation_leaves_output_uncommitted() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    document(vec![raw(b"q\n"), raw(b"Q\n")])
        .save(&input)
        .unwrap();
    let source = fs::read(&input).unwrap();
    let cancel = AtomicBool::new(false);
    let r = compress(&input, &output, &Options::default(), &cancel, |p| {
        if matches!(p.phase, squeeze_core::Phase::Validating) {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    assert!(matches!(r, Err(Error::Cancelled)));
    assert!(!output.exists());
    assert_eq!(fs::read(input).unwrap(), source);
}

#[test]
fn tagged_mcr_stream_identity_stays_in_the_page_contents_array() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("tagged.pdf");
    let output = dir.path().join("out.pdf");
    let mut doc = document(vec![
        raw(b"/P << /MCID 0 >> BDC BT /F 12 Tf 10 170 Td (Tagged text) Tj ET EMC\n"),
        raw(b"q 0.2 0.4 0.6 rg 10 10 80 80 re f Q\n"),
    ]);
    let page = doc.get_pages()[&1];
    let stream = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_reference()
        .unwrap();
    let tree = doc.new_object_id();
    let element=doc.add_object(dictionary!{"Type"=>"StructElem","S"=>"P","P"=>tree,"Pg"=>page,"K"=>dictionary!{"Type"=>"MCR","Pg"=>page,"Stm"=>stream,"MCID"=>0}});
    let parents =
        doc.add_object(dictionary! {"Nums"=>vec![0.into(),Object::Array(vec![element.into()])]});
    doc.objects.insert(tree,Object::Dictionary(dictionary!{"Type"=>"StructTreeRoot","K"=>element,"ParentTree"=>parents,"ParentTreeNextKey"=>1}));
    doc.catalog_mut().unwrap().set("StructTreeRoot", tree);
    doc.catalog_mut()
        .unwrap()
        .set("MarkInfo", dictionary! {"Marked"=>true});
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("StructParents", 0);
    doc.save(&input).unwrap();
    let source = fs::read(&input).unwrap();
    let r = compress(
        &input,
        &output,
        &Options::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.stream_optimization.merged_pages, 0);
    let out = Document::load(r.output.as_deref().unwrap_or(&input)).unwrap();
    let page = out.get_pages()[&1];
    let contents = out
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(contents.len(), 2);
    let tree = out
        .catalog()
        .unwrap()
        .get(b"StructTreeRoot")
        .unwrap()
        .as_reference()
        .unwrap();
    let element = out
        .get_dictionary(tree)
        .unwrap()
        .get(b"K")
        .unwrap()
        .as_reference()
        .unwrap();
    let mcr = out
        .get_dictionary(element)
        .unwrap()
        .get(b"K")
        .unwrap()
        .as_dict()
        .unwrap();
    assert_eq!(mcr.get(b"Pg").unwrap().as_reference().unwrap(), page);
    assert!(contents.contains(mcr.get(b"Stm").unwrap()));
    assert_eq!(
        out.extract_text(&[1]).unwrap(),
        doc.extract_text(&[1]).unwrap()
    );
    assert_eq!(fs::read(input).unwrap(), source);
}
