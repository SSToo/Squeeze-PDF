//! Shared conservative barrier for aliasing identity-bearing PDF objects.
use crate::{Limits, inspect};
use lopdf::{Document, Object};

pub(crate) fn bound(doc: &Document, limits: &Limits) -> bool {
    fn visit(o: &Object, remaining: &mut usize, depth: usize) -> bool {
        if *remaining == 0 || depth > 64 {
            return true;
        }
        *remaining -= 1;
        match o {
            Object::Array(a) => a.iter().any(|o| visit(o, remaining, depth + 1)),
            _ => inspect::dictionary(o).is_some_and(|d| {
                [
                    b"StructTreeRoot".as_slice(),
                    b"StructParent",
                    b"StructParents",
                    b"Stm",
                    b"MarkInfo",
                    b"AP",
                    b"AcroForm",
                    b"FT",
                ]
                .iter()
                .any(|key| d.has(key))
                    || [
                        b"MCR".as_slice(),
                        b"OBJR",
                        b"StructElem",
                        b"Annot",
                        b"OCG",
                        b"OCMD",
                    ]
                    .iter()
                    .any(|name| inspect::named(d, b"Type", name))
                    || d.iter().any(|(_, o)| visit(o, remaining, depth + 1))
            }),
        }
    }
    let mut remaining = limits.max_operations;
    doc.objects.values().any(|o| visit(o, &mut remaining, 0))
        || doc.trailer.iter().any(|(_, o)| visit(o, &mut remaining, 0))
}
