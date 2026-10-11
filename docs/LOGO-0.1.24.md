# 0.1.24: refreshed transparent logo

Date: 2026-10-11; macOS arm64. The replacement `Downloads/logo.png` was copied unchanged to `public/logo.png`: 1046 × 1046, 952,077 bytes, with its alpha channel preserved. SHA-256: `3eb78975f6929dd60ebe7c45ce93bbfc46b1a6d457301aeca3b4cfe7f50c50bf`.

- Regenerated Windows ICO, macOS ICNS and existing size variants, including the separate 64 px icon; updated the favicon. The generator also updates existing mobile assets; supported platforms remain Windows/macOS.
- Retained red accents, interface layout, Apache-2.0 licensing and compression strategies; synchronized the version to 0.1.24. Engine code and dependency versions were unchanged.
- TypeScript and production build passed. The local frontend was checked in light/dark themes at 1180 × 820. The logo loaded at 40 × 40 from its 1046 × 1046 source, without a white frame or horizontal overflow; no console errors occurred during inspection. Screenshots are in ignored `tmp/logo-refresh/ui-light.png` and `ui-dark.png`.
- macOS arm64 App/worker versions, strict ad-hoc signatures and DMG verification passed. After read-only mounting, the app icon matched the generated ICNS, executables matched the build, and project licensing plus complete third-party materials passed checks.

| Deliverable                                        | SHA-256                                                            |
| -------------------------------------------------- | ------------------------------------------------------------------ |
| `Squeeze PDF_0.1.24_aarch64.dmg`, 18,550,851 bytes | `99ba2cc196984357412227d130f3df60c81683d644eda4d00c72482674786aaa` |
| App executable                                     | `421f6b3dfc71f136f57577db28bfa595ce6e2d24518768a1ad09b50e107c7ba4` |
| Bundled worker                                     | `c883820f564ad6c4892c1c2023b4437fd47e043e43b54badf949b1faf42542e4` |

This update changed brand assets and version only. Engine regression and real Tauri consecutive-task tests were not rerun; new Windows/Intel packages, production signing and notarization were not completed in this round. Package evidence is in ignored `tmp/logo-refresh/package-proof.json`; the 0.1.23 DMG retained its previous checksum.

At the user's later request, the same Git commit included earlier licensing completion, Apache-2.0 adoption and branding, plus eight mutually linked README translations matching the interface. Documentation changes did not affect the verified package. Repository visibility was Private at the time of this record.

Pre-commit checks: 197 local links and eight-language navigation across the eight READMEs were valid; 21 commands matched across the seven translations; key preset parameters, environment requirements and licensing were checked. Ten license regressions, TypeScript and diff checks passed.
