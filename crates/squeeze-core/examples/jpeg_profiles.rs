// Developer-only same-pixel JPEG experiment. No external codec is a runtime dependency.
#[path = "../src/jpeg.rs"]
mod jpeg;
use std::{fs, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("input PPM")?);
    let output = PathBuf::from(args.next().ok_or("output directory")?);
    let quality: u8 = args.next().ok_or("quality")?.parse()?;
    let data = fs::read(input)?;
    let mut lines = data.splitn(4, |&c| c == b'\n');
    assert_eq!(lines.next(), Some(&b"P6"[..]));
    let dims = std::str::from_utf8(lines.next().ok_or("dimensions")?)?
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(lines.next(), Some(&b"255"[..]));
    let pixels = lines.next().ok_or("pixels")?;
    fs::create_dir_all(&output)?;
    let mut rows = Vec::new();
    let mut encoder = jpeg::Encoder::default();
    for (name, sampling, table, scan_trellis) in [
        (
            "baseline",
            if quality >= 90 { (1, 1) } else { (2, 2) },
            0,
            false,
        ),
        ("444-default", (1, 1), 0, false),
        ("422-default", (2, 1), 0, false),
        ("420-default", (2, 2), 0, false),
        ("444-annex", (1, 1), 1, false),
        ("444-msssim", (1, 1), 2, false),
        ("444-psnrhvs", (1, 1), 3, false),
        (
            "scans-trellis",
            if quality >= 90 { (1, 1) } else { (2, 2) },
            0,
            true,
        ),
    ] {
        let started = Instant::now();
        let encoded = if name == "baseline" {
            encoder.encode(pixels, dims[0], dims[1], false, quality)?
        } else {
            jpeg::encode_profile(
                pixels,
                dims[0],
                dims[1],
                false,
                quality,
                jpeg::Profile {
                    sampling,
                    table,
                    scan_trellis,
                },
            )?
        };
        fs::write(output.join(format!("{name}.jpg")), &encoded)?;
        rows.push(serde_json::json!({"name":name,"bytes":encoded.len(),"elapsed_ms":started.elapsed().as_millis(),"quality":quality,"dimensions":dims}));
    }
    fs::write(
        output.join("encoders.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    Ok(())
}
