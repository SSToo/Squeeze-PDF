# General PDF compression: deeper research and validation

Date: 2026-10-07. Product baseline: 0.1.15. This supplements the [previous review](PDF-OPTIMIZATION-NEXT.md) with additional code gaps, complete-file experiments and failed counterexamples.

Later implementation: the first three P0 items entered restricted default paths in 0.1.16, together with strict-lossless image candidates. Version 0.1.19 added chains, short Contents and complete fallback. See the [validation record](VALIDATION.md) for implementation, budgets and actual comparisons. The original research figures remain below; P1 graphics state, Form inlining and paired downsampling were not implemented at this point.

This research added development experiments/reports only, without changing the runtime engine, presets, DPI, quality or packages. The macOS arm64 0.1.15 DMG retained SHA-256 `bd90a0b0f791cc493c10d09b8b387ae0f22a53d741b5602480026ecb621da236`. No additional Windows, macOS Intel or Acrobat comparisons were performed.

## Findings and priorities

| Priority   | Direction                                                             | Gap at the time                                                                                                     | Evidence/status                                                                                                                       |
| ---------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| P0         | Exact raw-component re-encoding                                       | Images exclude 16-bit, Lab, DeviceN and other colors; generic Flate insufficiently exploits row correlation         | 16-bit RGB, 8-bit Lab and two-component DeviceN retained raw samples with identical pixels in three readers; ready for implementation |
| P0         | Recognize truly default Decode                                        | Any Decode currently blocks relevant optimization, including default RGB mapping                                    | An RGB sample unlocked Indexed: 5,737 → 1,790 B, identical in three readers                                                           |
| P0         | Object numbering, generations and layout                              | No compaction after cleanup; streams group by number; nonzero generations cannot enter current lopdf object streams | Two Fidelity results saved another 23,471 / 102,015 B; larger synthetic nonzero-generation gains; retain original-layout candidate    |
| P1         | Redundant graphics-state assignments                                  | No complete state tracking; repeated widths/colors/caps/dashes remain                                               | Restricted stroke sample: 6,229 → 5,523 B, identical in three readers                                                                 |
| P1         | Inline or reuse Forms based on call counts                            | Existing reuse focuses on closed solid fills; single-use ordinary Forms still incur object/stream overhead          | Single-use sample: 43,018 → 14,709 B, identical in three readers. Broader stroke reuse failed strict losslessness.                    |
| P1, staged | Mask-preserving optimization and paired Balanced/Smaller downsampling | Masks must match the main grid and have no Matte; SMask blocks downsampling                                         | Gap confirmed; paired downsampling unverified. Apparently opaque-mask deletion has counterexamples.                                   |
| Supporting | Budgeted cleanup/candidate selection                                  | Decoded sharing/cleanup occur early; later normalization can expose more sharing                                    | Pipeline analysis only, without real-file benefit evidence; do not repeatedly quantize JPEG                                           |

These supplement earlier chains, short Contents, strict-lossless images and automatic fonts rather than counting unimplemented plans as new capabilities.

## 1. Compress special images without converting their colors

`images.rs::decode` mainly uses supported `DynamicImage` colors. The entry excludes 16-bit images, and `color_channels` rejects Lab, Separation, DeviceN, CalRGB and others. The fifth-round exact CMYK path is useful but is not a general raw-component path.

Separate component encoding from RGB interpretation. For known component counts, bit depths, row sizes and chains, recover raw bytes exactly, then try PNG predictors or eligible TIFF Predictor 2. Preserve ColorSpace, Decode, ICC, spot-color names, tint functions, dimensions and bit depth, without altering spot colors, color management or resolution.

| Controlled sample     | Direct 0.1.15 | Candidate transform, then same version | Components | Poppler/CoreGraphics/PDF.js |
| --------------------- | ------------: | -------------------------------------: | ---------- | --------------------------- |
| 16-bit RGB gradient   |     915,963 B |                               28,235 B | Identical  | Identical pixels            |
| 8-bit Lab gradient    |     249,702 B |                                2,577 B | Identical  | Identical pixels            |
| Two-component DeviceN |       5,266 B |                                2,255 B | Identical  | Identical pixels            |

Candidates used PNG Up; 16-bit RGB stayed big-endian. pypdf checked every component byte, and each reader compared its own before/after pixels at 200 DPI. Color spaces and spot-color functions were unchanged.

These deliberately row-correlated samples do not imply 96%/98% savings for photographs. One DeviceN component did not affect display; preserving its bytes does not cover arbitrary printing spot functions, overprint or multicomponent documents. Add 16-bit Gray, CalGray/CalRGB, Separation, complex DeviceN, ICC Range and 1-/2-/4-bit row packing tests.

Start with bounded raw/Flate components and the previous chain parser. Limit overflow, row-end padding, predictor parameters, decoded lengths, corruption, recursive references, memory and cancellation. Appearance-shared protected images keep original encoding. The existing image function includes lossy JPEG and must not be used wholesale for Lossless.

## 2. Default Decode should not block all optimization

