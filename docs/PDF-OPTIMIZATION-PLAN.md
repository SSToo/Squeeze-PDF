# PDF compression research and optimization plan

Research date: 2026-10-06. Baseline: Squeeze PDF 0.1.10. This preserves the original plan; see the [validation record](VALIDATION.md) for implemented scope and limits. Later implementation may exceed this baseline; current source governs.

## Goals and constraints

Improve file size at acceptable comparable quality and extend the range of optimizable documents. At the time, requirements included Windows/macOS, local processing and closed-source commercial distribution.

Licensing update: since 0.1.23, original code is open source under Apache-2.0, still allowing commercial use and closed-source derivatives. Third-party materials retain their licenses; see the [open-source record](OPEN-SOURCE.md). The preceding paragraph preserves the original context.

- Preserve mode retains text, fonts and vector types. Fidelity retains original image dimensions; it must not gain compression through downsampling. Balanced/Smaller may resize within their quality constraints.
- Image mode has exactly one independent image per page. Encoding may vary, but the output must not silently become multilayer text/photo/mask composition.
- Preserve originals, existing outputs, bookmarks, links, attachments and accessibility information by default, except features explicitly relinquished by image mode.
- Lower DPI, globally lower JPEG quality and deleted user content do not count as algorithm improvements.
- Adopt only smaller validated candidates, retaining the old strategy as fallback. Untested strategies have no promised compression percentage.

## Existing 0.1.10 capabilities

Reviewed `lib.rs`, `images.rs`, `page_image.rs`, `jpeg.rs`, `usage.rs`, `structure.rs`, `lossless.rs`, `paths.rs`, `reusable_paths.rs`, `form_usage.rs` and validation records.

| Area            | 0.1.10 implementation                                                                                | Remaining opportunity                                                                           |
| --------------- | ---------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| JPEG            | Static MozJPEG, Trellis, deringing, progressive scans, optimized Huffman; full chroma at quality 90+ | No existing-JPEG lossless DCT optimization or content-based quantization/chroma selection       |
| Preserve images | 8-bit RGB/Gray, restricted ICCBased; smaller JPEG candidates only; no Fidelity downsampling          | No competition with exact Flate/Indexed; many 1-bit/CMYK types keep original encoding           |
| Image mode      | PDF.js at final size; Gray/few-color pages compare JPEG/Flate                                        | No Indexed/1-bit packing; only None/Sub/Up predictors; coarse few-color selection               |
| Vectors         | Independent page-coordinate compaction; closed solid-outline reuse across pages/nested Forms         | Form precision, cross-Contents analysis and larger repeated drawing blocks                      |
| Resources       | Deduplicate streams with matching bytes/dictionaries; prune unreachable objects                      | Equivalent pixels/different encodings, equivalent reference graphs and unused Resources entries |
| Fonts           | Preserve fonts/Type3 drawing; identical streams can deduplicate                                      | Actual-glyph subsetting and mapping analysis                                                    |
| PDF writer      | Object/xref streams; maximum 100 objects per stream; level 6                                         | Compare levels, group sizes/order. Object-stream support already exists.                        |
| Flate           | Recompress some non-image streams with flate2's best level                                           | Images, DecodeParms and filter chains skipped; no benefit-aware slow compressor                 |
| Protection      | Forms, appearance annotations or attachments disable image/two vector optimizations document-wide    | Refine protection to resources/drawing streams after proving every use safe                     |

## Size remaining in two existing results

Read-only analysis of 0.1.10 Fidelity results; no new PDFs. The app computes MiB but labels it MB.

| File       |    Total | Encoded images/masks | Page/Form/Type3 drawing | Other data/overhead |
| ---------- | -------: | -------------------: | ----------------------: | ------------------: |
| Document A |  6.67 MB |      4.83 MB, ~72.3% |                 1.40 MB |             0.45 MB |
| Document B | 48.73 MB |     30.35 MB, ~62.3% |                13.27 MB |             5.11 MB |

