# Squeeze PDF

[English](README.en.md) · [简体中文](../../README.md) · [Français](README.fr.md) · [Español](README.es.md) · **한국어** · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Português](README.pt.md)

Windows와 macOS용 오픈 소스 오프라인 PDF 압축 프로그램이며, 현재 MVP 단계입니다. 인터페이스는 Tauri 2와 React를 사용합니다. Rust로 압축 전략, 이미지 처리, 페이지 재구성, 파일 보호를 구현했습니다. PDF 읽기·쓰기는 lopdf, 데스크톱 렌더링은 로컬 PDF.js, CLI 기본 렌더링은 hayro를 사용합니다. JPEG 인코딩은 Rust 바인딩으로 정적 링크한 MozJPEG를 사용합니다. 실행 시 qpdf, Ghostscript, Python은 필요하지 않습니다.

[GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases)에서 Windows x64, Apple silicon Mac, Intel Mac 설치 파일을 다운로드할 수 있습니다. 앱 버전과 일치하는 `v` 태그를 푸시하면 세 플랫폼의 빌드가 모두 성공한 뒤 설치 파일과 체크섬을 자동으로 게시합니다. [릴리스 가이드](../RELEASING.md)(중국어/영어)를 참고하세요.

## 라이선스

프로젝트 자체 코드는 [Apache-2.0](../../LICENSE)이며 저작권 표시는 [NOTICE](../../NOTICE)에 있습니다. 라이선스와 고지 보존, 변경 사항 표시 등 조건을 준수하면 상업적 이용과 비공개 소스 파생 제품 개발이 가능합니다. 특허 허여의 범위와 조건은 라이선스 본문을 따릅니다.

외부 라이브러리, 글꼴, 자산은 각자의 라이선스를 유지합니다. 배포본에는 `third-party/PROJECT-LICENSE.txt`, `PROJECT-NOTICE.txt`, 외부 고지와 필요한 소스 자료가 포함됩니다. [기여 안내](../../CONTRIBUTING.md)와 [오픈 소스 준비 기록](../OPEN-SOURCE.md)을 참고하세요.

## 두 가지 출력 모드

| 모드                   | 출력 내용                                                    | 용도                                                   |
| ---------------------- | ------------------------------------------------------------ | ------------------------------------------------------ |
| 원본 내용 유지         | 텍스트, 글꼴, 벡터를 유지하고 경로·구조·지원 이미지를 최적화 | 보고서, 계약서, 도면, 텍스트 선택과 확대가 필요한 문서 |
| 페이지를 이미지로 변환 | 각 페이지를 하나의 독립 이미지로 렌더링한 뒤 PDF 재구성      | 텍스트 선택과 벡터가 필요 없는 이미지 페이지           |

출력 모드와 품질 설정은 별도로 선택합니다.

| 품질               | 원본 내용 유지                                                     | 페이지 이미지                                  |
| ------------------ | ------------------------------------------------------------------ | ---------------------------------------------- |
| 원본 품질 / 고화질 | 원본 이미지 해상도, JPEG 95, 경로 정밀도 최적화와 반복 윤곽 재사용 | 300 DPI, 렌더링한 픽셀을 Flate로 무손실 인코딩 |
| 균형               | 경로 최적화, JPEG 85, 최대 200 DPI                                 | 너비 1400 px, JPEG 품질 72                     |
| 더 작게            | 경로 최적화, JPEG 50, 최대 100 DPI                                 | 너비 1000 px, JPEG 품질 60                     |

원본 품질은 이미지 크기를 유지하지만 픽셀은 바뀔 수 있으므로 **엄밀한 무손실이 아닙니다**. 벡터 좌표 반올림은 알려진 변환에서 축별 오차를 약 0.0005포인트 이내로 제한합니다. 텍스트 위치와 색상 값은 반올림하지 않지만 아주 가는 경계는 변할 수 있습니다. 조건에 맞는 단색 채움의 닫힌 윤곽은 벡터 Form으로 재사용합니다.

고화질는 렌더링된 픽셀을 무손실 인코딩하는 설정이며, 텍스트와 벡터는 이미 래스터화된 상태입니다. 이미지 모드는 선택 가능한 텍스트, 벡터 경로, 링크, 책갈피를 유지하지 않습니다. 데스크톱에서는 렌더링 가능한 표시 양식과 주석을 이미지에 합칩니다. 숨겨진 필드와 주석 팝업 텍스트는 유지하지 않으며, 내장 첨부 파일 데이터는 보존합니다.

원본 유지 모드는 검증에 통과하고 원본보다 작은 PDF만 내보냅니다. 후보는 전체 파일 크기로 비교하고 커지면 이전 방식으로 돌아갑니다. 이미지 모드는 파일이 커져도 선택한 형식으로 출력하며 증가량을 표시합니다. 원본은 읽기 전용이고 기존 출력은 덮어쓰지 않습니다. 처리 중 입력이 바뀌면 출력 확정을 막습니다.

