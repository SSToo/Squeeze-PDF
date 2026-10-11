# Image-mode optimization investigation

Date: 2026-10-05. Sources: ImagesTool's public pages, frontend assets and built-in demo. Desktop PDFs were inspected locally and never uploaded to a third-party website. The initial investigation did not change the product; the user later authorized MozJPEG integration, implemented in 0.1.2 below. Website parameters reflect the implementation inspected on this date and may change later.

## 0.1.4: repeated vectors in preserve mode

Further analysis of source H identified repeatedly embedded outlined text as the main remaining overhead. Rust gained exact repeated-outline detection and vector Form reuse. Intersecting outlines stay together; patterns/gradients, clipping, open paths and interactive documents are skipped. Adoption requires positive total savings.

With image quality/DPI unchanged, H preserve Smaller fell from 35,418,667 to 3,034,553 bytes and Balanced from 36,520,413 to 4,136,053 bytes. All 52 pages of both presets passed independent rendering; median RGB RMSE against corresponding 0.1.3 results was about 0.243. Fonts, Type3 glyphs, transparency masks and ICC content were retained. Outline-heavy documents benefit most; PDFs already using fonts efficiently will not see comparable ratios.

See [VALIDATION.md](VALIDATION.md) for protections, quality boundaries and regression evidence.

## 0.1.3: findings and implementation

The user confirmed W as the reference website's Quality priority / bitmap PDF output. S was excluded from this round. Acrobat was unavailable; no new Acrobat output was requested and no measured ranking against Acrobat was claimed.

- H preserve Smaller in 0.1.2 contained about 44.51 MB of drawing/other streams and only about 1.10 MB of images. Image encoding alone could not remove most remaining size.
- Desktop preserve Balanced/Smaller gained path precision optimization for independent page streams. It tracks q/Q, cm and UserUnit, bounding rounding in physical coordinates. Rectangle corners account for accumulated x+w/y+h error. Contents arrays, Forms, glyphs, patterns, streams shared for other purposes and interactive documents are skipped. Fidelity and CLI defaults did not enable this then.
- Desktop image mode switched to existing local PDF.js, rendering directly at final dimensions, transferring each page as binary to Rust, then choosing MozJPEG/Flate and rebuilding. No intermediate JPEG file is used. Canvases are released per page, rendering supports cancellation/timeouts, and both ends check source hashes/dimensions.
- Local macOS arm64 H image Balanced was 9,813,647 bytes versus W at 9,811,180, about 0.025% apart. All 52 images were 1400 × 1811. W's resource dictionaries share all 52 images; comparison must identify the actually drawn image rather than the first dictionary entry.
- Median same-size decoded RMSE against W improved from about 12.35 to 5.63. The older 1812-pixel height was Lanczos-resized to 1811 for comparison. This means closer to the reference, not necessarily closer to the original or perceptually better. Canvas implementation affects output; Node/Skia experiments differ from actual macOS WebKit.
- H preserve Smaller was 35,418,667 bytes versus 45,869,733 previously; Balanced 36,520,413 versus 46,971,479. Text/vector types remained, but very fine edges can change. All 52 independently rendered pages had median RGB RMSE about 0.642 against previous preserve output. Sampled non-path operations/text were unchanged; masks, ICC and Type3 glyph streams retained identical decoded-byte sets.

See [VALIDATION.md](VALIDATION.md). Historical S measurements below are not this round's reference.

## Confirmed reference behavior