Streams are counted once by identity. Other overhead includes object/xref streams, other streams, dictionaries, references and syntax; it is not all removable redundancy. Rounded sums may differ. Neither result contains FontFile/FontFile2/FontFile3 programs; Type3 glyph streams exist, so font subsetting is not their main solution.

Document B still has 23,803 Forms and A 1,277. This suggests prioritizing image encoding for both and Form/small-object overhead for B, based on these distributions rather than all PDFs. Statistics: ignored `tmp/pdf-research-2026-10-06/size-audit.json`.

## Candidate work

### A. Explainable size and quality baselines

P0; small/medium cost; no direct savings, but necessary to judge later work.

1. Break down originals/presets into images, masks, drawing, fonts, attachments, metadata, object containers and syntax.
2. Record candidate before/after bytes, time, adoption/skip reasons, image dimensions and encodings instead of final ratios alone.
3. Establish fixed real/synthetic coverage: outlined reports, photos, screenshots/charts, monochrome/grayscale scans, large fonts, CMYK/ICC, transparency, forms/annotations/attachments, Contents arrays and nested Forms.
4. Use identical pixels/dimensions for encoding experiments; compare complete size and local text/edges, not equal quality numbers across encoders.

Acceptance: repeatable inputs/options and explainable stage data for every preset in both modes.

### B. Content-aware image encoding

P1, first round; medium cost; both modes.

- Compare original, MozJPEG, Flate + PNG predictors and exact Indexed, including dictionaries/palettes in cost.
- Try 1-bit packing for exactly black/white input, then CCITT G4. Lossless requires exact reconstruction; do not threshold gray antialiasing.
- Add Average/Paeth; compare fixed/per-row strategies. Few-color images try both Indexed and RGB while retaining exact samples.
- Extend candidates to supported images/masks without altering Mask, Decode, Matte or ICC semantics.
- Classification schedules work only; unknown content falls back, and “photo” classification never authorizes losing text detail.

