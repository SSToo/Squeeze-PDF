use crate::{Error, ImageResult, Options, Phase, Preset, Progress, Result, check_cancel, inspect};
use hayro::{
    RenderCache, RenderSettings,
    hayro_interpret::{InterpreterSettings, InterpreterWarning, font::FontQuery},
    hayro_syntax::Pdf,
    vello_cpu::color::palette::css::WHITE,
};
use lopdf::{Document, Object, Stream, dictionary};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn rebuild(
    source: Vec<u8>,
    original: &Document,
    options: &Options,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
    rgb_budget: &mut crate::image_encoding::RgbBudget,
) -> Result<(
    Document,
    Vec<ImageResult>,
    Vec<crate::page_image::PageReplacement>,
)> {
    check_filters(original, cancel)?;
    check_fonts(original, cancel)?;
    let pdf = Pdf::new(source).map_err(|e| Error::Invalid(format!("页面渲染解析失败：{e:?}")))?;
    let source_pages = inspect::pages(original)?;
    if pdf.pages().len() != source_pages.len() {
        return Err(Error::Validation(
            "两个解析器读取到的页数不同，未导出图片 PDF".into(),
        ));
    }
    let missing_font = Arc::new(AtomicBool::new(false));
    let font_flag = missing_font.clone();
    let render_failure = Arc::new(AtomicBool::new(false));
    let failure_flag = render_failure.clone();
    let settings = InterpreterSettings {
        warning_sink: Arc::new(move |warning| {
            if matches!(
                warning,
                InterpreterWarning::UnsupportedFont | InterpreterWarning::ImageDecodeFailure
            ) {
                failure_flag.store(true, Ordering::Relaxed);
            }
        }),
        font_resolver: Arc::new(move |query| match query {
            FontQuery::Standard(font) => Some(font.get_font_data()),
            FontQuery::Fallback(_) => {
                font_flag.store(true, Ordering::Relaxed);
                None
            }
        }),
        ..Default::default()
    };
    let mut result = Document::with_version("1.7");
    let page_tree = result.new_object_id();
    let mut kids = Vec::new();
    let mut reports = Vec::new();
    let mut replacements = Vec::new();
    let mut total_encoded = 0u64;
    let mut encoder = crate::jpeg::Encoder::default();
    for (index, page) in pdf.pages().iter().enumerate() {
        check_cancel(cancel)?;
        progress(Progress {
            phase: Phase::Rasterizing,
            completed: index,
            total: pdf.pages().len(),
        });
        check_cancel(cancel)?;
        let original_page = original.get_dictionary(source_pages[index])?;
        let unit = match original_page.get(b"UserUnit") {
            Err(_) => 1.0,
            Ok(o) => match original.dereference(o)?.1 {
                Object::Integer(n) => *n as f64,
                Object::Real(n) => *n as f64,
                _ => return Err(Error::Invalid("页面 UserUnit 无效".into())),
            },
        };
        let (w, h) = page.render_dimensions();
        let points = [w as f64 * unit, h as f64 * unit];
        let (dimensions, resolution) = if let Some(dpi) = options.target_dpi {
            (
                [
                    (points[0] * dpi as f64 / 72.).ceil(),
                    (points[1] * dpi as f64 / 72.).ceil(),
                ],
                format!("{dpi} DPI"),
            )
        } else if options.preset == Preset::Lossless {
            (
                [
                    (points[0] * 300. / 72.).ceil(),
                    (points[1] * 300. / 72.).ceil(),
                ],
                "300 DPI".into(),
            )
        } else {
            let width = if options.preset == Preset::Small {
                1000.
            } else {
                1400.
            };
            (
                [width, (points[1] / points[0] * width).round().max(1.)],
                format!("宽 {} 像素", width as u32),
            )
        };
        if !points.iter().all(|v| v.is_finite() && *v > 0.)
            || !dimensions
                .iter()
                .all(|v| v.is_finite() && *v >= 1. && *v <= 65500.)
        {
            return Err(Error::Invalid(format!("第 {} 页尺寸无效或过大", index + 1)));
        }
        let width = dimensions[0] as u32;
        let height = dimensions[1] as u32;
        let pixels = width as u64 * height as u64;
        if pixels > options.limits.max_image_pixels
            || pixels * 4 > options.limits.max_stream_bytes as u64
        {
            return Err(Error::Limit(format!(
                "第 {} 页渲染像素超过限制，请降低 DPI",
                index + 1
            )));
        }
        // Render lossy pages at 2x then downsample to improve small text edges.
        // Fall back to 1x when the larger buffer would exceed existing limits;
        // the requested output dimensions never change in that fallback.
        let supersample = options.preset != Preset::Lossless
            && width <= u16::MAX as u32 / 2
            && height <= u16::MAX as u32 / 2
            && pixels * 4 <= options.limits.max_image_pixels
            && pixels * 16 <= options.limits.max_stream_bytes as u64;
        let scale = if supersample { 2 } else { 1 };
        let render_width = width * scale;
        let render_height = height * scale;
        // A fresh cache per page prevents decoded images accumulating across a
        // large document. PDF page rotations and CropBox are applied by hayro.
        let cache = RenderCache::new();
        let pixmap = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hayro::render(
                page,
                &cache,
                &settings,
                &RenderSettings {
                    x_scale: (render_width as f64 / w as f64) as f32,
                    y_scale: (render_height as f64 / h as f64) as f32,
                    width: Some(render_width as u16),
                    height: Some(render_height as u16),
                    bg_color: WHITE,
                },
            )
        }))
        .map_err(|_| Error::Invalid(format!("第 {} 页渲染失败", index + 1)))?;
        if render_failure.load(Ordering::Relaxed) {
            return Err(Error::Unsupported(format!(
                "第 {} 页的字体或图片无法完整渲染，请使用保留原始内容模式",
                index + 1
            )));
        }
        if missing_font.load(Ordering::Relaxed) {
            return Err(Error::Unsupported(
                "渲染需要未嵌入的非标准字体，请使用保留原始内容模式".into(),
            ));
        }
        check_cancel(cancel)?;
        let mut rgb = Vec::with_capacity(render_width as usize * render_height as usize * 3);
        for pixel in pixmap.data_as_u8_slice().chunks_exact(4) {
            for channel in &pixel[..3] {
                rgb.push(channel.saturating_add(255 - pixel[3]));
            }
        }
        drop(pixmap);
        drop(cache);
        if supersample {
            let image = image::RgbImage::from_raw(render_width, render_height, rgb)
                .ok_or_else(|| Error::Invalid("页面像素长度无效".into()))?;
            rgb = image::imageops::resize(
                &image,
                width,
                height,
                image::imageops::FilterType::Lanczos3,
            )
            .into_raw();
        }
        check_cancel(cancel)?;
        let encoding_started = std::time::Instant::now();
        let ((encoded, encoding, codec, candidates), alternate) =
            crate::page_image::encode_variants(
                rgb,
                width,
                height,
                options.preset,
                options.quality(),
                &mut encoder,
                rgb_budget,
                cancel,
            )?;
        total_encoded = total_encoded.saturating_add(encoded.content.len() as u64);
        if total_encoded > options.limits.max_file_bytes {
            return Err(Error::Limit("图片 PDF 总大小超过限制，请降低画质".into()));
        }
        let image = result.add_object(encoded);
        let body = format!("q {} 0 0 {} 0 0 cm /PageImage Do Q", points[0], points[1]).into_bytes();
        let contents = result.add_object(Stream::new(dictionary! {}, body));
        let page_id=result.add_object(dictionary! {
            "Type"=>"Page","Parent"=>page_tree,"MediaBox"=>vec![0.into(),0.into(),Object::Real(points[0] as f32),Object::Real(points[1] as f32)],
            "Resources"=>dictionary! {"XObject"=>dictionary! {"PageImage"=>image}},"Contents"=>contents
        });
        kids.push(Object::Reference(page_id));
        reports.push(ImageResult {
            object: format!("{} 0", image.0),
            before_bytes: 0,
            after_bytes: result.get_object(image)?.as_stream()?.content.len(),
            before_dimensions: [0, 0],
            after_dimensions: [width, height],
            changed: true,
            encoding: codec,
            candidates,
            elapsed_ms: encoding_started.elapsed().as_millis() as u64,
            jpeg_cached: encoder.cached,
            reason: format!(
                "第 {} 页整页渲染为图片，{resolution}，{encoding}{}",
                index + 1,
                if supersample {
                    "，超采样抗锯齿"
                } else {
                    ""
                }
            ),
        });
        if let Some((stream, label, codec, candidates)) = alternate {
            let mut report = reports.last().expect("page report").clone();
            report.after_bytes = stream.content.len();
            report.encoding = codec;
            report.candidates = candidates;
            report
                .reason
                .push_str(&format!("，新增精确 RGB 候选 {label}"));
            replacements.push((image, stream, report));
        }
    }
    let count = kids.len() as i64;
    result.objects.insert(
        page_tree,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count}),
    );
    let root = result.add_object(dictionary! {"Type"=>"Catalog","Pages"=>page_tree});
    result.trailer.set("Root", root);
    Ok((result, reports, replacements))
}

