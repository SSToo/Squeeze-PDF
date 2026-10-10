//! Row-bounded exact component transcoding. Candidate payloads live on disk.
use crate::{Options, Result, check_cancel, inspect, native_samples, stream_filters};
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use lopdf::{Document, Object, Stream, dictionary};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::Instant,
};
use tempfile::NamedTempFile;

#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub considered: usize,
    pub changed: usize,
    pub skipped_budget: usize,
    pub skipped_invalid: usize,
    pub decoded_work_bytes: usize,
    pub max_row_bytes: usize,
    pub elapsed_ms: u64,
}
#[derive(Clone, Copy)]
struct Layout {
    width: usize,
    height: usize,
    stride: usize,
    channels: usize,
    bits: usize,
    predictor: i64,
}
enum Input<'a> {
    Raw(&'a [u8]),
    Flate {
        codec: Decompress,
        remaining: &'a [u8],
        ended: bool,
    },
}
impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Raw(r) => r.read(out),
            Self::Flate {
                codec,
                remaining,
                ended,
            } => {
                if out.is_empty() || *ended {
                    return Ok(0);
                }
                loop {
                    let old_in = codec.total_in();
                    let old_out = codec.total_out();
                    let status = codec
                        .decompress(remaining, out, FlushDecompress::None)
                        .map_err(|_| std::io::ErrorKind::InvalidData)?;
                    let consumed = (codec.total_in() - old_in) as usize;
                    let written = (codec.total_out() - old_out) as usize;
                    *remaining = &remaining[consumed..];
                    *ended = status == Status::StreamEnd;
                    if written > 0 || *ended {
                        return Ok(written);
                    }
                    if consumed == 0 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                }
            }
        }
    }
}
struct Rows<'a> {
    input: Input<'a>,
    layout: Layout,
    previous: Vec<u8>,
    remaining: usize,
}
fn number(doc: &Document, o: &Object) -> Option<usize> {
    usize::try_from(doc.dereference(o).ok()?.1.as_i64().ok()?).ok()
}
impl<'a> Rows<'a> {
    fn open(doc: &Document, s: &'a Stream, options: &Options) -> Option<Self> {
        if s.dict
            .get(b"SMaskInData")
            .is_ok_and(|o| number(doc, o) != Some(0))
        {
            return None;
        }
        let width = number(doc, s.dict.get(b"Width").ok()?)?;
        let height = number(doc, s.dict.get(b"Height").ok()?)?;
        let bits = number(doc, s.dict.get(b"BitsPerComponent").ok()?)?;
        let mask = match s.dict.get(b"ImageMask") {
            Ok(o) => doc.dereference(o).ok()?.1.as_bool().ok()?,
            Err(_) => false,
        };
        let channels = if mask {
            1
        } else {
            native_samples::channels(doc, s.dict.get(b"ColorSpace").ok()?)?
        };
        if width == 0
            || height == 0
            || !matches!(bits, 1 | 2 | 4 | 8 | 16)
            || (mask && bits != 1)
            || (width as u64).checked_mul(height as u64)? > options.limits.max_image_pixels
        {
            return None;
        }
        let stride = width.checked_mul(channels)?.checked_mul(bits)?.div_ceil(8);
        if stride > 1024 * 1024 || stride.checked_mul(height)? > options.limits.max_stream_bytes {
            return None;
        }
        let filters = stream_filters::filters(doc, s).ok()?;
        let filtered = filters.len() == 1 && filters[0].name == b"FlateDecode";
        if !filters.is_empty() && !filtered {
            return None;
        }
        let params = filters
            .first()
            .and_then(|f| f.params.as_ref())
            .map(|o| doc.dereference(o).ok()?.1.as_dict().ok())
            .transpose_option()?;
        if params.is_some_and(|d| {
            d.iter().any(|(k, _)| {
                !matches!(
                    k.as_slice(),
                    b"Predictor" | b"Colors" | b"BitsPerComponent" | b"Columns"
                )
            })
        }) {
            return None;
        }
        let get = |key: &[u8], default: usize| -> Option<usize> {
            match params.and_then(|d| d.get(key).ok()) {
                Some(o) => number(doc, o),
                None => Some(default),
            }
        };
        let predictor = get(b"Predictor", 1)? as i64;
        if !matches!(predictor, 1 | 2 | 10..=15)
            || (predictor != 1
                && (get(b"Colors", 1)? != channels
                    || get(b"BitsPerComponent", 8)? != bits
                    || get(b"Columns", 1)? != width))
        {
            return None;
        }
        Some(Self {
            input: if filtered {
                Input::Flate {
                    codec: Decompress::new(true),
                    remaining: &s.content,
                    ended: false,
                }
            } else {
                Input::Raw(&s.content)
            },
            layout: Layout {
                width,
                height,
                stride,
                channels,
                bits,
                predictor,
            },
            previous: vec![0; stride],
            remaining: height,
        })
    }
    fn next(&mut self, row: &mut [u8]) -> std::io::Result<()> {
        if self.remaining == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        let l = self.layout;
        let tag = if l.predictor >= 10 {
            let mut tag = [0];
            self.input.read_exact(&mut tag)?;
            tag[0]
        } else {
            0
        };
        if tag > 4 || (l.predictor >= 10 && l.predictor != 15 && i64::from(tag) != l.predictor - 10)
        {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        self.input.read_exact(row)?;
        let bpp = (l.channels * l.bits).div_ceil(8);
        if l.predictor == 2 {
            if l.bits < 8 {
                let mask = (1u8 << l.bits) - 1;
                for i in l.channels..l.width * l.channels {
                    let bit = i * l.bits;
                    let previous = (i - l.channels) * l.bits;
                    let shift = 8 - l.bits - bit % 8;
                    let old_shift = 8 - l.bits - previous % 8;
                    let value = ((row[bit / 8] >> shift) & mask)
                        .wrapping_add((row[previous / 8] >> old_shift) & mask)
                        & mask;
                    row[bit / 8] = (row[bit / 8] & !(mask << shift)) | (value << shift);
                }
            } else {
                let sample = l.bits / 8;
                for i in (l.channels * sample..l.stride).step_by(sample) {
                    if sample == 1 {
                        row[i] = row[i].wrapping_add(row[i - l.channels]);
                    } else {
                        let p = i - l.channels * 2;
                        let v = u16::from_be_bytes([row[i], row[i + 1]])
                            .wrapping_add(u16::from_be_bytes([row[p], row[p + 1]]));
                        row[i..i + 2].copy_from_slice(&v.to_be_bytes());
                    }
                }
            }
        } else if l.predictor >= 10 {
            for x in 0..l.stride {
                let left = if x >= bpp { row[x - bpp] } else { 0 };
                let up = self.previous[x];
                let corner = if x >= bpp { self.previous[x - bpp] } else { 0 };
                row[x] = row[x].wrapping_add(match tag {
                    0 => 0,
                    1 => left,
                    2 => up,
                    3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    _ => crate::image_encoding::paeth(left, up, corner),
                });
            }
        }
        self.previous.copy_from_slice(row);
        self.remaining -= 1;
        Ok(())
    }
    fn finish(&mut self) -> std::io::Result<()> {
        let mut extra = [0];
        if self.remaining != 0 || self.input.read(&mut extra)? != 0 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        if let Input::Flate {
            remaining, ended, ..
        } = &self.input
            && (!ended || !remaining.is_empty())
        {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
}
// Keep malformed indirect parameters distinct from absent parameters.
trait Optional<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> Optional<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(Some(v)) => Some(Some(v)),
            Some(None) => None,
        }
    }
}

