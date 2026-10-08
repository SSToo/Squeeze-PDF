use crate::{
    Error, ImageResult, Options, OutputMode, Phase, Preset, Progress, Result, check_cancel, inspect,
};
use hayro::hayro_syntax::Pdf;
use lopdf::{Document, Object, Stream, dictionary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::atomic::AtomicBool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterPage {
    pub dimensions: [u32; 2],
    pub points: [f64; 2],
    pub annotation_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RasterPlan {
    pub source_sha256: String,
    pub pages: Vec<RasterPage>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RasterManifest {
    pub source_sha256: String,
    pub pages: usize,
}

pub fn prepare_raster(
    input: impl AsRef<Path>,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<RasterPlan> {
    options.check()?;
    let (source, doc) = crate::load(input.as_ref(), &options.limits)?;
    plan(source, &doc, options, cancel)
}

fn plan(
    source: Vec<u8>,
    original: &Document,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<RasterPlan> {
    check_cancel(cancel)?;
    if options.output_mode != OutputMode::Raster {
        return Err(Error::Invalid("页面渲染计划只能用于图片模式".into()));
    }
    inspect::document(original, &options.limits)?;
    crate::raster::check_fonts(original, cancel)?;
    let source_sha256 = format!("{:x}", Sha256::digest(&source));
    let pdf = Pdf::new(source).map_err(|e| Error::Invalid(format!("页面解析失败：{e:?}")))?;
    let ids = inspect::pages(original)?;
    if pdf.pages().len() != ids.len() {
        return Err(Error::Validation("两个解析器读取到的页数不同".into()));
    }
    let mut pages = Vec::with_capacity(ids.len());
    for (page, id) in pdf.pages().iter().zip(ids) {
        check_cancel(cancel)?;
        let d = original.get_dictionary(id)?;
        let unit = match d.get(b"UserUnit") {
            Err(_) => 1.,
            Ok(o) => match original.dereference(o)?.1 {
                Object::Integer(n) => *n as f64,
                Object::Real(n) => *n as f64,
                _ => return Err(Error::Invalid("页面 UserUnit 无效".into())),
            },
        };
        let (w, h) = page.render_dimensions();
        let points = [w as f64 * unit, h as f64 * unit];
        if !points.iter().all(|n| n.is_finite() && *n > 0.) {
            return Err(Error::Invalid("页面尺寸无效".into()));
        }
        let dimensions = if let Some(dpi) = options.target_dpi {
            points.map(|n| (n * dpi as f64 / 72.).ceil())
        } else if options.preset == Preset::Lossless {
            points.map(|n| (n * 300. / 72.).ceil())
        } else {
            let width = if options.preset == Preset::Small {
                1000.
            } else {
                1400.
            };
            [width, (points[1] / points[0] * width).floor().max(1.)]
        };
        if !dimensions
            .iter()
            .all(|n| n.is_finite() && *n >= 1. && *n <= 65_500.)
        {
            return Err(Error::Limit("页面尺寸超过限制".into()));
        }
        let dimensions = dimensions.map(|n| n as u32);
        let pixels = dimensions[0] as u64 * dimensions[1] as u64;
        if pixels > options.limits.max_image_pixels
            || pixels * 4 > options.limits.max_stream_bytes as u64
        {
            return Err(Error::Limit("页面渲染像素超过限制".into()));
        }
        let annotation_ids = match d.get(b"Annots") {
            Ok(value) => match original.dereference(value)?.1 {
                Object::Null => Vec::new(),
                Object::Array(items) => items
                    .iter()
                    .filter_map(|o| o.as_reference().ok())
                    .map(|(id, generation)| {
                        if generation == 0 {
                            format!("{id}R")
                        } else {
                            format!("{id}R{generation}")
                        }
                    })
                    .collect(),
                _ => return Err(Error::Invalid("页面批注列表无效".into())),
            },
            Err(_) => Vec::new(),
        };
        pages.push(RasterPage {
            dimensions,
            points,
            annotation_ids,
        });
    }
    Ok(RasterPlan {
        source_sha256,
        pages,
    })
}

pub(crate) fn rebuild(
    source: Vec<u8>,
    original: &Document,
    manifest_path: &Path,
    options: &Options,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
) -> Result<(Document, Vec<ImageResult>)> {
    let manifest: RasterManifest =
        serde_json::from_slice(&crate::read_bounded(manifest_path, 16_384)?)
            .map_err(|e| Error::Invalid(format!("页面像素清单无效：{e}")))?;
    let plan = plan(source, original, options, cancel)?;
    if manifest.source_sha256 != plan.source_sha256 {
        return Err(Error::InputChanged);
    }
    if manifest.pages != plan.pages.len() {
        return Err(Error::Validation("页面像素数量与原件不符".into()));
    }
    let directory = manifest_path
        .parent()
        .ok_or_else(|| Error::Invalid("页面像素目录无效".into()))?;
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let mut kids = Vec::new();
    let mut reports = Vec::new();
    let mut total = 0u64;
    let mut encoder = crate::jpeg::Encoder::default();
    for (index, page) in plan.pages.iter().enumerate() {
        check_cancel(cancel)?;
        progress(Progress {
            phase: Phase::Images,
            completed: index,
            total: plan.pages.len(),
        });
        check_cancel(cancel)?;
        let [width, height] = page.dimensions;
        let expected = width as u64 * height as u64 * 3;
        let pixels = crate::read_bounded(&directory.join(format!("{index}.rgb")), expected)?;
        if pixels.len() as u64 != expected {
            return Err(Error::Validation(format!(
                "第 {} 页像素长度与尺寸不符",
                index + 1
            )));
        }
        let encoding_started = std::time::Instant::now();
        let (image, encoding, codec, candidates) = crate::page_image::encode(
            pixels,
            width,
            height,
            options.preset,
            options.quality(),
            &mut encoder,
        )?;
        total = total.saturating_add(image.content.len() as u64);
        if total > options.limits.max_file_bytes {
            return Err(Error::Limit("图片 PDF 总大小超过限制".into()));
        }
        let length = image.content.len();
        let image_id = doc.add_object(image);
        let [w, h] = page.points;
        let contents = doc.add_object(Stream::new(
            dictionary! {},
            format!("q {w} 0 0 {h} 0 0 cm /PageImage Do Q").into_bytes(),
        ));
        let page_id = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>tree, "MediaBox"=>vec![0.into(),0.into(),Object::Real(w as f32),Object::Real(h as f32)], "Resources"=>dictionary! {"XObject"=>dictionary! {"PageImage"=>image_id}}, "Contents"=>contents});
        kids.push(Object::Reference(page_id));
        reports.push(ImageResult {
            object: format!("{} 0", image_id.0),
            before_bytes: 0,
            after_bytes: length,
            before_dimensions: [0, 0],
            after_dimensions: page.dimensions,
            changed: true,
            encoding: codec,
            candidates,
            elapsed_ms: encoding_started.elapsed().as_millis() as u64,
            jpeg_cached: encoder.cached,
            reason: format!(
                "第 {} 页整页渲染为图片，PDF.js，{width} × {height} 像素，{encoding}",
                index + 1
            ),
        });
    }
    doc.objects.insert(
        tree,
        Object::Dictionary(
            dictionary! {"Type"=>"Pages", "Kids"=>kids, "Count"=>plan.pages.len() as i64},
        ),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>tree});
    doc.trailer.set("Root", catalog);
    Ok((doc, reports))
}