// lopdf and the desktop PDF.js renderer can decode this PDF 2.0 extension,
// but hayro 0.7.1 silently omits streams using it. Reject conservatively before
// rendering, including nested Forms/fonts and filter arrays. This check belongs
// only to the native renderer; caller-provided desktop pixels remain supported.
fn check_filters(doc: &Document, cancel: &AtomicBool) -> Result<()> {
    fn brotli(doc: &Document, object: &Object, depth: usize) -> Result<bool> {
        if depth > 64 {
            return Err(Error::Limit("过滤器预检对象嵌套过深".into()));
        }
        match doc.dereference(object)?.1 {
            Object::Null => Ok(false),
            Object::Name(name) => Ok(name == b"BrotliDecode"),
            Object::Array(filters) => {
                for filter in filters {
                    if brotli(doc, filter, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Err(Error::Invalid("数据流过滤器无效".into())),
        }
    }
    for object in doc.objects.values() {
        check_cancel(cancel)?;
        if let Ok(stream) = object.as_stream()
            && let Ok(filter) = stream.dict.get(b"Filter")
            && brotli(doc, filter, 0)?
        {
            return Err(Error::Unsupported(
                "CLI 页面渲染暂不支持 BrotliDecode，请使用桌面图片模式处理".into(),
            ));
        }
    }
    Ok(())
}

// Some simple-font fallbacks in hayro select a standard face before invoking
// the resolver. Check source font dictionaries as well, so a missing embedded
// font cannot silently turn into a different face or lose its glyphs.
// hayro renders page contents but not annotation appearances. The desktop
// PDF.js path can flatten them; never silently omit them in the CLI renderer.
pub(crate) fn check_annotations(doc: &Document, cancel: &AtomicBool) -> Result<()> {
    for id in inspect::pages(doc)? {
        check_cancel(cancel)?;
        let page = doc.get_dictionary(id)?;
        let Ok(value) = page.get(b"Annots") else {
            continue;
        };
        let value = doc.dereference(value)?.1;
        if matches!(value, Object::Null) {
            continue;
        }
        for annotation in value.as_array()? {
            let d = doc.dereference(annotation)?.1.as_dict()?;
            let flags = d.get(b"F").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
            if flags & (1 | 2 | 32) != 0 {
                continue;
            }
            if inspect::named(d, b"Subtype", b"Popup") {
                continue;
            }
            if inspect::named(d, b"Subtype", b"Link") && !d.has(b"AP") {
                continue;
            }
            return Err(Error::Unsupported(
                "此文件的表单或可见批注需要桌面图片模式处理，请在软件中转换".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn check_fonts(doc: &Document, cancel: &AtomicBool) -> Result<()> {
    fn visit(doc: &Document, object: &Object, depth: usize) -> Result<()> {
        if depth > 64 {
            return Err(Error::Limit("字体预检对象嵌套过深".into()));
        }
        let dict = match object {
            Object::Dictionary(d) => Some(d),
            Object::Stream(s) => Some(&s.dict),
            Object::Array(a) => {
                for o in a {
                    visit(doc, o, depth + 1)?;
                }
                None
            }
            _ => None,
        };
        if let Some(d) = dict {
            if d.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Font") {
                let subtype = d.get(b"Subtype")?.as_name()?;
                if subtype == b"Type0" {
                    let (_, descendants) = doc.dereference(d.get(b"DescendantFonts")?)?;
                    for child in descendants.as_array()? {
                        visit(doc, doc.dereference(child)?.1, depth + 1)?;
                    }
                } else if subtype != b"Type3" {
                    let base = d.get(b"BaseFont").ok().and_then(|o| o.as_name().ok());
                    let standard = subtype == b"Type1"
                        && base.is_some_and(|n| {
                            [
                                b"Times-Roman".as_slice(),
                                b"Times-Bold",
                                b"Times-Italic",
                                b"Times-BoldItalic",
                                b"Helvetica",
                                b"Helvetica-Bold",
                                b"Helvetica-Oblique",
                                b"Helvetica-BoldOblique",
                                b"Courier",
                                b"Courier-Bold",
                                b"Courier-Oblique",
                                b"Courier-BoldOblique",
                                b"Symbol",
                                b"ZapfDingbats",
                            ]
                            .contains(&n)
                        });
                    let embedded = d
                        .get(b"FontDescriptor")
                        .ok()
                        .and_then(|o| doc.dereference(o).ok())
                        .and_then(|(_, o)| o.as_dict().ok())
                        .is_some_and(|descriptor| {
                            [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
                                .iter()
                                .any(|key| {
                                    descriptor
                                        .get(key)
                                        .ok()
                                        .and_then(|o| doc.dereference(o).ok())
                                        .and_then(|(_, o)| o.as_stream().ok())
                                        .is_some_and(|s| !s.content.is_empty())
                                })
                        });
                    if !standard && !embedded {
                        return Err(Error::Unsupported(
                            "整页渲染需要已嵌入的非标准字体，请使用保留原始内容模式".into(),
                        ));
                    }
                }
            }
            for (_, o) in d.iter() {
                visit(doc, o, depth + 1)?;
            }
        }
        Ok(())
    }
    for object in doc.objects.values() {
        check_cancel(cancel)?;
        visit(doc, object, 0)?;
    }
    Ok(())
}
