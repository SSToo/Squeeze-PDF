#[path = "../tests/common/mod.rs"]
mod common;
use lopdf::{Object, Stream, dictionary};
use std::{env, fs, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = env::args().nth(1).ok_or("output directory")?;
    let dir = Path::new(&dir);
    fs::create_dir_all(dir)?;
    for name in ["unused", "equivalent", "interactive", "pattern"] {
        let mut f = common::fixture(false);
        let mut second = f.doc.get_object(f.image)?.as_stream()?.clone();
        if name == "equivalent" {
            use std::io::Write;
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(&second.content)?;
            second.dict.set("Filter", "FlateDecode");
            second.set_content(encoder.finish()?);
        } else {
            second.content[0] = 255;
        }
        let second = f.doc.add_object(second);
        f.doc
            .get_dictionary_mut(f.resources)?
            .get_mut(b"XObject")?
            .as_dict_mut()?
            .set("Second", second);
        match name {
            "equivalent" => {
                f.doc
                    .get_object_mut(f.content)?
                    .as_stream_mut()?
                    .content
                    .extend_from_slice(b"q 300 0 0 400 0 0 cm /Second Do Q");
            }
            "interactive" => {
                let appearance=f.doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),80.into(),80.into()],"Resources"=>dictionary!{"XObject"=>dictionary!{"I"=>second}}},b"q 80 0 0 80 0 0 cm /I Do Q".to_vec()));
                let annotation=f.doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Stamp","Rect"=>vec![15.into(),15.into(),95.into(),95.into()],"AP"=>dictionary!{"N"=>appearance},"Contents"=>Object::string_literal("Keep this annotation")});
                f.doc
                    .get_dictionary_mut(f.page)?
                    .set("Annots", vec![annotation.into()]);
                let field=f.doc.add_object(dictionary!{"FT"=>"Tx","T"=>Object::string_literal("customer"),"V"=>Object::string_literal("Ada")});
                f.doc
                    .catalog_mut()?
                    .set("AcroForm", dictionary! {"Fields"=>vec![field.into()]});
                let file = f.doc.add_object(Stream::new(
                    dictionary! {"Type"=>"EmbeddedFile"},
                    b"Important original attachment".to_vec(),
                ));
                let spec=f.doc.add_object(dictionary!{"Type"=>"Filespec","F"=>Object::string_literal("notes.txt"),"EF"=>dictionary!{"F"=>file}});
                f.doc.catalog_mut()?.set("Names",dictionary!{"EmbeddedFiles"=>dictionary!{"Names"=>vec![Object::string_literal("notes.txt"),spec.into()]}});
            }
            "pattern" => {
                let pattern=f.doc.add_object(Stream::new(dictionary!{"Type"=>"Pattern","PatternType"=>1,"PaintType"=>1,"TilingType"=>1,"BBox"=>vec![0.into(),0.into(),10.into(),10.into()],"XStep"=>10,"YStep"=>10,"Resources"=>dictionary!{"XObject"=>dictionary!{"I"=>second}}},b"/I Do".to_vec()));
                f.doc
                    .get_dictionary_mut(f.resources)?
                    .set("Pattern", dictionary! {"P"=>pattern});
                // Draw the pattern too, so visual QA includes its protected data.
                f.doc
                    .get_object_mut(f.content)?
                    .as_stream_mut()?
                    .content
                    .extend_from_slice(b"q /Pattern cs /P scn 0 0 10 10 re f Q");
            }
            _ => {}
        }
        let content = f.doc.get_object_mut(f.content)?.as_stream_mut()?;
        content.dict.set("Length", content.content.len() as i64);
        f.doc.save(dir.join(format!("{name}.pdf")))?;
    }
    Ok(())
}
