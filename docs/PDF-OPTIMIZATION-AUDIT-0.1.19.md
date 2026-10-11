# 0.1.19 compression review and implementation plan

Date: 2026-10-08. Scope: the workspace's 0.1.19 engine, desktop image mode and seventh-round validation records. This review inspected code, existing measurements and official references only. It did not change product code, parameters, versions or packages, or generate new compression results. Previously delivered capabilities are documented in the [validation record](VALIDATION.md).

There is still room for general optimization, starting with entry-point and scale coverage for existing algorithms. Recommended order: protection and validation → image decoding → encoding selection → large images/long documents → exact resource sharing → desktop image quality → CMYK JPEG → automatic font subsetting → vectors and masks. The last five mostly continue earlier unimplemented plans; they are not new discoveries or delivered capabilities in this review.

## Confirmed gaps

| Item                                                       | Evidence at the time                                                                                                                                                             | Assessment                                                                                                                                                                                                  |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Image optimization precedes encoding-chain removal         | In lib.rs, images::optimize runs first and stream_candidates::choose last; the latter explicitly avoids rerunning lossy image processing                                         | Removing ASCII wrappers only enables stripping, structure and writer candidates; JPEG coefficient optimization, image prediction and decoded sharing cannot use the newly exposed input in the same pass    |
| Incomplete RGB candidate coverage above 256 colors         | Ordinary Flate in image_encoding.rs requires force_flate, a single channel or an exact palette; the native-sample path in images.rs mainly handles decode failures or Lossless   | Normally decodable RGB illustrations and colored-text screenshots may lack a Flate candidate in Fidelity/Balanced/Smaller despite strong row correlation. Desktop page encoding shares this limitation.     |
| New work is often skipped for large images/documents       | native_samples.rs has a 4 MiB per-image budget; images.rs has 16 MiB/four-second document budgets; complete-stream and writer candidates have a 64 MiB encoded-payload threshold | These are resource budgets, not user file-size limits. Removing the default file cap did not extend every optimization to large files.                                                                      |
| Sharing omits native samples and some encoding equivalence | resources.rs uses images::decode; resource_graph.rs compares encoded bytes                                                                                                       | 16-bit images, special colors, parameter forms and wrappers can miss sharing. Decoded equivalence for non-image leaf resources is not in production.                                                        |
| Inconsistent identity protection                           | New candidates skip deduplication after coalesce::identity_bound; the older lib.rs path calls structure::deduplicate without the same check                                      | A protection gap is confirmed, but no new corruption example was constructed here. Add duplicate content-stream cases referenced by MCR /Stm, OBJR and AP before judging page pixels alone.                 |
| Final image diagnostics can disagree with stage results    | Seventh-round hex-jpeg-image / fidelity reports images as original, 52,523 B, while final size_after.images is 26,261 B                                                          | Complete-file size is correct, but per-image reporting misses later stripping. Distinguish attempted, adopted and reverted candidates.                                                                      |
| Latest comparison set has limited scale                    | verified/reports.json has 47 categories / 326 pairs, all one-page; largest input 916,813 B, largest image 199,557 pixels                                                         | This validates local structures, not 100-page documents, images of tens of MiB, cross-page sharing or consecutive desktop tasks. Existing large-file entry tests are not large-image optimization evidence. |
| Desktop and CLI rendering differ                           | rasterCompress.ts renders at output size; raster.rs can render at 2× and downsample; desktop rendering has a 120-second whole-document limit                                     | Seventh-round validation established 18 native WKWebView baselines using isolated processes and QA IPC. Real consecutive tasks, Windows WebView2 and desktop supersampling still need separate acceptance.  |

Relevant entry points: [image candidates](../crates/squeeze-core/src/image_encoding.rs), [image processing](../crates/squeeze-core/src/images.rs), [native samples](../crates/squeeze-core/src/native_samples.rs), [pipeline](../crates/squeeze-core/src/lib.rs), [complete candidates](../crates/squeeze-core/src/stream_candidates.rs), [older stream deduplication](../crates/squeeze-core/src/structure.rs), [desktop rendering](../src/rasterCompress.ts).

