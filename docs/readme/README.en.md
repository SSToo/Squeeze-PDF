# Squeeze PDF

**English** · [简体中文](../../README.md) · [Français](README.fr.md) · [Español](README.es.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Português](README.pt.md)

An open-source, offline PDF compressor for Windows and macOS, currently an MVP. The interface uses Tauri 2 and React. Rust implements compression strategies, image processing, page reconstruction and file protection. PDF reading/writing uses lopdf; desktop rendering uses local PDF.js, while the CLI uses hayro by default. JPEG encoding uses statically linked MozJPEG through Rust bindings. No qpdf, Ghostscript or Python is required at runtime.

Download Windows x64, macOS Apple silicon and macOS Intel installers from [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases). Pushing a `v` tag matching the app version automatically publishes the installers and checksums after all three builds succeed. See the [release guide](../RELEASING.md) (Chinese/English).

Starting with 0.1.25, release builds check for stable updates on GitHub about five seconds after launch and every 24 hours while running. Use the update icon in the header to check manually. Installation and restart require your choice and wait for active file tasks to finish. Offline checks do not interrupt compression. Only version checks and update downloads use the network; PDFs are never uploaded. Version 0.1.24 and earlier need one manual upgrade first.

## License

Original project code is licensed under [Apache-2.0](../../LICENSE), with attribution in [NOTICE](../../NOTICE). Commercial use and closed-source derivatives are permitted, subject to the license's notice, modification and other requirements. The license text governs the patent grant and its conditions.

Third-party libraries, fonts and assets retain their own licenses. Distributions include `third-party/PROJECT-LICENSE.txt`, `PROJECT-NOTICE.txt`, third-party notices and required source materials. See [Contributing](../../CONTRIBUTING.md) and the [open-source preparation record](../OPEN-SOURCE.md).

## Two output modes

| Mode                      | Output                                                                         | Typical use                                                          |
| ------------------------- | ------------------------------------------------------------------------------ | -------------------------------------------------------------------- |
| Preserve original content | Keeps text, fonts and vectors; optimizes paths, structure and supported images | Reports, contracts, drawings, selectable text and zooming            |
| Full-page images          | Renders each page as one separate image, then builds a PDF                     | Uniform image pages when selectable text and vectors are unnecessary |

Output mode and quality preset are independent:

| Preset                  | Preserve original content                                                            | Full-page images                                    |
| ----------------------- | ------------------------------------------------------------------------------------ | --------------------------------------------------- |
| Fidelity / High quality | Original image resolution, JPEG 95, path precision and repeated-outline optimization | 300 DPI, lossless Flate encoding of rendered pixels |
| Balanced                | Path optimization, JPEG 85, up to 200 DPI                                            | 1400 px wide, JPEG quality 72                       |
| Smaller                 | Path optimization, JPEG 50, up to 100 DPI                                            | 1000 px wide, JPEG quality 60                       |

Fidelity preserves image dimensions but can change pixels; it is **not strictly lossless**. Vector coordinate rounding is bounded to approximately 0.0005 points per axis across known transforms. Text positioning and color parameters are not rounded; very fine edges may still change. Repeated eligible solid-filled outlines remain vectors and are reused as Forms.

High quality encodes rendered pixels losslessly; text and vectors have already been rasterized. Image mode removes selectable text, vector paths, links and bookmarks. The desktop app flattens renderable visible forms and annotations into images; hidden fields and annotation popup text are omitted. Embedded attachment data is retained.

Preserve mode exports only a validated candidate smaller than the original. Candidates are compared as complete PDFs and fall back when larger. Image mode honors the requested format even if the file grows, and reports the increase. Originals are read-only, existing outputs are never overwritten, and a changed input prevents the output from being committed.

The engine compares JPEG and exact Flate, Indexed palette, 1-bit and CCITT G4 candidates where supported. It also optimizes existing JPEG coefficients, resource sharing, restricted CID font subsets and file layout. Desktop rasterization can compare direct rendering with a budgeted 2× render-and-downsample candidate using quality gates. See the [implementation and validation limits](../VALIDATION.md).

## Desktop features

- Drag in or select multiple PDFs; deduplicate and process sequentially.
- Choose either mode and three presets; see processing stages and completed-file counts.
- Compare sizes, savings or growth, processing details and inline failure reasons.
- Preview original and result side by side with page navigation and zoom; all preview assets are local.
- Select an output folder; automatic filename numbering, cancellation, retry and opening results or their folder.
- Remember mode, preset, folder and theme; the file queue lasts only for the current session.
- The language icon to the right of the theme toggle opens eight languages: English, Simplified Chinese, French, Spanish, Korean, Japanese, German and Portuguese. Initially follows a supported system language, otherwise English; remembers later choices.
- UI text, stages, errors and preview controls update without interrupting tasks. Filenames, paths and unknown low-level errors remain unchanged.
- Transparent logo and red accents, with a charcoal dark theme.

## Development

Requires Node.js 22.13+, Rust 1.92.0 (pinned by `rust-toolchain.toml`) and a platform C compiler. Use Xcode Command Line Tools on macOS; MSVC C++ Build Tools and WebView2 on Windows. MozJPEG is compiled into the worker. Optional assembly SIMD is disabled, so NASM and CMake are not required.