fn transcode(
    doc: &Document,
    s: &Stream,
    options: &Options,
    parent: &Path,
    cancel: &AtomicBool,
    stats: &mut Stats,
    started: Instant,
) -> Result<Option<Stream>> {
    let Some(mut rows) = Rows::open(doc, s, options) else {
        return Ok(None);
    };
    let l = rows.layout;
    let decoded = l.stride * l.height;
    if decoded <= 4 * 1024 * 1024 {
        return Ok(None);
    }
    stats.considered += 1;
    if decoded > (256 * 1024 * 1024usize).saturating_sub(stats.decoded_work_bytes) {
        stats.skipped_budget += 1;
        return Ok(None);
    }
    stats.decoded_work_bytes += decoded;
    stats.max_row_bytes = stats.max_row_bytes.max(l.stride);
    let mut encoders = Vec::new();
    for _ in 0..3 {
        encoders.push(ZlibEncoder::new(
            NamedTempFile::new_in(parent)?,
            Compression::new(6),
        ));
    }
    let mut row = vec![0; l.stride];
    let mut previous = vec![0; l.stride];
    let mut predicted = vec![0; l.stride + 1];
    let mut trial = vec![0; l.stride + 1];
    let mut digest = Sha256::new();
    for _ in 0..l.height {
        check_cancel(cancel)?;
        if started.elapsed().as_secs() >= 30 {
            stats.skipped_budget += 1;
            return Ok(None);
        }
        if rows.next(&mut row).is_err() {
            stats.skipped_invalid += 1;
            return Ok(None);
        }
        digest.update(&row);
        for (strategy, encoder) in encoders.iter_mut().enumerate() {
            let filters: &[u8] = match strategy {
                0 => &[1],
                1 => &[2],
                _ => &[0, 1, 2, 3, 4],
            };
            let mut best = u64::MAX;
            for &tag in filters {
                trial[0] = tag;
                let mut score = 0;
                for x in 0..l.stride {
                    let left = if x >= (l.channels * l.bits).div_ceil(8) {
                        row[x - (l.channels * l.bits).div_ceil(8)]
                    } else {
                        0
                    };
                    let up = previous[x];
                    let corner = if x >= (l.channels * l.bits).div_ceil(8) {
                        previous[x - (l.channels * l.bits).div_ceil(8)]
                    } else {
                        0
                    };
                    let value = row[x].wrapping_sub(match tag {
                        0 => 0,
                        1 => left,
                        2 => up,
                        3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                        _ => crate::image_encoding::paeth(left, up, corner),
                    });
                    trial[x + 1] = value;
                    score += i16::from(value as i8).unsigned_abs() as u64;
                }
                if score < best {
                    best = score;
                    predicted.copy_from_slice(&trial);
                }
            }
            encoder.write_all(&predicted)?;
        }
        previous.copy_from_slice(&row);
    }
    if rows.finish().is_err() {
        stats.skipped_invalid += 1;
        return Ok(None);
    }
    let expected = digest.finalize();
    let original_cost = crate::image_encoding::cost(s)?;
    let mut best_cost = original_cost;
    let mut selected = None;
    for encoder in encoders {
        check_cancel(cancel)?;
        let file = encoder.finish()?;
        if file.as_file().metadata()?.len() >= best_cost as u64 {
            continue;
        }
        let content = std::fs::read(file.path())?;
        let mut d = s.dict.clone();
        d.set("Filter", "FlateDecode");
        d.set("DecodeParms",dictionary!{"Predictor"=>15,"Colors"=>l.channels as i64,"BitsPerComponent"=>l.bits as i64,"Columns"=>l.width as i64});
        let candidate = Stream::new(d, content);
        let cost = crate::image_encoding::cost(&candidate)?;
        if cost >= best_cost {
            continue;
        }
        let Some(mut verify) = Rows::open(doc, &candidate, options) else {
            continue;
        };
        let mut digest = Sha256::new();
        for _ in 0..l.height {
            check_cancel(cancel)?;
            verify.next(&mut row)?;
            digest.update(&row);
        }
        verify.finish()?;
        if digest.finalize() != expected {
            return Err(crate::Error::Validation("大图逐行样本校验失败".into()));
        }
        best_cost = cost;
        selected = Some(candidate);
    }
    Ok(selected)
}
pub(crate) fn optimize(
    doc: &mut Document,
    options: &Options,
    parent: &Path,
    cancel: &AtomicBool,
) -> Result<Stats> {
    let started = Instant::now();
    let mut stats = Stats::default();
    let protected = crate::usage::analyze(doc, &options.limits, cancel)?.protected;
    let mut ids: Vec<_> = doc
        .objects
        .iter()
        .filter_map(|(&id, o)| {
            let s = o.as_stream().ok()?;
            if !inspect::named(&s.dict, b"Subtype", b"Image") || protected.contains(&id) {
                return None;
            }
            let rows = Rows::open(doc, s, options)?;
            Some((rows.layout.stride * rows.layout.height, id))
        })
        .collect();
    // Small useful work can reach later pages without a single giant image taking the budget.
    ids.sort_unstable();
    for (_, id) in ids {
        check_cancel(cancel)?;
        if started.elapsed().as_secs() >= 30 {
            stats.skipped_budget += 1;
            continue;
        }
        if let Some(stream) = transcode(
            doc,
            doc.get_object(id)?.as_stream()?,
            options,
            parent,
            cancel,
            &mut stats,
            started,
        )? {
            doc.objects.insert(id, Object::Stream(stream));
            stats.changed += 1;
        }
    }
    stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(stats)
}

