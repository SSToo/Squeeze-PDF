use crate::{DocumentInfo, Error, Limits, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashSet;
pub(crate) fn dictionary(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}
pub(crate) fn named(dict: &Dictionary, key: &[u8], value: &[u8]) -> bool {
    dict.get(key).and_then(Object::as_name).ok() == Some(value)
}
pub(crate) fn is_container(object: &Object) -> bool {
    matches!(object.type_name(), Ok(b"ObjStm" | b"XRef"))
        || dictionary(object).is_some_and(|d| d.has(b"Linearized"))
}
pub(crate) fn document(doc: &Document, limits: &Limits) -> Result<DocumentInfo> {
    if doc.objects.len() > limits.max_objects {
        return Err(Error::Limit("PDF 对象数量过多".into()));
    }
    if doc.encryption_state.is_some() || doc.trailer.has(b"Encrypt") {
        return Err(Error::Unsupported("加密 PDF".into()));
    }
    if doc.catalog()?.has(b"Collection") {
        return Err(Error::Unsupported("PDF 组合包".into()));
    }
    let mut interactive = false;
    let mut conservative = false;
    for object in doc.objects.values() {
        scan(doc, object, &mut interactive, &mut conservative, 0)?;
        if dictionary(object).is_some_and(|d| named(d, b"Subtype", b"Image"))
            && !matches!(object, Object::Stream(_))
        {
            return Err(Error::Invalid("图片对象缺少数据流".into()));
        }
        if let Object::Stream(s) = object
            && named(&s.dict, b"Type", b"Metadata")
        {
            let metadata = s.get_plain_content_with_limit(limits.max_stream_bytes)?;
            let marker = b"www.aiim.org/pdfa/ns/id";
            if metadata.windows(marker.len()).any(|s| s == marker) {
                return Err(Error::Unsupported("PDF/A 文档".into()));
            }
        }
    }
    crate::validate::references(doc)?;
    let pages = pages(doc)?;
    if pages.is_empty() {
        return Err(Error::Invalid("PDF 没有页面".into()));
    }
    let images = doc
        .objects
        .values()
        .filter(|o| dictionary(o).is_some_and(|d| named(d, b"Subtype", b"Image")))
        .count();
    Ok(DocumentInfo {
        bytes: 0,
        pages: pages.len(),
        images,
        has_interactive_content: interactive,
        requires_conservative_optimization: conservative,
    })
}
fn scan(
    doc: &Document,
    object: &Object,
    interactive: &mut bool,
    conservative: &mut bool,
    depth: usize,
) -> Result<()> {
    if depth > 64 {
        return Err(Error::Limit("PDF 对象嵌套过深".into()));
    }
    if let Some(d) = dictionary(object) {
        if named(d, b"Type", b"Sig")
            || d.has(b"ByteRange")
            || (named(d, b"FT", b"Sig") && d.get(b"V").is_ok_and(|v| !matches!(v, Object::Null)))
            || d.has(b"DocMDP")
        {
            return Err(Error::Unsupported("已有数字签名的 PDF".into()));
        }
        if d.has(b"XFA") {
            return Err(Error::Unsupported("XFA 表单".into()));
        }
        let has_annots = match d.get(b"Annots") {
            Ok(o) => match doc.dereference(o)?.1 {
                Object::Null => false,
                Object::Array(a) => {
                    for annotation in a {
                        let d = doc.dereference(annotation)?.1.as_dict()?;
                        // Link targets and rectangles are preserved unchanged.
                        // Without an appearance stream they cannot share the
                        // image or drawing resources that we optimize.
                        if !named(d, b"Subtype", b"Link") || d.has(b"AP") {
                            *conservative = true;
                        }
                    }
                    !a.is_empty()
                }
                _ => return Err(Error::Invalid("页面批注列表无效".into())),
            },
            Err(_) => false,
        };
        if has_annots || d.has(b"AcroForm") || d.has(b"EmbeddedFiles") {
            *interactive = true;
        }
        if d.has(b"AcroForm") {
            *conservative = true;
        }
        for (_, value) in d.iter() {
            scan(doc, value, interactive, conservative, depth + 1)?;
        }
    } else if let Object::Array(items) = object {
        for value in items {
            scan(doc, value, interactive, conservative, depth + 1)?;
        }
    }
    Ok(())
}
pub(crate) fn pages(doc: &Document) -> Result<Vec<ObjectId>> {
    fn visit(
        doc: &Document,
        id: ObjectId,
        seen: &mut HashSet<ObjectId>,
        result: &mut Vec<ObjectId>,
        depth: usize,
    ) -> Result<usize> {
        if depth > 64 || !seen.insert(id) {
            return Err(Error::Invalid("页面树存在循环或过深嵌套".into()));
        }
        let d = doc.get_dictionary(id)?;
        if named(d, b"Type", b"Page") {
            result.push(id);
            return Ok(1);
        }
        if !named(d, b"Type", b"Pages") {
            return Err(Error::Invalid("页面树节点类型错误".into()));
        }
        let mut count = 0;
        for kid in d.get(b"Kids")?.as_array()? {
            count += visit(doc, kid.as_reference()?, seen, result, depth + 1)?;
        }
        if d.get(b"Count")?.as_i64()? != count as i64 {
            return Err(Error::Invalid("页面树页数不一致".into()));
        }
        Ok(count)
    }
    let root = doc.catalog()?.get(b"Pages")?.as_reference()?;
    let mut result = Vec::new();
    visit(doc, root, &mut HashSet::new(), &mut result, 0)?;
    Ok(result)
}
pub(crate) fn inherited<'a>(
    doc: &'a Document,
    id: ObjectId,
    key: &[u8],
) -> Result<Option<&'a Object>> {
    let mut current = id;
    let mut seen = HashSet::new();
    for _ in 0..64 {
        if !seen.insert(current) {
            return Err(Error::Invalid("页面继承存在循环".into()));
        }
        let d = doc.get_dictionary(current)?;
        if let Ok(value) = d.get(key) {
            return Ok(Some(doc.dereference(value)?.1));
        }
        match d.get(b"Parent") {
            Ok(p) => current = p.as_reference()?,
            Err(_) => return Ok(None),
        }
    }
    Err(Error::Limit("页面继承过深".into()))
}