지원 입력에서는 JPEG, 정확한 Flate, Indexed 팔레트, 1비트, CCITT G4 후보를 비교합니다. 기존 JPEG 계수, 자원 공유, 제한된 CID 글꼴 서브셋, 파일 배치도 최적화합니다. 데스크톱은 예산과 품질 기준 안에서 직접 렌더링과 2배 렌더링 후 축소 결과를 비교할 수 있습니다. [구현 및 검증 범위](../VALIDATION.md)를 참고하세요.

## 데스크톱 기능

- PDF 여러 개를 끌어오거나 선택하고, 중복을 제거한 뒤 순서대로 처리합니다.
- 두 모드와 세 품질 설정, 진행 단계와 완료 파일 수를 표시합니다.
- 전후 크기, 절감·증가율, 처리 설명과 파일별 오류를 표시합니다.
- 원본과 결과를 나란히 미리 보고 페이지 이동과 확대를 지원합니다. 미리보기 자산도 모두 로컬입니다.
- 출력 폴더 선택, 이름 자동 번호 부여, 취소, 재시도, 결과 및 폴더 열기를 지원합니다.
- 모드·품질·폴더·테마를 기억합니다. 파일 대기열은 현재 세션에만 유지됩니다.
- 테마 버튼 오른쪽 언어 아이콘에서 영어, 중국어 간체, 프랑스어, 스페인어, 한국어, 일본어, 독일어, 포르투갈어를 선택합니다. 처음에는 지원되는 시스템 언어 또는 영어를 사용하고 이후 선택을 기억합니다.
- 언어를 바꿔도 실행 중 작업은 계속됩니다. 인터페이스, 단계, 오류, 미리보기를 갱신하며 파일명·경로·알 수 없는 하위 오류는 그대로 표시합니다.
- 투명 로고와 빨간 강조색, 차콜 배경의 다크 테마를 사용합니다.

## 개발 실행

Node.js 22.13+, `rust-toolchain.toml`로 고정한 Rust 1.92.0, 플랫폼 C 컴파일러가 필요합니다. macOS는 Xcode Command Line Tools, Windows는 MSVC C++ Build Tools와 WebView2를 사용합니다. MozJPEG는 worker에 컴파일됩니다. 선택적 어셈블리 SIMD는 꺼져 있어 NASM과 CMake는 필요하지 않습니다.

모든 명령은 저장소 루트에서 실행합니다.

```sh
npm ci
npm run tauri -- dev
```

`npm run preview`는 프런트엔드만 실행합니다. 네이티브 파일 기능은 `tauri dev`를 사용하세요. 스크립트가 현재 아키텍처용 worker, 로컬 PDF.js 자산과 라이선스를 준비합니다. `private: true`는 실수로 npm에 게시하는 것을 막는 설정이며 오픈 소스 라이선스와 무관합니다.

## 검사와 패키징

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

# macOS 앱
npm run tauri -- build --bundles app
# macOS 앱과 간단한 DMG
npm run package:mac
# Windows에서 실행하는 EXE 설치 파일 빌드
npm run tauri -- build --bundles nsis
```

패키지는 호스트 아키텍처 기준으로 `target/release/bundle/`에 생성됩니다. `--target`을 지정할 때는 worker도 맞도록 `CARGO_BUILD_TARGET`을 함께 설정하세요. Windows 설치 파일에는 WebView2 오프라인 설치 프로그램이 포함됩니다.

CI는 Windows x64, macOS arm64와 Intel을 대상으로 구성되어 있습니다. 로컬 검증은 macOS arm64에서 진행했으며 Windows/Intel 설치 회귀 검증은 남아 있습니다. macOS는 로컬 시험용 ad-hoc 서명을 사용합니다. 정식 Windows 서명, macOS Developer ID 서명과 공증은 아직 구성하지 않았습니다. 배포 서명에는 `APPLE_SIGNING_IDENTITY`와 [Tauri 공식 문서](https://v2.tauri.app/distribute/sign/macos/)를 참고하세요.

## CLI와 엔진

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode`는 기본값 `preserve` 또는 `raster`입니다. `--preset`은 `lossless`, `fidelity`, `balanced`, `small`이며 지원 설정에는 `--quality 85 --dpi 200`을 사용할 수 있습니다. CLI의 `--compact-paths`, `--reuse-paths`는 기본적으로 꺼져 있고, 데스크톱 원본 유지의 세 설정에서는 모두 켜져 있습니다.

`lossless`는 이미지를 재양자화하거나 축소하지 않고 두 벡터 옵션도 무시합니다. 정확한 인코딩, JPEG 계수 최적화, 동등한 자원 공유는 가능합니다. `fidelity`는 이미지 크기를 유지하고 기본 JPEG 95를 사용하며 목표 DPI를 받지 않습니다. 이미지 모드의 고화질는 픽셀 무손실 인코딩입니다. Ctrl+C로 취소를 요청합니다. 데스크톱은 별도 worker를 사용하며 취소 시 렌더링 또는 worker를 종료합니다. 페이지 렌더링과 worker 인코딩은 각각 120초 제한입니다.