Object streams, five PNG row predictors, exact Indexed/G4, RGB/Gray JPEG coefficient optimization, resource cleanup and vector-outline reuse already exist. This plan extends their eligibility, candidate selection, scale and validation rather than presenting them as new features.

## Recommended implementation order

### 0. Unify protection, baselines and diagnostics — P0

Goal: establish correct comparisons and explain each skip.

- Apply consistent identity protection to old and new object-merging paths. Protect objects constrained by content-stream identity, tag relationships and interactive appearances. Skip a strategy when boundaries are unclear; assess safe leaf resources separately.
- Add counterexamples with identical MCIDs in different Stm objects on the same page, cross-page sharing and independent duplicate appearances. Check tag references/ownership, form fields/values, attachment bytes and sharing. Pixel equality cannot replace object-relationship checks.
- Extend single-page validators to all pages. Add 10-/100-page shared-resource samples, synthetic large images, mixed encodings, differing page boxes/rotation/UserUnit, cancellation and consecutive tasks. Users should not have to collect a comprehensive corpus.
- Report final per-image encoding/bytes/dimensions, skip classes, exhausted budgets, stage timings and peak memory. Reverted candidates are not final savings. Keep diagnostics traceable after renumbering.
- Freeze a 0.1.19 baseline. Record correctness fixes separately from compression gains: an older, smaller output that breaks identity is not a valid size target.

Deliverables: unified protection, executable multipage regression samples and final-candidate diagnostics, without claiming a new compression percentage.

### 1. Unify and move image decoding earlier — P1, first optimization

Goal: wrappers, arrays and indirect references should not prevent the same image from accessing existing algorithms.

- Build a restricted normalized view before resource sharing and main image optimization, reusing strict filter parsing. Strip wrappers while retaining terminal JPEG/G4 payloads, original provenance and semantic attributes for baselines/candidates.
- Unify indirect-reference handling for Filter, DecodeParms, BitsPerComponent, dimensions and color spaces. Support clearly interpretable LZW/RunLength samples. Restore LZW/Flate predictors according to their parameters and bit depth rather than assuming Predictor 1.
- Avoid rejecting equivalent null/array DecodeParms forms in a later entry point after unwrapping. Never delete unknown or meaningful nondefault parameters merely to enable optimization.
- Generate lossy JPEG candidates once from this run's original samples, and coefficient candidates from the original terminal JPEG. Do not repeatedly quantize already re-encoded output.
- Derive each complete candidate from a fixed source and retain existing complete-file fallback. Moving a stage earlier must preserve the previous compression baseline.

Acceptance: equivalent content with different wrappers/references gets equivalent candidate coverage; pixels, components, coefficients and text/tag semantics meet the relevant promises; complete output does not grow relative to a correct baseline.

