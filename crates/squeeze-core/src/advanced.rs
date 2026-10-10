//! A bounded, exact-only finishing candidate, loaded after the earlier plans
//! have been released. Large documents do not require another source clone.
use crate::{Built, Options, OutputMode, Result, check_cancel};
use lopdf::Document;
use serde::Serialize;
use std::{path::Path, sync::atomic::AtomicBool};
use tempfile::NamedTempFile;
#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub status: String,
    pub baseline_bytes: u64,
    pub candidate_bytes: Option<u64>,
    pub selected: bool,
    pub rejection: Option<String>,
    pub large_images: crate::large_images::Stats,
    pub exact_sharing: crate::exact_sharing::Stats,
    pub shared_leaves: usize,
    pub cmyk_jpegs: usize,
    pub fonts: crate::font_subset::Stats,
    pub vectors: crate::vector_state::Stats,
    pub transparency: crate::transparent_images::Stats,
}
pub(crate) fn choose(
    chosen: &mut Built,
    options: &Options,
    parent: &Path,
    cancel: &AtomicBool,
) -> Result<Stats> {
    let mut stats = Stats {
        status: "not_applicable".into(),
        baseline_bytes: chosen.candidate.as_file().metadata()?.len(),
        transparency: chosen.transparency.clone(),
        ..Default::default()
    };
    if options.output_mode != OutputMode::Preserve {
        return Ok(stats);
    }
    check_cancel(cancel)?;
    let mut doc = Document::load_with_options(
        chosen.candidate.path(),
        crate::load_options(&options.limits),
    )?;
    let stage = crate::audit::Stage::start("advanced_exact", &doc);
    let parent = options.temporary_directory.as_deref().unwrap_or(parent);
    stats.large_images = crate::large_images::optimize(&mut doc, options, parent, cancel)?;
    stats.shared_leaves = crate::resource_leaves::deduplicate(&mut doc, options, cancel)?;
    stats.exact_sharing = crate::exact_sharing::optimize(&mut doc, options, cancel)?;
    stats.cmyk_jpegs = crate::cmyk_jpeg::optimize(&mut doc, options, cancel)?;
    stats.fonts = crate::font_subset::optimize(&mut doc, options, cancel)?;
    stats.vectors = crate::vector_state::optimize(&mut doc, options, cancel)?;
    let changes = stats.large_images.changed
        + stats.shared_leaves
        + stats.exact_sharing.merged
        + stats.cmyk_jpegs
        + stats.fonts.fonts_changed
        + stats.vectors.changed_streams
        + stats.vectors.inlined_forms;
    if changes == 0 {
        stats.status = "no_change".into();
        return Ok(stats);
    }
    doc.objects.retain(|_, o| !crate::inspect::is_container(o));
    doc.prune_objects();
    crate::validate::references(&doc)?;
    let mut writer = chosen.writer.clone();
    let file = crate::writing::best(
        &mut doc,
        NamedTempFile::new_in(parent)?,
        &mut writer,
        &options.limits,
        cancel,
    )?;
    let bytes = file.as_file().metadata()?.len();
    stats.candidate_bytes = Some(bytes);
    check_cancel(cancel)?;
    let verified = Document::load_with_options(file.path(), crate::load_options(&options.limits))?;
    crate::validate::round_trip(&doc, &verified)?;
    stats.selected = bytes < stats.baseline_bytes;
    stats.status = if stats.selected {
        "selected"
    } else {
        "kept_baseline"
    }
    .into();
    chosen.strategy_candidates.push(crate::StrategyCandidate {
        strategy: "advanced_exact".into(),
        bytes,
        selected: stats.selected,
    });
    if stats.selected {
        chosen.candidate = file;
        chosen.writer = writer;
        chosen.size_after = crate::audit::size(&verified, bytes);
        chosen.output_images = crate::final_images(&verified);
        chosen.optimized_streams += changes;
        chosen.stages.push(stage.finish(&doc));
    }
    Ok(stats)
}
