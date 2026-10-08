use squeeze_core::{Limits, Options, Outcome, OutputMode, Preset, compress, inspect_file};
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    sync::atomic::AtomicBool,
};
use tempfile::tempdir;

#[test]
fn default_limits_accept_pdfs_larger_than_256_mib_in_both_modes() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("large.pdf");
    let mut file = File::create(&input).unwrap();
    file.write_all(b"%PDF-1.7\n").unwrap();
    // NUL is PDF whitespace. Leave a sparse gap before the objects so the
    // regression crosses the old file cap without allocating a huge fixture.
    file.seek(SeekFrom::Start(257 * 1024 * 1024)).unwrap();
    let content = "0 0 1 rg 10 10 80 80 re f\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> /Contents 4 0 R >>"
            .to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
    ];
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(file.stream_position().unwrap());
        writeln!(file, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let xref = file.stream_position().unwrap();
    write!(file, "xref\n0 5\n0000000000 65535 f \n").unwrap();
    for offset in offsets {
        writeln!(file, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        file,
        "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    )
    .unwrap();
    let original_bytes = file.metadata().unwrap().len();
    drop(file);

    let info = inspect_file(&input, &Limits::default()).unwrap();
    assert_eq!(info.pages, 1);
    assert_eq!(info.bytes, original_bytes);
    assert!(info.bytes > 256 * 1024 * 1024);
    for output_mode in [OutputMode::Preserve, OutputMode::Raster] {
        let output = dir.path().join(format!("{output_mode:?}.pdf"));
        let report = compress(
            &input,
            &output,
            &Options {
                output_mode,
                preset: Preset::Small,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.outcome, Outcome::Saved);
        assert_eq!(report.original_bytes, original_bytes);
        assert_eq!(inspect_file(&output, &Limits::default()).unwrap().pages, 1);
    }
}