핵심 API는 `squeeze_core::inspect_file`, `squeeze_core::audit_file`, `squeeze_core::compress`입니다. `squeeze audit input.pdf`는 읽기 전용 크기 분석을 출력합니다. 압축 JSON은 전후 크기, 단계, 이미지 후보, 시간, 캐시, 쓰기 방식 선택을 기록합니다. 스트림·후보 비용은 전체 PDF 크기와 다르며 모두 삭제 가능한 중복을 의미하지 않습니다.

## 지원 범위와 제한

- 고정 PDF 크기 제한은 없고 256 MiB 초과 파일도 가용 메모리에 따라 처리합니다. 압축 해제 스트림은 최대 64 MiB, 이미지는 2400만 픽셀이며 순차 처리합니다.
- RGB/Gray와 제한된 ICCBased, Indexed, G4, 원시 색상 성분 입력을 최적화합니다. 알 수 없는 인코딩·매핑은 보수적으로 유지합니다. 일부 CMYK/YCCK JPEG는 픽셀 재인코딩 없이 계수 최적화가 가능합니다. 필요한 경우 크기와 색상/Decode/마스크 의미를 보호하며 모든 코덱을 지원하지는 않습니다.
- 그리기 배율을 알 수 없거나 분석이 불완전하면 해당 이미지 축소를 끕니다. 원본 품질은 항상 원본 이미지 크기를 유지합니다.
- 원본 유지 모드는 양식·주석·첨부 파일을 보존하고 외형 자원과 공유 이미지를 보호합니다. 무관한 일반 페이지 이미지는 계속 최적화합니다. 양식이나 외형 주석이 있으면 두 손실 벡터 최적화를 보수적으로 건너뜁니다. 암호화, 디지털 서명, PDF/A, XFA, 포트폴리오 PDF는 건너뜁니다.
- 데스크톱 이미지 모드는 렌더링 가능한 양식·주석, 일반 링크, 빈 양식 컨테이너, 첨부 파일을 처리합니다. 그릴 수 있는 외형이 없거나 주석이 손상되면 페이지 오류를 냅니다. CLI hayro는 그려야 하는 양식·주석과 `BrotliDecode` 스트림을 거부하므로 데스크톱 PDF.js를 사용하세요.
- 회전, 보이는 자르기 영역, 물리적 페이지 크기를 처리합니다. 비표준 글꼴이 내장되어 있지 않으면 변환을 거부합니다. 복잡한 페이지는 육안 검토가 필요하며 hayro가 모든 상용 PDF 호환성을 보장하지는 않습니다.
- 파서가 상호 참조를 복구할 수 있으며 모든 비정상 PDF를 엄격하게 거부하지는 않습니다. 개별 스트림·픽셀·분석 제한은 있지만 모든 할당을 포괄하는 메모리 예산은 없습니다.
- 정상 취소는 종료된 worker의 임시 디렉터리를 정리합니다. 강제 종료나 정전 후 시작 시 복구 정리는 아직 없습니다.
- OCR, PDF 편집, 목표 크기 보장, 상업용 활성화 시스템은 포함하지 않습니다.

## 벤치마크와 기술 기록

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

큰 샘플은 취소와 대용량 파일 시험용 약 138 MB, 24페이지 PDF를 추가 생성합니다. 합성 샘플의 압축률로 실제 문서를 예측할 수 없습니다. Poppler, pypdf, Pillow, NumPy는 개발 중 독립 검증 도구이며 실행 의존성이 아닙니다.

상세 기술 기록은 현재 중국어입니다. [검증](../VALIDATION.md), [계획](../../PLAN.md). Jpegli와 Brotli 출력은 개발 실험이며 제품 의존성이나 출력 옵션이 아닙니다.

## 외부 구성 요소

잠금 파일로 의존성을 고정합니다. `npm run notices`는 라이선스 본문, hayro NOTICE, PDF.js 글꼴/WASM 고지, QuickJS/Mozilla MIT 고지, 수정하지 않은 MPL 소스, Liberation 1.07.4 소스와 실제 배포 글꼴의 편집 가능한 TTX 소스를 모읍니다. `third-party/`에 포함하며 목록은 빌드 의존성도 보수적으로 포함합니다. [소스와 재구성 안내](../../licenses/SOURCE-CODE.md), [라이선스 기록](../LICENSING-0.1.22.md)을 참고하세요.

`npm run test:licenses`는 Windows/macOS CI에서 라이선스, 자산·소스 해시, MPL 완전성을 검사합니다. PDF.js나 자산 변경 시 라이선스와 대응 소스를 다시 검토해야 합니다. `scripts/make-font-sources.py`의 FontTools 4.66.1은 개발 도구입니다. Liberation은 글꼴 예외가 있는 GPL을 유지하며 다른 구성 요소도 원래 라이선스를 유지합니다.

PDF 입출력은 [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/), 렌더링은 [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) 또는 [hayro](https://github.com/LaurenzV/hayro), 디코딩은 image, JPEG는 MozJPEG, G4는 MIT `fax 0.3.0`, 제한된 CID TrueType 서브셋은 MIT/Apache-2.0 `subsetter 0.2.6`을 사용합니다. 개발 시험 글꼴은 배포하지 않습니다. 자체 코드는 0.1.23부터 Apache-2.0이며 외부 자료의 라이선스를 변경하지 않습니다.
