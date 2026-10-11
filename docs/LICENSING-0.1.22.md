# 0.1.22: completing third-party licenses and source distribution

Date: 2026-10-10. This update addressed font-source distribution and QuickJS notices identified during the commercial-use dependency review. Scope: licensing materials, build checks and installer resources. Engine code and quality parameters remained at 0.1.21.

This record describes licensing as of 0.1.22. Original code subsequently adopted Apache-2.0 in 0.1.23; see the [open-source preparation record](OPEN-SOURCE.md). Third-party licenses remain unchanged.

## Materials added

| Component                        | Distributed content                                             | Completion                                                                                                                                                               |
| -------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Liberation Sans 1.07.4           | Four PDF.js TTF fonts, GPLv2 with the Liberation font exception | Bundled complete upstream SFD sources, complete editable TTX for the four actual fonts, reconstruction script, build requirements, full license and original font hashes |
| QuickJS / PDF.js QuickJS sandbox | `quickjs-eval.js`, `quickjs-eval.wasm`                          | Added full Bellard/Gordon and Mozilla MIT copyright/license notices to the distribution inventory                                                                        |
| Five MPL-2.0 Rust dependencies   | selectors, cssparser, cssparser-macros, dtoa-short, option-ext  | Retained complete unmodified crate sources; added the general MPL text, explicit access instructions and per-file hash checks                                            |

`licenses/pdfjs-assets.json` pins PDF.js 6.4.299 and SHA-256 hashes for six font/QuickJS files plus licensing/source materials. QuickJS links pin the revision supplying the license text; they do not prove the WASM's exact build commit. The original license defines the font exception's scope. PDF.js's top-level npm Apache-2.0 license does not describe every bundled asset.

## Font-source correspondence

[PDF.js font documentation](https://github.com/mozilla/pdf.js/blob/v6.4.299/external/standard_fonts/README.md) identifies version 1.07.4 and states that glyph order participates in width/scaling mappings. Inspection found 682 glyphs each in Regular/Bold and 687 each in Italic/BoldItalic in the [upstream 1.07.4 binary release](https://releases.pagure.org/liberation-fonts/), versus 681 in each PDF.js font. Retained outlines, instructions and widths were checked against upstream; matching version strings alone cannot establish whole-font binary identity.

Both the full upstream SFD sources and complete TTX exported from the actual PDF.js fonts are therefore supplied. TTX includes outlines, composite references, font/glyph hints, metrics, character maps, names, layout tables and reconstruction scripts. The product continues using unchanged PDF.js font bytes. Rebuilt fonts may differ in table packing, padding and checksums; byte-identical reconstruction is not promised.

| Source archive                       |     Bytes | SHA-256                                                            |
| ------------------------------------ | --------: | ------------------------------------------------------------------ |
| `liberation-fonts-1.07.4.tar.gz`     | 2,937,949 | `ad98b7498dc2992f7f0868f79b65ce4a720a3acdb63ab3f1f1cb6881117a5406` |
| `pdfjs-liberation-1.07.4-ttx.tar.gz` |   820,775 | `2750c1446ea31839af039a531a6fb52d112516acc0be6166187b1b7d08872e09` |

`scripts/make-font-sources.py` and FontTools 4.66.1 reproduce the TTX archive. Its `rebuild.py` preserves historical OS/2 character ranges rather than allowing automatic compiler correction. Original font binaries are not overwritten.

## Accessing the sources

The bundled `third-party/SOURCE-CODE.md` explains locations and rebuilding. Sources are included locally without another payment or download. This guide was bilingual in 0.1.22; current project documentation is maintained in English.

- macOS: right-click the application → Show Package Contents → `Contents/Resources/third-party/`.
- Windows: `third-party/` in the installation directory.
- Standalone CLI/worker distributions must include the complete `third-party/` directory.

`NOTICES.txt` contains original licenses and copyrights; `mpl-sources/` and `font-sources/` contain the corresponding sources. Custom product terms cannot withdraw rights granted by third-party licenses. See [Mozilla's official FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) for MPL and larger closed-source works. At the time, final product terms and the publishing entity still needed to be determined by the product owner.

## Automated checks

Before building, checks verify the actual PDF.js version, font/QuickJS bytes and license/source archives. Changes or newly discovered relevant assets block the build until review materials are updated. Generated notices are checked for complete license texts, source instructions, archives and per-file MPL source hashes. Generated directories are cleaned and rebuilt to avoid stale materials.

`npm run test:licenses` covers valid materials, unreviewed versions, changed fonts, additional QuickJS files, missing font sources, removed copyrights and missing MPL source files. These checks were added to Windows/macOS CI.

## Local verification and delivery

The local platform was macOS arm64. Completed checks:

- `npm run notices` generated 358 dependency/asset records; all 71 files across five MPL source directories matched byte for byte. All seven `npm run test:licenses` tests passed, as did TypeScript and relevant Prettier/Node syntax checks. Diff checks excluded only two verbatim upstream texts, `licenses/MPL-2.0.txt` and `licenses/liberation-1.07.4-LICENSE.txt`, whose original trailing spaces and reviewed hashes were preserved. Other files passed.
- Extracted the actual bundled TTX archive and ran `rebuild.py`. All four fonts retained the same 681-glyph order. Normalized XML matched for 18 non-offset tables per font, including outlines, metrics, hints, character maps and GPOS/GSUB. Only computed `head.checkSumAdjustment` and offset-recording `loca` were excluded. Build-output hashes for all four production fonts and both QuickJS assets still matched the manifest.
- `npm run package:mac` generated the 0.1.22 App/DMG. Complete licenses, both font-source archives, access instructions and MPL sources passed checks in the App and again after an actual read-only DMG mount. The DMG root also included `Third-party licenses and sources.txt`, matching the bundled instructions.
- Strict ad-hoc signatures for App/worker and DMG verification passed. Both mounted executables matched the build App; worker and application versions were 0.1.22.
- The final bundled worker reran 38 combinations across eight sample categories: 32 preserve and six CLI image-mode combinations. Font subsets, editable/appearance protections, CMYK, transparency, large-image and vector outputs were byte-identical to accepted 0.1.21 results, with unchanged input hashes. The earlier 141 Rust tests and all 488 combinations were not rerun; engine code and dependency versions were unchanged.

Deliverable: `target/release/bundle/dmg/Squeeze PDF_0.1.22_aarch64.dmg`, 16,037,900 bytes.

| File                  | SHA-256                                                            |
| --------------------- | ------------------------------------------------------------------ |
| 0.1.22 DMG            | `e05314b96046ea1d73bfaf94fdcfb6969f7e2478b11de4583b242d87948ff0b7` |
| DMG `squeeze-desktop` | `439c1af695d1539c03eec0b7ed2d77f6267c7e925d9d8cabe89cbdea636ae5f2` |
| DMG `squeeze-worker`  | `ccfdea7337bb7923492138b205f508f19484b00356d2492e4c22295145d94da4` |

The original 0.1.21 DMG remained unchanged at SHA-256 `1ff18a143a2c4f1edbca9385aeb0acafdcf2d0e2df7c986b6f4f5a83929658e4`. Raw records are in ignored `tmp/license-completion/`, including `package-proof.json`, `font-source-check/results.json` and `packaged-0.1.22/results.json`.

The same distribution rules and CI checks cover Windows/Intel, but their 0.1.22 packages were not generated or device-tested in this round. Production developer signing, notarization and outstanding real-desktop consecutive-task acceptance retained their previous status. This update completed identified license/source materials; it did not determine final product licensing terms.
