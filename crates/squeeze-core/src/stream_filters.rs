//! Strict, bounded reversible PDF filters. Never use a decoder's partial output.
use crate::{Error, Result, check_cancel};
use flate2::{Decompress, FlushDecompress, Status};
use lopdf::{Dictionary, Document, Object, Stream};
use std::sync::atomic::AtomicBool;
use weezl::{BitOrder, LzwStatus, decode::Decoder};

#[derive(Clone)]
pub(crate) struct Filter {
    pub name: Vec<u8>,
    pub params: Option<Object>,
}
fn invalid() -> Error {
    Error::Invalid("流编码或参数无效".into())
}
fn limit() -> Error {
    Error::Limit("流解码预算不足".into())
}
pub(crate) fn space(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}
fn resolve<'a>(doc: &'a Document, mut o: &'a Object) -> Result<&'a Object> {
    // A small bounded walk also rejects reference cycles.
    for _ in 0..16 {
        match o {
            Object::Reference(id) => o = doc.get_object(*id)?,
            _ => return Ok(o),
        }
    }
    Err(invalid())
}
pub(crate) fn filters(doc: &Document, s: &Stream) -> Result<Vec<Filter>> {
    if [b"F".as_slice(), b"FFilter", b"FDecodeParms"]
        .iter()
        .any(|k| s.dict.has(k))
    {
        return Err(Error::Unsupported("外部流".into()));
    }
    let (names, array) = match s.dict.get(b"Filter") {
        Err(_) => {
            if s.dict.has(b"DecodeParms") {
                return Err(invalid());
            }
            return Ok(Vec::new());
        }
        Ok(o) => match resolve(doc, o)? {
            Object::Name(n) => (vec![n.clone()], false),
            Object::Array(a) if !a.is_empty() && a.len() <= 4 => (
                a.iter()
                    .map(|o| Ok(resolve(doc, o)?.as_name()?.to_vec()))
                    .collect::<Result<Vec<_>>>()?,
                true,
            ),
            _ => return Err(invalid()),
        },
    };
    let params = match s.dict.get(b"DecodeParms") {
        Err(_) => vec![None; names.len()],
        Ok(o) => match resolve(doc, o)? {
            Object::Null => vec![None; names.len()],
            Object::Dictionary(_) if !array => vec![Some(o.clone())],
            Object::Array(a) if array && a.len() == names.len() => {
                let mut params = Vec::new();
                for o in a {
                    params.push(match resolve(doc, o)? {
                        Object::Null => None,
                        Object::Dictionary(_) => Some(o.clone()),
                        _ => return Err(invalid()),
                    });
                }
                params
            }
            _ => return Err(invalid()),
        },
    };
    Ok(names
        .into_iter()
        .zip(params)
        .map(|(name, params)| Filter { name, params })
        .collect())
}
fn dictionary<'a>(doc: &'a Document, filter: &'a Filter) -> Result<Option<&'a Dictionary>> {
    filter
        .params
        .as_ref()
        .map(|o| Ok(resolve(doc, o)?.as_dict()?))
        .transpose()
}
fn integer(doc: &Document, dict: Option<&Dictionary>, key: &[u8], default: i64) -> Result<i64> {
    match dict.and_then(|d| d.get(key).ok()) {
        None => Ok(default),
        Some(o) => Ok(resolve(doc, o)?.as_i64()?),
    }
}
fn append(out: &mut Vec<u8>, data: &[u8], cap: usize) -> Result<()> {
    if data.len() > cap.saturating_sub(out.len()) {
        return Err(limit());
    }
    out.extend_from_slice(data);
    Ok(())
}

