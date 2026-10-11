# 0.1.23: Apache-2.0 and brand assets

Date: 2026-10-11. The user selected Apache-2.0, allowing commercial use and closed-source derivatives.

Subsequent logo update: 0.1.24 replaced `public/logo.png` with the newly supplied transparent PNG. This page records the initial 0.1.23 brand assets and package checksums; see the [0.1.24 record](LOGO-0.1.24.md).

## Licensing and distribution

- Added the full `LICENSE` and project `NOTICE`; aligned Cargo workspace, npm root/lockfile and Tauri installer metadata to Apache-2.0. Crates continue inheriting the workspace license.
- Documented scope and third-party exceptions in the README and added CONTRIBUTING.md. `package.json` retains `private: true` to prevent accidental npm publication; this does not affect open-source licensing.
- All desktop builds include `PROJECT-LICENSE.txt` and `PROJECT-NOTICE.txt` under application resources `third-party/`. The custom macOS DMG also includes `LICENSE.txt` and `NOTICE.txt` at its root. License checks reject truncated texts or missing attribution.
- Apache-2.0 for original code does not change third-party library, font or asset licenses. Liberation upstream/actual-font TTX sources, complete MPL dependency sources and QuickJS/Mozilla MIT notices remain bundled; see the [0.1.22 licensing record](LICENSING-0.1.22.md).

Apache-2.0 permits use, modification, distribution and commercial use subject to retaining licenses/notices, identifying changes and its other conditions. The [official text](https://www.apache.org/licenses/LICENSE-2.0) governs the patent grant and termination conditions. Separately licensed third-party material was not relabeled Apache-2.0, and no additional commercial-use prohibition was added.

## Logo and colors

The supplied `Downloads/logo.png` was preserved as `public/logo.png`, a 1254 × 1254 PNG, without redrawing. SHA-256: `7f57ce7c68bf1fd0df096c83e6eb871c6d5f52e509d013f5e8948dcc35fb5b38`.

The window header and favicon use this image. Tauri CLI generates Windows ICO, macOS ICNS and size variants from the same source. Updating existing mobile icon assets does not imply mobile-platform support.

Brand accents became red: primary buttons, selected borders, radio controls, preset badges, progress and the empty-state illustration. Light backgrounds are neutral gray and dark backgrounds charcoal. Both compression modes, language selection and theme switching were retained; engine code and compression parameters did not change.

WCAG contrast for principal text/background pairs: white text on red buttons 4.80:1; light-theme brand text on pale red 4.64:1; dark-theme brand text on selected surfaces 6.17:1; secondary text in light/dark themes 4.93:1 / 7.08:1. Control borders against surfaces: 3.16:1 / 3.48:1. Radio colors have an older-WebKit fallback; forced-colors mode uses system colors. This does not claim acceptance testing with assistive technologies on every platform.

## Validation scope

- Ten license regressions, TypeScript, five eight-language regressions and frontend PDF.js rendering/form-and-annotation preparation checks passed.
- The local frontend was inspected at 1180 × 820 in both themes, both modes and Chinese/German. German image mode at the 900 × 650 minimum window had no horizontal overflow; the logo loaded and no browser console errors occurred during inspection. Screenshots are in ignored `tmp/open-source/ui-*.png`.
- macOS arm64 App/worker version 0.1.23, strict ad-hoc signatures and DMG verification passed. A read-only DMG mount confirmed complete license/source materials, root project license/notices, generated icons and both executables. The bundled worker's 38 combinations across eight sample categories were byte-identical to the previous accepted version, with unchanged input hashes. The 141 Rust tests and all 488 combinations were not rerun; engine code and dependency versions were unchanged.
- These interface checks did not exercise Tauri native file selection or the actual desktop compression flow. New Windows/macOS Intel packages were not built locally. Consecutive real desktop tasks, production signing and notarization still required separate acceptance tests.

| 0.1.23 deliverable                                 | SHA-256                                                            |
| -------------------------------------------------- | ------------------------------------------------------------------ |
| `Squeeze PDF_0.1.23_aarch64.dmg`, 18,751,284 bytes | `ddb958178af4381e7059ebb31f50c287d2ee42cb4648138ab71b2724525516c1` |
| App executable                                     | `eb600df350a27da9c41411ae2a3957aaf55d51d94d025bd71cabe99d7d7ac94c` |
| Bundled worker                                     | `fd72fcf7c1aa81e7722300b0d8354d35243b3e85edf25470c973158d3ac46df2` |

Package evidence and regression results are in ignored `tmp/open-source/package-proof.json` and `tmp/open-source/packaged-0.1.23/results.json`. The older 0.1.22 DMG retained its previous hash.

## Repository publication checks

A basic scan covered current non-ignored files and text objects in three reachable Git commits: 388 text objects inspected, 105 binary/large objects skipped, no high-confidence secret patterns found, and no tracked customer PDFs, `.env` or private keys. Git author email used GitHub noreply. Evidence is in ignored `tmp/open-source/history-audit.json`; this was not a comprehensive security audit.

Development validation scripts now use `python3` from PATH with a `SQUEEZE_QA_PYTHON` override instead of a fixed developer-machine directory. Personal home paths in current validation documents were replaced with relative `Downloads/` descriptions. Historical Git commits retain earlier development paths; history was not rewritten.

These materials completed open-source licensing preparation and were committed with the 0.1.24 branding and eight-language READMEs. Repository `SSToo/Squeeze-PDF` was still Private at the time of this record. License selection and repository visibility are separate settings; publishing an existing repository also exposes its existing commit history.
