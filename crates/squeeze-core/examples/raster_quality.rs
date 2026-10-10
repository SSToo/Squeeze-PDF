//! QA entry point for the production desktop quality gate.
use std::{env, sync::atomic::AtomicBool};
fn main() {
    let a = env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(
        a.len(),
        5,
        "baseline candidate direct-directory supersampled-directory flags-json"
    );
    let pages: Vec<bool> = serde_json::from_str(&a[4]).unwrap();
    let report = squeeze_core::compare_raster_quality(
        &a[0],
        &a[1],
        &a[2],
        &a[3],
        &pages,
        &squeeze_core::Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    println!("{}", serde_json::to_string(&report).unwrap());
}