An explicit `/Decode [0 1 0 1 0 1]` need not mean special mapping. The current `has(Decode)` test is too broad: it prevents exact Indexed encoding for a controlled RGB image.

Removing only a proven DeviceRGB default reduced the same 480 × 320, 16-color Fidelity result from 5,737 to 1,790 B, with identical pixels in all three readers. JPEG quality was not reduced.

PDF.js also tests whether Decode is default for the color space before applying mapping, supporting a semantic check. [PDF.js image source](https://github.com/mozilla/pdf.js/blob/master/src/core/image.js).

Initially recognize exact DeviceRGB/Gray/CMYK default arrays and indirect references. Other colors need their own semantics: Indexed's default upper bound depends on bit depth, not universally hival; ICCBased Range, Lab ranges, inverted Decode and ImageMask polarity must remain meaningful. Raw-component exact Flate can still work with nondefault Decode, so these are independent optimizations.

## 3. Small, stable real-file gains from writer layout

Current cleanup writes directly without `renumber_objects`. Document A has 3,321 live objects up to number 28,492; Document B has 12,513 up to 41,875. Gaps alone may be cheap because cross-reference streams compress. Shorter references, object-stream headers and clustering similar dictionaries together affect final size.

Development example `crates/squeeze-core/examples/writer_identity_probe.rs` compares five layouts: same settings, compact numbering, compact/zero generations, dictionary-type clustering, and reference-frequency priority with type sorting. Object-stream group sizes and levels follow existing product rules.

| Read-only input: existing 0.1.15 Fidelity |        Input | Same settings |      Compact | Type grouping | Reference priority | Additional savings vs input |
| ----------------------------------------- | -----------: | ------------: | -----------: | ------------: | -----------------: | --------------------------: |
| Document A, 52 pages                      |  6,966,379 B |   6,966,387 B |  6,958,557 B |   6,946,185 B |        6,942,908 B |            23,471 B, ~0.34% |
| Document B, 57 pages                      | 40,079,726 B |  40,079,734 B | 40,016,435 B |  40,010,215 B |       39,977,711 B |           102,015 B, ~0.25% |

Same-settings rewriting adds 8 B, which is not counted as a strategy gain. All input generations were zero, so zeroing adds no benefit for these two files.

A controlled document with 297 live objects, sparse numbering and generation 7 measured 35,442 B rewritten, 31,821 B compacted and 9,719 B compacted/zeroed. lopdf 0.45.0 packs only eligible generation-zero objects into object streams. Generation normalization deserves investigation for full rewrites, not arbitrary incremental saves. Evidence comes from installed `src/writer.rs`/`src/processor.rs`; see [lopdf source](https://github.com/J-F-Liu/lopdf).

Validation uses bidirectional correspondence across the entire live graph. Apart from number/generation mapping, dictionaries, arrays, scalars and encoded bytes of every existing stream remain identical, including after reopening. Encrypted/ByteRange documents were skipped.

Seventeen general sample classes × five layouts, two real results and the nonzero-generation sample produced 100 graph-validated outputs. Three-reader pixel checks covered 17 best synthetic candidates, first/middle/last real pages (six total) and one nonzero-generation page: 24 pages, all identical. This was not every page of the real files.

No ordering wins universally: reference-frequency sorting grew some resource samples by 78–116 B against the same-settings rewrite, and all candidates for several small images exceeded their original files. Production must retain the previous engine candidate and original within budget and compare complete files. Existing object streams are not a new optimization.

## 4. Simplify vector execution semantics

### Redundant state assignments

Repeatedly setting identical color, width, caps, joins and dashes before each line can be redundant after the first effective assignment. Removing them in a controlled 600-stroke sample reduced 6,229 to 5,523 B, identical in three readers.

Track q/Q, stroke/fill color spaces and gs effects. Stop simplification on unknown operators/state. cm multiplies matrices and cannot be deleted because its numbers appeared before; repeated text positioning, clipping or drawing strings are not automatically redundant.

MuPDF's content filter tracks pending/committed state and emits it as drawing requires. This is a technical reference for an original Rust implementation, not integrated code. [MuPDF content filter](https://github.com/ArtifexSoftware/mupdf/blob/master/source/pdf/pdf-op-filter.c). Only manually constructed equivalent candidates were tested here; no general state optimizer was implemented.

### Inline single-use Forms, reuse repeated Forms

Inlining a simple single-use Form can eliminate dictionary/reference/stream overhead and compress adjacent content together. A sample with 100 single-use ordinary Forms shrank from 43,018 to 14,709 B, identical in three readers.

The sample had empty resources, no transparency Group/tags/optional content, explicit BBox and a known empty current path. Candidates inserted transformations and BBox clipping without rasterizing.

A general implementation must handle Matrix, BBox, resource renaming/inheritance and graphics-state boundaries. q/Q does not save the current path: `re W n` can discard a caller's pending path, so wrapping q/Q is insufficient. Initially skip transparency groups, OC, StructParent/MCID, Ref/OPI, interactive appearances and unknown resource contexts.

Repeated complex stroke blocks could supplement solid-fill reuse. However, a 3,600-segment block repeated six times shrank 119,455 → 21,841 B while PDF.js changed 2,205 pixels, maximum channel difference 21; Poppler/CoreGraphics matched. This fails strict losslessness and cannot count as releasable savings. Investigate floating-point transforms, clipping and drawing boundaries rather than weakening the lossless assertion.

Choose inlining versus reuse by call relationships and actual compressed cost rather than applying either universally.

## 5. Expand transparent-image support while preserving semantics

Current decoding requires same-sized image/mask grids, 8-bit Gray and no Matte/special properties; Balanced/Smaller downsampling additionally requires no SMask. Transparent textures/designs can therefore retain excess pixels.

First encode image/mask independently and exactly on original grids, preserving Decode, Matte, Mask, ICC and references. Only later try paired Balanced/Smaller downsampling, handling premultiplication, Matte unblending, different grids, nonwhite backgrounds and edge interpolation. Fidelity retains dimensions; strict Lossless retains samples. Compositing on white is not equivalent.

PDF Association recommends avoiding unnecessary opaque soft masks when creating PDFs, but this is generation/RIP-performance advice, not proof that arbitrary existing masks can be removed. [Creation guide, PDF page 31](https://pdfa.org/wp-content/uploads/2022/06/VDP_BestPractice_Developer.pdf). Two counterexamples were measured:

| Counterexample                          | Smaller candidate                                     | Result                                                                                           |
| --------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| 480 × 320 image with opaque 1 × 1 SMask | Remove mask, enabling exact encoding: 5,897 → 1,790 B | Poppler matches; CoreGraphics changes 667 edge pixels, maximum difference 230. Reject.           |
| Opaque SMask plus black color-key Mask  | Remove SMask: 5,915 → 5,735 B                         | Exposes previously overridden Mask; changes 25,385 Poppler / 19,123 CoreGraphics pixels. Reject. |

PDF.js explicitly prioritizes SMask and handles Mask only when SMask is absent, supporting the second explanation. [PDF.js mask handling](https://github.com/mozilla/pdf.js/blob/master/src/core/image.js). Graphics-state soft masks and nested transparency add further context.

Focus on optimization while retaining masks, then paired sampling, rather than automatically deleting apparently ineffective masks. Paired sampling was only a confirmed gap, without measured compression/quality claims here.

## 6. Optimization order and complete-file candidates

In lib.rs, resource cleanup and decoded image sharing precede image candidates; paths, outline reuse, content, Flate, Zopfli, resource-byte/subgraph deduplication and writing follow. Decode normalization, raw encoding or Form inlining may create sharing not visible earlier.

A bounded structural finish can refresh usage, deduplicate under current semantics, remove newly unused objects and try limited layouts. Retain the best complete candidate after each step, with explicit depth/iteration/memory/time limits and resource-identity, tag, editable-field and shared-appearance protection.

Never decode/requantize the previous pass's JPEG repeatedly. Generate lossy candidates from original components/coefficients or fixed source pixels, then cache them. Iteration may affect provably equivalent structure only. This was pipeline analysis; no experiment proved that another pass generally reduces size.

## Validation and suggested order

Experiments are in ignored `tmp/research-deep2-2026-10-07/`. The Rust writer probe remains in `crates/squeeze-core/examples/writer_identity_probe.rs`, without product calls. Release example builds, strict example clippy and rustfmt passed.

- Nine new sample classes, 18 worker runs: source/output pixels matched within Poppler, CoreGraphics and PDF.js; source SHA-256 remained unchanged.
- Six of seven candidate comparisons passed three readers: default Decode, 16-bit RGB, Lab, DeviceN, redundant state and single-use Form. Stroke reuse failed PDF.js. Two mask-removal counterexamples were rejected during native-reader checks.
- PDF.js had 24 passes and one documented failure among 25 comparisons. Failures were not hidden or counted as lossless gains. Text/position comparisons matched, but drawing-only samples lack extractable text and do not establish font-map coverage.
- All 100 writer graph checks passed; 24 sampled pages matched in three readers. CoreGraphics used fixed 200 DPI with antialiasing/interpolation disabled for within-reader comparisons, not every display setting.
- Complete-file tables compare direct 0.1.15 processing with candidate-transformed input processed by the same version. They are not stream-payload comparisons. Controlled feasibility is not completed general production implementation.

Next, unify P0 into bounded image-chain parsing and an exact-only path supporting default Decode/raw predictors, then compact numbering and budgeted complete-file layouts. Develop state simplification and single-use inlining independently. For transparency, exact original-grid encoding precedes paired downsampling.

Automatic font subsets and CMYK/YCCK coefficient optimization retain the previous plan's priorities. Rechecking 384 existing JPEG candidates found none smaller while passing every original strict quality gate, so there is no new case for default Jpegli. More coordinate error, lower DPI, removing tags/attachments, flattening transparency and multilayer MRC remain outside scope. Image mode stays one image per page.