Adobe distinguishes continuous-tone JPEG/JPEG2000 from flat-color ZIP; W3C describes indexed color and all five PNG filters. [Adobe image settings](https://helpx.adobe.com/africa/acrobat/desktop/create-documents/optimize-pdfs/pdf-optimizer-settings.html), [W3C filter selection](https://www.w3.org/TR/png/#12Filter-selection). Use their encoding principles in PDF image streams, not whole PNG files inside FlateDecode.

Acceptance: exact decoded pixels for lossless candidates, unchanged Fidelity Width/Height, one image per page and inexpensive rejection of unhelpful photo candidates.

### C. Optimize existing JPEG without requantization

P1, first round; medium cost; preserve and strictly validated image-reuse paths.

- Use existing MozJPEG/libjpeg APIs to read DCT coefficients, preserving quantization, coefficients, dimensions and sampling while comparing sequential/progressive scans and Huffman coding.
- Prioritize already-compressed JPEGs to avoid another decode/re-encode for tiny gains.
- Preserve markers, ICC and PDF ColorSpace/Decode/ColorTransform relationships; initially ordinary RGB/Gray only.
- “Lossless” means no additional JPEG quantization loss, not restoration of already lost detail. Check with the same decoder and independent PDF readers.

[MozJPEG jpegtran](https://github.com/mozilla/mozjpeg/blob/master/usage.txt) describes coefficient-domain transforms/entropy optimization without decode/recompression loss.

Acceptance: unchanged quantization, coefficients, sampling and dimensions; decoded/render checks pass; complete candidate is smaller.

### D. JPEG parameters for text versus photos

P1, experimental before defaults; medium cost; both modes.

- Below quality 90, current sampling is 4:2:0. Compare 4:4:4/4:2:2/4:2:0 and quantization for colored small text, thin lines and screenshots.
- At fixed dimensions, bound the candidate set and gate local text edges, color error and image metrics. Fidelity uses stricter gates; other presets have their own.
- Quality 95 does not imply equal error on every image. Whole-page white areas must not dilute local text damage.
- Compare Jpegli/MozJPEG at matched quality. Jpegli can produce ordinary JPEG/DCTDecode; start with ordinary 8-bit colors, without XYB or other paths requiring extra color validation. Do not replace the default yet.
- Jpegli's authors report better size/perceptual results for high-quality photos. This warrants experiments, not a claim that this app's PDFs shrink 35%.

References: [MozJPEG extensions](https://github.com/mozilla/mozjpeg/blob/master/README-mozilla.txt), [Jpegli](https://github.com/google/jpegli/blob/main/README.md), [perceptual study](https://arxiv.org/abs/2403.18589).

Acceptance: bytes at equal dimensions/acceptable quality plus text/line comparisons at similar size. Keep previous candidates when unimproved; bound time/memory.

### E. Broader vectors and small-Form overhead

P1; first experiment on Document B; medium/high cost; preserve.

- Measure untouched Form drawing, repetition, dictionary/reference overhead and skips before extending algorithms.
- Bound nested-Form coordinate error across every known Matrix/cm/UserUnit call. Skip unknown/cyclic/soft-mask/special calls. The existing ~0.0005-point per-axis page budget cannot be applied to local Form coordinates alone.
- Analyze Contents arrays as continuous state, without resetting q/Q, colors, clipping or text per stream.
- Reuse repeated blocks with proven state boundaries; compare against per-outline Forms. Share safe identical resource dictionaries and reduce small-object storage overhead.
- Do not convert outlined text back into fonts or extend to unproven patterns, gradients, blending or clipping.

`paths.rs` handles independent page streams; `reusable_paths.rs` already supports restricted nested Forms. B's ~13.27 MB drawing and numerous Forms are investigation targets, not all removable data.

Acceptance: independently expand outlines/transforms, retain holes/fill rules, check fine lines/gradients/groups/masks in two renderers, and achieve complete-file savings.

### F. Per-object image usage and interactive protection

P2; high cost; preserve, with resizing only in Balanced/Smaller.

- Replace global `Usage.complete` with per-image known calls, maximum drawn dimensions and unknown references. Unsupported Type3/pattern/mask paths currently can disable all downsampling.
- Keep dimensions for images called in unknown contexts; optimize unrelated fully analyzed images. Fidelity never downsamples.
- Protect forms/annotations/attachments and every shared use rather than disabling whole documents. An attachment alone should not veto ordinary page optimization.
- Parse inline binary boundaries/abbreviations reliably before converting to optimizable objects. Finding an EI byte sequence is insufficient.

Evidence: global protection in `usage.rs`/`inspect.rs`; [qpdf image behavior](https://qpdf.readthedocs.io/en/stable/cli.html#pdf-modification) includes inline handling as a reference.

Acceptance: all shared uses correct; form data/appearances, annotations, attachments and links intact; no incorrect downsampling/rewrites in unknown contexts.

### G. Cleanup and deeper deduplication

P2, with some low-risk tasks earlier; medium/high cost; mainly preserve.

- Analyze Do/Tf/gs/sh/cs/scn and inheritance to remove only provably unused Resources entries, then prune unreachable objects.
- Share differently encoded but identical decoded images only with matching dimensions, bit depth, color, Decode, mask and transparency semantics. Appearance/RGB hashes alone are insufficient.
- Normalize/share dictionaries, ICC and Form subgraphs with cycle/context handling, preserving tags/MCID and accessibility.
- For already strict single-image pages, study direct copying/re-encoding without rendering, proving crop/rotation/color/all visible drawing equivalence and retaining one image per page.

Acceptance: order, page boxes, fonts, color, transparency, tags and links unchanged; actual duplicate-image reductions and stable shared-object output.

### H. Font subsetting and merging

P2, font-heavy files first; high cost; preserve.

- Collect glyphs from actual PDF codes, CMaps, CIDToGIDMap and programs, not Unicode extraction alone.
- Retain composites, widths, hinting and ToUnicode. Check code/CID/GID mappings together. Do not remove glyphs needed for future field editing.
- Start with recognizable CID TrueType; separately scope CFF, Type1, Type3 and cross-subset merging.
- Rust `subsetter` targets PDF but remaps and requires CID fonts; it cannot replace arbitrary font streams. HarfBuzz retain-gids is another approach. fontTools stays an independent development reference, without runtime Python.
- Keep font embedding rather than using system substitutes to save bytes.

References: [subsetter limits](https://docs.rs/subsetter/latest/subsetter/), [HarfBuzz API](https://harfbuzz.github.io/harfbuzz-hb-subset.html), [fontTools](https://fonttools.readthedocs.io/en/latest/subset/). The two outlined samples offer little expected benefit; validate font-heavy samples.

### I. Flate and object containers

P2; writer-level/group experiments can begin earlier; small/medium cost; both modes.

- Compare object-stream levels 6/9, group sizes and stable order. Small serialization differences are not content-algorithm improvements.
- Add bounded exact decode/re-encode for supported chains, DecodeParms and predictors currently skipped.
- Selectively try Zopfli on promising substantial Flate data; preserve valid zlib framing and verify decoded bytes. Do not wrap JPEG/JPX/JBIG2 in generic compression.
- Stop slow candidates at budget and use the best existing result rather than turning a successful document into a 120-second timeout failure.

References: [qpdf size optimization](https://qpdf.readthedocs.io/en/stable/cli.html#optimizing-file-size), [Zopfli](https://github.com/google/zopfli). Flate-only improvement percentages are not whole-PDF percentages.

### J. Scan encodings and special images

P3; medium/high cost; both modes, ordinary cases first.

- Exact binary: CCITT G4 first, then lossless JBIG2 generic region. Cross-page dictionaries add separate implementation/validation cost.
- No lossy JBIG2 glyph substitution; pale marks, gray antialiasing and colored stamps are not automatically disposable.
- Evaluate JPEG2000/OpenJPEG for lossless/lossy high-resolution images and line scans, requiring PDF.js, PDFium, system preview and Windows reader checks before adoption.
- Expand CMYK, 16-bit, special Decode, Indexed input and unusual transparency type by type, prioritizing exact encodings without indiscriminate RGB conversion.

References: [OCRmyPDF optimizer](https://ocrmypdf.readthedocs.io/en/latest/optimizer.html), [OpenJPEG](https://www.openjpeg.org/), [jbig2enc](https://github.com/agl/jbig2enc). Upstream refinement compatibility warnings exclude that path initially. Review each new codec's version, dependencies, license and native builds.

### K. Brotli as a separate compatibility experiment

P3; medium cost; mainly drawing/fonts/objects, not a JPEG replacement.

PDF Association announced the Brotli extension on 2026-08-19, defining BrotliDecode for PDF 2.0. It is a published industry extension, but not universally supported. Decode paths were found in pinned lopdf 0.45.0 and PDF.js 6.3.289; complete product compatibility had not yet been tested. [Announcement](https://pdfa.org/no-excuses-brotli-compression-advances-pdf/), [specification](https://pdfa.org/download-area/publications/pdf-extension-brotli.pdf).

- Compare Flate/Brotli bytes and speed offline on identical decoded streams without changing default output.
- Check PDF 2.0/extension declarations, predictors, large windows and readers. Keep xref and Catalog-containing object streams conventional initially, since Brotli there can obstruct extension discovery.
- Any future option needs explicit compatibility labeling and a conventional-PDF fallback. Filter arrays mean sequential decoding, not “try Brotli, else Flate.”
- Review published JPEG XL/AVIF specifications and actual readers separately. No evidence here establishes broad compatibility as default PDF image formats.

### L. Time-saving work

Alongside P1/P2; medium cost; usually no direct size gain, but more budget for useful encoding.

- Cache candidates within a batch by original pixels/encoding and color semantics to avoid shared-image repetition.
- Consider limited image parallelism with total memory budgets, stable order and cancellation; retain file-queue behavior.
- Evaluate MozJPEG SIMD rather than iterations alone, validating Windows MSVC and both macOS architectures/toolchains/packages.
- Spend slow experiments on large resources, retaining tiny originals cheaply; failed candidates must not invalidate an existing good result.

## Directions outside the current two modes

MRC separates scan backgrounds, foregrounds and text-edge masks for independent compression. [Internet Archive's MRC documentation](https://github.com/internetarchive/archive-pdf-tools/blob/master/docs/source/index.rst) explains it and the project's AGPL licensing.

Multilayer MRC changes the one-image-per-page contract; applying it to digital PDFs also rasterizes text/vectors. It is excluded from current defaults and that AGPL project is not integrated. A future explicit layered-scan mode would need separate design and suitable distribution licensing.

Defaults also retain attachments, bookmarks, annotations, tags, embedded fonts and transparency groups, with no automatic grayscale/thresholding. Adobe offers such options, but they can change content/functions and require an explicit product contract. [Adobe settings](https://helpx.adobe.com/acrobat/desktop/create-documents/optimize-pdfs/advance-size-reduction.html).

Fast Web View mainly changes ordering/first-page access and can grow files, so it is not a size priority. [OCRmyPDF linearization](https://ocrmypdf.readthedocs.io/en/latest/optimizer.html#fast-web-view).

## Delivery sequence

| Stage  | Scope                                                                   | Acceptance                                                                                            |
| ------ | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| First  | A baselines, B candidates, C coefficients, small I writer experiments   | Independent gain/quality evidence, unchanged Fidelity resolution, fallback for both modes/all presets |
| Second | D content parameters/Jpegli, E Document B Forms, L costs                | Smaller at comparable quality or clearer small text at similar size; more than ratio tables           |
| Third  | F usage/protection, G cleanup/sharing, H fonts, J G4/lossless JBIG2/JP2 | Broader coverage with intact special resources/mappings/interactivity                                 |
| Fourth | K Brotli/new formats                                                    | Reader matrix passes before product options; conventional PDF remains default                         |

Five rounds completed implementation or restricted experiments as recorded in validation. Form precision/cache, per-image protection, XObject cleanup, exact-sample sharing/G4, and fifth-round subgraphs, broader cleanup, Indexed/G4/CMYK inputs and bounded Zopfli entered restricted defaults. Fonts completed explicit-CID experiments only. JP2/Brotli failed compatibility gates; JPEG parameters/Jpegli failed all required local-quality gates, so none became defaults. JBIG2, inline images and general font usage remained unimplemented at that point. See the [post-fifth-round review](PDF-OPTIMIZATION-NEXT.md). Each stage is independently accepted; failed gain/quality candidates stay experimental.

## Common acceptance method

1. Freeze 0.1.10 and start every run from originals. Toggle strategies independently before combining them to diagnose growth/fallback.
2. Compare modes separately. Preserve checks text, font mapping, page boxes, image dimensions, ICC, masks, links/forms/attachments/tags. Image checks one image per page, order, rotation, crop and visible fields/annotations.
3. Verify decoded bytes/coefficients for lossless claims. Vector checks include transforms, fill rules, holes and order. Parsing success is not render validation.
4. Independently render every page; inspect fine colored text, numbers, charts, gradients, transparency and stamps at higher resolution/crops. Whole-page RMSE/SSIM alone cannot establish readability.
5. Compare encoders on identical pixels, dimensions/colors and acceptable quality, plus comparable-size views. Photo-library studies do not replace actual PDF evidence.
6. Record time, peak memory, cancellation and timeouts. Slow candidates must not turn successful large files into failures. Keep output stable, originals read-only and no overwriting.
7. Native packaging, installation and reader checks are required for macOS arm64/Intel and Windows x64 before release conclusions. Acrobat was unavailable; no new user-generated comparison was required and superiority over Acrobat was not established.

The sixth round added exact raw components, default Decode and complete-layout competition, allowing strict Lossless image candidates. See [validation](VALIDATION.md) for budgets, protection and results. These are restricted general capabilities; graphics-state simplification, Form inlining, chains and automatic fonts were still outstanding at that historical point.

This document preserves original research. Validation governs delivered scope; completed experiments are not automatically product acceptance, and failed candidates were not released as defaults.
