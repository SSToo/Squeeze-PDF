//! Automatic Identity-H CIDFontType2 usage, without changing PDF character codes.
use crate::{Options, Result, check_cancel, inspect, stream_filters, usage};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, content::Content, dictionary};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::atomic::AtomicBool,
    time::Instant,
};
#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub status: String,
    pub fonts_changed: usize,
    pub glyphs_retained: usize,
    pub before_bytes: usize,
    pub after_bytes: usize,
    pub skipped_unsupported: usize,
}
#[derive(Clone)]
struct Font {
    cid: ObjectId,
    descriptor: Dictionary,
    file: ObjectId,
    roots: Vec<ObjectId>,
}
fn id(doc: &Document, o: &Object) -> Option<ObjectId> {
    doc.dereference(o).ok()?.0
}
fn table<'a>(doc: &'a Document, r: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    doc.dereference(r.get(key).ok()?).ok()?.1.as_dict().ok()
}
fn eligible(doc: &Document, root: ObjectId) -> Option<Font> {
    let f = doc.get_dictionary(root).ok()?;
    if !inspect::named(f, b"Subtype", b"Type0") || !inspect::named(f, b"Encoding", b"Identity-H") {
        return None;
    }
    let descendants = doc
        .dereference(f.get(b"DescendantFonts").ok()?)
        .ok()?
        .1
        .as_array()
        .ok()?;
    if descendants.len() != 1 {
        return None;
    }
    let cid = id(doc, &descendants[0])?;
    let child = doc.get_dictionary(cid).ok()?;
    if !inspect::named(child, b"Subtype", b"CIDFontType2") {
        return None;
    }
    let descriptor = doc
        .dereference(child.get(b"FontDescriptor").ok()?)
        .ok()?
        .1
        .as_dict()
        .ok()?
        .clone();
    if descriptor.has(b"FontFile") || descriptor.has(b"FontFile3") {
        return None;
    }
    Some(Font {
        cid,
        file: id(doc, descriptor.get(b"FontFile2").ok()?)?,
        descriptor,
        roots: vec![root],
    })
}
struct Collector<'a> {
    doc: &'a Document,
    options: &'a Options,
    cancel: &'a AtomicBool,
    uses: HashMap<ObjectId, BTreeSet<u16>>,
    eligible: HashSet<ObjectId>,
    path: HashSet<ObjectId>,
    work: usize,
}
impl Collector<'_> {
    fn text(&mut self, font: Option<ObjectId>, text: &Object) -> Option<()> {
        let font = font?;
        let bytes = text.as_str().ok()?;
        if !self.eligible.contains(&font) {
            return Some(());
        }
        if !bytes.len().is_multiple_of(2) {
            return None;
        }
        for pair in bytes.chunks_exact(2) {
            self.uses
                .entry(font)
                .or_default()
                .insert(u16::from_be_bytes([pair[0], pair[1]]));
        }
        Some(())
    }
    fn walk(
        &mut self,
        bytes: &[u8],
        resources: &Dictionary,
        mut font: Option<ObjectId>,
        depth: usize,
    ) -> Option<()> {
        if depth > 32 || resources.has(b"Pattern") {
            return None;
        }
        if table(self.doc, resources, b"Font").is_some_and(|t| {
            t.iter().any(|(_, o)| {
                self.doc
                    .dereference(o)
                    .ok()
                    .and_then(|(_, o)| o.as_dict().ok())
                    .is_some_and(|d| inspect::named(d, b"Subtype", b"Type3"))
            })
        }) {
            return None;
        }
        let content = Content::decode(bytes).ok()?;
        let mut stack = Vec::new();
        for op in content.operations {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) || self.work == 0 {
                return None;
            }
            self.work -= 1;
            match op.operator.as_str() {
                "q" if op.operands.is_empty() => stack.push(font),
                "Q" if op.operands.is_empty() => font = stack.pop()?,
                "Tf" if op.operands.len() == 2 => {
                    let name = op.operands[0].as_name().ok()?;
                    let fonts = table(self.doc, resources, b"Font")?;
                    let selected = id(self.doc, fonts.get(name).ok()?)?;
                    font = Some(selected);
                    if self.eligible.contains(&selected) {
                        self.uses.entry(selected).or_default();
                    }
                }
                "Tj" | "'" => self.text(font, op.operands.last()?)?,
                "\"" if op.operands.len() == 3 => self.text(font, &op.operands[2])?,
                "TJ" if op.operands.len() == 1 => {
                    for value in op.operands[0].as_array().ok()? {
                        match value {
                            Object::String(_, _) => self.text(font, value)?,
                            Object::Integer(_) | Object::Real(_) => {}
                            _ => return None,
                        }
                    }
                }
                "gs" if op.operands.len() == 1 => {
                    let state = table(self.doc, resources, b"ExtGState")?;
                    let state = self
                        .doc
                        .dereference(state.get(op.operands[0].as_name().ok()?).ok()?)
                        .ok()?
                        .1
                        .as_dict()
                        .ok()?;
                    if state
                        .get(b"SMask")
                        .is_ok_and(|o| o.as_name().ok() != Some(b"None"))
                    {
                        return None;
                    }
                    if let Ok(value) = state.get(b"Font") {
                        let array = self.doc.dereference(value).ok()?.1.as_array().ok()?;
                        if array.len() != 2 {
                            return None;
                        }
                        font = Some(id(self.doc, &array[0])?);
                    }
                }
                "Do" if op.operands.len() == 1 => {
                    let objects = table(self.doc, resources, b"XObject")?;
                    let object = objects.get(op.operands[0].as_name().ok()?).ok()?;
                    let target = id(self.doc, object)?;
                    let stream = self.doc.get_object(target).ok()?.as_stream().ok()?;
                    if inspect::named(&stream.dict, b"Subtype", b"Image") {
                        continue;
                    }
                    if !inspect::named(&stream.dict, b"Subtype", b"Form")
                        || stream.dict.has(b"Ref")
                        || !self.path.insert(target)
                    {
                        return None;
                    }
                    let own = match stream.dict.get(b"Resources") {
                        Ok(o) => self.doc.dereference(o).ok()?.1.as_dict().ok()?,
                        Err(_) => resources,
                    };
                    let raw = stream_filters::content(
                        self.doc,
                        stream,
                        self.options.limits.max_stream_bytes,
                    )
                    .ok()?;
                    self.walk(&raw, own, font, depth + 1)?;
                    self.path.remove(&target);
                }
                "cm" | "w" | "J" | "j" | "M" | "d" | "ri" | "i" | "m" | "l" | "c" | "v" | "y"
                | "h" | "re" | "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n"
                | "W" | "W*" | "BT" | "ET" | "Tc" | "Tw" | "Tz" | "TL" | "Tr" | "Ts" | "Td"
                | "TD" | "Tm" | "T*" | "CS" | "cs" | "SC" | "SCN" | "sc" | "scn" | "G" | "g"
                | "RG" | "rg" | "K" | "k" | "sh" | "MP" | "DP" | "BMC" | "BDC" | "EMC" => {}
                _ => return None,
            }
        }
        stack.is_empty().then_some(())
    }
}
fn tables(data: &[u8]) -> Option<BTreeMap<[u8; 4], Vec<u8>>> {
    if data.get(..4)? != [0, 1, 0, 0] {
        return None;
    }
    let n = u16::from_be_bytes(data.get(4..6)?.try_into().ok()?) as usize;
    if n == 0 || n > 256 {
        return None;
    }
    let mut result = BTreeMap::new();
    for i in 0..n {
        let row = data.get(12 + i * 16..28 + i * 16)?;
        let tag = row[..4].try_into().ok()?;
        let offset = u32::from_be_bytes(row[8..12].try_into().ok()?) as usize;
        let length = u32::from_be_bytes(row[12..16].try_into().ok()?) as usize;
        if result
            .insert(tag, data.get(offset..offset.checked_add(length)?)?.to_vec())
            .is_some()
        {
            return None;
        }
    }
    Some(result)
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut b = [0; 4];
        b[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(b))
    })
}
/// Preserve gasp/OS2 in addition to the subsetter's glyph programs and hints.
fn preserve_tables(subset: Vec<u8>, original: &BTreeMap<[u8; 4], Vec<u8>>) -> Option<Vec<u8>> {
    let mut t = tables(&subset)?;
    for tag in [*b"gasp", *b"OS/2"] {
        if let Some(bytes) = original.get(&tag) {
            t.insert(tag, bytes.clone());
        }
    }
    let head = t.get_mut(b"head")?;
    head.get_mut(8..12)?.fill(0);
    let count = t.len();
    let mut out = vec![0; 12 + 16 * count];
    out[..4].copy_from_slice(&[0, 1, 0, 0]);
    out[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    let selector = (usize::BITS - 1 - count.leading_zeros()) as u16;
    let search = 16u16 << selector;
    out[6..8].copy_from_slice(&search.to_be_bytes());
    out[8..10].copy_from_slice(&selector.to_be_bytes());
    out[10..12].copy_from_slice(&((count * 16) as u16 - search).to_be_bytes());
    let mut head_offset = 0;
    for (i, (tag, bytes)) in t.into_iter().enumerate() {
        let offset = out.len();
        let p = 12 + i * 16;
        out[p..p + 4].copy_from_slice(&tag);
        out[p + 4..p + 8].copy_from_slice(&checksum(&bytes).to_be_bytes());
        out[p + 8..p + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        out[p + 12..p + 16].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
        if tag == *b"head" {
            head_offset = offset;
        }
        out.extend(bytes);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Some(out)
}
fn rename(d: &mut Dictionary, key: &[u8], tag: &[u8]) {
    if let Ok(name) = d.get(key).and_then(Object::as_name) {
        let base =
            if name.len() > 7 && name[6] == b'+' && name[..6].iter().all(u8::is_ascii_uppercase) {
                &name[7..]
            } else {
                name
            };
        let mut n = tag.to_vec();
        n.push(b'+');
        n.extend(base);
        d.set(key, Object::Name(n));
    }
}
pub(crate) fn optimize(
    doc: &mut Document,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<Stats> {
    let mut stats = Stats {
        status: "no_eligible_font".into(),
        ..Default::default()
    };
    if doc
        .objects
        .values()
        .filter_map(inspect::dictionary)
        .any(|d| d.has(b"AcroForm") || d.has(b"AP") || d.has(b"FT"))
    {
        stats.status = "preserved_editable_fonts".into();
        return Ok(stats);
    }
    let mut fonts = HashMap::<ObjectId, Font>::new();
    let mut blocked = HashSet::new();
    for (&root, o) in &doc.objects {
        check_cancel(cancel)?;
        let Some(d) = inspect::dictionary(o) else {
            continue;
        };
        if !inspect::named(d, b"Subtype", b"Type0") {
            continue;
        }
        if let Some(f) = eligible(doc, root) {
            fonts.insert(root, f);
        } else {
            stats.skipped_unsupported += 1;
            if let Some(a) = d
                .get(b"DescendantFonts")
                .ok()
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(_, o)| o.as_array().ok())
            {
                for o in a {
                    if let Some(cid) = id(doc, o) {
                        blocked.insert(cid);
                    }
                }
            }
        }
    }
    if fonts.is_empty() {
        return Ok(stats);
    }
    let mut collector = Collector {
        doc,
        options,
        cancel,
        uses: HashMap::new(),
        eligible: fonts.keys().copied().collect(),
        path: HashSet::new(),
        work: options.limits.max_operations,
    };
    for page in inspect::pages(doc)? {
        check_cancel(cancel)?;
        let r = inspect::inherited(doc, page, b"Resources")?.and_then(|o| o.as_dict().ok());
        let Some(r) = r else {
            stats.status = "skipped_incomplete_usage".into();
            return Ok(stats);
        };
        let Ok(bytes) = usage::page_content(
            doc,
            doc.get_dictionary(page)?,
            options.limits.max_stream_bytes,
        ) else {
            stats.status = "skipped_incomplete_usage".into();
            return Ok(stats);
        };
        if collector.walk(&bytes, r, None, 0).is_none() {
            check_cancel(cancel)?;
            stats.status = "skipped_incomplete_usage".into();
            return Ok(stats);
        }
    }
    let uses = collector.uses;
    let started = Instant::now();
    let mut groups = BTreeMap::<ObjectId, BTreeMap<ObjectId, Font>>::new();
    for (root, f) in fonts {
        if !blocked.contains(&f.cid) {
            groups
                .entry(f.file)
                .or_default()
                .entry(f.cid)
                .and_modify(|old| old.roots.push(root))
                .or_insert(f);
        }
    }
    let mut remaining = 32 * 1024 * 1024usize;
    for (file, group) in groups {
        check_cancel(cancel)?;
        if !group
            .values()
            .any(|f| f.roots.iter().any(|r| uses.contains_key(r)))
        {
            continue;
        }
        if started.elapsed().as_secs() >= 12 {
            stats.status = "budget_exhausted".into();
            break;
        }
        let original = doc.get_object(file)?.as_stream()?;
        let Ok(data) = stream_filters::content(doc, original, remaining.min(16 * 1024 * 1024))
        else {
            continue;
        };
        remaining = remaining.saturating_sub(data.len());
        let Some(t) = tables(&data) else { continue };
        if !t.contains_key(b"glyf") || t.contains_key(b"fvar") || t.contains_key(b"gvar") {
            continue;
        }
        if t.get(b"OS/2")
            .and_then(|b| b.get(8..10))
            .is_some_and(|b| u16::from_be_bytes([b[0], b[1]]) & 0x302 != 0)
        {
            continue;
        }
        let Some(maxp) = t.get(b"maxp").and_then(|b| b.get(4..6)) else {
            continue;
        };
        let glyph_count = u16::from_be_bytes([maxp[0], maxp[1]]);
        let mut mappings = BTreeMap::<ObjectId, BTreeMap<u16, u16>>::new();
        let mut mapper = subsetter::GlyphRemapper::new();
        let mut valid = true;
        for (cid, f) in &group {
            let child = doc.get_dictionary(*cid)?;
            let raw = match child.get(b"CIDToGIDMap") {
                Err(_) => None,
                Ok(o)
                    if doc
                        .dereference(o)
                        .is_ok_and(|(_, o)| o.as_name().ok() == Some(b"Identity")) =>
                {
                    None
                }
                Ok(o) => {
                    let Ok((_, Object::Stream(s))) = doc.dereference(o) else {
                        valid = false;
                        break;
                    };
                    let Ok(bytes) = stream_filters::content(doc, s, 131072) else {
                        valid = false;
                        break;
                    };
                    if !bytes.len().is_multiple_of(2) {
                        valid = false;
                        break;
                    }
                    Some(bytes)
                }
            };
            let mut map = BTreeMap::new();
            for &code in f.roots.iter().filter_map(|r| uses.get(r)).flatten() {
                let gid = if let Some(bytes) = &raw {
                    let p = code as usize * 2;
                    let Some(pair) = bytes.get(p..p + 2) else {
                        valid = false;
                        break;
                    };
                    u16::from_be_bytes([pair[0], pair[1]])
                } else {
                    code
                };
                if gid >= glyph_count {
                    valid = false;
                    break;
                }
                map.insert(code, gid);
                mapper.remap(gid);
            }
            if !valid {
                break;
            }
            mappings.insert(*cid, map);
        }
        if !valid {
            continue;
        }
        let Ok(subset) = subsetter::subset(&data, 0, &mapper) else {
            continue;
        };
        check_cancel(cancel)?;
        let Some(subset) = preserve_tables(subset, &t) else {
            continue;
        };
        let encoded = crate::lossless::deflate(&subset)?;
        if encoded.len() >= original.content.len() {
            continue;
        }
        let hash = Sha256::digest(&subset);
        let tag: Vec<_> = hash[..6].iter().map(|b| b'A' + b % 26).collect();
        let new_file = doc.add_object(Stream::new(
            dictionary! {"Filter"=>"FlateDecode","Length1"=>subset.len() as i64},
            encoded,
        ));
        stats.before_bytes += data.len();
        stats.after_bytes += subset.len();
        for (cid, f) in group {
            let mapping = &mappings[&cid];
            let maximum = mapping.keys().next_back().copied().unwrap_or(0) as usize;
            let mut bytes = vec![0; (maximum + 1) * 2];
            for (&code, &gid) in mapping {
                let new = mapper
                    .get(gid)
                    .ok_or_else(|| crate::Error::Validation("字体字形映射缺失".into()))?;
                bytes[code as usize * 2..code as usize * 2 + 2].copy_from_slice(&new.to_be_bytes());
            }
            let new_map = doc.add_object(Stream::new(
                dictionary! {"Filter"=>"FlateDecode"},
                crate::lossless::deflate(&bytes)?,
            ));
            let mut descriptor = f.descriptor;
            descriptor.set("FontFile2", new_file);
            rename(&mut descriptor, b"FontName", &tag);
            let descriptor = doc.add_object(descriptor);
            let child = doc.get_object_mut(cid)?.as_dict_mut()?;
            child.set("FontDescriptor", descriptor);
            child.set("CIDToGIDMap", new_map);
            rename(child, b"BaseFont", &tag);
            for root in f.roots {
                rename(doc.get_object_mut(root)?.as_dict_mut()?, b"BaseFont", &tag);
            }
            stats.fonts_changed += 1;
            stats.glyphs_retained += mapping.len();
        }
        stats.status = "subset_created".into();
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_tracks_contents_state_saved_fonts_forms_and_tj_codes() {
        let mut d = Document::new();
        let f = d.add_object(dictionary! {"Subtype"=>"Type0"});
        let g = d.add_object(dictionary! {"Subtype"=>"Type0"});
        let form = d.add_object(Stream::new(
            dictionary! {"Subtype"=>"Form"},
            b"BT <0004> Tj ET".to_vec(),
        ));
        let r = dictionary! {"Font"=>dictionary!{"F"=>f,"G"=>g},"XObject"=>dictionary!{"V"=>form}};
        let options = Options::default();
        let cancel = AtomicBool::new(false);
        let mut c = Collector {
            doc: &d,
            options: &options,
            cancel: &cancel,
            uses: HashMap::new(),
            eligible: HashSet::from([f, g]),
            path: HashSet::new(),
            work: 1000,
        };
        c.walk(
            b"BT /F 12 Tf <0001> Tj q /G 8 Tf [<0002> 10 <0003>] TJ Q <0005> Tj ET /V Do",
            &r,
            None,
            0,
        )
        .unwrap();
        assert_eq!(c.uses[&f], BTreeSet::from([1, 4, 5]));
        assert_eq!(c.uses[&g], BTreeSet::from([2, 3]));
        assert!(c.walk(b"BT /F 12 Tf <01> Tj ET", &r, None, 0).is_none());
        assert!(
            c.walk(b"BT /F 12 Tf UnknownOperator ET", &r, None, 0)
                .is_none()
        );
        assert!(c.walk(b"Q", &r, None, 0).is_none());
    }
    #[test]
    fn dynamic_patterns_softmasks_and_type3_leave_usage_incomplete() {
        let mut d = Document::new();
        let f = d.add_object(dictionary! {"Subtype"=>"Type3"});
        let options = Options::default();
        let cancel = AtomicBool::new(false);
        for r in [
            dictionary! {"Font"=>dictionary!{"F"=>f}},
            dictionary! {"Pattern"=>Dictionary::new()},
            dictionary! {"ExtGState"=>dictionary!{"S"=>dictionary!{"SMask"=>dictionary!{"S"=>"Alpha"}}}},
        ] {
            let mut c = Collector {
                doc: &d,
                options: &options,
                cancel: &cancel,
                uses: HashMap::new(),
                eligible: HashSet::new(),
                path: HashSet::new(),
                work: 100,
            };
            assert!(c.walk(b"/S gs", &r, None, 0).is_none());
        }
    }
}
