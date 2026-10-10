//! Development-only genuine CMYK and YCCK source JPEGs.
use std::{env, fs, path::Path};
fn main() {
    let root = env::args().nth(1).expect("directory");
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    for (name, space) in [
        ("cmyk", mozjpeg::ColorSpace::JCS_CMYK),
        ("ycck", mozjpeg::ColorSpace::JCS_YCCK),
    ] {
        let mut c = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_CMYK);
        c.set_size(320, 240);
        c.set_quality(94.);
        c.set_fastest_defaults();
        c.set_color_space(space);
        let mut c = c.start_compress(Vec::new()).unwrap();
        let raw: Vec<_> = (0..320 * 240)
            .flat_map(|i| {
                [
                    (i % 320 % 256) as u8,
                    (i / 320 % 256) as u8,
                    ((i % 320 + i / 320) % 256) as u8,
                    20,
                ]
            })
            .collect();
        c.write_scanlines(&raw).unwrap();
        fs::write(root.join(format!("{name}.jpg")), c.finish().unwrap()).unwrap();
    }
}