pub(crate) fn decoded_size(doc: &Document, s: &Stream, options: &Options) -> Option<usize> {
    let rows = Rows::open(doc, s, options)?;
    rows.layout.stride.checked_mul(rows.layout.height)
}

pub(crate) fn fingerprint(
    doc: &Document,
    s: &Stream,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<Option<([u8; 32], usize)>> {
    let Some(mut rows) = Rows::open(doc, s, options) else {
        return Ok(None);
    };
    let mut digest = Sha256::new();
    let l = rows.layout;
    let mut row = vec![0; l.stride];
    for v in [l.width, l.height, l.bits, l.channels] {
        digest.update((v as u64).to_be_bytes());
    }
    for _ in 0..l.height {
        check_cancel(cancel)?;
        if rows.next(&mut row).is_err() {
            return Ok(None);
        }
        digest.update(&row);
    }
    if rows.finish().is_err() {
        return Ok(None);
    }
    Ok(Some((digest.finalize().into(), l.stride * l.height)))
}
pub(crate) fn same_samples(
    doc: &Document,
    a: &Stream,
    b: &Stream,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<bool> {
    let (Some(mut a), Some(mut b)) = (Rows::open(doc, a, options), Rows::open(doc, b, options))
    else {
        return Ok(false);
    };
    let x = a.layout;
    let y = b.layout;
    if [x.width, x.height, x.channels, x.bits] != [y.width, y.height, y.channels, y.bits] {
        return Ok(false);
    }
    let mut ar = vec![0; x.stride];
    let mut br = vec![0; x.stride];
    for _ in 0..x.height {
        check_cancel(cancel)?;
        if a.next(&mut ar).is_err() || b.next(&mut br).is_err() || ar != br {
            return Ok(false);
        }
    }
    Ok(a.finish().is_ok() && b.finish().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_decoding_rejects_bad_checksums_truncation_and_trailing_bytes() {
        let doc = Document::new();
        let options = Options::default();
        let raw = vec![150; 20 * 9 * 3];
        let s = Stream::new(
            dictionary! {"Subtype"=>"Image","Width"=>20,"Height"=>9,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB","Filter"=>"FlateDecode"},
            crate::lossless::deflate(&raw).unwrap(),
        );
        assert!(
            fingerprint(&doc, &s, &options, &AtomicBool::new(false))
                .unwrap()
                .is_some()
        );
        for bytes in [
            s.content[..s.content.len() - 1].to_vec(),
            [s.content.clone(), vec![0]].concat(),
        ] {
            let mut bad = s.clone();
            bad.set_content(bytes);
            assert!(
                fingerprint(&doc, &bad, &options, &AtomicBool::new(false))
                    .unwrap()
                    .is_none()
            );
        }
        let mut bad = s.clone();
        *bad.content.last_mut().unwrap() ^= 1;
        assert!(
            fingerprint(&doc, &bad, &options, &AtomicBool::new(false))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn image_above_old_cap_uses_disk_candidates_and_only_row_buffers() {
        let dir = tempfile::tempdir().unwrap();
        let doc = Document::new();
        let options = Options::default();
        let (w, h) = (1700, 1000);
        let raw: Vec<_> = (0..w * h * 3)
            .map(|i| ((i / 3 % w + i / (w * 3)) % 256) as u8)
            .collect();
        let s = Stream::new(
            dictionary! {"Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB"},
            raw,
        );
        assert!(native_samples::read(&doc, &s, &options).is_none());
        let mut stats = Stats::default();
        let candidate = transcode(
            &doc,
            &s,
            &options,
            dir.path(),
            &AtomicBool::new(false),
            &mut stats,
            Instant::now(),
        )
        .unwrap()
        .unwrap();
        assert!(candidate.content.len() < s.content.len() / 20);
        assert!(same_samples(&doc, &s, &candidate, &options, &AtomicBool::new(false)).unwrap());
        assert_eq!(stats.max_row_bytes, w * 3);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(matches!(
            transcode(
                &doc,
                &s,
                &options,
                dir.path(),
                &AtomicBool::new(true),
                &mut Stats::default(),
                Instant::now(),
            ),
            Err(crate::Error::Cancelled)
        ));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[test]
    fn expired_work_budget_closes_all_disk_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let doc = Document::new();
        let mut stats = Stats::default();
        let s = Stream::new(
            dictionary! {"Subtype"=>"Image","Width"=>1700,"Height"=>1000,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB"},
            vec![1; 1700 * 1000 * 3],
        );
        assert!(
            transcode(
                &doc,
                &s,
                &Options::default(),
                dir.path(),
                &AtomicBool::new(false),
                &mut stats,
                Instant::now() - std::time::Duration::from_secs(31)
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(stats.skipped_budget, 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[test]
    fn sixteen_bit_components_and_png_predictors_keep_component_bytes() {
        let doc = Document::new();
        let options = Options::default();
        for bits in [1, 2, 4, 8, 16] {
            let (w, h, channels) = (9usize, 3usize, 3usize);
            let stride = (w * channels * bits).div_ceil(8);
            let raw: Vec<_> = (0..stride * h).map(|i| (i * 31) as u8).collect();
            let s = Stream::new(
                dictionary! {"Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64,"BitsPerComponent"=>bits as i64,"ColorSpace"=>"DeviceRGB"},
                raw.clone(),
            );
            for tag in 0..5 {
                let mut predicted = Vec::new();
                let bpp = (channels * bits).div_ceil(8);
                for y in 0..h {
                    predicted.push(tag);
                    for x in 0..stride {
                        let left = if x >= bpp {
                            raw[y * stride + x - bpp]
                        } else {
                            0
                        };
                        let up = if y > 0 { raw[(y - 1) * stride + x] } else { 0 };
                        let corner = if y > 0 && x >= bpp {
                            raw[(y - 1) * stride + x - bpp]
                        } else {
                            0
                        };
                        predicted.push(raw[y * stride + x].wrapping_sub(match tag {
                            0 => 0,
                            1 => left,
                            2 => up,
                            3 => ((left as u16 + up as u16) / 2) as u8,
                            _ => crate::image_encoding::paeth(left, up, corner),
                        }));
                    }
                }
                let mut p = s.clone();
                p.dict.set("Filter", "FlateDecode");
                p.dict.set("DecodeParms",dictionary!{"Predictor"=>15,"Colors"=>channels as i64,"Columns"=>w as i64,"BitsPerComponent"=>bits as i64});
                p.set_content(crate::lossless::deflate(&predicted).unwrap());
                assert!(same_samples(&doc, &s, &p, &options, &AtomicBool::new(false)).unwrap());
                assert_eq!(native_samples::read(&doc, &p, &options).unwrap().bytes, raw);
            }
        }
    }
}
