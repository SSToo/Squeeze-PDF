use crate::{SizeAudit, StageResult, inspect};
use lopdf::{Document, Object, ObjectId};
use std::{collections::HashSet, time::Instant};

pub(crate) fn size(doc: &Document, file_bytes: u64) -> SizeAudit {
    let mut masks = HashSet::new();
    let mut fonts = HashSet::new();
    let mut glyphs = HashSet::new();
    let mut pages = HashSet::new();
    fn refs(doc: &Document, object: &Object, ids: &mut HashSet<ObjectId>) {
        let mut pending = vec![object];
        let mut seen = HashSet::new();
        while let Some(object) = pending.pop() {
            match object {
                Object::Reference(id) if seen.insert(*id) => {
                    if let Some(object) = doc.objects.get(id) {
                        match object {
                            Object::Stream(_) => {
                                ids.insert(*id);
                            }
                            Object::Array(a) => pending.extend(a),
                            Object::Reference(_) => pending.push(object),
                            _ => {}
                        }
                    }
                }
                Object::Array(a) => pending.extend(a),
                _ => {}
            }
        }
    }
    for d in doc.objects.values().filter_map(inspect::dictionary) {
        for key in [b"Mask".as_slice(), b"SMask"] {
            if let Ok(o) = d.get(key) {
                refs(doc, o, &mut masks);
            }
        }
        for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
            if let Ok(o) = d.get(key) {
                refs(doc, o, &mut fonts);
            }
        }
        if inspect::named(d, b"Subtype", b"Type3")
            && let Ok(o) = d.get(b"CharProcs")
            && let Ok((_, Object::Dictionary(chars))) = doc.dereference(o)
        {
            for (_, o) in chars {
                refs(doc, o, &mut glyphs);
            }
        }
        if inspect::named(d, b"Type", b"Page")
            && let Ok(o) = d.get(b"Contents")
        {
            refs(doc, o, &mut pages);
        }
    }
    let mut audit = SizeAudit {
        file_bytes,
        objects: doc.objects.len(),
        ..Default::default()
    };
    for (id, object) in &doc.objects {
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        let n = stream.content.len() as u64;
        audit.streams += 1;
        let target = if masks.contains(id) {
            &mut audit.masks
        } else if inspect::named(&stream.dict, b"Subtype", b"Image") {
            &mut audit.images
        } else if fonts.contains(id) {
            &mut audit.fonts
        } else if pages.contains(id)
            || glyphs.contains(id)
            || inspect::named(&stream.dict, b"Subtype", b"Form")
        {
            &mut audit.drawing
        } else if inspect::named(&stream.dict, b"Type", b"EmbeddedFile") {
            &mut audit.attachments
        } else if inspect::named(&stream.dict, b"Type", b"Metadata") {
            &mut audit.metadata
        } else if inspect::is_container(object) {
            &mut audit.containers
        } else {
            &mut audit.other_streams
        };
        *target += n;
    }
    let payload = payload(doc);
    audit.syntax_and_dictionaries = file_bytes.saturating_sub(payload);
    audit
}
pub(crate) fn payload(doc: &Document) -> u64 {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .map(|s| s.content.len() as u64)
        .sum()
}
pub(crate) struct Stage {
    name: &'static str,
    started: Instant,
    bytes: u64,
    objects: usize,
}
impl Stage {
    pub fn start(name: &'static str, doc: &Document) -> Self {
        Self {
            name,
            started: Instant::now(),
            bytes: payload(doc),
            objects: doc.objects.len(),
        }
    }
    pub fn finish(self, doc: &Document) -> StageResult {
        StageResult {
            stage: self.name.into(),
            before_stream_bytes: self.bytes,
            after_stream_bytes: payload(doc),
            before_objects: self.objects,
            after_objects: doc.objects.len(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        }
    }
}
