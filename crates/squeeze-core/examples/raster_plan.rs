//! Development-only bridge for testing the production browser renderer.
use squeeze_core::{Options, OutputMode, Preset, prepare_raster};
use std::sync::atomic::AtomicBool;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let preset = match args.get(2).map(String::as_str) {
        Some("lossless") => Preset::Lossless,
        Some("balanced") => Preset::Balanced,
        Some("small") => Preset::Small,
        _ => return Err("input.pdf lossless|balanced|small".into()),
    };
    let options = Options {
        preset,
        output_mode: OutputMode::Raster,
        ..Default::default()
    };
    println!(
        "{}",
        serde_json::to_string(&prepare_raster(
            args.get(1).ok_or("input.pdf")?,
            &options,
            &AtomicBool::new(false)
        )?)?
    );
    Ok(())
}
