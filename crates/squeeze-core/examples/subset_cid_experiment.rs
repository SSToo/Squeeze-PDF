//! Development-only experiment. Requires an explicit fixture CID->GID map;
//! not a general PDF font-use collector and never called by the product.
use lopdf::{Document, Stream, dictionary};
use std::{collections::BTreeMap, env, fs};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = env::args().collect();
    if a.len() != 6 {
        return Err("input.pdf output.pdf cid-font-object mapping.json font-output.ttf".into());
    }
    let mut doc = Document::load(&a[1])?;
    let id = (a[3].parse()?, 0);
    let map: BTreeMap<u16, u16> = serde_json::from_slice(&fs::read(&a[4])?)?;
    let font = doc.get_dictionary(id)?.clone();
    if font.get(b"Subtype")?.as_name()? != b"CIDFontType2" {
        return Err("requires CID TrueType fixture".into());
    }
    let descriptor = doc
        .dereference(font.get(b"FontDescriptor")?)?
        .1
        .as_dict()?
        .clone();
    let s = doc
        .dereference(descriptor.get(b"FontFile2")?)?
        .1
        .as_stream()?;
    let data = s.get_plain_content_with_limit(64 * 1024 * 1024)?;
    let mut glyphs = subsetter::GlyphRemapper::new();
    for &gid in map.values() {
        glyphs.remap(gid);
    }
    let subset = subsetter::subset(&data, 0, &glyphs)?;
    fs::write(&a[5], &subset)?;
    let file = doc.add_object(Stream::new(
        dictionary! {"Length1"=>subset.len() as i64},
        subset,
    ));
    let mut descriptor = descriptor;
    descriptor.set("FontFile2", file);
    let descriptor = doc.add_object(descriptor);
    let max = *map.keys().max().ok_or("empty map")? as usize;
    let mut bytes = vec![0; (max + 1) * 2];
    for (cid, gid) in &map {
        let new = glyphs.get(*gid).ok_or("unmapped glyph")?;
        bytes[*cid as usize * 2..*cid as usize * 2 + 2].copy_from_slice(&new.to_be_bytes());
    }
    let mapping = doc.add_object(Stream::new(dictionary! {}, bytes));
    let d = doc.get_dictionary_mut(id)?;
    d.set("FontDescriptor", descriptor);
    d.set("CIDToGIDMap", mapping);
    doc.prune_objects();
    doc.compress();
    doc.save(&a[2])?;
    println!(
        "{}",
        serde_json::json!({"before_font_bytes":data.len(),"after_font_bytes":doc.get_object(file)?.as_stream()?.get_plain_content()?.len(),"cids":map.len(),"mapping":map.iter().map(|(cid,gid)|(*cid,*gid,glyphs.get(*gid).unwrap())).collect::<Vec<_>>()})
    );
    // Content strings, widths and ToUnicode are intentionally not rewritten.
    Ok(())
}
