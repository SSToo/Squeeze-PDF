# General PDF compression: review after the fifth round

Research date: 2026-10-07. Code and measurement baseline: Squeeze PDF 0.1.15.

Later status: 0.1.16 enabled strict-lossless images, raw components and writer layouts; 0.1.19 added restricted encoding chains, short Contents candidates and complete-file fallback. See the [validation record](VALIDATION.md). Automatic font usage and other P1/P2 work were still unimplemented at this point. The following preserves the research-time code state and manual experiments rather than claiming everything was delivered.

General improvements remain worthwhile: extend encoding and drawing-structure coverage first, then automatic font subsetting. Retain current image dimensions, JPEG quality and color constraints. This review checked references, audited code and ran development experiments; it did not modify the production engine or release a version.

“General” means handling a structural class across PDFs from different sources, not making every file smaller. Existing object streams, RGB/Gray JPEG coefficient re-encoding, five PNG predictors, Indexed, G4, resource cleanup/subgraph sharing and bounded Zopfli are not new features in this plan.

## Inspection and experiments

Read-only statistics covered 17 synthetic sample categories and two existing 0.1.15 Fidelity results: 19 files, 126 pages. Five additional structural samples were generated, with manually constructed equivalent encoding/structure candidates. The same packaged 0.1.15 worker exported each using preserve + Lossless.

The table compares final complete PDFs from both paths, not image payloads or uncompressed streams. Development scripts constructed candidates; these were not yet automatic product algorithms.

| Synthetic structural sample                          | Direct 0.1.15 | Equivalent candidate, then 0.1.15 |         Additional change |
| ---------------------------------------------------- | ------------: | --------------------------------: | ------------------------: |
| ASCII85-wrapped JPEG                                 |      27,234 B |                          21,917 B |            19.52% smaller |
| ASCII85-wrapped Flate RGB                            |      75,804 B |                          22,799 B |            69.92% smaller |
| 386 short page-content streams with cross-stream q/Q |      45,617 B |                           1,664 B |            96.35% smaller |
| Strict-lossless RGB with exact Up prediction         |      22,799 B |                           2,278 B |            90.01% smaller |
| Inline JPEG externalized as an image object          |      13,765 B |                          21,917 B | **59.22% larger; reject** |

These deliberately expose structural gaps and cannot predict ordinary-document compression percentages. ASCII85 + Flate gains combine wrapper removal and access to existing Zopfli, not wrapper removal alone. The strict-lossless RGB experiment used neither JPEG nor downsampling; pypdf decoded identical sample bytes.

All ten worker exports succeeded without changing generated-input SHA-256 hashes. Poppler and macOS CoreGraphics at 200 DPI produced byte-identical pixels for inputs/results and the two structural alternatives. PDF.js performed another 15 pixel/text comparisons, all identical. Inspected RGB and cross-stream renders were nonempty and complete. This demonstrates restricted-sample feasibility, not a general parser, malformed-input handling or Windows packaging.

## Recommended priorities

### 1. Normalize encoding chains and indirect parameters

P0, mainly preserve mode. The non-image Flate path accepts only a parameter-free single Name; image handling also supports few single-filter cases, with inconsistent Filter/DecodeParms array/reference support.

- Support restricted ASCII85/ASCIIHex → Flate, ASCII85/ASCIIHex → JPEG/G4, and reversible LZW/RunLength → Flate candidates.
- Strip text wrappers from JPEG/G4 while keeping terminal compressed bytes, without decoding/requantizing images first.
- Parse filter order, parameter arrays, null, references and LZW EarlyChange correctly. Change only parameters corresponding to altered filters.
- Budget decoded bytes, chain length and cancellation. Preserve malformed/unknown chains, external streams and protected interactive images.

