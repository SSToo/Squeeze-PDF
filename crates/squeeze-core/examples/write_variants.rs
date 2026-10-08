//! Developer-only experiment: rewrite an already optimized PDF with different
//! object-stream groups/levels. No image or drawing transformations are run.
use lopdf::{Document, SaveOptions};
use serde_json::json;
use std::{fs, path::PathBuf, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let input = PathBuf::from(args.get(1).ok_or("input required")?);
    let directory = PathBuf::from(args.get(2).ok_or("output directory required")?);
    fs::create_dir_all(&directory)?;
    let mut base = Document::load(&input)?;
    base.objects.retain(|_, o| {
        !o.as_stream().is_ok_and(|s| {
            matches!(
                s.dict.get(b"Type").and_then(|o| o.as_name()),
                Ok(b"ObjStm" | b"XRef")
            )
        })
    });
    let mut rows = Vec::new();
    for group in [100, 250, 500, 1000] {
        for level in [6, 9] {
            let mut doc = base.clone();
            let started = Instant::now();
            let path = directory.join(format!("group-{group}-level-{level}.pdf"));
            if path.exists() {
                return Err("experiment output exists".into());
            }
            let mut file = fs::File::create_new(&path)?;
            doc.save_with_options(
                &mut file,
                SaveOptions::builder()
                    .use_object_streams(true)
                    .use_xref_streams(true)
                    .max_objects_per_stream(group)
                    .compression_level(level)
                    .build(),
            )?;
            let elapsed = started.elapsed().as_millis();
            let reread = Document::load(&path)?;
            if reread.get_pages().len() != base.get_pages().len() {
                return Err("page count mismatch".into());
            }
            // Every original live stream must retain its dictionary and payload.
            for (id, o) in &base.objects {
                if let Ok(s) = o.as_stream() {
                    let other = reread.get_object(*id)?.as_stream()?;
                    if s.content != other.content || s.dict != other.dict {
                        return Err("stream changed".into());
                    }
                }
            }
            rows.push(json!({"group":group,"level":level,"bytes":file.metadata()?.len(),"elapsed_ms":elapsed}));
        }
    }
    fs::write(
        directory.join("results.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}
