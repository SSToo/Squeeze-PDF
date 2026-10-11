# Squeeze PDF implementation plan

Date: 2026-10-05.

Status at the time of this plan: the Rust engine, CLI, desktop MVP and both output modes were implemented. Version 0.1.4 added repeated vector-outline reuse. Preserve mode combines path precision, resource sharing and image optimization; desktop image mode uses local PDF.js rendering. This is a historical plan; see the [README](README.md) for current behavior and limits.

## Confirmed requirements

- Windows and macOS desktop application. On 2026-10-10, Apache-2.0 was selected for original code, allowing commercial use and closed-source redistribution; third-party components retain their original licenses.
- Offline processing by default, without requiring users to install external PDF tools.
- Original Rust compression strategies with foundational parsing/rendering libraries; MozJPEG integration is allowed.
- Two independent output modes: retain original text and vectors, or convert each page to one image and assemble a PDF.
- Initial release: file queue, quality selection, output directory, cancellation/retry, result reports and before/after preview.

## Output modes and quality

| Output mode               | Content behavior                                                                      | Three presets                                                                                                                                                                |
| ------------------------- | ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Preserve original content | Keeps text, fonts and vectors; optimizes paths, structure and supported images        | All presets enable path precision and repeated-outline optimization. Fidelity: original image resolution / JPEG 95. Balanced: JPEG 85 / 200 DPI. Smaller: JPEG 50 / 100 DPI. |
| Full-page images          | Renders the visible area with page rotation; each page contains one independent image | High quality: 300 DPI / Flate. Balanced: 1400 px wide / JPEG 72. Smaller: 1000 px wide / JPEG 60.                                                                            |

Preserve mode exports only smaller results. Image mode is an explicitly requested content conversion and exports validated results even when larger, reporting the size accurately.

At this stage, image mode did not retain selectable text, vector paths, links or bookmarks, and rejected forms, annotations, attachments and missing nonstandard fonts. “Lossless” in High quality referred only to rendered image pixels and could not restore original vectors. Later support changes are documented in the README.

## Implemented architecture

| Layer               | Implementation                                                                                                |
| ------------------- | ------------------------------------------------------------------------------------------------------------- |
| Desktop             | Tauri 2, native dialogs, separate worker, timeouts/cancellation and restricted path access                    |
| Interface           | React + TypeScript + Vite, independent mode/quality selection, sequential queue                               |
| Rust engine         | `squeeze-core`: strategies, preflight, file protection and validation                                         |
| PDF I/O             | lopdf: objects and object streams                                                                             |
| Images              | image + static MozJPEG + flate2 Rust backend; restricted decoding, Lanczos downsampling, candidate comparison |
| Full-page rendering | Desktop PDF.js renders each page; Rust encodes and rebuilds the PDF. CLI defaults to hayro.                   |
| Preview             | PDF.js with locally bundled CMaps, fonts and WASM                                                             |
| CLI                 | `squeeze-cli`: two modes, three presets, JSON reports and cancellation                                        |
| Build               | Pinned Rust 1.92.0 and locked dependencies; three native CI targets; bundled third-party notices              |

The project does not reimplement PDF syntax, JPEG or font-rendering standards from scratch. qpdf and Ghostscript are not runtime dependencies.

## File protection and support scope

Processing pipeline:

`Preflight → Read original and record hash → Optimize or render pages → Write temporary candidate → Reopen and validate → Check original for changes → Commit without overwriting`

- Originals are read-only; output names are numbered automatically and existing files are never overwritten.
- At this stage: 256 MiB file limit, 64 MiB per decoded stream, 24 million pixels per image; object and analysis-operation limits.
- Digitally signed, encrypted (including empty-password), PDF/A, XFA and portfolio files are skipped.
- Preserve mode treats forms, annotations and attachments conservatively.
- Effective-DPI analysis covers inherited resources, shared images, Form Matrix and UserUnit; unanalyzed paths are not downsampled.
- Restricted default-range 1-/3-channel ICC images and ordinary grayscale soft masks are supported. Original ICC profiles and mask pixels are retained. Unverified CMYK, Matte and hard-mask types retain their original data.
- Image mode limits per-page bitmap dimensions and releases render caches page by page; it handles CropBox, Rotate, UserUnit and page order. The desktop renders directly at final dimensions and compares a single JPEG against lossless Flate for each page. Entirely grayscale pages use one channel.
- The desktop uses a separate process. Rendering and encoding each have a 120-second timeout; normal cancellation removes temporary files.
- A total memory budget, strict rejection of all malformed PDFs and recovery cleanup after forced exit remain outstanding.

## Validation at this stage

Rust automated tests, macOS arm64 compilation, generated-sample compression and independent Poppler rendering checks were completed. See [docs/VALIDATION.md](docs/VALIDATION.md) for measurements and desktop-flow validation records.

Synthetic samples help find implementation issues but cannot replace real-PDF compatibility tests or establish a universal compression ratio.

## Next steps recorded in this plan

| Stage                          | Work                                                                                                   | Acceptance                                                                                       |
| ------------------------------ | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| Real-document regression       | Collect 40–60 licensed Chinese, Office, scanned, drawing, complex-font and transparent-image documents | Record compatibility, size, quality, time and peak memory by category in both modes              |
| Resources and failures         | Total memory budget, disk full, read-only directories, crashes and power-loss recovery                 | Original hashes unchanged; no invalid or misleading leftover output; reliable cancellation/retry |
| Platform validation            | Native Windows x64 and macOS arm64/Intel CI plus clean-environment installation tests                  | Offline launch, queue, both modes, preview, export and uninstall pass                            |
| Commercial release preparation | Review distributed dependencies, project terms, signing, notarization, versions and checksums          | Windows installer and macOS DMG ready for public distribution                                    |
| Beta feedback                  | Small real-user trial focused on small text, colors and complex pages in image mode                    | Reproducible issues added to regression coverage; stable support scope defined                   |

This plan prioritized those checks before additional color/mask types, encryption, target-size output, OCR, automatic updates or commercial activation. Later releases may implement individual items; consult current documentation.

## Release gates

1. Supported outputs open correctly and retain the expected page count, visible dimensions and order.
2. Preserve mode retains text, vectors, form values, annotations, bookmarks and attachments; Fidelity receives independent page-by-page render comparisons.
3. Image mode contains exactly one image per page and no residual text/vector objects; independent readers check rotation, crop, fonts and colors.
4. Original hashes remain unchanged on success, failure and cancellation; existing outputs are not overwritten; reports correctly show savings or growth.
5. Review small text, tables, stamps, barcodes and colors manually in lossy outputs; image-difference metrics are supplementary.
6. Invalid files, memory pressure, timeouts and cancellation do not bring down the queue.
7. Windows and both macOS architectures complete installation regression; signing and licensing are reviewed before public distribution.
