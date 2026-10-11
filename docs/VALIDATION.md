# Validation record

This is a historical record of the checks performed for each version. Platform support, repository visibility and outstanding work below describe the corresponding validation date; see the [README](../README.md) for current product behavior. Public document references use anonymous identifiers.

## 0.1.25: signed update checks

Date: 2026-10-11. Release builds check the stable GitHub feed after startup and daily, with a manual header control. Detection does not install automatically: users choose installation/restart, and active file tasks prevent it. Update downloads require signature verification with an authenticated matching app version. PDF processing remains local.

- Local TypeScript/production build, updater tests (10), release preparation/publishing tests (16), i18n tests (five), rendering checks, license tests (11), Rust workspace tests and strict clippy/fmt passed. Notices covered 411 dependencies/assets. Only the updater and release feed use the network; PDFs are not uploaded.
- Native macOS arm64 checks covered manual checking and retryable feed failure. The frontend dialog was separately exercised with a simulated version for themes, German text, task blocking, progress, failure retry and focus restoration. Simulated UI checks are not evidence of a real installed upgrade.
- [Three-platform CI](https://github.com/SSToo/Squeeze-PDF/actions/runs/38138512707) passed for Windows x64 and macOS arm64/Intel, including tests and signed installer/update generation.
- [Release preparation rehearsal](https://github.com/SSToo/Squeeze-PDF/actions/runs/38140288073) passed against those actual artifacts. It verified all three update signatures and their bound version, prepared three installers, two macOS update archives, three signatures, `latest.json` and `SHA256SUMS.txt`, and uploaded the ten files as `release-preview`. This did not publish a GitHub Release.
- Local macOS packaging and strict ad-hoc signature checks passed. DMG: 20,013,390 bytes, SHA-256 `ba0c28bce7db381e4875647550a367c9bdff0fdfa373e6f1f391627c4ca7bd33`. Update archive: 19,086,704 bytes, SHA-256 `c85bfac17ed26dd1a2508b9415198b7c90be0330316ce870d856083de89f9840`. Its version-bound signature verified against the configured public key.
- No 0.1.25 release tag was published during these checks. A live cross-version installation/relaunch was not tested because a stable update manifest was not yet published. Versions 0.1.24 and earlier require one manual installation of an updater-capable release. Windows/Intel device installation acceptance and production OS code signing/notarization remain separate from successful CI and updater signing.

## 0.1.24: transparent logo refresh

Date: 2026-10-11, macOS arm64. Replaced the transparent PNG, regenerated app icons/favicon, and checked light/dark rendering and the production build. Bundled icons, versions, license materials, strict ad-hoc signatures and read-only-mounted file hashes passed. DMG SHA-256: `99ba2cc196984357412227d130f3df60c81683d644eda4d00c72482674786aaa`. Engine code/dependency versions were unchanged; engine regressions and real desktop tasks were not rerun. New Windows/Intel packages and production signing/notarization were not accepted in this round. See the [logo record](LOGO-0.1.24.md).

## 0.1.23: Apache-2.0 and logo colors

Date: 2026-10-11, macOS arm64. Original code adopted Apache-2.0 at the user's choice, with bundled project license/notices. The supplied red logo generated app icons, with corresponding light/dark accents. See the [open-source record](OPEN-SOURCE.md).

- Ten license regressions, TypeScript, five eight-language regressions, frontend PDF.js rendering/form-and-annotation preparation, Rust formatting and relevant script syntax checks passed.
- Local frontend checks covered both modes/themes and Chinese/German at normal/minimum desktop dimensions. The logo loaded without horizontal overflow. Browser checks did not replace native Tauri compression acceptance.
- App/worker strict ad-hoc signatures, DMG verification and mounted license/source/icon/executable hashes passed. The bundled worker's 38 combinations were byte-identical to the previous accepted version, with unchanged inputs. No engine/dependency changes; 141 Rust tests and all 488 combinations were not rerun.
- DMG SHA-256: `ddb958178af4381e7059ebb31f50c287d2ee42cb4648138ab71b2724525516c1`. New Windows/Intel packages, consecutive real desktop tasks and production signing/notarization remained outstanding. GitHub visibility was Private at the time; licensing/branding were committed with 0.1.24.

## 0.1.22: licenses and source distribution

Date: 2026-10-10, macOS arm64. Completed Liberation upstream/actual editable font sources, rebuild instructions and QuickJS copyright/license notices; added integrity checks for 71 source files across five MPL dependencies. Materials/instructions are bundled, with license checks in Windows/macOS CI. See the [licensing record](LICENSING-0.1.22.md).

- Seven license regressions, TypeScript, relevant script formatting/syntax and diff checks passed. Four fonts rebuilt from bundled sources, with matching glyph order, outlines, widths, hints and 18 table semantics per font. Byte-identical rebuilds are not promised; production font bytes stayed unchanged.
- Actually mounted the 0.1.22 DMG and verified licenses, archives, instructions and MPL files. Both executable hashes matched the App. Strict ad-hoc signatures and DMG verification passed. DMG SHA-256: `e05314b96046ea1d73bfaf94fdcfb6969f7e2478b11de4583b242d87948ff0b7`.
- The bundled worker's 38 combinations across eight sample classes matched accepted previous outputs byte for byte; source hashes were unchanged. With no engine/dependency changes, 141 Rust tests and all 488 combinations were not rerun. New Windows/Intel packages, production signing/notarization and full consecutive-desktop-task acceptance remained outstanding.

## 0.1.21: large images, resources, CMYK, fonts and desktop candidates

Date: 2026-10-10, macOS arm64. Implemented review steps 3–8. **Implementation was completed, but full consecutive real desktop-task acceptance was not:** the locked Mac paused WKWebView callbacks, and Windows/Intel device tests were outstanding.

- Frozen final 0.1.20 baseline: 80 classes / 488 complete-file pairs; 63 smaller, 425 unchanged, no size regression. All new gains were preserve mode, including 14 Fidelity pairs. The existing 396 new outputs stayed byte-identical and inherited previous validation. Three independent readers checked 104 page/preset combinations across 92 additional results.
- All 26 additional strict-lossless page combinations matched source pixels in three readers; Fidelity error against source did not increase. Six transparent-image Balanced/Smaller combinations explicitly use lossy downsampling. FontTools independently checked 264 glyphs, 24 composites, hints, widths and ToUnicode.
- Supersampling requires smaller complete files and whole-page/edge error gates against two fixed references. An actual slightly smaller candidate with worse fine-text error was rejected. Chunking, deadlines, file budgets and cancellation have separate regressions.
- Final full-workspace 141 Rust tests, strict clippy/fmt, TypeScript, eight-language and frontend rendering checks passed. The packaged worker reran all 488 combinations byte-identically to accepted outputs. DMG executables matched the App. The 0.1.21 DMG hash is recorded in the [licensing record](LICENSING-0.1.22.md). No passing real Tauri consecutive-task/cancellation-retry record exists; QA adapters/unit tests cannot replace it. After unlocking, rerun `node scripts/desktop-raster-native.mjs tmp/round9-remaining/native-retry`.

## 0.1.20: earlier image inputs, exact RGB and unified identity protection

Date: 2026-10-09, macOS arm64. Implemented 0.1.19 review steps 0/1/2: restricted image-representation normalization, content probes/exact RGB Flate above 256 colors and shared identity protection. Both modes compare complete files; Fidelity resolution, JPEG/DPI settings and vector-error budgets remain unchanged.

- 57 sample classes / 396 complete-file pairs: 49 smaller (44 preserve, five CLI image), all others unchanged. All 396 new outputs were executed; frozen baselines include previously executed, hash-checked records. These gains are not extrapolated to every PDF/photo.
- PDF.js checked every one of 1,152 page/preset combinations independently. Text positions and strict-lossless pixels passed without warnings. The production render function in native WKWebView additionally passed 24 cases / 51 pages: five smaller, 19 complete-byte-identical. Changed image-page decoded samples matched actual staged input; other page encodings matched.
- Poppler/CoreGraphics each checked all 1,152 pages. Their 165 strict-lossless combinations matched source pixels; new Fidelity RMSE did not exceed baseline. Ten Balanced/Smaller combinations gained selected-preset lossy changes by unlocking previously untreated wrappers. Five changed CLI image pages retained every fixed pre-encoding RGB sample, though source RMSE could still be higher in four Poppler/one CoreGraphics combinations. No universal reader/perceptual quality improvement was claimed.
- Sixteen independent counterexample checks confirmed MCR/Stm and distinct form AP identities. Correctness fixes were not counted against smaller but incorrectly shared old outputs.
- Final workspace: 121 distinct Rust tests, strict clippy/fmt, TypeScript, eight-language and frontend rendering/form-and-annotation preparation passed. The final bundled worker reran all 396 combinations byte-identically, with unchanged originals.
- macOS arm64 App/worker 0.1.20, strict ad-hoc signatures and DMG verification passed. DMG SHA-256: `5617c46ff5594096f0f5cb9f033d19081a12db60e1591159d5c5dfba54a73cbe`. Windows/Intel devices and production signing/notarization were untested. Original Documents A/B were unavailable on the desktop; no new Acrobat comparison was made.

## 0.1.19: chains, short content streams and complete fallback

Added strict reversible filter chains, text-wrapper removal for images, private short Contents candidates, tag /Stm protection and complete-file fallback without changing quality parameters. Across 47 sample classes / 326 complete-file pairs, 32 became smaller and all others stayed unchanged. Three-reader before/after pixel/text comparisons passed. An independent 18-case production-render baseline passed in native WKWebView; real consecutive UI tasks and Windows WebView2 were untested. Coverage included 110 distinct Rust regressions. The macOS arm64 0.1.19 package was built/verified, including bundled-byte checks.

## 0.1.17: removing the default file-size cap

Date: 2026-10-08, Windows 11 x64.

- The engine no longer limits PDF bytes by default. The shared desktop preview/PDF.js read entry also removed its 256 MiB cap; import messages were updated in eight languages. API callers may still set explicit caps.
- Added a 257 MiB one-page regression reproducing the old “file cannot exceed 256 MB” failure. Preflight, preserve and CLI image mode passed after the fix. PDF whitespace pads the boundary sample; its ratio/performance does not represent real large documents.
- 97 Rust tests, TypeScript, eight-language, PDF.js rendering and Rust formatting passed. The installed 0.1.17 worker exported both modes from the 269,484,486-byte sample without changing source SHA-256.
- Updated/installed Windows x64 NSIS; App/worker were 0.1.17. The desktop recognized a real 314.94 MiB, 63-page PDF; this did not establish completed compression or page-by-page visual validation of that file.

## 0.1.16: raw color samples and file layout

Added restricted raw/single-Flate component encoding, explicit default Decode recognition and complete-file numbering/layout candidates. Strict Lossless gained exact images. Fidelity dimensions, JPEG/DPI/vector budgets stayed unchanged; protected appearance-shared images retained their encoding.

- 30 main sample classes × seven mode/preset combinations = 210 pairs; 67 smaller, none larger. Seven extra DefaultRGB → ICCBased nondefault-Range pairs brought totals to 31 classes/217 pairs. Inputs were unchanged. 119 older main-corpus records reused verified baselines rather than being rerun here.
- Poppler/CoreGraphics each checked 217 new results. Main-corpus 60 Fidelity/Lossless outputs matched source pixels; 90 image outputs matched baseline pixels; extra default-color checks passed. PDF.js independently checked pixels/text/positions for the 60 main results and two extra results. Order/boxes/one-image-per-page passed. An initially incorrect synthetic Matte input size was corrected, old data isolated and tests fully rerun.
- All 95 workspace Rust regressions passed. After one final default-color end-to-end test, 31 core unit/four native-image tests passed, covering 96 distinct tests in total. Workspace strict clippy/fmt, TypeScript, eight-language and PDF.js rendering/form-and-annotation preparation passed.
- Each preset of two real files saved 23,500 B / 102,103 B. All six outputs' reachable graphs/original encoded streams were independently checked; every page box/crop/rotation matched. Three readers compared ten pages: first/middle/last of both Fidelity results and first pages of Balanced/Smaller. This is not every real page/every PDF, and there was no new Acrobat comparison.
- Final macOS arm64 bundled worker produced byte-identical 210 main-corpus results and two real Fidelity results; the seven extra default-color pairs used that worker directly. App/worker 0.1.16, strict ad-hoc signatures and DMG verification passed. SHA-256: `1491e3412e730a0c34b97decb8e6372f19d99ec3c81d175240c07299256aa0fd`. Native UI operations, Windows/Intel devices and production signing/notarization were untested.

## 0.1.15: general resources and special image inputs

Restricted cleanup across six resource tables, equivalent subgraphs, Indexed/G4/CMYK inputs and bounded Zopfli entered defaults. Future appearance-font glyphs and original image encoding remained protected. Fidelity dimensions, JPEG/DPI and vector budgets stayed unchanged.

- 17 synthetic categories, 119 two-mode/preset combinations: 238 old/new runs succeeded without complete-file regression. The final bundled worker reran 119 combinations; 115 matched development bytes. Four preserve outputs affected by default-color protection were rendered again.
- Poppler/CoreGraphics each checked 119 new outputs: 34 Fidelity/Lossless pages matched source and 51 image pages matched baseline. PDF.js checked 34 pages' pixels/text/positions. CoreGraphics was explicitly rerun at 200 DPI; earlier fit-to-canvas records do not prove actual 200 DPI quality.
- 88 distinct Rust tests, strict workspace clippy/fmt, TypeScript, eight-language, frontend rendering and form/annotation preparation passed.
- Both real PDFs had no regressions across three presets: A saved nine bytes per preset; B 58,473. Eleven important Fidelity pages matched in PDF.js; both packaged Fidelity outputs matched development bytes and source hashes stayed unchanged.
- Final macOS arm64 0.1.15 App/worker signatures and DMG verification passed. SHA-256: `bd90a0b0f791cc493c10d09b8b387ae0f22a53d741b5602480026ecb621da236`. Native clicks, Windows/Intel devices and production signing/notarization were untested.

## 0.1.14: fourth-round format experiments and CLI rendering protection

Matched Brotli variants of existing A/B Fidelity results saved another 2.75% / 0.69%, but failed Poppler, CoreGraphics and CLI native rendering. Product Brotli output was not enabled. All 113 pages matched pixels/extracted text in PDF.js; JPEG/G4 controls passed.

- hayro reported success but rendered a blank Brotli-font page. A new native preflight rejects it. Page/nested-Form × single-name/array combinations verified unchanged originals/no output; desktop geometry planning still accepts these inputs. Eleven raster regressions and strict workspace clippy/fmt passed.
- Using the actual PDF.js Brotli-font pixel manifest, the final worker produced an independent 1667 × 2223 single-image page; pypdf/Pillow confirmed identical, nonblank RGB components.
- macOS arm64 App/DMG build, disk verification and strict ad-hoc signatures passed; worker 0.1.14. DMG SHA-256: `e625dc123e2a037be6c12b45b7398d9c4d445844a2d89e2dc28555e72dd44c6b`. The Mac was locked; native interactions, Windows/Intel devices and production signing/notarization were untested.
- Standard presets/third-round strategies did not change. Raw records: ignored `tmp/round4-brotli/`.
- The final worker exported both originals in Fidelity with complete bytes identical to round three: A 6,966,388 bytes/~23 seconds; B 40,138,199/~41 seconds. Originals stayed unchanged. Brotli-font input was explicitly rejected without a file; JPEG/G4 controls still passed.

## 0.1.13: third-round resources and monochrome scans

Per-image usage, appearance protection, owner-resource cleanup, exact-sample sharing and G4 became defaults. Font subsets/JPEG2000 remained development experiments.

- Six real results across two files/three presets matched 0.1.12 bytes; eleven important PDF.js pages matched, with unchanged originals. Four fixed encoding samples completed 48 old/new two-mode/three-preset runs and Poppler checks.
- Four resource classes/24 comparisons preserved selected-preset ordinary-page optimization, shared appearance images, fields, annotations and attachments. Strict-Lossless sharing sample: 6,580,782 → 820,631 bytes. Monochrome Fidelity: 86,334 → 77,974, matching source pixels in Poppler/PDF.js and previous 1-bit output in CoreGraphics.
- 77 distinct workspace Rust tests (81 executions including example duplicates) passed. After final identity fixes, six resource regressions and strict workspace clippy passed again; fmt, TypeScript, eight-language and rendering checks passed.
- macOS arm64 App/DMG/signature checks passed, bundled worker 0.1.13. DMG SHA-256: `83058b6eef0a8cda968aafe65668313432bdaba87b447562d619307e86456178`. Locked Mac prevented native interaction checks; Windows/Intel devices and production signing/notarization were untested.

## 0.1.12: second-round Form precision and JPEG experiments

Form precision uses cumulative transforms across every known call. Bounded JPEG caching retained existing encoding settings/output.

- Six real-file/preset comparisons: A saved 17,108 bytes per preset; B 10,937,031. Image bytes/dimensions/color semantics stayed at 0.1.11; gains came from Form drawing/further outline reuse.
- All 109 Fidelity pages rendered with Poppler; eleven also checked in PDF.js at 200 DPI. Local text/lines looked correct. Geometry budgets, unknown calls, cycles, budgets and cancellation passed.
- 48 fixed-sample compressions/24 pairs matched 0.1.11 complete bytes in both modes/all presets. 69 distinct Rust regressions (73 executions with duplicates) passed. After safety fixes, 20 core tests and strict workspace clippy passed again.
- None of 384 JPEG/Jpegli candidates was smaller while passing all local-error gates; MozJPEG remained default. Neither real file had cache hits, so no caching speedup was claimed.
- Native desktop Fidelity batch processed both originals byte-identically to CLI; side-by-side preview worked. Final strict App/worker signatures and DMG verification passed. DMG SHA-256: `7199379c3560e8cd9e228337e49184989b6ea04ade86558454ccf3a72502aa3c`. Windows/macOS Intel devices and production notarization remained untested.
- Raw records: ignored `tmp/round2-0.1.12/`.

## 0.1.11: first-round image encoding and size diagnostics

DPI, JPEG quality and vectors stayed at 0.1.10; Fidelity retained image dimensions.

- Fixed old/new workers and four synthetic samples completed 48 runs (two modes × three presets × two versions) without input changes; no new result among 24 pairs grew. Poppler passed; four preserve Fidelity outputs matched source pixels; image outputs contained one image per page.
- Six real comparisons: A saved 11,102 bytes per preset, B 21,079, limited gains. Color-image bytes and all non-image business objects/drawing remained unchanged. Decoded mask samples matched; Fidelity dimensions matched originals; both original hashes stayed unchanged.
- All 109 Fidelity pages rendered at 100 DPI in Poppler and matched 0.1.10 Fidelity pixels. Existing original-to-Fidelity lossy differences remained; this does not imply source losslessness.
- 63 full-workspace Rust regressions passed; affected 48 plus one mask regression ran after solid-mask/buffered-writing changes, covering 64 distinct tests. Coefficients/quantization/sampling/metadata/pixels, 1-/2-/4-/8-bit palettes/odd widths, five predictors, creation races, cancellation, ICC, interactivity and attachments passed. Strict clippy/fmt, TypeScript, five multilingual tests and PDF.js rendering passed.
- Final macOS arm64 app imported both desktop originals and completed Fidelity. Downloads `_compressed_4.pdf` outputs matched baseline bytes. Document B's PDF.js original/result preview and quality check passed. UI retained Chinese/light/preserve/Fidelity/Downloads.
- Final buffered build reprocessed B byte-identically, reducing solid-mask attempts/write syscalls. Concurrent build/render load affected timings, so no speed multiplier was claimed.
- App/DMG build, disk verification and strict signatures passed; bundled worker 0.1.11. DMG SHA-256: `a93950dc02834f7d5e6cf1d9d25df8910b2247b45513bbb33dcd5cc13cee73c6`. Windows/Intel devices and production signing/notarization were untested.
- Reports, eight container experiments, sampled diagnostics and renders: ignored `tmp/round1-0.1.11/`, not distributed.

## 0.1.10: Fidelity vector optimization at original image resolution

Date: 2026-10-06, macOS arm64. At the user's request, desktop preserve Fidelity enabled path precision/outline reuse. Supported images retained dimensions and used MozJPEG 95/full chroma; original encoding wins when smaller. Transparency masks remain lossless. Fidelity no longer means strict whole-PDF losslessness; eight-language explanations were updated.

- Added engine `fidelity`, rejecting downsampling, target DPI and image-mode combinations. Desktop retains its settings ID but maps to this preset and both vector options. CLI/API `lossless` retained original image encoding and skipped these vector options at this point. Image High quality stayed unchanged.
- Initially enabling vector reuse alone, with original image encoding/precision, left A at 69,284,487 bytes and B at 172,513,546, dominated by images. Original-resolution high-quality encoding and existing restricted Balanced path precision produced the results below.

Original SHA-256 hashes matched previous tests and stayed read-only. The app calculates MiB labeled MB; exact bytes are included.

| Original   | Original bytes | 0.1.9 Fidelity | 0.1.10 Fidelity | Old/new app display | Reduction vs original |
| ---------- | -------------: | -------------: | --------------: | ------------------: | --------------------: |
| Document A |     81,313,254 |     69,284,487 |       6,994,598 |     66.07 / 6.67 MB |                91.40% |
| Document B |    215,135,570 |    199,008,180 |      51,096,309 |   189.79 / 48.73 MB |                76.25% |

- Independently parsed A's 3,321 and B's 25,792 business objects. All non-image dictionaries/drawing matched verified 0.1.9 Balanced results: 1,731 / 24,308 non-image streams. Text, fonts, geometry and vectors remained, with 88,544 / 119,332 reused outline groups and no full-page rasterization.
- Every image/mask retained source Width/Height, color space and bit depth. Some originals were smaller than JPEG 95 candidates and retained original bytes; encoding need not match Balanced.
- All 109 pages rendered in Poppler at 100 DPI. Additional source comparisons: A pages 1/8/52 and B 3/8/54. All six RGB RMSE values were slightly below Balanced. Sampling does not rank human quality for all documents. Text, charts, photos and backgrounds looked correct.
- 59 Rust tests passed, adding Fidelity paths/reuse/original-resolution regressions with RGB/JPEG, selectable text and unchanged originals. After final strict-lossless compatibility adjustment, five outline tests passed again; workspace clippy/fmt, TypeScript, five eight-language checks and formatting passed.
- Actual Chinese/light desktop preserve/Fidelity processed both files to `Downloads/document-a_compressed_3.pdf` and `Downloads/document-b_compressed_3.pdf`, byte-identical to independent worker outputs. Existing files remained intact and preview worked. Spanish Fidelity layout was checked, then Chinese restored.
- macOS arm64 0.1.10 App/DMG, disk verification and strict signatures passed; bundled worker 0.1.10, checksum beside the package. Windows/Intel installation regressions remained outstanding.
- Evidence: ignored `tmp/fidelity-0.1.10/`. Original dimensions, JPEG 95 and bounded coordinate precision are high-fidelity strategies, not strict pixel/geometry losslessness or universal sample-level ratios.

## 0.1.9: Document A regression and stable resource order

Date: 2026-10-06, macOS arm64. Desktop `document-a.pdf`: 81,313,254 bytes, 52 pages; same original hash as 0.1.4.

- Investigated reported growth using the same original/preset/path options with retained 0.1.6/0.1.8 workers. Balanced: 4,136,151 / 4,136,352 bytes; Smaller: 3,034,535 / 3,034,595. Reproduced only 201/60 bytes growth. Both reused 88,544 outlines without additional nested-Form rewrites.
- Each preset's 3,321 business objects/1,764 streams, including all 33 images/masks, matched independently. Differences were nested XObject dictionary entry order on 51 pages. Object-container encoded size 177,684 / 177,885 explains the 201-byte Balanced difference. An existing 4,136,404-byte Downloads preserve result was equivalent; the 9,813,647-byte image result is a different mode.
- A randomized shared-outline hash cache previously determined insertion order. New resources now insert by object number without changing names/references/drawing/streams. This removes that serialization randomness, not a promise that every future algorithm/preset shrinks files.

| Preserve preset | 0.1.6 baseline | 0.1.8 rerun |     0.1.9 | App display | Reduction vs original |
| --------------- | -------------: | ----------: | --------: | ----------: | --------------------: |
| Balanced        |      4,136,151 |   4,136,352 | 4,133,276 |     3.94 MB |                94.92% |
| Smaller         |      3,034,535 |   3,034,595 | 3,031,530 |     2.89 MB |                96.27% |

- Two independent 0.1.9 Balanced exports matched bytes: SHA-256 `6d7b7ef6ca8a07300e5be769c11b8f595ebd890f359949d469cf041bd94bc414`. Both presets retained every 0.1.8 business object, dictionary value and encoded stream. Stability tests use eight holed closed outlines, name conflicts, shared pages and ordinary links; repeated complete-file comparisons fail without the fix and pass with it.
- B preserve/Balanced stayed 34,577,527 bytes / 32.98 MB. All 25,792 objects and 24,343 encoded streams matched 0.1.8, with unchanged source. Resource order changed in 55 Forms, but those dictionaries were outside compressed object containers, so total size stayed identical.
- 58 Rust tests, workspace clippy/fmt and TypeScript passed. macOS App/DMG, disk verification and strict signatures passed; worker 0.1.9, checksum recorded. Image mode, JPEG parameters and geometry algorithms were unchanged.
- Actual Chinese/light desktop preserve/Balanced exported `Downloads/document-a_compressed_2.pdf`, 4,133,276 bytes, identical to both worker runs. First-page preview worked, outputs were not overwritten, settings/preview retained.
- Evidence: ignored `tmp/document-a-regression-0.1.9/`. The user's allegedly much larger output path/size remained unavailable; these findings explain the small reproduced differences only, not a larger unexplained increase.

## 0.1.8: outline reuse in nested vector Forms

Date: 2026-10-06, macOS arm64. Read the specified `document-b.pdf` locally, keeping the original read-only.

### Bottleneck and changes

- Original: 215,135,570 bytes, 57 pages, 64 images, no interactive content. The 0.1.7 Balanced result was 61,072,161 bytes, matching the existing Downloads output: ~15.31 MB images and ~45.22 MB Form drawing. Old reuse covered independent page streams, while most outlined text was in nested Forms.
- Extended reuse to Forms with their own resources. Retained BBox, Matrix, transparency Group, optional content and structural metadata, adding only child Forms for solid closed outlines. Exact decimal fixed-point translations, grouped intersecting control ranges, holes and fill rules remain.
- Added `form_usage` for page/Form calls, color-space changes and q/Q. Inherited-color paths reuse only when every known caller uses solid-color spaces. Appearances, masks, Type3, patterns, legacy Forms without resources, unknown/over-budget calls remain conservative; explicit solid-color assignments may enable processing. Gradients/clipping retain coordinates.
- New resource names avoid every page/original-Form XObject name; dictionaries are copied independently. Adoption includes encoding/object/resource overhead. Up to two extra passes reconsider streams initially lacking savings using later shared-cache entries; successful streams are not rewritten repeatedly.
- Image settings remain 0.1.7 Balanced: JPEG 85, original dimensions, ICC and lossless masks. This file still is not downsampled.

### Same-file measurements

The app computes MiB labeled MB; exact bytes distinguish this from decimal MB.

| Result                  |       Bytes | App display | Reduction vs original |
| ----------------------- | ----------: | ----------: | --------------------: |
| Original                | 215,135,570 |   205.17 MB |                     — |
| 0.1.7 preserve/Balanced |  61,072,161 |    58.24 MB |                71.61% |
| 0.1.8 preserve/Balanced |  34,577,527 |    32.98 MB |                83.93% |

Added 119,332 reused drawings, mainly across 55 original Forms. No full-page rasterization or further image-quality reduction produced this difference. The user reported Acrobat at about 33M, but neither that output nor its settings was available; equal quality was not established.

### Validation

- 58 Rust tests passed. New coverage: nested groups/masks, original Matrix/BBox, caller Contents arrays, name conflicts, exact control points/holes, no reuse under inherited pattern spaces, explicit solid-color reuse, appearance/Type3/mask/budget/cancellation calls and early streams benefiting from later cache entries.
- Independently checked rewritten fills in 50 + five real Forms; expanded decimal coordinates matched exactly. Page content/dimensions/text extraction stayed unchanged. All 35 image/mask/ICC streams retained 0.1.7 Balanced encoded bytes.
- Initial/final outputs rendered all 57 pages in Poppler at 100 DPI. Final vs older Balanced median RGB RMSE ~0.351, maximum ~0.791. PDFium checked pages 3/8/54 at 100/300 DPI, RMSE ~0.019–0.027. Text, photos, transparency and gradients were readable. Four group combinations, masks, skew transforms, thin strokes and holes passed Poppler/PDFium at 100/300 DPI. Renderer rounding/antialiasing still causes pixel differences; lossless quality was not promised.
- TypeScript, workspace clippy/fmt, real PDF.js render-failure/form regressions and eight-language checks passed. macOS App/DMG, disk verification and App/worker ad-hoc signatures passed; worker 0.1.8. Windows/Intel installation regression remained outstanding.
- Actual Chinese/light desktop preserve/Balanced with Downloads exported the same original, displaying 32.98 MB / 83.9% reduction. Automatic numbering preserved older outputs; 25,792 parsed business objects/dictionaries/streams matched worker results. Preview worked and settings were retained.
- Original checksum stayed unchanged. Evidence/scripts: ignored `tmp/document-b-0.1.8/`, not distributed.

## 0.1.7: ordinary links and image-mode compatibility

Date: 2026-10-06, macOS arm64. Read specified `document-c.pdf` locally without uploading.

### Fixes

- Original: 240,855,270 bytes, 54 pages, 12 ordinary Link annotations, no forms/attachments. Treating every nonempty Annots array as fidelity-sensitive disabled preserve image/vectors and rejected image mode.
- Separated interactive-content presence from fidelity protection. Ordinary links without appearance streams permit image/path/outline optimization while retaining objects, targets and positions. Forms, links with appearances, other annotations and attachments retain conservative protection.
- Desktop PDF.js ENABLE_STORAGE draws visible forms/annotations and generates missing text-field appearances from current values. Hidden fields are not exposed; visible content is flattened into pixels and annotation popup text omitted. Original preview uses the same preparation/render settings.
- Rust records annotation references; frontend checks PDF.js results and verifies drawable annotations entered operators. Undrawable note/attachment icons and damaged annotations report page numbers and block incomplete output. Image/font decode checks remain.
- Rebuilt attachment name trees retain data, descriptions and document/page associations. CLI hayro still cannot draw form/annotation appearances and directs these cases to desktop; ordinary links/empty form containers work.
- Updated mode descriptions, attachment reporting and errors in all eight languages.

### Same-file results

Bytes below. Balanced preserve/image ran in the actual macOS release app; Smaller preserve used the same-version worker.

| Mode/preset                 | Output bytes | Reduction vs original |
| --------------------------- | -----------: | --------------------: |
| Old 0.1.6 preserve/Balanced |  227,972,661 |                 5.35% |
| New preserve/Balanced       |   61,138,039 |                74.62% |
| New preserve/Smaller        |   54,537,766 |                77.36% |
| New desktop image/Balanced  |   10,874,253 |                95.49% |

- Desktop image flow took ~44.9 seconds: 54 pages, each one 1400 × 1811 image, no fonts/selectable text/annotation objects. Preserve worker Balanced ~55.1 seconds, Smaller ~30.6; timings depend on device/load.
- Both preserve outputs retained all 54 page geometries/drawing operations and all 12 link targets/attributes. Link coordinates inherited lopdf's single-precision read/write error: maximum ~0.000017 points versus original, matching old rewriting. Thirty-one source masks deduplicated to three, with every distinct mask decoded exactly.
- Desktop/worker Balanced business objects/dictionaries/decoded streams were equivalent. Complex usage was incompletely analyzed, so downsampling remained disabled; only supported color images re-encoded. Mask/pattern/font protection was not weakened.
- All 54 pages of both preserve and desktop image results rendered in Poppler at 100 DPI without errors. Source comparisons on pages 1/8/50/54: preserve Balanced RGB RMSE ~0.45–0.50, Smaller ~0.33–0.55, image ~21.83–44.44. These metrics do not directly rank human quality. Text, charts and reference-link pages looked correct.
- Original checksum unchanged. Evidence: ignored `tmp/real-0.1.7/`.

### Regressions and packaging

- 52 Rust tests passed, including ordinary links with image/vector optimization, CLI links/empty forms, desktop attachment trees/page associations/data, cyclic-tree no-output behavior, and existing cancellation/input-change/corruption/file protections.
- Real PDF.js pixel tests covered existing/generated field appearances, checkbox state, visible nonprinting highlights, independent annotation canvases, hidden fields, missing parse results and damaged-appearance rejection. Missing-image/font-cache regressions passed.
- Actual desktop batch: a form/highlight/Chinese-named-attachment sample exported successfully, with identical attachment bytes and no AcroForm/page Annots. HTML note-icon and damaged-annotation negatives reported pages; counts were one complete/two failed, without outputs or leftover pixel directories. Poppler/manual checks confirmed values, checks, highlights and hidden regions.
- TypeScript, Prettier, fmt, workspace clippy and eight-language passed. macOS App/DMG and disk verification passed. Original language/light/image Balanced settings were retained, Downloads restored and queue cleared. Windows/Intel device checks remained outstanding.

## 0.1.6: simpler main interface

Date: 2026-10-06, macOS arm64.

- Removed the top introduction (headline, description/privacy callout) and promotional footer in all eight languages; settings/files now follow the header directly.
- Removed related CSS and empty-message reserved height. Retained the live announcement region, adding spacing only when needed.
- TypeScript passed. Actual page checks confirmed removed nodes, 32 px workspace/header spacing, zero empty-message height and no button/preset text overflow.
- Checked 900 × 650, German image mode and a nonempty message. Actual release 0.1.6 opened directly on settings/files, retaining Chinese/light/Balanced/Downloads.
- macOS App/DMG, disk verification and ad-hoc signatures passed; worker 0.1.6, package-side `SHA256SUMS.txt` updated.

## 0.1.5: eight interface languages

Date: 2026-10-06, macOS arm64.

- Added English, Simplified Chinese, French, Spanish, Korean, Japanese, German and Portuguese through the native header menu. Initial supported system-language matching, otherwise English; later choices persist.
- 273 messages cover UI, status, stages, preview, descriptions and known app/engine errors. Existing tasks/reports update immediately. Filenames/paths/unknown low-level errors receive no substring translation. Compression algorithms/parameters stayed unchanged.
- Five `npm run test:i18n` tests passed for messages/placeholders, locales/fallback, dynamic/nested errors, path preservation and number/byte/percentage formatting; added to native CI.
- TypeScript, Prettier and real PDF.js render-failure regressions passed. Six existing real-file reports had no leftover Chinese descriptions in non-Chinese/Japanese languages.
- Actual frontend checks switched all eight languages/both modes at 800 px/normal widths without text overflow. French dark theme was readable; reload retained language.
- Dark theme became neutral charcoal across illustration, borders, selection and buttons. Language icon sits immediately right of theme switching, with no collapsed text and the native eight-language popup. Final release keyboard space/arrows/Enter and clicks selected languages.
- Native app imported a Chinese-named PDF, completed both modes and displayed previews. Preserve output/report switched to French immediately without altering filenames. Corrupt-JPEG image conversion failed correctly with localized reason/counts and no incomplete export.
- Restart confirmed language persistence. Final App/DMG, disk verification and ad-hoc signatures passed; worker 0.1.5, checksum in `SHA256SUMS.txt`.

## 0.1.4: repeated vector-outline reuse

Date: 2026-10-06, macOS arm64. Local source H only, read-only. No same-input Acrobat result was available, so no quality/size ranking was made.

### Implementation

H contains many repeated outlined glyphs, previously written as full curves each time. Rust `reusable_paths` identifies exactly translation-equivalent closed solid-fill paths and stores shared vector Forms, restoring positions with exact decimal fixed-point coordinates. Image quality/DPI remain 0.1.3; gains primarily reflect vector reuse.

- Touching/overlapping control-point bounds remain grouped, preserving inner/outer contours, holes and f/f* rules.
- Skip patterns/gradients, clipping, open contours, paths inside text objects, interactive documents, Contents arrays and page streams shared for other uses; existing Forms/glyphs are not rewritten.
- Copy inherited resources per page, preserve source XObject names and avoid new-name collisions. Rewrite shared page streams independently. Include Form/object/resource overhead and prune unused old streams only with positive savings.
- Bound scans, intersections, objects, cache and lengths; worker cancellation remains. Fidelity skipped this then; desktop Balanced/Smaller enabled it and CLI required `--reuse-paths`.
- Isolated thin rectangles triggered different renderer thin-line behavior. Final Forms include zero-area closed segments, adding no filled area but avoiding changed thin-stroke weight. Gradients retain their original coordinate system.

### Size

H: 81,313,254 bytes, 52 pages. Same preserve presets; bytes below.

| Preset   |      0.1.3 |     0.1.4 | Additional reduction |
| -------- | ---------: | --------: | -------------------: |
| Smaller  | 35,418,667 | 3,034,553 |               91.43% |
| Balanced | 36,520,413 | 4,136,053 |               88.67% |

Shared 88,544 drawings; reduction versus original ~96.27% Smaller/~94.91% Balanced. Benefits depend on repeated outlines, not a universal PDF ratio.

### Independent checks

- All 52 pages of both outputs rendered in Poppler at 100 DPI without errors/warnings. Median RGB RMSE vs corresponding 0.1.3 ~0.243, maximum ~0.795; pixel identity was not promised.
- All 124,385 closed fills across 52 pages matched expanded exact decimal page coordinates, excluding zero-area helpers. Parsed font resources matched; decoded-byte sets for six masks, one ICC and 325 Type3 glyph streams matched. Text extraction on pages 1/8/52 stayed unchanged.
- Synthetic transparent/holed/thin-stroke samples rendered independently: Poppler 100/300 DPI RMSE ~0.753/0; PDFium ~0.021/0.009. Vectors remained vectors.
- PDFium page 8 at 100/300 DPI RMSE ~0.019/0.044. Manual pages 1/8/52 showed correct colors/order/layout/glyphs.
- H/W source hashes unchanged. Evidence: ignored `tmp/real-0.1.4/`.

### Build and desktop

- Final 48 Rust tests passed: nine unit, 24 preserve, one path precision, nine default image, four desktop-pixel and one outline reuse. fmt, locked clippy, frontend types/formatting and real PDF.js RGB/corrupt-JPEG/font-cache regressions passed.
- Quit old app, opened release 0.1.4 and imported H through native selection. Desktop Smaller 3,034,544 bytes, Balanced 4,136,138, displaying 96.3%/94.9% savings. Dictionary ordering caused small CLI serialization differences; parsed business objects/dictionaries/streams matched, without requiring object/xref-container bytes to match.
- Page-8 original/Balanced preview worked. Downloads restored; preview retained. No user file was sent to a website.
- App/DMG/disk/ad-hoc checks passed; worker 0.1.4 with system-only dynamic libraries. Bundled complete MozJPEG/IJG licenses and source version were checked; DMG hash beside package in `SHA256SUMS.txt`.
- Windows/macOS Intel devices and production signing/notarization remained outstanding.

## 0.1.3: path size and desktop bitmap rendering

Date: 2026-10-05–06, macOS arm64; Windows/Intel untested. H/W were read locally; W was confirmed reference-site Quality priority bitmap output and S excluded. Acrobat was unavailable, so matching/exceeding it was not claimed.

### Implementation and protection

- Desktop lossy preserve presets gained path precision with q/Q, cm and UserUnit physical-error bounds, limited to independent page streams. Skip arrays, shared streams, Forms, glyphs, patterns and interactive documents. Fidelity did not round then. Text/vector types remain; very fine edges may change.
- Desktop bitmap uses local PDF.js per page, binary pixels to Rust, with source hash/page count/visible dimensions/length checks before MozJPEG/Flate and reconstruction. CLI still defaults to hayro; its experiments are not macOS WebKit evidence.
- Rendering/worker encoding each have 120-second limits. Render cancellation terminates PDF.js and frees pixels. Worker input directories are removed after confirmed exit; otherwise retained to avoid Windows handle conflicts. Total time includes rendering/encoding.
- Device checks found PDF.js can resolve asynchronous image failures as null even with stopAtErrors. Added local/shared cache failure checks for images/fonts, covered by real valid-RGB/corrupt-JPEG regressions.

### Real H results

Original: 81,313,254 bytes, 52 pages. Product preset comparisons below are not same-quality rankings.

| Mode/preset            | 0.1.2 bytes | 0.1.3 bytes |
| ---------------------- | ----------: | ----------: |
| Preserve/Smaller       |  45,869,733 |  35,418,667 |
| Preserve/Balanced      |  46,971,479 |  36,520,413 |
| Desktop image/Smaller  |   5,388,392 |   5,310,174 |
| Desktop image/Balanced |   9,974,565 |   9,813,647 |

Preserve Smaller saved ~22.8% against old output/~56.4% against original, mainly path numbers in 51 page streams. Balanced images: 1400 × 1811; Smaller: 1000 × 1294. Balanced differs from W's 9,811,180 bytes by 2,467 (~0.025%).

### Independent checks and quality limits

- Both preserve outputs' 52 pages rendered in Poppler at 100 DPI without warnings/errors. Median RGB RMSE vs old preserve ~0.642, maximum ~1.278. Pages 1/8/52 retained operation counts/types, non-path parameters and extracted text. Six masks, one ICC and 325 Type3 stream decoded sets matched.
- Desktop preserve Smaller matched independently validated CLI SHA-256. Both desktop bitmap outputs had 52 pages, one image each, no fonts/extractable text. Page-8 preview order/layout/text/charts looked correct.
- Median same-size RGB RMSE against the actually drawn W images: new Balanced ~5.63, old ~12.35. Old height 1812 was Lanczos-resized to 1811; each page's Do selects its image from shared 52-image resources. This only shows closer reference pixels, not improved human/source quality.
- Both bitmap outputs rendered all 52 pages in Poppler at 100 DPI. Median source RMSE: new Balanced ~42.58, old ~40.32, new Smaller ~43.93. These source metrics do not show universal improvement; closeness to W is not superiority over source/reference quality.
- Node/Skia PDF.js experiment: 9,845,671 bytes, different from macOS WebKit. Canvas/renderer differences require Windows/Intel regression; no universal size/quality advantage is promised.

## 0.1.2: MozJPEG integration

Date: 2026-10-05. macOS arm64, Rust 1.92.0, Node.js 23.11.0. Windows/macOS Intel devices were untested.

### Encoding, build and regressions

- Pinned `mozjpeg 0.10.13` / `mozjpeg-sys 2.2.3`, underlying source `c2bc3511d547870c4f67bbafda5fba794ba71f7f`. JPEG encoding is statically linked C; Rust handles PDFs, parameters, errors and file protection. Pure-Rust JPEG encoding was not claimed.
- 40 Rust tests passed: seven unit, 24 preserve, nine image. New cases: progressive decoding, RGB/Gray, invalid input, scan settings preserving quality, reversible predictors, pale colors, default widths and supersampling-budget fallback.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `npm run check` passed. App/DMG built and disk/ad-hoc checks passed; worker 0.1.2.
- `otool -L` confirmed system-only worker libraries, without external JPEG linkage. Actual bundled notices contain the pinned source's full IJG README, MozJPEG LICENSE and IJG acknowledgment.

### Real files

The same three desktop 52-page documents were read-only and never uploaded. Bytes:

| File/mode            |   Original |      0.1.1 |      0.1.2 |
| -------------------- | ---------: | ---------: | ---------: |
| H/image Smaller      | 81,313,254 |  5,211,292 |  5,388,392 |
| H/image Balanced     | 81,313,254 | 23,388,194 |  9,974,565 |
| H/image High quality | 81,313,254 | 63,565,665 | 63,565,665 |
| H/preserve Smaller   | 81,313,254 | 46,236,745 | 45,869,733 |
| H/preserve Balanced  | 81,313,254 | 47,538,744 | 46,971,479 |
| W/image Smaller      |  9,811,180 |  5,011,768 |  5,211,891 |
| S/image Smaller      |  5,277,729 |  4,840,824 |  4,990,702 |
| W/preserve Smaller   |  9,811,180 |  5,197,953 |  3,731,320 |
| S/preserve Smaller   |  5,277,729 |  5,008,349 |  3,672,124 |

All nine runs succeeded. H Smaller changed 851 × 1100 → 1000 × 1294: ~38.2% more pixels/~3.4% larger, quality 50 → 60. Balanced changed 1701 × 2200/85 → 1400 × 1812/72; its reduction is not same-quality gain. Preserve also changed quantization/encoding, and quality numbers are not universal across encoders.

### Independent checks and limits

- pypdf reopened all nine 52-page outputs. Five image outputs contain exactly one image per page, drawing `q / cm / Do / Q`, no fonts/text. JPEGs decode independently as progressive; Flate lengths match dimensions/channels.
- Preserve masks/ICC decoded identically and Type3 operations remained. Pages 1/8/52 sampled drawing/text. Existing float serialization differences required 0.0001-point page/glyph tolerance; byte identity was not promised.
- All 52 pages of H's two lossy image/two preserve outputs rendered at 100 DPI in Poppler without warnings/errors. Manual pages 1/8/17/26/40/52 and page-8 text/chart crops retained order/layout/charts, but small-text weight/details and some pale backgrounds changed.
- Median source 100 DPI RGB RMSE: old image Smaller ~28.19, new ~44.00, new Balanced ~40.32. This does not show universal improvement; rendering/resizing/lossy coding all contribute. Encoding selection/dimensions improved, but readability superiority over old/reference output is unproven. More Chinese/colored small-text/fine-line/scan tests at equal dimensions are needed.
- All three original hashes remained unchanged. Evidence/scripts/crops: ignored `tmp/real-0.1.2/`, not distributed.

### Synthetic samples

`node scripts/benchmark.mjs tmp/benchmark-0.1.2`: two samples × two modes × three presets = 12 runs, checking original hashes each time. Bytes:

| Sample/mode   |  Original | Fidelity/High quality | Balanced | Smaller |
| ------------- | --------: | --------------------: | -------: | ------: |
| RGB/preserve  | 5,773,152 |             5,761,015 |   87,310 |  16,634 |
| JPEG/preserve |   407,843 |               395,706 |   87,390 |  16,596 |
| RGB/image     | 5,773,152 |                40,898 |  122,782 |  64,049 |
| JPEG/image    |   407,843 |               581,124 |  121,784 |  64,159 |

All 14 PDFs (two sources + 12 outputs) rendered independently. Both Fidelity outputs matched source pixels; preserve titles were extractable and image outputs had one image/no fonts/text per page. High quality can grow files as disclosed. Repeated/uncompressed synthetic data does not represent commercial-file ratios.

### Desktop

Quit the old process and opened release 0.1.2. Both modes, 1000/1400 px labels and 300 DPI High quality were visible. Native selection imported H; image Smaller/Balanced and preserve Smaller all succeeded, displaying 93.4%/87.7%/43.6% reduction and numbered duplicate names. Page-8 preview worked. Outputs: `tmp/native-0.1.2/`; Downloads restored afterward.

## 0.1.1 historical record

Date: 2026-10-05; fix version 0.1.1. Environment: macOS arm64, Rust 1.92.0, Node.js 23.11.0, locked dependencies. Windows/macOS Intel were not locally validated.

### Checks and packages

- 33 Rust tests passed: two unit, 24 preserve, seven image.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `npm run check` and frontend formatting passed.
- macOS arm64 App/DMG built; `hdiutil verify` passed.
- App/worker `codesign --verify --deep --strict` / `--strict` passed with ad-hoc signing; worker 0.1.1.
- DMG hash is in adjacent `SHA256SUMS.txt`; third-party notices include jpeg-encoder MIT, Apache-2.0 and IJG files.

New coverage: direct/indirect empty Annots arrays, ICC/masks, complex-operation budget fallback, duplicate-stream sharing, equivalent number syntax, cyclic content references and rejection of incomplete pages after image decode failures. Existing tests cover vectors/text, shared images/inheritance/Form Matrix, CropBox/Rotate/UserUnit, unknown colors, forms/bookmarks/attachments, cancellation, encryption/signatures, changed inputs and non-overwriting commits.

### Real-file reproduction

Three desktop 52-page PDFs are identified as H/W/S, read-only and not distributed with software/code. H has many vectors, Type3 fonts, ICC images and masks. W/S already contain JPEG pages.

Old issue: each H `/Annots []` was mistaken for a real annotation, rejecting image mode and disabling lossy preserve images. Old JPEG settings yielded almost no gains on compressed W/S.

The table uses Smaller: old JPEG 65/150 DPI versus new optimized Huffman/chroma, JPEG 50/100 DPI. This compares product presets, not same-quality algorithms. Transparent images retain dimensions; masks remain exact; vector coordinates are not rounded.

| File | Original bytes | Old preserve (reduction) | New preserve (reduction) | New image (reduction) |
| ---- | -------------: | -----------------------: | -----------------------: | --------------------: |
| H    |     81,313,254 |      71,272,666 (12.35%) |      46,236,745 (43.14%) |    5,211,292 (93.59%) |
| W    |      9,811,180 |        9,803,484 (0.08%) |       5,197,953 (47.02%) |    5,011,768 (48.92%) |
| S    |      5,277,729 |        5,270,033 (0.15%) |        5,008,349 (5.10%) |     4,840,824 (8.28%) |

H preserve Balanced: 47,538,744 bytes (41.54%); Fidelity: 69,284,487 (14.79%). Image Balanced: 23,388,194 (71.24%); 300 DPI High quality: 63,565,665 (21.83%). All six H runs succeeded. Tightly compressed S gained only ~0.15% structure savings in Balanced.

These three files establish no universal ratio and were not compared at matched quality against unspecified competing software.

### Independent rendering and structure

Development-only Poppler, PIL/NumPy and pypdf checked:

- All 52 pages of H source, Fidelity, Smaller, image Smaller and old Fidelity rendered at 100 DPI without errors/warnings.
- New/old Fidelity matched every pixel. Against source, 42 pages matched exactly; ten had 1–3 edge-pixel differences each, also present in old Fidelity. Page 8's 50,562 drawing operations matched source operands/values. Real-file pixel-perfect fidelity was not claimed.
- Preserve retains text/fonts/vectors, masks and ICC; no full-page rasterization produced its gains.
- Five image outputs (three H presets, W/S Smaller) each have 52 pages, one independent image each, no residual fonts/text.
- H Smaller per-channel median RGB RMSE: preserve ~[0.814, 0.607, 0.751], image ~[28.529, 28.062, 27.999]. Metrics are supplementary and include antialiasing/renderer effects.
- Manual pages 1/8/17/26/40/52 showed correct text/layout/charts/order. Lossy small text/colors still require review for the intended use.
- All three original hashes stayed unchanged.

Raw records/checks: ignored `tmp/real-before/` and `tmp/real-after-2/`, not product assets.

### Synthetic rerun

`node scripts/benchmark.mjs tmp/benchmark-0.1.1`: two synthetic sources × two modes × three presets, checking original hashes each run. Positive percentages mean smaller.

| Sample |  Original | Preserve Fidelity (reduction) | Balanced (reduction) | Smaller (reduction) |
| ------ | --------: | ----------------------------: | -------------------: | ------------------: |
| RGB    | 5,773,152 |             5,761,015 (0.21%) |     101,300 (98.25%) |     21,722 (99.62%) |
| JPEG   |   407,843 |               395,706 (2.98%) |     101,512 (75.11%) |     21,695 (94.68%) |

| Sample |  Original | Image High quality (reduction) | Balanced (reduction) | Smaller (reduction) |
| ------ | --------: | -----------------------------: | -------------------: | ------------------: |
| RGB    | 5,773,152 |               184,063 (96.81%) |     126,343 (97.81%) |     28,103 (99.51%) |
| JPEG   |   407,843 |             910,577 (-123.27%) |     127,123 (68.83%) |     28,005 (93.13%) |

All 14 PDFs rendered independently at 100 DPI. Both Fidelity results matched source pixels; preserve titles were extractable; six image outputs had one image/no fonts/text per page. Synthetic repetition/uncompressed data does not represent commercial files. Reports: `tmp/benchmark-0.1.1/`.

### Desktop regression

Quit the old app and opened release 0.1.1; verified version, modes and 100 DPI Smaller. Native selection imported H and a local output folder. Image/preserve both succeeded, displaying 93.6%/43.1% reductions, numbered outputs and working original/preserve previews.

A synthetic text-annotation negative showed a specific inline rejection reason without opening details; batch counts correctly showed one complete/one failed. H's empty arrays processed normally.

Previously, the packaged 0.1.0 worker canceled a ~138 MB, 24-page sample during rendering without committing a PDF, leaving an empty temporary directory and unchanged original. New automated cancellation tests passed. Desktop force termination/timeouts and recovery after killing the application still require separate regression.

### Outstanding before release at this stage

- More licensed Chinese/small-text scans/transparency/complex-color/drawing documents, checking fonts/colors/visible areas on every page.
- Resolve rare edge-pixel serialization differences before any pixel-perfect fidelity promise.
- Total memory budgets, malicious inputs, disk full, read-only folders, timeouts, forced cancellation and crash recovery.
- Native Windows x64/macOS Intel builds and clean offline installations.
- Production Developer ID signing/notarization, Windows signing and license review.