qpdf distinguishes general reversible filters from image decoders and can rewrite general encodings as Flate. This supports feasibility, not safety of every chain in this project. [qpdf stream controls](https://qpdf.readthedocs.io/en/stable/cli.html#option-decode-level).

### 2. Coalesce short content streams

P0, mainly preserve mode. Usage analysis already spans Contents arrays, but each segment is stored/compressed separately.

- Try one compressed stream for pages made of many short streams; retain the original array as fallback.
- Preserve array order and continuous graphics/text state. Do not insert q/Q or reset state at boundaries. Handle trailing comments, lexical boundaries, shared streams and private page references.
- Initially require parseable pages without inline binary images; skip unknown syntax and over-budget pages.
- Bound merged lengths and avoid forcing large streams together, which can increase reader memory requirements.

Official coalesce-contents behavior also recognizes that splitting can serve editing/memory needs, supporting selective merging. [qpdf content coalescing](https://qpdf.readthedocs.io/en/stable/cli.html#option-coalesce-contents).

### 3. Reuse exact image algorithms in strict Lossless

P0 for the low-level CLI/API Lossless preset. lib.rs calls image optimization only when preset != Lossless, skipping some safe algorithms entirely. Desktop Fidelity already has image candidates and is a different preset.

- Create a separate exact-only path: supported-color exact predictors, Indexed/bit packing/G4 and supported JPEG coefficient-domain entropy coding.
- Let original encoding compete. Forbid lossy candidates, requantization, downsampling, approximate grayscale detection and thresholding.
- Preserve ICC/Decode/Mask/Matte semantics. Images protected through AP, forms or special calls retain original encoding.

Reuse code without calling the entire existing images::optimize function, which includes lossy JPEG candidates, for Lossless.

### 4. Automatic font usage and resubsetting

P1, separate implementation with higher development cost; most promising for font-heavy Word, report and presentation exports.

Third-round restricted CID TrueType experiments reduced a synthetic font program from 773,236 to 22,708 B. They require an external CID→GID list, so general automatic usage collection was not delivered.

- Initially support static pages, CIDFontType2 and interpretable Identity-H/CIDToGIDMap; validate other CMaps, vertical writing and CFF separately.
- Track actual PDF codes, CIDs and GIDs through Tf/Tj/TJ, graphics-state save/restore, Contents arrays and Form resource scopes. Extracted Unicode alone is insufficient.
- Retain composite dependencies, hinting, widths, ToUnicode and original content codes. Update mappings when GIDs change.
- Protect editable fields and shared fonts; current displayed text does not establish all glyphs needed for future input.

Continue the Rust subsetter experiment first. HarfBuzz retain-gids offers another route, but still requires PDF usage analysis. [HarfBuzz subset source](https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-subset.h), [validation](VALIDATION.md).

### 5. CMYK/YCCK JPEG coefficient re-encoding

P1, mainly preserve mode for print/design documents. Fifth-round CMYK support covers raw/Flate four-component samples; jpeg_lossless.rs still accepts only 1/3 components, and images.rs excludes common CMYK cases.

- Rewrite Huffman/scan candidates from four-component DCT coefficients, preserving quantization tables, sampling, dimensions and coefficients.
- Check Adobe APP14, PDF Decode/ColorTransform, ICC and CMYK/YCCK inversion relationships together.
- Avoid RGB conversion; merely allowing num_components == 4 is insufficient.
- Independently compare coefficients and multireader color blocks/gradients. Adopt only smaller, validated candidates.

MozJPEG jpegtran supports compressed-coefficient optimization; this extends the existing approach without proving CMYK PDF compatibility. [MozJPEG usage](https://github.com/mozilla/mozjpeg/blob/master/usage.txt).

### 6. Share decoded-equivalent non-image resources

P1, alongside chain support. Fifth-round subgraph sharing works across object numbers, but streams still compare encoded bytes/dictionaries. Identical font programs, ICC or CMaps at different Flate levels/wrappers can miss merging.

Start with FontFile2, ICC and ToUnicode leaves whose decoded bytes and semantic dictionaries match. Separate encoding attributes such as Length/Filter without ignoring ICC N/Range/Alternate, font-program length semantics or mappings. Bound decoding caches, verify bytes after hash matches, and choose an existing smaller valid encoding.

This does not justify merging different font subsets, tagged Forms or interactive identities. Protect special resources.

## Conditional directions

| Direction                                          | Gap and recommendation                                                                                                                                                                                                                                                 | Scope                                                              |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Inline-image parsing/externalization               | BI/ID/EI causes conservative skips. Parse binary boundaries, abbreviations, color resources and EI inside data; retain inline encoding and choose externalization by complete-file cost. The counterexample above rules out unconditional conversion.                  | Preserve; may enable later image/resource optimization             |
| Further exact encoding selection                   | Five PNG filters already exist. Add entropy/budgeted compression probes, TIFF Predictor 2, palette permutations and exact direct 2-/4-bit Gray. Existing Indexed is not new.                                                                                           | Both modes, especially charts/grayscale/few-color images           |
| Lossless JBIG2 generic region                      | Accept only images already exactly 1-bit, encode through API, check every pixel and compare against G4. Do not apply CLI-default thresholding to Gray/RGB or substitute similar glyphs.                                                                                | Exact monochrome scans in either mode, still one image per page    |
| Direct reuse of single-image pages                 | Bypass rendering only if sampling grid matches output and crop, rotation, color, interpolation and visible coverage are provably equivalent. Otherwise render normally.                                                                                                | Image mode; may reduce repeated JPEG loss/time; measure size gains |
| Encoding budgets and best complete-file candidates | The first 48 Zopfli candidates sorted by encoded size may all exceed 256 KiB decoded eligibility, starving later valid streams. Improve eligibility/benefit estimates, document budgets, cancellation and caching; retain a complete candidate without new strategies. | Both modes, balancing gains and stability                          |

ZopfliPNG implements MinSum, Entropy and BruteForce filters, showing that minimum absolute residuals need not give the smallest encoding. Borrow filter-search ideas while writing legal PDF image streams rather than embedding entire PNG files. [ZopfliPNG strategies](https://github.com/google/zopfli/blob/master/src/zopflipng/zopflipng_lib.h).

jbig2enc documents refinement compatibility issues; its CLI source also disables that path. Generic region and symbol modes differ. “JBIG2” does not automatically mean lossless glyph sharing, and thresholding command examples should not be copied. [jbig2enc](https://github.com/agl/jbig2enc), [encoding/preprocessing source](https://github.com/agl/jbig2enc/blob/master/src/jbig2.cc).

## Lower-priority work

- **Whitespace/comments alone:** approximately 2 MiB of drawing data in each of two existing Fidelity results was inspected. Equivalent lexical rewrites of 18/17 streams saved only 463/861 B against zlib-9 of the same data. These are stream-level results without complete-file measurements, insufficient as a principal strategy. Combine with a reliable parser later.
- **Globally increasing Zopfli iterations:** 47/48 of the first 48 candidates in the two existing results fit decoded limits. Theoretical starvation does not explain their current compression. More iterations may not justify the time.
- **Reintroducing Jpegli, JP2 or Brotli:** earlier local-quality/reader gates still apply. Keep experiments without new evidence.
- **More coordinate rounding, lower DPI/quality, deleting attachments/tags:** retain current error/content contracts; these are outside this general-optimization effort.

Both existing Fidelity results mainly contain Flate and JPEG and no embedded font programs. New chains/fonts primarily expand benefits to other document sources. This review did not demonstrate major additional reductions for these two files.

## Next implementation and acceptance

Implement P0 first: chains/indirect parameters → selective short Contents coalescing → strict-lossless images. Then separately tackle font usage, CMYK JPEG and non-image decoded sharing. Inline images, JBIG2 and extra predictors begin as budgeted experiments.

Synthetic coverage should include ASCII85/Hex, LZW EarlyChange, RunLength, Filter/parameter arrays/references, cross-stream state, trailing comments, shared Contents, binary EI, CID mappings, CMYK/YCCK markers, damaged streams, budgets and cancellation. Users need not supply a comprehensive corpus.

Candidates must reduce complete-file size against the previous engine candidate; preserve promised lossless pixels/coefficients/decoded bytes; retain text mappings/geometry, ICC, masks, editable forms, attachments and tags; pass common readers; and respect task deadlines. Comparing only with the original or a single stream is insufficient.

Experiments are in ignored `tmp/research-round6-2026-10-07/`: probe.py/probe-results.json (complete files), coverage.py/coverage.json (19 read-only files), content_probe.py/content-results.json (lexical streams), pdfjs/results.json (part of three-reader validation). These tools are not runtime dependencies.

See [deeper research](PDF-OPTIMIZATION-DEEPER.md) for default Decode, 16-bit/special-color raw components, numbering/layout, graphics state/single-use Forms, masks and optimization order. It also records two failed mask deletions and a stroke-reuse lossless failure in PDF.js; product defaults did not change.
