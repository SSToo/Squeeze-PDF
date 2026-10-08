#[path = "../tests/common/mod.rs"]
mod common;
fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("output directory required"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    for (name, jpeg) in [("scan-rgb.pdf", false), ("photo-jpeg.pdf", true)] {
        let mut f = common::fixture(jpeg);
        f.doc.save(dir.join(name)).unwrap();
        println!("{}", dir.join(name).display());
    }
    if std::env::args().any(|a| a == "--large") {
        use lopdf::{Object, dictionary};
        let mut f = common::fixture(false);
        let pages = f
            .doc
            .catalog()
            .unwrap()
            .get(b"Pages")
            .unwrap()
            .as_reference()
            .unwrap();
        let original = f.doc.get_object(f.image).unwrap().clone();
        let mut kids = vec![Object::Reference(f.page)];
        for _ in 1..24 {
            let image = f.doc.add_object(original.clone());
            let resources = f
                .doc
                .add_object(dictionary! {"XObject"=>dictionary! {"Photo"=>image}});
            let mut page = f.doc.get_dictionary(f.page).unwrap().clone();
            page.set("Resources", resources);
            kids.push(f.doc.add_object(page).into());
        }
        let root = f.doc.get_object_mut(pages).unwrap().as_dict_mut().unwrap();
        root.set("Kids", kids);
        root.set("Count", 24);
        // Avoid requiring the omitted title font on the additional pages.
        f.doc
            .get_object_mut(f.content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_content(b"q 300 0 0 400 30 30 cm /Photo Do Q".to_vec());
        f.doc.save(dir.join("batch-large.pdf")).unwrap();
        println!("{}", dir.join("batch-large.pdf").display());
    }
}
