use crate::{Error, Result, inspect};
use lopdf::{Dictionary, Document, Object};
pub(crate) fn references(doc: &Document) -> Result<()> {
    fn walk(doc: &Document, o: &Object, depth: usize) -> Result<()> {
        if depth > 64 {
            return Err(Error::Limit("对象嵌套过深".into()));
        }
        match o {
            Object::Reference(id) if !doc.objects.contains_key(id) => {
                return Err(Error::Invalid(format!("缺失引用 {} {}", id.0, id.1)));
            }
            Object::Array(items) => {
                for o in items {
                    walk(doc, o, depth + 1)?;
                }
            }
            Object::Dictionary(d) => {
                for (_, o) in d.iter() {
                    walk(doc, o, depth + 1)?;
                }
            }
            Object::Stream(s) => {
                for (_, o) in s.dict.iter() {
                    walk(doc, o, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for o in doc.objects.values() {
        if !inspect::is_container(o) {
            walk(doc, o, 0)?;
        }
    }
    for key in [b"Root".as_slice(), b"Info", b"ID"] {
        if let Ok(o) = doc.trailer.get(key) {
            walk(doc, o, 0)?;
        }
    }
    Ok(())
}
pub(crate) fn round_trip(expected: &Document, output: &Document) -> Result<()> {
    references(output)?;
    if inspect::pages(expected)? != inspect::pages(output)? {
        return Err(Error::Validation("页面数量或顺序发生变化".into()));
    }
    for (&id, o) in &expected.objects {
        if inspect::is_container(o) {
            continue;
        }
        let actual = output
            .objects
            .get(&id)
            .ok_or_else(|| Error::Validation(format!("对象 {} 丢失", id.0)))?;
        if !equivalent(o, actual) {
            return Err(Error::Validation(format!(
                "对象 {} 写入后发生非预期变化",
                id.0
            )));
        }
    }
    for key in [b"Root".as_slice(), b"Info", b"ID"] {
        if let Ok(o) = expected.trailer.get(key)
            && !output.trailer.get(key).is_ok_and(|a| equivalent(o, a))
        {
            return Err(Error::Validation("文档目录或元数据引用发生变化".into()));
        }
    }
    Ok(())
}
fn dict_equal(a: &Dictionary, b: &Dictionary, stream: bool) -> bool {
    let count = |d: &Dictionary| {
        d.iter()
            .filter(|(k, _)| !stream || k.as_slice() != b"Length")
            .count()
    };
    count(a) == count(b)
        && a.iter()
            .all(|(k, v)| (stream && k == b"Length") || b.get(k).is_ok_and(|b| equivalent(v, b)))
}
fn equivalent(a: &Object, b: &Object) -> bool {
    match (a, b) {
        (Object::Integer(a), Object::Integer(b)) => a == b,
        (Object::Integer(a), Object::Real(b)) => *a as f64 == *b as f64,
        (Object::Real(a), Object::Integer(b)) => *a as f64 == *b as f64,
        (Object::Real(a), Object::Real(b)) => a == b,
        (Object::Null, Object::Null) => true,
        (Object::Boolean(a), Object::Boolean(b)) => a == b,
        (Object::Name(a), Object::Name(b)) => a == b,
        (Object::String(a, _), Object::String(b, _)) => a == b,
        (Object::Reference(a), Object::Reference(b)) => a == b,
        (Object::Array(a), Object::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Object::Dictionary(a), Object::Dictionary(b)) => dict_equal(a, b, false),
        (Object::Stream(a), Object::Stream(b)) => {
            a.content == b.content && dict_equal(&a.dict, &b.dict, true)
        }
        _ => false,
    }
}
