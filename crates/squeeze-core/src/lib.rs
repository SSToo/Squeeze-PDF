mod advanced;
mod attachments;
mod audit;
mod ccitt;
mod cmyk_jpeg;
mod coalesce;
mod exact_sharing;
mod font_subset;
mod form_precision;
mod form_usage;
mod identity;
mod image_encoding;
mod image_input;
mod images;
mod inspect;
mod jpeg;
mod jpeg_lossless;
mod large_images;
mod lossless;
mod native_samples;
mod page_image;
mod paths;
mod raster;
mod raster_input;
mod raster_quality;
pub use raster_quality::{RasterQuality, compare_raster_quality};
mod resource_graph;
mod resource_leaves;
mod resources;
mod reusable_paths;
mod stream_candidates;
mod stream_filters;
mod structure;
mod transparent_images;
mod usage;
mod validate;
mod vector_state;
mod writing;

pub use advanced::Stats as AdvancedOptimization;
pub use image_encoding::{RgbFlateStats, RgbSampleProof};
pub use image_input::Preparation as ImagePreparation;
use lopdf::{Document, LoadOptions};
pub use raster_input::{RasterManifest, RasterPage, RasterPlan, prepare_raster};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
pub use stream_candidates::{StrategyCandidate, StreamOptimization};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("PDF 读取失败：{0}")]
    Pdf(#[from] lopdf::Error),
    #[error("暂不支持：{0}")]
    Unsupported(String),
    #[error("文件无效：{0}")]
    Invalid(String),
    #[error("超过处理限制：{0}")]
    Limit(String),
    #[error("任务已取消")]
    Cancelled,
    #[error("输出文件已存在，未覆盖：{0}")]
    OutputExists(PathBuf),
    #[error("结果校验失败：{0}")]
    Validation(String),
    #[error("处理期间原文件发生变化，未导出结果")]
    InputChanged,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    #[default]
    Lossless,
    /// Original image dimensions, high-quality JPEG and vector optimization.
    Fidelity,
    Balanced,
    Small,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    #[default]
    Preserve,
    Raster,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    /// Optional caller-imposed file cap. `u64::MAX` (the default) is unlimited.
    pub max_file_bytes: u64,
    pub max_stream_bytes: usize,
    pub max_image_pixels: u64,
    pub max_objects: usize,
    pub max_operations: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: u64::MAX,
            max_stream_bytes: 64 * 1024 * 1024,
            max_image_pixels: 24_000_000,
            max_objects: 250_000,
            max_operations: 500_000,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub preset: Preset,
    pub output_mode: OutputMode,
    pub limits: Limits,
    pub jpeg_quality: Option<u8>,
    pub target_dpi: Option<u16>,
    /// Optional directory on the output filesystem, owned by a supervising
    /// process so it can clean up even after a forcefully terminated worker.
    pub temporary_directory: Option<PathBuf>,
    /// Private staging protocol for pages rendered by the desktop PDF.js renderer.
    pub raster_manifest: Option<PathBuf>,
    /// Lossy page/form path precision optimization, bounded in physical points.
    /// Disabled by default for API/CLI callers; the desktop lossy presets opt in.
    pub compact_paths: bool,
    /// Factor closed constant-colour paths into reusable vector forms.
    /// Desktop preserve presets opt in; source coordinate precision is kept
    /// unless compact_paths is separately enabled outside the lossless preset.
    pub reuse_paths: bool,
}
impl Options {
    fn quality(&self) -> u8 {
        self.jpeg_quality
            .unwrap_or(match (self.output_mode, self.preset) {
                (OutputMode::Raster, Preset::Small) => 60,
                (OutputMode::Raster, _) => 72,
                (_, Preset::Fidelity) => 95,
                (_, Preset::Small) => 50,
                _ => 85,
            })
    }
    fn dpi(&self) -> u16 {
        self.target_dpi.unwrap_or(if self.preset == Preset::Small {
            100
        } else {
            200
        })
    }
    fn check(&self) -> Result<()> {
        if self.preset == Preset::Fidelity
            && (self.output_mode != OutputMode::Preserve || self.target_dpi.is_some())
        {
            return Err(Error::Invalid(
                "高保真仅用于保留原始内容，图片保持原分辨率，不能设置目标 DPI".into(),
            ));
        }
        if self.raster_manifest.is_some() && self.output_mode != OutputMode::Raster {
            return Err(Error::Invalid("页面像素输入只能用于整页图片模式".into()));
        }
        if self.quality() == 0 || self.quality() > 100 || !(72..=600).contains(&self.dpi()) {
            return Err(Error::Invalid("图片质量应为 1–100，DPI 应为 72–600".into()));
        }
        if self.preset == Preset::Lossless
            && self.output_mode == OutputMode::Preserve
            && (self.jpeg_quality.is_some() || self.target_dpi.is_some())
        {
            return Err(Error::Invalid("保真模式不能设置有损图片参数".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Reading,
    Inspecting,
    Images,
    Rasterizing,
    Optimizing,
    Saving,
    Validating,
    Completed,
}
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub completed: usize,
    pub total: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct DocumentInfo {
    pub bytes: u64,
    pub pages: usize,
    pub images: usize,
    pub has_interactive_content: bool,
    #[serde(skip_serializing)]
    pub(crate) requires_conservative_optimization: bool,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Saved,
    Unchanged,
}
#[derive(Debug, Clone, Serialize)]
pub struct EncodingCandidate {
    pub encoding: String,
    /// Dictionary plus payload cost; includes constant indirect-object framing.
    pub bytes: usize,
    pub lossless: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImageResult {
    pub object: String,
    pub before_bytes: usize,
    pub after_bytes: usize,
    pub before_dimensions: [u32; 2],
    pub after_dimensions: [u32; 2],
    pub changed: bool,
    pub reason: String,
    pub encoding: String,
    pub candidates: Vec<EncodingCandidate>,
    pub elapsed_ms: u64,
    pub jpeg_cached: bool,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct SizeAudit {
    pub file_bytes: u64,
    pub objects: usize,
    pub streams: usize,
    pub images: u64,
    pub masks: u64,
    pub drawing: u64,
    pub fonts: u64,
    pub attachments: u64,
    pub metadata: u64,
    pub containers: u64,
    pub other_streams: u64,
    pub syntax_and_dictionaries: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct StageResult {
    pub stage: String,
    pub before_stream_bytes: u64,
    pub after_stream_bytes: u64,
    pub before_objects: usize,
    pub after_objects: usize,
    pub elapsed_ms: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct WriterConfiguration {
    pub max_objects_per_stream: usize,
    pub compression_level: u32,
    pub layout: String,
    pub candidates: Vec<WriterCandidate>,
}
#[derive(Debug, Clone, Serialize)]
pub struct WriterCandidate {
    pub layout: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub outcome: Outcome,
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub preset: Preset,
    pub output_mode: OutputMode,
    pub original_bytes: u64,
    pub result_bytes: u64,
    pub candidate_bytes: u64,
    pub saved_bytes: u64,
    pub reduction_percent: f64,
    pub pages: usize,
    pub optimized_streams: usize,
    /// Attempts in the selected pipeline; source identities may be renumbered later.
    pub images: Vec<ImageResult>,
    pub output_images: Vec<FinalImageResult>,
    pub image_optimization: ImageOptimization,
    pub image_preparation: ImagePreparation,
    pub rgb_flate: RgbFlateStats,
    pub advanced_optimization: AdvancedOptimization,
    pub size_before: SizeAudit,
    pub size_after: SizeAudit,
    pub stages: Vec<StageResult>,
    pub writer: WriterConfiguration,
    pub strategy_candidates: Vec<StrategyCandidate>,
    pub stream_optimization: StreamOptimization,
    pub warnings: Vec<String>,
    pub elapsed_ms: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct FinalImageResult {
    /// Object identity in the final serialized PDF, after any renumbering.
    pub object: String,
    pub bytes: usize,
    pub dimensions: [u32; 2],
    pub filters: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImageOptimization {
    pub status: String,
    pub baseline_bytes: u64,
    pub candidate_bytes: Option<u64>,
    pub selected: bool,
    pub rejection: Option<String>,
}
fn final_images(doc: &Document) -> Vec<FinalImageResult> {
    doc.objects
        .iter()
        .filter_map(|(id, o)| {
            let stream = o.as_stream().ok()?;
            if !inspect::named(&stream.dict, b"Subtype", b"Image") {
                return None;
            }
            let dim = |key: &[u8]| {
                stream
                    .dict
                    .get(key)
                    .ok()
                    .and_then(|o| doc.dereference(o).ok())
                    .and_then(|(_, o)| o.as_i64().ok())
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(0)
            };
            let filters = stream_filters::filters(doc, stream)
                .map(|f| {
                    f.iter()
                        .map(|f| String::from_utf8_lossy(&f.name).into_owned())
                        .collect()
                })
                .unwrap_or_else(|_| vec!["unresolved".into()]);
            Some(FinalImageResult {
                object: format!("{} {}", id.0, id.1),
                bytes: stream.content.len(),
                dimensions: [dim(b"Width"), dim(b"Height")],
                filters,
            })
        })
        .collect()
}
fn eligible_alternate(doc: &Document, limits: &Limits) -> bool {
    if doc.objects.len() > 50_000 {
        return false;
    }
    let mut pending: Vec<_> = doc.objects.values().map(|o| (o, 0usize)).collect();
    pending.extend(doc.trailer.iter().map(|(_, o)| (o, 0usize)));
    let mut work = limits.max_operations.min(500_000);
    let mut bytes = 0usize;
    while let Some((o, depth)) = pending.pop() {
        if work == 0 || depth > 64 {
            return false;
        }
        work -= 1;
        bytes = bytes.saturating_add(32);
        let dict = match o {
            lopdf::Object::Name(v) | lopdf::Object::String(v, _) => {
                bytes = bytes.saturating_add(v.len());
                None
            }
            lopdf::Object::Array(a) => {
                pending.extend(a.iter().map(|o| (o, depth + 1)));
                None
            }
            lopdf::Object::Dictionary(d) => Some(d),
            lopdf::Object::Stream(s) => {
                bytes = bytes.saturating_add(s.content.len());
                Some(&s.dict)
            }
            _ => None,
        };
        if let Some(d) = dict {
            for (k, o) in d.iter() {
                bytes = bytes.saturating_add(k.len());
                pending.push((o, depth + 1));
            }
        }
        if bytes > 64 * 1024 * 1024 {
            return false;
        }
    }
    true
}

struct Built {
    candidate: NamedTempFile,
    image_results: Vec<ImageResult>,
    stages: Vec<StageResult>,
    warnings: Vec<String>,
    writer: WriterConfiguration,
    strategy_candidates: Vec<StrategyCandidate>,
    stream_optimization: StreamOptimization,
    optimized_streams: usize,
    size_after: SizeAudit,
    output_images: Vec<FinalImageResult>,
    image_preparation: image_input::Preparation,
    rgb_flate: image_encoding::RgbFlateStats,
    raster_alternate: Option<(Document, Vec<ImageResult>)>,
    transparency: transparent_images::Stats,
}
#[allow(clippy::too_many_arguments)]
fn build_candidate(
    source: Vec<u8>,
    mut doc: Document,
    options: &Options,
    info: &DocumentInfo,
    parent: &Path,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
    enhanced: bool,
    paired: bool,
    encoder: &mut jpeg::Encoder,
    ready_images: Option<Vec<ImageResult>>,
) -> Result<Built> {
    let emit = |phase| Progress {
        phase,
        completed: 0,
        total: 0,
    };
    let original_images: std::collections::HashMap<_, _> =
        if options.output_mode == OutputMode::Preserve {
            final_images(&doc)
                .into_iter()
                .map(|i| (i.object, (i.bytes, i.dimensions)))
                .collect()
        } else {
            Default::default()
        };
    let mut stages = Vec::new();
    let image_preparation = if enhanced && options.output_mode == OutputMode::Preserve {
        let stage = audit::Stage::start("image_preparation", &doc);
        let preparation = image_input::prepare(&mut doc, &options.limits, cancel)?;
        stages.push(stage.finish(&doc));
        preparation
    } else {
        image_input::Preparation {
            status: "not_applicable".into(),
            ..Default::default()
        }
    };
    let mut rgb_budget = image_encoding::RgbBudget::new(&options.limits);
    let mut raster_alternate = None;
    let mut warnings = Vec::new();
    let mut image_results;
    let mut protected_images = HashSet::new();
    let mut transparency = transparent_images::Stats::default();
    if options.output_mode == OutputMode::Preserve {
        let stage = audit::Stage::start("resources", &doc);
        let removed = resources::cleanup(&mut doc, &options.limits, cancel)?;
        doc.prune_objects();
        if removed > 0 {
            warnings.push(format!("已移除 {removed} 个未使用的资源入口"));
        }
        stages.push(stage.finish(&doc));
        let stage = audit::Stage::start("decoded_image_sharing", &doc);
        let merged = resources::deduplicate_images(&mut doc, options, cancel)?;
        if merged > 0 {
            warnings.push(format!("已合并 {merged} 个像素与颜色语义相同的图片资源"));
        }
        stages.push(stage.finish(&doc));
    }
    let image_stage = audit::Stage::start("images", &doc);
    if let Some(results) = ready_images {
        image_results = results;
    } else if options.output_mode == OutputMode::Raster {
        let (mut rebuilt, results, replacements) = if let Some(manifest) = &options.raster_manifest
        {
            raster_input::rebuild(
                source,
                &doc,
                manifest,
                options,
                cancel,
                progress,
                &mut rgb_budget,
            )?
        } else {
            raster::check_annotations(&doc, cancel)?;
            raster::rebuild(source, &doc, options, cancel, progress, &mut rgb_budget)?
        };
        let attachment_count = attachments::copy(&doc, &mut rebuilt, &options.limits, cancel)?;
        if attachment_count > 0 {
            warnings.push(format!(
                "已保留 {attachment_count} 个嵌入附件，附件数据未重新编码"
            ));
        }
        if !replacements.is_empty() && eligible_alternate(&rebuilt, &options.limits) {
            let mut alternate = rebuilt.clone();
            let mut reports = results.clone();
            for (id, stream, report) in replacements {
                alternate.objects.insert(id, lopdf::Object::Stream(stream));
                if let Some(old) = reports.iter_mut().find(|r| r.object == report.object) {
                    *old = report;
                }
            }
            raster_alternate = Some((alternate, reports));
        }
        doc = rebuilt;
        image_results = results;
        warnings.push("每页已转为一张图片；原有矢量、可选文字、链接及书签不保留。页面以可见裁剪区域重建，请检查文字与画质".into());
        if info.has_interactive_content && options.raster_manifest.is_some() {
            warnings.push("可见表单和批注已合并到页面图片，不再可编辑；批注弹窗内容不保留".into());
        }
    } else {
        drop(source);
        let mut usage = usage::analyze(&doc, &options.limits, cancel)?;
        let original_protected_count = usage.protected.len();
        let mut paired_images = HashSet::new();
        if paired {
            let stage = audit::Stage::start("paired_transparency", &doc);
            let (stats, paired) = transparent_images::optimize(&mut doc, options, &usage, cancel)?;
            transparency = stats;
            paired_images.clone_from(&paired);
            usage.protected.extend(paired);
            stages.push(stage.finish(&doc));
        }
        protected_images.clone_from(&usage.protected);
        {
            if !usage.unknown.is_empty() {
                warnings.push(format!(
                    "无法完整确认 {} 张图片的所有使用位置，已对这些图片禁用降采样",
                    usage.unknown.len()
                ));
            }
            if original_protected_count > 0 {
                warnings.push(format!(
                    "已保留 {} 张特殊调用或交互外观共享图片的原始编码",
                    original_protected_count
                ));
            }
            image_results = images::optimize(
                &mut doc,
                options,
                &usage,
                cancel,
                progress,
                enhanced,
                encoder,
                &mut rgb_budget,
            )?;
            for report in &mut image_results {
                if paired_images
                    .iter()
                    .any(|id| format!("{} {}", id.0, id.1) == report.object)
                {
                    report.reason = "已同步优化图片和透明边缘".into();
                }
            }
        }
    }
    if options.output_mode == OutputMode::Preserve {
        for report in &mut image_results {
            if let Some(original) = original_images.get(&report.object) {
                report.before_bytes = original.0;
                report.before_dimensions = original.1;
                report.changed |= report.after_bytes != report.before_bytes;
            }
        }
    }
    check_cancel(cancel)?;
    progress(emit(Phase::Optimizing));
    stages.push(image_stage.finish(&doc));
    let path_stage = audit::Stage::start("paths", &doc);
    let optimized_paths = if options.output_mode == OutputMode::Preserve
        && options.compact_paths
        && !info.requires_conservative_optimization
    {
        paths::optimize(&mut doc, options.preset, &options.limits, cancel)?
    } else {
        0
    };
    if optimized_paths > 0 {
        warnings.push(format!(
            "已优化 {optimized_paths} 个绘制流的路径精度；文字和矢量类型保留，极细边缘可能变化"
        ));
    }
    stages.push(path_stage.finish(&doc));
    let reuse_stage = audit::Stage::start("reusable_paths", &doc);
    let (reused_streams, reused_shapes) = if options.output_mode == OutputMode::Preserve
        && options.reuse_paths
        && options.preset != Preset::Lossless
        && !info.requires_conservative_optimization
    {
        reusable_paths::optimize(&mut doc, &options.limits, cancel)?
    } else {
        (0, 0)
    };
    if reused_shapes > 0 {
        warnings.push(format!(
            "已复用 {reused_shapes} 组矢量轮廓，重复形状共享矢量对象；请检查细字与边缘"
        ));
    }
    stages.push(reuse_stage.finish(&doc));
    let content_stage = audit::Stage::start("content", &doc);
    let optimized_content = if options.output_mode == OutputMode::Preserve {
        structure::optimize_content(&mut doc, &options.limits, cancel)?
    } else {
        0
    };
    stages.push(content_stage.finish(&doc));
    let flate_stage = audit::Stage::start("flate", &doc);
    let mut optimized_streams = optimized_paths
        + reused_streams
        + optimized_content
        + lossless::optimize(&mut doc, &options.limits, cancel)?;
    stages.push(flate_stage.finish(&doc));
    let zopfli_stage = audit::Stage::start("zopfli", &doc);
    optimized_streams += lossless::zopfli(&mut doc, &options.limits, &protected_images, cancel)?;
    stages.push(zopfli_stage.finish(&doc));
    let dedup_stage = audit::Stage::start("deduplication", &doc);
    doc.objects
        .retain(|_, object| !inspect::is_container(object));
    if options.output_mode == OutputMode::Preserve {
        let merged = if info.has_interactive_content {
            0
        } else {
            resource_graph::deduplicate(&mut doc, &options.limits, cancel)?
        } + structure::deduplicate(&mut doc, &options.limits, cancel)?;
        doc.prune_objects();
        if merged > 0 {
            warnings.push(format!("已合并 {merged} 个重复数据资源"));
        }
    }
    stages.push(dedup_stage.finish(&doc));
    validate::references(&doc)?;
    check_cancel(cancel)?;
    progress(emit(Phase::Saving));
    let mut candidate =
        NamedTempFile::new_in(options.temporary_directory.as_deref().unwrap_or(parent))?;
    let mut writer = if doc.objects.len() >= 1000 {
        WriterConfiguration {
            max_objects_per_stream: 1000,
            compression_level: 9,
            layout: String::new(),
            candidates: Vec::new(),
        }
    } else {
        WriterConfiguration {
            max_objects_per_stream: 100,
            compression_level: 6,
            layout: String::new(),
            candidates: Vec::new(),
        }
    };
    let writing_stage = audit::Stage::start("writing", &doc);
    candidate = writing::best(&mut doc, candidate, &mut writer, &options.limits, cancel)?;
    stages.push(writing_stage.finish(&doc));
    candidate.flush()?;
    let candidate_bytes = candidate.as_file().metadata()?.len();
    check_cancel(cancel)?;
    progress(emit(Phase::Validating));
    let result_doc = Document::load_with_options(candidate.path(), load_options(&options.limits))?;
    validate::round_trip(&doc, &result_doc)?;
    let mut size_after = audit::size(&result_doc, candidate_bytes);
    drop(result_doc);
    let (strategy_candidates, stream_optimization) = if options.output_mode == OutputMode::Preserve
    {
        let choice = stream_candidates::choose(
            &mut doc,
            candidate,
            &mut writer,
            &options.limits,
            cancel,
            info.has_interactive_content,
        )?;
        if let Some(audit) = choice.size_after {
            size_after = audit;
        }
        optimized_streams += choice.added_streams;
        stages.extend(choice.stages);
        candidate = choice.file;
        (choice.strategies, choice.diagnostics)
    } else {
        (
            vec![StrategyCandidate {
                strategy: "current".into(),
                bytes: candidate_bytes,
                selected: true,
            }],
            StreamOptimization {
                status: "not_applicable".into(),
                ..Default::default()
            },
        )
    };
    let final_doc = Document::load_with_options(candidate.path(), load_options(&options.limits))?;
    let output_images = final_images(&final_doc);
    Ok(Built {
        candidate,
        image_results,
        stages,
        warnings,
        writer,
        strategy_candidates,
        stream_optimization,
        optimized_streams,
        size_after,
        output_images,
        image_preparation,
        rgb_flate: rgb_budget.stats,
        transparency,
        raster_alternate,
    })
}
pub fn inspect_file(path: impl AsRef<Path>, limits: &Limits) -> Result<DocumentInfo> {
    let (bytes, doc) = load(path.as_ref(), limits)?;
    let mut info = inspect::document(&doc, limits)?;
    info.bytes = bytes.len() as u64;
    Ok(info)
}
/// Byte categories count each encoded stream once, including object/xref
/// containers. The remainder is syntax and dictionaries, not removable waste.
pub fn audit_file(path: impl AsRef<Path>, limits: &Limits) -> Result<SizeAudit> {
    let (bytes, doc) = load(path.as_ref(), limits)?;
    Ok(audit::size(&doc, bytes.len() as u64))
}
/// Keeps the source and all existing outputs intact. Preserve mode commits only
/// a smaller result; raster mode commits the requested image-page format even
/// when larger. Both validate output and check cancellation between steps.
pub fn compress(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Report> {
    let started = Instant::now();
    options.check()?;
    check_cancel(cancel)?;
    let input = input.as_ref();
    let output = output.as_ref();
    if output.try_exists()? {
        return Err(Error::OutputExists(output.to_owned()));
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let emit = |phase| Progress {
        phase,
        completed: 0,
        total: 0,
    };
    progress(emit(Phase::Reading));
    let (source, doc) = load(input, &options.limits)?;
    let source_hash = Sha256::digest(&source);
    let original_bytes = source.len() as u64;
    check_cancel(cancel)?;
    progress(emit(Phase::Inspecting));
    let info = inspect::document(&doc, &options.limits)?;
    let size_before = audit::size(&doc, original_bytes);
    let original_output_images = final_images(&doc);
    let try_pairs = options.output_mode == OutputMode::Preserve
        && matches!(options.preset, Preset::Balanced | Preset::Small)
        && eligible_alternate(&doc, &options.limits)
        && doc
            .objects
            .values()
            .any(|o| o.as_stream().is_ok_and(|s| s.dict.has(b"SMask")));
    let alternate = (options.output_mode == OutputMode::Preserve
        && eligible_alternate(&doc, &options.limits))
    .then(|| doc.clone());
    let mut encoder = jpeg::Encoder::default();
    let mut chosen = build_candidate(
        source,
        doc,
        options,
        &info,
        parent,
        cancel,
        &mut progress,
        false,
        false,
        &mut encoder,
        None,
    )?;
    let baseline_bytes = chosen.candidate.as_file().metadata()?.len();
    let mut image_optimization = ImageOptimization {
        status: "skipped_budget".into(),
        baseline_bytes,
        ..Default::default()
    };
    let mut preparation = chosen.image_preparation.clone();
    let mut rgb_flate = chosen.rgb_flate.clone();
    let next = alternate.map(|doc| (doc, None)).or_else(|| {
        chosen
            .raster_alternate
            .take()
            .map(|(doc, images)| (doc, Some(images)))
    });
    if let Some((doc, ready)) = next {
        match build_candidate(
            Vec::new(),
            doc,
            options,
            &info,
            parent,
            cancel,
            &mut progress,
            true,
            false,
            &mut encoder,
            ready,
        ) {
            Ok(mut enhanced) => {
                let bytes = enhanced.candidate.as_file().metadata()?.len();
                image_optimization.candidate_bytes = Some(bytes);
                preparation = enhanced.image_preparation.clone();
                if options.output_mode == OutputMode::Preserve {
                    rgb_flate = enhanced.rgb_flate.clone();
                }
                image_optimization.selected = bytes < baseline_bytes;
                image_optimization.status = if bytes < baseline_bytes {
                    "selected"
                } else {
                    "kept_baseline"
                }
                .into();
                let strategies = vec![
                    StrategyCandidate {
                        strategy: "image_baseline".into(),
                        bytes: baseline_bytes,
                        selected: bytes >= baseline_bytes,
                    },
                    StrategyCandidate {
                        strategy: "image_enhanced".into(),
                        bytes,
                        selected: bytes < baseline_bytes,
                    },
                ];
                if bytes < baseline_bytes {
                    if options.output_mode == OutputMode::Raster {
                        enhanced.warnings.clone_from(&chosen.warnings);
                    }
                    chosen = enhanced;
                }
                chosen.strategy_candidates.extend(strategies);
            }
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(error) => {
                image_optimization.status = "rejected".into();
                image_optimization.rejection = Some(error.to_string());
            }
        }
    } else if options.output_mode == OutputMode::Raster {
        image_optimization.status = if rgb_flate.skipped_budget > 0 {
            "skipped_budget"
        } else {
            "no_improving_page_candidate"
        }
        .into();
    }
    if try_pairs {
        check_cancel(cancel)?;
        let (_, original) = load(input, &options.limits)?;
        let baseline = chosen.candidate.as_file().metadata()?.len();
        match build_candidate(
            Vec::new(),
            original,
            options,
            &info,
            parent,
            cancel,
            &mut progress,
            true,
            true,
            &mut encoder,
            None,
        ) {
            Ok(candidate) => {
                let bytes = candidate.candidate.as_file().metadata()?.len();
                let selected = bytes < baseline;
                if selected {
                    preparation = candidate.image_preparation.clone();
                    rgb_flate = candidate.rgb_flate.clone();
                    chosen = candidate;
                }
                chosen.strategy_candidates.push(StrategyCandidate {
                    strategy: "paired_transparency".into(),
                    bytes,
                    selected,
                });
            }
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(_) => { /* Optional pair candidate leaves both earlier plans intact. */ }
        }
    }
    let advanced_optimization = match advanced::choose(&mut chosen, options, parent, cancel) {
        Ok(stats) => stats,
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(error) => AdvancedOptimization {
            status: "rejected".into(),
            rejection: Some(error.to_string()),
            ..Default::default()
        },
    };
    let Built {
        candidate,
        image_results,
        stages,
        mut warnings,
        writer,
        strategy_candidates,
        stream_optimization,
        optimized_streams,
        mut size_after,
        mut output_images,
        ..
    } = chosen;
    let candidate_bytes = candidate.as_file().metadata()?.len();
    let current = read_bounded(input, options.limits.max_file_bytes)?;
    if Sha256::digest(&current) != source_hash {
        return Err(Error::InputChanged);
    }
    drop(current);
    check_cancel(cancel)?;
    if options.output_mode == OutputMode::Raster && candidate_bytes > options.limits.max_file_bytes
    {
        return Err(Error::Limit(
            "图片 PDF 超过输出大小限制，请降低分辨率".into(),
        ));
    }
    if options.output_mode == OutputMode::Raster && candidate_bytes >= original_bytes {
        warnings.push("整页转图片后的文件比原件大；已按所选内容模式导出".into());
    }
    let (outcome, output_path, result_bytes) =
        if candidate_bytes < original_bytes || options.output_mode == OutputMode::Raster {
            candidate.as_file().sync_all()?;
            match candidate.persist_noclobber(output) {
                Ok(_) => (Outcome::Saved, Some(output.to_owned()), candidate_bytes),
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(Error::OutputExists(output.to_owned()));
                }
                Err(error) => return Err(Error::Io(error.error)),
            }
        } else {
            (Outcome::Unchanged, None, original_bytes)
        };
    if outcome == Outcome::Unchanged {
        size_after = size_before.clone();
        output_images = original_output_images;
    }
    progress(Progress {
        phase: Phase::Completed,
        completed: 1,
        total: 1,
    });
    Ok(Report {
        outcome,
        input: input.to_owned(),
        output: output_path,
        preset: options.preset,
        output_mode: options.output_mode,
        original_bytes,
        result_bytes,
        candidate_bytes,
        saved_bytes: original_bytes.saturating_sub(result_bytes),
        reduction_percent: (original_bytes as f64 - result_bytes as f64) * 100.0
            / original_bytes as f64,
        pages: info.pages,
        optimized_streams,
        images: image_results,
        output_images,
        image_optimization,
        image_preparation: preparation,
        rgb_flate,
        advanced_optimization,
        size_before,
        size_after,
        stages,
        writer,
        strategy_candidates,
        stream_optimization,
        warnings,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}
fn load_options(limits: &Limits) -> LoadOptions {
    LoadOptions {
        strict: true,
        max_decompressed_size: Some(limits.max_stream_bytes),
        ..Default::default()
    }
}
fn load(path: &Path, limits: &Limits) -> Result<(Vec<u8>, Document)> {
    let bytes = read_bounded(path, limits.max_file_bytes)?;
    if !bytes.starts_with(b"%PDF-") {
        return Err(Error::Invalid("不是以 PDF 文件头开头的文档".into()));
    }
    let doc = Document::load_mem_with_options(&bytes, load_options(limits))?;
    if doc.encryption_state.is_some() || doc.trailer.has(b"Encrypt") {
        return Err(Error::Unsupported("加密 PDF".into()));
    }
    Ok((bytes, doc))
}
fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(Error::Invalid("请选择普通文件".into()));
    }
    if metadata.len() > max {
        return Err(Error::Limit(format!(
            "文件不能超过 {} MB",
            max / 1024 / 1024
        )));
    }
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(Error::Limit("读取期间文件超出大小限制".into()));
    }
    Ok(bytes)
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