/// Strict zlib framing, checksum, end marker, and output cap. No raw-deflate or
/// partial-result recovery, and no silently discarded trailing encoded bytes.
pub(crate) fn flate(input: &[u8], cap: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut decoder = Decompress::new(true);
    let mut out = Vec::new();
    let mut offset = 0;
    let mut chunk = [0u8; 16384];
    loop {
        check_cancel(cancel)?;
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(&input[offset..], &mut chunk, FlushDecompress::None)
            .map_err(|_| invalid())?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let written = (decoder.total_out() - before_out) as usize;
        offset += consumed;
        append(&mut out, &chunk[..written], cap)?;
        if status == Status::StreamEnd {
            if offset != input.len() {
                return Err(invalid());
            }
            return Ok(out);
        }
        if consumed == 0 && written == 0 {
            return Err(invalid());
        }
    }
}
fn ascii_hex(input: &[u8], cap: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut high = None;
    for (i, &c) in input.iter().enumerate() {
        if i & 0x3fff == 0 {
            check_cancel(cancel)?;
        }
        if c == b'>' {
            if !input[i + 1..].iter().all(|&c| space(c)) {
                return Err(invalid());
            }
            if let Some(h) = high {
                append(&mut out, &[h << 4], cap)?;
            }
            return Ok(out);
        }
        if space(c) {
            continue;
        }
        let n = match c {
            b'0'..=b'9' => c - b'0',
            b'A'..=b'F' => c - b'A' + 10,
            b'a'..=b'f' => c - b'a' + 10,
            _ => return Err(invalid()),
        };
        match high.take() {
            Some(h) => append(&mut out, &[(h << 4) | n], cap)?,
            None => high = Some(n),
        }
    }
    Err(invalid())
}
fn ascii85(input: &[u8], cap: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let (mut value, mut count) = (0u32, 0usize);
    for (i, &c) in input.iter().enumerate() {
        if i & 0x3fff == 0 {
            check_cancel(cancel)?;
        }
        if c == b'~' {
            if input.get(i + 1) != Some(&b'>')
                || count == 1
                || !input[i + 2..].iter().all(|&c| space(c))
            {
                return Err(invalid());
            }
            if count > 1 {
                for _ in count..5 {
                    value = value
                        .checked_mul(85)
                        .and_then(|v| v.checked_add(84))
                        .ok_or_else(invalid)?;
                }
                append(&mut out, &value.to_be_bytes()[..count - 1], cap)?;
            }
            return Ok(out);
        }
        if space(c) {
            continue;
        }
        if c == b'z' {
            if count != 0 {
                return Err(invalid());
            }
            append(&mut out, &[0; 4], cap)?;
            continue;
        }
        if !(b'!'..=b'u').contains(&c) {
            return Err(invalid());
        }
        value = value
            .checked_mul(85)
            .and_then(|v| v.checked_add((c - b'!') as u32))
            .ok_or_else(invalid)?;
        count += 1;
        if count == 5 {
            append(&mut out, &value.to_be_bytes(), cap)?;
            value = 0;
            count = 0;
        }
    }
    Err(invalid())
}
fn run_length(input: &[u8], cap: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&n) = input.get(i) {
        check_cancel(cancel)?;
        i += 1;
        match n {
            128 => {
                if !input[i..].iter().all(|&c| space(c)) {
                    return Err(invalid());
                }
                return Ok(out);
            }
            0..=127 => {
                let end = i + n as usize + 1;
                append(&mut out, input.get(i..end).ok_or_else(invalid)?, cap)?;
                i = end;
            }
            _ => {
                let byte = *input.get(i).ok_or_else(invalid)?;
                let size = 257 - n as usize;
                if size > cap.saturating_sub(out.len()) {
                    return Err(limit());
                }
                out.resize(out.len() + size, byte);
                i += 1;
            }
        }
    }
    Err(invalid())
}
fn lzw(input: &[u8], cap: usize, early: i64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut decoder = if early == 1 {
        Decoder::with_tiff_size_switch(BitOrder::Msb, 8)
    } else {
        Decoder::new(BitOrder::Msb, 8)
    };
    let mut out = Vec::new();
    let mut offset = 0;
    let mut chunk = [0u8; 16384];
    loop {
        check_cancel(cancel)?;
        let result = decoder.decode_bytes(&input[offset..], &mut chunk);
        offset += result.consumed_in;
        append(&mut out, &chunk[..result.consumed_out], cap)?;
        match result.status.map_err(|_| invalid())? {
            LzwStatus::Done => {
                // Residual bits are held by the decoder; no following bytes
                // are interpreted as another stream.
                if offset != input.len() {
                    return Err(invalid());
                }
                return Ok(out);
            }
            LzwStatus::NoProgress => return Err(invalid()),
            LzwStatus::Ok if result.consumed_in == 0 && result.consumed_out == 0 => {
                return Err(invalid());
            }
            _ => {}
        }
    }
}
pub(crate) fn layer(
    doc: &Document,
    filter: &Filter,
    input: &[u8],
    cap: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let params = dictionary(doc, filter)?;
    if matches!(filter.name.as_slice(), b"FlateDecode" | b"LZWDecode")
        && integer(doc, params, b"Predictor", 1)? != 1
    {
        return Err(Error::Unsupported("带预测器的流保留编码".into()));
    }
    layer_bytes(doc, filter, input, cap, cancel)
}
/// Decodes only the compression layer. Image callers must separately validate
/// and undo Predictor using its declared row layout, not infer it from pixels.
pub(crate) fn layer_bytes(
    doc: &Document,
    filter: &Filter,
    input: &[u8],
    cap: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    check_cancel(cancel)?;
    let params = dictionary(doc, filter)?;
    let allowed: &[&[u8]] = match filter.name.as_slice() {
        b"FlateDecode" => &[b"Predictor", b"Colors", b"Columns", b"BitsPerComponent"],
        b"LZWDecode" => &[
            b"Predictor",
            b"Colors",
            b"Columns",
            b"BitsPerComponent",
            b"EarlyChange",
        ],
        b"ASCII85Decode" | b"ASCIIHexDecode" | b"RunLengthDecode" => &[],
        _ => return Err(Error::Unsupported("未知流编码".into())),
    };
    if params.is_some_and(|d| d.iter().any(|(k, _)| !allowed.contains(&k.as_slice()))) {
        return Err(Error::Unsupported("未知流参数".into()));
    }
    match filter.name.as_slice() {
        b"FlateDecode" => flate(input, cap, cancel),
        b"ASCII85Decode" => ascii85(input, cap, cancel),
        b"ASCIIHexDecode" => ascii_hex(input, cap, cancel),
        b"RunLengthDecode" => run_length(input, cap, cancel),
        b"LZWDecode" => {
            let early = integer(doc, params, b"EarlyChange", 1)?;
            if !matches!(early, 0 | 1) {
                return Err(invalid());
            }
            lzw(input, cap, early, cancel)
        }
        _ => unreachable!(),
    }
}
pub(crate) fn decode(
    doc: &Document,
    s: &Stream,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    check_cancel(cancel)?;
    let chain = filters(doc, s)?;
    if chain.is_empty() {
        if s.content.len() > cap {
            return Err(limit());
        }
        return Ok(s.content.clone());
    }
    let mut decoded: Option<Vec<u8>> = None;
    for filter in chain {
        decoded = Some(layer(
            doc,
            &filter,
            decoded.as_deref().unwrap_or(&s.content),
            cap,
            cancel,
        )?);
    }
    Ok(decoded.unwrap())
}
/// Shared by content/resource analyses. Unsupported or damaged bytes must not
/// participate in resource removal, resizing, or drawing rewrites.
pub(crate) fn content(doc: &Document, s: &Stream, cap: usize) -> lopdf::Result<Vec<u8>> {
    decode(doc, s, cap, &AtomicBool::new(false))
        .map_err(|_| lopdf::Error::InvalidStream("内容流未通过严格解码".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    fn decode_name(name: &str, data: &[u8], cap: usize) -> Result<Vec<u8>> {
        let doc = Document::new();
        decode(
            &doc,
            &Stream::new(dictionary! {"Filter"=>name}, data.to_vec()),
            cap,
            &AtomicBool::new(false),
        )
    }
    #[test]
    fn strict_ascii_vectors_and_terminators() {
        assert_eq!(
            decode_name("ASCII85Decode", b"87cURD_*#TDfTZ)+T~>\n", 32).unwrap(),
            b"Hello, world!"
        );
        assert_eq!(decode_name("ASCII85Decode", b"z~>", 4).unwrap(), [0; 4]);
        assert_eq!(
            decode_name("ASCIIHexDecode", b"6 1\x002>\n", 4).unwrap(),
            [0x61, 0x20]
        );
        for bytes in [
            b"uuuuu~>".as_slice(),
            b"!~>",
            b"!!z~>",
            b"87cUR",
            b"!!~>trash",
            b"!!?~~>",
        ] {
            assert!(
                decode_name("ASCII85Decode", bytes, 100).is_err(),
                "{bytes:?}"
            );
        }
        for bytes in [b"61".as_slice(), b"61x>", b"61>q"] {
            assert!(decode_name("ASCIIHexDecode", bytes, 100).is_err());
        }
        assert!(matches!(
            decode_name("ASCII85Decode", b"zz~>", 7),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            decode_name("ASCIIHexDecode", b"6162>", 1),
            Err(Error::Limit(_))
        ));
    }
    #[test]
    fn run_length_rejects_truncation_and_excess_output() {
        assert_eq!(
            decode_name("RunLengthDecode", &[1, b'a', b'b', 254, b'c', 128], 5).unwrap(),
            b"abccc"
        );
        for bytes in [vec![2, 1, 2], vec![255], vec![0, 1], vec![128, 1]] {
            assert!(decode_name("RunLengthDecode", &bytes, 10).is_err());
        }
        assert!(matches!(
            decode_name("RunLengthDecode", &[129, 0, 128], 127),
            Err(Error::Limit(_))
        ));
    }
    #[test]
    fn lzw_both_width_switches_cross_all_boundaries() {
        let raw: Vec<u8> = (0..24000u32)
            .map(|i| (i.wrapping_mul(73) ^ (i >> 3) ^ (i >> 8)) as u8)
            .collect();
        for early in [0, 1] {
            let mut encoder = if early == 1 {
                weezl::encode::Encoder::with_tiff_size_switch(BitOrder::Msb, 8)
            } else {
                weezl::encode::Encoder::new(BitOrder::Msb, 8)
            };
            let encoded = encoder.encode(&raw).unwrap();
            let doc = Document::new();
            let s = Stream::new(
                dictionary! {"Filter"=>"LZWDecode","DecodeParms"=>dictionary!{"EarlyChange"=>early}},
                encoded.clone(),
            );
            assert_eq!(
                decode(&doc, &s, raw.len(), &AtomicBool::new(false)).unwrap(),
                raw
            );
            assert!(matches!(
                decode(&doc, &s, 100, &AtomicBool::new(false)),
                Err(Error::Limit(_))
            ));
            let mut truncated = s.clone();
            truncated.content.truncate(encoded.len() / 2);
            assert!(decode(&doc, &truncated, raw.len(), &AtomicBool::new(false)).is_err());
        }
    }
    #[test]
    fn indirect_chain_parameters_are_per_filter_and_bounded() {
        let mut doc = Document::new();
        let filter = doc.add_object(Object::Name(b"RunLengthDecode".to_vec()));
        let filters = doc.add_object(vec![
            Object::Name(b"ASCIIHexDecode".to_vec()),
            filter.into(),
        ]);
        let empty = doc.add_object(dictionary! {});
        let params = doc.add_object(vec![Object::Null, empty.into()]);
        let s = Stream::new(
            dictionary! {"Filter"=>filters,"DecodeParms"=>params},
            b"0261626380>".to_vec(),
        );
        assert_eq!(
            decode(&doc, &s, 16, &AtomicBool::new(false)).unwrap(),
            b"abc"
        );
        let mut bad = s.clone();
        bad.dict.set("DecodeParms", vec![Object::Null]);
        assert!(decode(&doc, &bad, 16, &AtomicBool::new(false)).is_err());
        bad = s.clone();
        bad.dict.set("F", Object::string_literal("external"));
        assert!(decode(&doc, &bad, 16, &AtomicBool::new(false)).is_err());
        bad = s.clone();
        bad.dict.set("Filter", "Crypt");
        assert!(decode(&doc, &bad, 16, &AtomicBool::new(false)).is_err());
        assert!(matches!(
            decode(&doc, &s, 16, &AtomicBool::new(true)),
            Err(Error::Cancelled)
        ));
        let cycle = doc.new_object_id();
        doc.objects.insert(cycle, Object::Reference(cycle));
        bad.dict.set("Filter", cycle);
        assert!(decode(&doc, &bad, 16, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn flate_requires_checksum_end_marker_and_complete_payload() {
        let raw = b"q 0 0 100 100 re f Q\n".repeat(100);
        let encoded = crate::lossless::deflate(&raw).unwrap();
        assert_eq!(
            decode_name("FlateDecode", &encoded, raw.len()).unwrap(),
            raw
        );
        assert!(matches!(
            decode_name("FlateDecode", &encoded, 20),
            Err(Error::Limit(_))
        ));
        let mut bad = encoded.clone();
        bad.pop();
        assert!(decode_name("FlateDecode", &bad, raw.len()).is_err());
        let mut bad = encoded.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        assert!(decode_name("FlateDecode", &bad, raw.len()).is_err());
        let mut bad = encoded;
        bad.push(0);
        assert!(decode_name("FlateDecode", &bad, raw.len()).is_err());
    }
}
