use clap::{Parser, Subcommand, ValueEnum};
use squeeze_core::{Limits, Options, Outcome, OutputMode, Preset};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Parser)]
#[command(
    name = "squeeze",
    version,
    about = "离线 Rust PDF 压缩工具；保留原文件"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Audit {
        input: PathBuf,
    },
    Inspect {
        input: PathBuf,
    },
    Compress {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value = "lossless")]
        preset: Mode,
        #[arg(long, value_enum, default_value = "preserve")]
        output_mode: ContentMode,
        #[arg(long,value_parser=clap::value_parser!(u8).range(1..=100))]
        quality: Option<u8>,
        #[arg(long,value_parser=clap::value_parser!(u16).range(72..=600))]
        dpi: Option<u16>,
        #[arg(long)]
        json: bool,
        #[arg(long, hide = true)]
        events: bool,
        #[arg(long, hide = true)]
        temp_dir: Option<PathBuf>,
        #[arg(long, hide = true)]
        raster_manifest: Option<PathBuf>,
        #[arg(long)]
        compact_paths: bool,
        #[arg(long)]
        reuse_paths: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Lossless,
    Fidelity,
    Balanced,
    Small,
}
#[derive(Clone, Copy, ValueEnum)]
enum ContentMode {
    Preserve,
    Raster,
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.command {
        Command::Audit { input } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&squeeze_core::audit_file(
                    input,
                    &Limits::default()
                )?)?
            );
        }
        Command::Inspect { input } => {
            let info = squeeze_core::inspect_file(input, &Limits::default())?;
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
        Command::Compress {
            input,
            output,
            preset,
            output_mode,
            quality,
            dpi,
            json,
            events,
            temp_dir,
            raster_manifest,
            compact_paths,
            reuse_paths,
        } => {
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = cancel.clone();
            ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::Relaxed))?;
            let preset = match preset {
                Mode::Lossless => Preset::Lossless,
                Mode::Fidelity => Preset::Fidelity,
                Mode::Balanced => Preset::Balanced,
                Mode::Small => Preset::Small,
            };
            let options = Options {
                preset,
                output_mode: match output_mode {
                    ContentMode::Preserve => OutputMode::Preserve,
                    ContentMode::Raster => OutputMode::Raster,
                },
                jpeg_quality: quality,
                target_dpi: dpi,
                temporary_directory: temp_dir,
                raster_manifest,
                compact_paths,
                reuse_paths,
                ..Default::default()
            };
            let report = squeeze_core::compress(input, output, &options, &cancel, |p| {
                if events {
                    if let Ok(line) = serde_json::to_string(&p) {
                        eprintln!("{line}");
                    }
                } else if !json {
                    eprintln!("{:?} {}/{}", p.phase, p.completed, p.total);
                }
            })?;
            if json {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                println!(
                    "{}；{} → {} 字节，缩小 {:.1}%",
                    if report.outcome == Outcome::Saved {
                        "已导出"
                    } else {
                        "没有更小的结果，保留原文件"
                    },
                    report.original_bytes,
                    report.result_bytes,
                    report.reduction_percent
                );
                for warning in report.warnings {
                    println!("{warning}");
                }
            }
        }
    }
    Ok(())
}