[qpdf's filter levels](https://qpdf.readthedocs.io/en/stable/cli.html#option-decode-level) support separating general reversible encodings from terminal image encodings such as JPEG. This project's tests determine its safe scope. [PDF parameter errata](https://pdf-issues.pdfa.org/32000-2-2020/clause07.html) explain why predictor BitsPerComponent must not simply be equated with the same-named image-dictionary key.

### 2. Content-aware image encoding — P1, shared by both modes

Goal: colored text, charts, illustrations and gradients should have more than JPEG/original encoding just because they exceed 256 colors.

- Sample dispersed rows/regions for adjacent-pixel differences, repeated rows and compressibility to schedule exact RGB Flate candidates. Sampling schedules work; it neither changes pixels nor proves whole-image equivalence.
- Try a small number of predictor/Deflate combinations first, expanding only when a complete candidate looks promising. Retain existing predictors without unconditionally performing eight full-image compressions on every photo.
- Compare original encoding, exact Flate, eligible exact Indexed/G4 and existing MozJPEG candidates. Lossless forbids lossy candidates. Fidelity keeps dimensions and existing JPEG parameters; exact candidates must not introduce approximate palettes or grayscale detection.
- Screen by dictionary-plus-payload cost, then compare complete PDFs. A choice affecting sharing cannot be judged from one image's payload alone.

Acceptance: colored text, antialiased charts and gradients above 256 colors can try appropriate exact encodings. Photo processing stays budgeted, with no size/quality regression. Measure gains after implementation.

### 3. Bounded processing for large images and long documents — P1

Goal: extend verified exact algorithms to common high-resolution images without simply raising every memory limit.

- Read raw samples, predict and Deflate row by row, retaining the previous row and necessary history. Use temporary files for large candidate payloads to reduce full-image/document clones.
- Separate per-sample peak memory, total document work and deadlines. Schedule by expected benefit/cost so early pages do not consume all additional work.
- For documents above 64 MiB of encoded payload, study replacing candidate objects only and sequential-write validation. This depends on storage-path validation; initial releases can expand eligibility feature by feature without promising every limit disappears at once.
- Add cancellation boundaries in real tasks. Distinguish noninterruptible FFI calls from soft deadlines. Retain desktop worker timeouts and termination.
- Cache usage relationships/parsing first, invalidating on graph changes. Bound decoded-cache bytes rather than accumulating all page pixels.

Acceptance: valid samples beyond the current 4 MiB per-image and 16 MiB document-work thresholds receive useful candidates. Measure time, peak memory and cancellation delay; lower peak memory does not itself mean a smaller PDF.

### 4. Exact resource sharing and bounded cleanup — P1

Goal: share identical samples/resources with different encodings and avoid repeated decoding.

- Compare normalized raw components, bit depth, dimensions and complete color/Decode/mask semantics. Expand sharing to 16-bit images, special colors and supported chains. RGB appearance alone is insufficient.
- Initially restrict non-image sharing to interpretable leaves such as FontFile2, ICC and ToUnicode. Merge only identical decoded bytes with matching semantic dictionaries, preserving ICC N/Range/Alternate, font-program lengths and mapping meaning.
- Use hashes as indexes and compare full data on a match. Bounded caches avoid repeated decoding. Do not merge different subsets, interactive identities or tagged Forms merely because they look alike.
- After encoding normalization, perform only bounded structural cleanup; never rerun lossy image processing. Select and fall back using complete-file comparisons.

Acceptance: equivalent encodings share safely; inverted Decode, ICC Range, spot-color functions and distinct tagged identities do not merge. Cross-page tests include the final page and shared objects.

### 5. Desktop full-page image quality and consecutive tasks — P1, separate acceptance

Goal: improve small text/charts in actual desktop output, with desktop evidence separate from CLI results.

- At unchanged final dimensions, compare direct 1× rendering against 2× rendering/downsampling; evaluate native Canvas and explicit resampling. Cover fine/colored text, line charts, scanned photos, annotations and transparency.
- Equal JPEG quality numbers do not establish equal quality across pipelines. Compare fine-text regions, edge/color errors and complete-file size. Retain the verified desktop baseline and adopt alternatives only when quality and cost gates pass.
- Fall back to 1× when over budget without changing output dimensions. Generate both candidates from the original page, not a previous lossy PDF.
- Chunk pixel conversion/IPC and reuse memory. Retain scheduler.yield where available and its fallback; assess side effects before choosing workers.
- Review the fixed 120-second document-render limit using long documents. Determine total deadlines, per-page no-progress timeouts and cancellation behavior; unlimited timeouts must not conceal stalls.
- Process multiple documents consecutively through real Tauri IPC, covering cancellation/restart, failure cleanup, Windows WebView2 and macOS WKWebView.

Acceptance: each page still contains one independent image, with fields/annotations handled as promised. Explain same-preset size, detail and time relative to the desktop baseline. Do not claim cross-platform rendering acceptance before Windows device testing.

[PDF.js examples](https://mozilla.github.io/pdf.js/examples/) control page viewport and Canvas output scaling separately. These are fixed-dimension export experiments; screen devicePixelRatio is not an export-sizing rule.

### 6. CMYK/YCCK JPEG coefficient optimization — P1, separate color regression

jpeg_lossless.rs currently accepts 1/3 components. Native CMYK Flate support does not imply CMYK JPEG optimization. Coefficient re-encoding should preserve quantization tables, sampling, dimensions and coefficients plus Adobe APP14, ICC and PDF Decode/ColorTransform semantics. Avoid RGB conversion; simply allowing four components is insufficient. Add multireader tests for normal/inverted CMYK, YCCK, APP14 variants and parameter combinations.

[MozJPEG usage documentation](https://github.com/mozilla/mozjpeg/blob/master/usage.txt) explains jpegtran coefficient-domain transforms. This supports the implementation approach, not a claim that this project's four-component PDF compatibility already passed.

### 7. Automatic font usage and subsetting — P1, most useful for font-heavy files

subset_cid_experiment.rs still requires a manual CID → GID list and is not product functionality. Initially limit support to CIDFontType2, Identity-H and explicit CIDToGIDMap. Track Tf/Tj/TJ, q/Q, Contents arrays and Form resource scopes automatically. Collect actual character codes, CIDs and GIDs; retain composite glyphs, hinting, widths, ToUnicode and original text codes. Update maps when renumbering. Preserve glyphs needed for future input in editable fields/shared fonts; skip unknown CMaps, vertical writing and CFF initially.

Acceptance: multilingual glyphs, extraction/copying, positioning and editable fields remain correct; font programs and complete files shrink. Continue the existing Rust subsetter approach. [HarfBuzz's subset API](https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-subset.h) provides glyph-set/retained-GID reference interfaces, but does not replace PDF usage analysis.

### 8. Vector state/Forms and transparency — P2, separate workstreams

- Vectors: track known graphics state and remove genuinely redundant color/line-width assignments. Skip unknown state. Identical literals do not justify deleting cumulative cm transforms, current paths, clipping or text positioning.
- Forms: attempt single-use inlining/reuse only when call counts, BBox/Matrix/resource scope and current-path safety are provable. Protect transparency groups, tags and appearances separately. Retain the earlier PDF.js pixel-difference counterexample for stroke reuse.
- Transparency: encode image/mask exactly on their original grids first, then separately study paired downsampling for Balanced/Smaller, accounting for premultiplication, Matte, different grids, nonwhite backgrounds and edges. Fidelity retains dimensions. Keep failed examples involving apparently opaque SMask removal.

Each candidate must pass semantic and rendering acceptance independently. Vector reuse and mask processing cannot all be labeled strictly lossless.

## Later experiments, lower priority

- Exact Indexed palette reordering/equivalent bit-depth selection: current indices follow first color occurrence. Study compression under equivalent permutations, retaining the original and never quantizing colors.
- Object-stream grouping/compression: ordering candidates exist, but group sizes/levels mainly use two object-count tiers. Try a few configurations only after proving complete-file gains justify extra writes.
- Benefit-aware Zopfli scheduling: retain strict budgets instead of blindly raising iterations. Favor promising streams so large ineligible streams do not occupy search slots.
- Inline-image externalization: experiment only for repetition with complete-file savings. Retain the counterexample where externalizing one image grows the file.

Existing Brotli/new-JPEG experiments add no new compatibility/quality evidence here, so changing the default encoder is not recommended. Multilayer MRC conflicts with the one-image-per-page contract and is excluded.

## Common acceptance and execution boundaries

1. Compare complete PDFs against a correct 0.1.19 baseline, not just streams. Reject invalid or larger optional candidates.
2. Exact strategies check components/JPEG coefficients/decoded streams and object relationships. Compare pixels and text geometry within each of three readers. Check every page of new multipage samples, plus different zoom/antialiasing settings.
3. Preserve mode keeps text/vector types. Fidelity retains dimensions and quality parameters without claiming whole-file strict losslessness. Low-level Lossless forbids lossy candidates.
4. Image mode stays one image per page. Validate the real desktop production-render path and record CLI/desktop separately.
5. Record size, quality, time, peak memory, budget skips and cancellation latency. Use synthetic coverage for boundaries and full real-file comparisons when originals are available; special samples do not establish general compression ratios.

The next batch should implement only steps 0/1/2: unified protection and multipage validation, earlier decoding, and exact RGB candidates above 256 colors. Steps 3/4 then cover scale/sharing; step 5 separately addresses desktop quality. CMYK, fonts, vectors and masks each need their own results. This document is a plan; the changes above were not implemented by this review.