Run all commands from the repository root:

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` starts only the frontend; native file operations require `tauri dev`. Build scripts prepare the worker for the current architecture, local PDF.js assets and license materials. The npm package's `private: true` prevents accidental npm publication and does not change the open-source license.

## Checks and packages

```sh
cargo test --workspace --locked
node scripts/prepare-worker.mjs
npm run notices
npm run test:licenses
npm run check
npm run test:render
npm run test:i18n
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# macOS app
npm run tauri -- build --bundles app
# macOS app and a simple DMG
npm run package:mac
# Windows EXE installer; run on Windows
npm run tauri -- build --bundles nsis
```

Packages are written to `target/release/bundle/` for the host architecture. When passing `--target`, also set `CARGO_BUILD_TARGET` so the worker matches. The Windows installer includes the offline WebView2 installer.

CI is configured for Windows x64, macOS arm64 and macOS Intel. Local validation has been on macOS arm64; Windows and Intel installation regression tests remain outstanding. macOS packages use ad-hoc signatures for local trials. Production Windows signing, macOS Developer ID signing and notarization are not configured. For release signing, set `APPLE_SIGNING_IDENTITY` and follow [Tauri's signing documentation](https://v2.tauri.app/distribute/sign/macos/).

## CLI and engine

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode` accepts `preserve` (default) or `raster`. Presets are `lossless`, `fidelity`, `balanced` and `small`; supported presets can use `--quality 85 --dpi 200`. Path precision and outline reuse are opt-in in the CLI via `--compact-paths` and `--reuse-paths`, and enabled for all three desktop preserve presets.

`lossless` does not requantize or downsample images and ignores those two vector options; it can use exact encodings, coefficient-domain JPEG optimization and equivalent resource sharing. `fidelity` keeps image dimensions, defaults to JPEG 95 and does not accept a target DPI. Desktop image-mode High quality uses lossless pixel encoding. Ctrl+C requests cancellation. The desktop app runs a separate worker; cancellation terminates rendering or the worker. Page rendering and worker encoding each have a 120-second limit.

Core APIs are `squeeze_core::inspect_file`, `squeeze_core::audit_file` and `squeeze_core::compress`. `squeeze audit input.pdf` produces a read-only size breakdown. Compression JSON reports before/after sizes, stages, image candidates, timings, cache use and writer selection. Stream bytes or candidate costs are not the complete PDF size and do not imply removable redundancy.

## Scope and limitations

- No fixed PDF file-size cap; files over 256 MiB are supported subject to available memory. A decoded stream is limited to 64 MiB and an image to 24 million pixels; processing is sequential.
- Image optimization covers supported RGB/Gray, restricted ICCBased, Indexed, G4 and raw color-component inputs. Unknown encodings and mappings are kept conservatively; some CMYK/YCCK JPEGs permit coefficient optimization without pixel re-encoding. Image dimensions and color/Decode/mask semantics are protected where required. This is not universal codec or color-space support.
- Unknown drawing scales or incomplete analysis disable affected downsampling. Fidelity always retains original image dimensions.
- Preserve mode retains forms, annotations and attachments, protecting appearance resources and their shared images. Unrelated page images can still be optimized. Documents with forms or appearance annotations conservatively skip the two lossy vector optimizations. Encrypted, digitally signed, PDF/A, XFA and portfolio PDFs are skipped.
- Desktop image mode accepts renderable forms/annotations, ordinary links, empty form containers and attachments. Missing drawable appearances or damaged annotations cause page errors rather than incomplete output. CLI hayro rendering rejects forms/annotations requiring drawing and `BrotliDecode` streams; use desktop PDF.js for these cases.
- Rotation, visible crop and physical page size are handled. Missing embedded nonstandard fonts cause conversion to be rejected. Complex pages still require visual review; hayro is evolving and does not guarantee full commercial-PDF compatibility.
- The parser can repair cross-references; not every malformed PDF is strictly rejected. Per-stream, pixel and analysis limits exist, but there is no memory budget covering every allocation.
- Normal cancellation cleans up terminated workers' temporary directories. Startup recovery after a forced kill or power loss is not implemented.
- No OCR, PDF editor, guaranteed target size or commercial activation system.

## Benchmarks and technical records

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

The large sample adds an approximately 138 MB, 24-page PDF for cancellation and large-file testing. Synthetic compression ratios do not predict real documents. Independent development checks use Poppler, pypdf, Pillow and NumPy; these are not runtime dependencies.

Detailed technical records are currently in Chinese: [validation](../VALIDATION.md), [plan](../../PLAN.md). Jpegli and Brotli output experiments are not production dependencies or output options.

## Third-party materials

Lockfiles pin dependencies. `npm run notices` collects license texts, hayro's NOTICE, PDF.js font/WASM notices, QuickJS/Mozilla MIT notices, unmodified MPL dependency sources and Liberation 1.07.4 sources plus editable TTX sources for the actual bundled fonts. Materials are placed in `third-party/`; the inventory conservatively includes build dependencies. See [source and rebuild instructions](../../licenses/SOURCE-CODE.md) and the [licensing record](../LICENSING-0.1.22.md).

`npm run test:licenses` checks license texts, asset/source hashes and MPL source completeness in Windows/macOS CI. PDF.js or asset updates require a new license/source review. `scripts/make-font-sources.py` needs FontTools 4.66.1 only as a development tool. Liberation fonts retain their GPL license with font exceptions; MPL components and other dependencies keep their respective licenses.

PDF I/O uses [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/); rendering uses [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) or [hayro](https://github.com/LaurenzV/hayro); decoding uses image, JPEG encoding MozJPEG, G4 encoding MIT-licensed `fax 0.3.0`, and restricted CID TrueType subsetting MIT/Apache-2.0 `subsetter 0.2.6`. Development test fonts are not distributed. Original code has been Apache-2.0 since 0.1.23; this does not relicense third-party material.
