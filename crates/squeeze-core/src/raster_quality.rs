//! Conservative quality gate against both fixed desktop render inputs.
use crate::{Error, Limits, Options, Result, check_cancel, inspect};
use lopdf::Document;
use serde::Serialize;
use std::{path::Path, sync::atomic::AtomicBool};
#[derive(Debug, Default, Serialize)]
pub struct RasterQuality {
    pub accepted: bool,
    pub checked_pages: usize,
    pub pages: Vec<PageQuality>,
    pub rejection: Option<String>,
}
#[derive(Debug, Default, Serialize)]
pub struct PageQuality {
    pub page: usize,
    pub direct_reference: Scores,
    pub supersampled_reference: Scores,
}
#[derive(Debug, Default, Serialize)]
pub struct Scores {
    pub baseline_rmse: f64,
    pub candidate_rmse: f64,
    pub baseline_edge_rmse: f64,
    pub candidate_edge_rmse: f64,
}
fn scores(
    reference: &[u8],
    a: &[u8],
    b: &[u8],
    width: usize,
    cancel: &AtomicBool,
) -> Result<Scores> {
    let mut sum = [0u64; 2];
    let mut edges = [0u64; 2];
    let mut count = 0usize;
    for (i, pixel) in reference.chunks_exact(3).enumerate() {
        if i % 16384 == 0 {
            check_cancel(cancel)?;
        }
        let edge = (i % width > 0
            && (0..3).any(|c| pixel[c].abs_diff(reference[(i - 1) * 3 + c]) > 24))
            || (i >= width
                && (0..3).any(|c| pixel[c].abs_diff(reference[(i - width) * 3 + c]) > 24));
        for (c, &component) in pixel.iter().enumerate() {
            let x = i * 3 + c;
            for (k, data) in [a, b].iter().enumerate() {
                let error = i64::from(data[x]) - i64::from(component);
                let e = (error * error) as u64;
                sum[k] += e;
                if edge {
                    edges[k] += e;
                }
            }
        }
        if edge {
            count += 3;
        }
    }
    Ok(Scores {
        baseline_rmse: (sum[0] as f64 / reference.len() as f64).sqrt(),
        candidate_rmse: (sum[1] as f64 / reference.len() as f64).sqrt(),
        baseline_edge_rmse: if count == 0 {
            0.
        } else {
            (edges[0] as f64 / count as f64).sqrt()
        },
        candidate_edge_rmse: if count == 0 {
            0.
        } else {
            (edges[1] as f64 / count as f64).sqrt()
        },
    })
}
fn good(s: &Scores) -> bool {
    s.candidate_rmse <= s.baseline_rmse + 1e-9
        && s.candidate_edge_rmse <= s.baseline_edge_rmse + 1e-9
}
fn image_stream(doc: &Document, page: lopdf::ObjectId) -> Result<&lopdf::Stream> {
    let r = inspect::inherited(doc, page, b"Resources")?
        .ok_or_else(|| Error::Validation("页面资源缺失".into()))?
        .as_dict()?;
    let x = doc.dereference(r.get(b"XObject")?)?.1.as_dict()?;
    Ok(doc.dereference(x.get(b"PageImage")?)?.1.as_stream()?)
}
fn sample_size(doc: &Document, page: lopdf::ObjectId) -> Result<usize> {
    let image = image_stream(doc, page)?;
    let dimension = |name| -> Result<usize> {
        let n = doc.dereference(image.dict.get(name)?)?.1.as_i64()?;
        usize::try_from(n)
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| Error::Validation("页面像素长度与尺寸不符".into()))
    };
    dimension(b"Width")?
        .checked_mul(dimension(b"Height")?)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| Error::Validation("页面像素长度与尺寸不符".into()))
}
fn pixels(doc: &Document, page: lopdf::ObjectId, options: &Options) -> Result<image::RgbImage> {
    let s = image_stream(doc, page)?;
    crate::images::decode(doc, s, options)
        .map(|image| image.to_rgb8())
        .map_err(Error::Validation)
}
/// Only alternate pages are checked; missing alternate pages retain direct
/// staging. Budget exhaustion rejects the optional candidate as a whole.
pub fn compare_raster_quality(
    baseline: impl AsRef<Path>,
    candidate: impl AsRef<Path>,
    direct_directory: impl AsRef<Path>,
    supersampled_directory: impl AsRef<Path>,
    alternate_pages: &[bool],
    limits: &Limits,
    cancel: &AtomicBool,
) -> Result<RasterQuality> {
    check_cancel(cancel)?;
    if std::fs::metadata(baseline.as_ref())?
        .len()
        .saturating_add(std::fs::metadata(candidate.as_ref())?.len())
        > 128 * 1024 * 1024
    {
        return Ok(RasterQuality {
            accepted: false,
            rejection: Some("quality_budget".into()),
            ..Default::default()
        });
    }
    let a = Document::load_with_options(baseline, crate::load_options(limits))?;
    let b = Document::load_with_options(candidate, crate::load_options(limits))?;
    let (ap, bp) = (inspect::pages(&a)?, inspect::pages(&b)?);
    let options = Options {
        limits: limits.clone(),
        ..Default::default()
    };
    let mut result = RasterQuality {
        accepted: true,
        ..Default::default()
    };
    let mut remaining = 32 * 1024 * 1024usize;
    if ap.len() != bp.len() || ap.len() != alternate_pages.len() {
        return Err(Error::Validation("页面像素数量与原件不符".into()));
    }
    for (i, (&ap, &bp)) in ap.iter().zip(&bp).enumerate() {
        check_cancel(cancel)?;
        if !alternate_pages[i] && image_stream(&a, ap)? == image_stream(&b, bp)? {
            continue;
        }
        // Reject before decoding: otherwise an optional quality comparison
        // could allocate several full pages before discovering its budget.
        let bytes = sample_size(&a, ap)?;
        if sample_size(&b, bp)? != bytes {
            return Err(Error::Validation("页面像素长度与尺寸不符".into()));
        }
        if bytes > remaining {
            result.accepted = false;
            result.rejection = Some("quality_budget".into());
            return Ok(result);
        }
        let (aa, bb) = (pixels(&a, ap, &options)?, pixels(&b, bp, &options)?);
        if aa.dimensions() != bb.dimensions() || aa.as_raw().len() != bytes {
            return Err(Error::Validation("页面像素长度与尺寸不符".into()));
        }
        remaining -= bytes;
        let read = |directory: &Path| -> Result<Vec<u8>> {
            let path = directory.join(format!("{i}.rgb"));
            if std::fs::metadata(&path)?.len() != bytes as u64 {
                return Err(Error::Validation("页面像素长度与尺寸不符".into()));
            }
            Ok(std::fs::read(path)?)
        };
        let direct = read(direct_directory.as_ref())?;
        let supersampled = read(supersampled_directory.as_ref())?;
        let direct_reference = scores(
            &direct,
            aa.as_raw(),
            bb.as_raw(),
            aa.width() as usize,
            cancel,
        )?;
        let supersampled_reference = scores(
            &supersampled,
            aa.as_raw(),
            bb.as_raw(),
            aa.width() as usize,
            cancel,
        )?;
        result.accepted &= good(&direct_reference) && good(&supersampled_reference);
        result.checked_pages += 1;
        result.pages.push(PageQuality {
            page: i + 1,
            direct_reference,
            supersampled_reference,
        });
    }
    if !result.accepted {
        result.rejection = Some("pixel_or_edge_error".into());
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_budget_is_checked_before_image_decoding() {
        use lopdf::{Object, Stream, dictionary};
        let mut doc = Document::with_version("1.7");
        let tree = doc.new_object_id();
        let image = doc.add_object(Stream::new(
            dictionary! {"Type"=>"XObject", "Subtype"=>"Image", "Width"=>4096, "Height"=>4096, "BitsPerComponent"=>8, "ColorSpace"=>"DeviceRGB", "Filter"=>"DCTDecode"},
            vec![0], // Invalid JPEG must never be decoded above the work budget.
        ));
        let page = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>tree, "MediaBox"=>vec![0.into(),0.into(),72.into(),72.into()], "Resources"=>dictionary! {"XObject"=>dictionary! {"PageImage"=>image}}});
        doc.objects.insert(
            tree,
            Object::Dictionary(
                dictionary! {"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1},
            ),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>tree});
        doc.trailer.set("Root", root);
        let file = tempfile::NamedTempFile::new().unwrap();
        doc.save(file.path()).unwrap();
        let result = compare_raster_quality(
            file.path(),
            file.path(),
            ".",
            ".",
            &[true],
            &Limits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!result.accepted);
        assert_eq!(result.checked_pages, 0);
        assert_eq!(result.rejection.as_deref(), Some("quality_budget"));
    }
    #[test]
    fn quality_budget_rejects_before_loading_large_documents() {
        let a = tempfile::NamedTempFile::new().unwrap();
        a.as_file().set_len(128 * 1024 * 1024 + 1).unwrap();
        let b = tempfile::NamedTempFile::new().unwrap();
        let r = compare_raster_quality(
            a.path(),
            b.path(),
            ".",
            ".",
            &[],
            &Limits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!r.accepted);
        assert_eq!(r.checked_pages, 0);
        assert_eq!(r.rejection.as_deref(), Some("quality_budget"));
    }
    #[test]
    fn cancellation_precedes_any_quality_file_io() {
        assert!(matches!(
            compare_raster_quality(
                "missing-a",
                "missing-b",
                ".",
                ".",
                &[],
                &Limits::default(),
                &AtomicBool::new(true)
            ),
            Err(Error::Cancelled)
        ));
    }
    #[test]
    fn smaller_but_blurred_edges_cannot_pass_both_fixed_references() {
        let cancel = AtomicBool::new(false);
        let sharp = [0, 0, 0, 255, 255, 255, 0, 0, 0];
        let blurred = [20, 20, 20, 230, 230, 230, 20, 20, 20];
        assert!(!good(
            &scores(&sharp, &sharp, &blurred, 3, &cancel).unwrap()
        ));
        assert!(good(&scores(&sharp, &blurred, &sharp, 3, &cancel).unwrap()));
        assert!(good(&scores(&sharp, &sharp, &sharp, 3, &cancel).unwrap()));
    }
}