- The [compression interface](https://pdf.imagestool.com/zh-CN/compress-pdf) offers native/bitmap modes. Bitmap advanced controls include JPG/PNG, quality, pixel width, grayscale and thresholding, plus PNG color count.
- Inspected [compression component](https://pdf.imagestool.com/b/js/cf6b0ada.js) and [processing module](https://pdf.imagestool.com/b/js/4a0bf74f.js) use 1400 px/JPEG 72 for bitmap quality priority and 1000 px/quality 60 for high compression. Advanced settings override presets. These are implementation parameters, not permanent guarantees.
- Bitmap rendering uses PDF.js and JPEG encoding a MozJPEG worker with progressive encoding, optimized Huffman, quantization-table option 3 and chroma sampling. UI copy does not establish automatic page classification; advertised ratios are not same-quality measurements.
- [MozJPEG documentation](https://github.com/mozilla/mozjpeg/blob/master/README-mozilla.txt) describes scan optimization, Trellis and deringing, with higher compression costing encoding time. Not every capability's final activation in the website worker was independently verified.

## 0.1.1 gaps

`raster.rs` rendered with hayro directly at final DPI, then encoded all lossy pages as RGB JPEG. `jpeg.rs` enabled optimized Huffman but used basic quantization/sequential coding without MozJPEG scan optimization/Trellis. Monochrome text, line art and photos shared one encoding path.

Smaller used 100 DPI/JPEG 50 and Balanced 200 DPI/JPEG 85. The reference uses pixel width, so identically named presets are not directly comparable: Letter at widths 1000/1400 px is about 118/165 DPI.

Local H Smaller was 5,211,292 bytes; page 8 was 851 × 1100. Desktop S was 5,277,729 bytes; its actually drawn page-8 image was progressive JPEG at 1000 × 1294. Similar size with more pixels identified an improvement opportunity, without claiming S came from the reference site or ranking encoders with different rendered inputs.

## Common-input pixel experiment

H page 8's existing 300 DPI lossless image was resized to 1000 × 1294 as shared RGB input. Rust `jpeg-encoder` 0.7.1 used quality 60, 4:2:0 and optimized Huffman. Files are in ignored `tmp/codec-probe/`, not distributed assets.

| Candidate                                  | JPEG bytes | RMSE vs common RGB input |
| ------------------------------------------ | ---------: | -----------------------: |
| Default quantization, sequential           |    135,755 |                    5.808 |
| Default quantization, library progressive  |    148,394 |                    5.808 |
| Robidoux quantization, sequential          |    123,606 |                    6.245 |
| Robidoux quantization, library progressive |    135,825 |                    6.245 |

Simply enabling this library's progressive mode grew this sample about 9.3% without changing decoded pixels. Its mainly spectral-selection implementation is not equivalent to MozJPEG scan optimization. Quantization changes size and error distribution; RMSE alone cannot establish perceptual quality or small-text readability.

MozJPEG was added to the same-input experiment later: MozJPEG 0.10.13 / mozjpeg-sys 2.2.3, default quantization, progressive scan optimization, Trellis and 4:2:0. Quality 60 produced 97,724 bytes/RMSE 8.512; quality 85 produced 159,134 bytes/RMSE 4.828, similar in size/error to the older quality 72. Equal quality numbers yield different errors, so their size reductions are not same-quality gains.

## Implemented in 0.1.2

- Statically linked MozJPEG; Rust validates parameters, catches errors, handles PDFs and protects files. Optional assembly SIMD is disabled, requiring only the existing C compiler; the worker has no dynamic JPEG dependency.
- Image Smaller: 1000 px/quality 60; Balanced: 1400 px/quality 72, proportional height. Explicit CLI `--dpi` overrides default width. High quality stays 300 DPI/lossless pixels.
- Lossy pages render at 2× then Lanczos3 downsample when existing pixel/stream budgets permit; otherwise render directly at unchanged final dimensions.
- Exactly grayscale pages collapse identical RGB channels. Grayscale or at-most-256-color pages compare JPEG and lossless Flate with None/Sub/Up predictors. No automatic color removal, thresholding or palette quantization; one image per page remains.
- MozJPEG scan configuration resets defaults, so it now runs before quality settings. Regression tests verify that quality actually changes bytes/error.

Real H image Smaller became 5,388,392 bytes at 1000 × 1294 versus 5,211,292 at 851 × 1100: about 3.4% more bytes and 38.2% more pixels. Balanced became 9,974,565 at 1400 × 1812 versus 23,388,194 at 1701 × 2200. Balanced changed both dimensions and encoding quality, so this is not a same-quality algorithm ranking. See [VALIDATION.md](VALIDATION.md) for complete regressions and limits.

## Further evaluation

1. Fix final dimensions and benchmark text, small colored text, fine lines, photos and scans. Quality numbers are not universal across encoders; compare size, decoded error, local readability and time.
2. Evaluate MozJPEG quantization, chroma and scan parameters for colored small-text edges/fine lines. Keep versions pinned and update licenses on upgrades. Rust handles PDFs; MozJPEG itself is C.
3. Improve exact grayscale and Flate/JPEG selection for mixed text/photos; consider lossless indexed color without thresholding away pale stamps, gray lines or light text.
4. Grayscale, thresholding and palette quantization alter colors and require explicit advanced options. Color remains default; control dimensions separately from quality and show final dimensions in preview.
5. Study safe avoidance of needless upscaling/recompression for existing full-page images. Current rendering already encodes pixels directly without intermediate JPEGs.
6. Extend one-image-per-page, ordering/dimensions/rotation, font/image, cancellation and budget regressions, then test Windows/macOS Intel devices. Preserve mode continues retaining text/vectors.

Acceptance compares quality/size at equal dimensions and small-text readability at similar sizes. Lowering DPI or quality alone is not an encoding improvement.
