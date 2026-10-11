# Squeeze PDF

[English](README.en.md) · [简体中文](../../README.md) · [Français](README.fr.md) · [Español](README.es.md) · [한국어](README.ko.md) · **日本語** · [Deutsch](README.de.md) · [Português](README.pt.md)

Windows・macOS 向けのオープンソース、完全オフライン PDF 圧縮アプリです。現在は MVP 段階です。画面は Tauri 2 と React、圧縮戦略・画像処理・ページ再構築・ファイル保護は Rust で実装しています。PDF の読み書きは lopdf、デスクトップの描画はローカルの PDF.js、CLI の既定の描画は hayro を使用します。JPEG は Rust 経由で静的リンクした MozJPEG で符号化します。実行時に qpdf、Ghostscript、Python は不要です。

Windows x64、Apple silicon Mac、Intel Mac のインストーラーは [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases) からダウンロードできます。アプリのバージョンと一致する `v` タグをプッシュすると、3 プラットフォームのビルド成功後にインストーラーとチェックサムを自動公開します。[リリースガイド](../RELEASING.md)（中国語・英語）を参照してください。

0.1.25 以降の配布版は起動約 5 秒後と実行中の 24 時間ごとに GitHub の安定版更新を確認します。上部の更新アイコンから手動確認もできます。インストールと再起動はユーザーが選択し、ファイル処理が完了してから行います。オフラインでの確認失敗は圧縮を中断しません。通信はバージョン確認と更新のダウンロードに限られ、PDF はアップロードしません。0.1.24 以前のバージョンは最初に一度手動でアップグレードしてください。

## ライセンス

プロジェクト独自のコードは [Apache-2.0](../../LICENSE) です。著作権表示は [NOTICE](../../NOTICE) を参照してください。ライセンス・表示の保持、変更箇所の明示などの条件に従えば、商用利用やクローズドソースの派生製品も可能です。特許許諾の範囲と条件はライセンス本文に従います。

外部ライブラリ、フォント、アセットはそれぞれのライセンスを維持します。配布物には `third-party/PROJECT-LICENSE.txt`、`PROJECT-NOTICE.txt`、外部の表示と必要なソースを同梱します。[貢献ガイド](../../CONTRIBUTING.md)と[オープンソース化の記録](../OPEN-SOURCE.md)を参照してください。

## 2 つの出力モード

| モード             | 出力内容                                                       | 用途                                                   |
| ------------------ | -------------------------------------------------------------- | ------------------------------------------------------ |
| 元の内容を保持     | 文字・フォント・ベクターを保持し、パス・構造・対応画像を最適化 | 報告書、契約書、図面、文字選択や拡大が必要な文書       |
| ページ全体を画像化 | 各ページを独立した 1 枚の画像として描画し、PDF を再構築        | 文字選択やベクターが不要で、画像ページに統一したい文書 |

出力モードと画質は別々に選択します。

| 画質               | 元の内容を保持                                          | ページ画像                                     |
| ------------------ | ------------------------------------------------------- | ---------------------------------------------- |
| 高画質（両モード） | 元の画像解像度、JPEG 95、パス精度の最適化と輪郭の再利用 | 300 DPI、描画済みピクセルを Flate で可逆符号化 |
| バランス           | パス最適化、JPEG 85、最大 200 DPI                       | 幅 1400 px、JPEG 品質 72                       |
| サイズ優先         | パス最適化、JPEG 50、最大 100 DPI                       | 幅 1000 px、JPEG 品質 60                       |

高画質は画像の寸法を保ちますが、ピクセルは変わる場合があり、**厳密な可逆圧縮ではありません**。ベクター座標の丸めは既知の変換で軸ごとの誤差を約 0.0005 ポイント以内に制限します。文字位置や色は丸めませんが、非常に細い境界は変化することがあります。条件を満たす単色塗りの閉じた輪郭は、ベクター Form として再利用します。

高画質は描画済みピクセルの可逆符号化を意味し、文字とベクターは既にラスタライズされています。画像モードでは選択可能な文字、ベクターパス、リンク、しおりを保持しません。デスクトップでは描画可能な表示フォームや注釈を画像に統合します。非表示フィールドと注釈ポップアップの文章は保持しません。埋め込み添付ファイルのデータは保持します。

保持モードは検証に成功し、元より小さい PDF だけを書き出します。候補は PDF 全体のサイズで比較し、大きくなれば以前の方式に戻します。画像モードは容量が増えても選択形式で出力し、増加量を表示します。原本は読み取り専用で、既存の出力を上書きしません。処理中に入力が変わると出力の確定を防ぎます。

対応する入力では JPEG、正確な Flate、Indexed パレット、1 ビット、CCITT G4 を比較します。既存 JPEG の係数、リソース共有、制限付き CID フォントのサブセット、ファイル配置も最適化します。デスクトップでは予算と画質基準の範囲で、直接描画と 2 倍描画後の縮小を比較できます。[実装と検証の範囲](../VALIDATION.md)を参照してください。

## デスクトップ機能

- 複数 PDF のドラッグ・選択、重複除去、順次処理。
- 2 モードと 3 画質、処理段階と完了ファイル数の表示。
- 前後のサイズ、削減・増加率、処理説明、ファイル行でのエラー表示。
- 原本と結果の並列プレビュー、ページ移動とズーム。表示アセットもすべてローカル。
- 出力先選択、自動連番、キャンセル、再試行、結果とフォルダーを開く操作。
- モード・画質・フォルダー・テーマを記憶。ファイル一覧は現在のセッションのみ。
- テーマ切り替え右の言語アイコンから英語、中国語簡体字、フランス語、スペイン語、韓国語、日本語、ドイツ語、ポルトガル語を選択。初回は対応するシステム言語、なければ英語を使用し、以降は選択を記憶。
- 言語変更は実行中の処理に影響しません。画面、段階、エラー、プレビューを更新します。ファイル名・パス・未知の低層エラーは元の値を保持。
- 透明ロゴと赤いアクセント、チャコール背景のダークテーマ。

## 開発実行

Node.js 22.13+、`rust-toolchain.toml` 固定の Rust 1.92.0、プラットフォーム用 C コンパイラーが必要です。macOS は Xcode Command Line Tools、Windows は MSVC C++ Build Tools と WebView2 を使用します。MozJPEG は worker に組み込みます。任意のアセンブリ SIMD は無効なので NASM・CMake は不要です。

すべてリポジトリのルートで実行します。

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` はフロントエンドのみを起動します。ネイティブのファイル操作には `tauri dev` を使います。スクリプトが現在のアーキテクチャ用 worker、ローカル PDF.js、ライセンスを準備します。`private: true` は npm への誤公開を防ぐ設定で、オープンソースの許諾には影響しません。

## 検査とパッケージ

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

# macOS アプリ
npm run tauri -- build --bundles app
# macOS アプリと簡易 DMG
npm run package:mac
# Windows で実行する EXE インストーラーのビルド
npm run tauri -- build --bundles nsis
```

ホストのアーキテクチャ向けに `target/release/bundle/` へ生成します。`--target` 使用時は worker を合わせるため `CARGO_BUILD_TARGET` も設定してください。Windows インストーラーは WebView2 のオフラインインストーラーを含みます。

CI は Windows x64、macOS arm64・Intel 向けに設定済みです。ローカル検証は macOS arm64 で行っており、Windows・Intel のインストール回帰検証は未実施です。macOS はローカル試用向けの ad-hoc 署名です。正式な Windows 署名、macOS Developer ID 署名・公証は未設定です。公開用の署名は `APPLE_SIGNING_IDENTITY` と [Tauri 公式文書](https://v2.tauri.app/distribute/sign/macos/)を参照してください。

## CLI とエンジン

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode` は既定の `preserve` または `raster`。画質は `lossless`、`fidelity`、`balanced`、`small` で、対応設定には `--quality 85 --dpi 200` を指定できます。CLI の `--compact-paths` と `--reuse-paths` は既定で無効、デスクトップの保持モード 3 画質では有効です。

`lossless` は画像の再量子化・縮小を行わず、2 つのベクターオプションも無視します。正確な符号化、JPEG 係数最適化、等価なリソース共有は可能です。`fidelity` は画像寸法を維持し、既定 JPEG 95、目標 DPI は受け付けません。画像モードの高画質はピクセル可逆符号化です。Ctrl+C でキャンセルを要求します。デスクトップは別 worker を使用し、キャンセル時は描画または worker を終了します。ページ描画と worker 符号化はそれぞれ 120 秒制限です。

主要 API は `squeeze_core::inspect_file`、`squeeze_core::audit_file`、`squeeze_core::compress`。`squeeze audit input.pdf` は読み取り専用のサイズ分類を出力します。圧縮 JSON は前後サイズ、段階、画像候補、時間、キャッシュ、書き込み方式を記録します。ストリーム・候補の費用は PDF 全体のサイズとは異なり、すべてが削除可能な冗長データではありません。

## 対応範囲と制限

- PDF の固定サイズ上限はなく、256 MiB 超も空きメモリ次第で処理します。展開ストリームは最大 64 MiB、画像は 2400 万ピクセル、順次処理です。
- RGB/Gray と制限付き ICCBased、Indexed、G4、生の色成分入力を最適化します。不明な符号化・マッピングは保守的に保持。一部 CMYK/YCCK JPEG はピクセルを再符号化せず係数最適化できます。必要に応じて寸法と色/Decode/マスクの意味を保護し、すべてのコーデックには対応しません。
- 描画倍率が不明、または分析不完全なら該当画像の縮小を無効化します。高画質は常に元の画像寸法を維持します。
- 保持モードはフォーム・注釈・添付ファイルを保持し、外観リソースと共有画像を保護します。無関係な通常画像は最適化可能。フォームや外観注釈がある場合、2 つの非可逆ベクター最適化は保守的に省略します。暗号化、電子署名、PDF/A、XFA、ポートフォリオ PDF はスキップします。
- デスクトップ画像モードは描画可能なフォーム・注釈、通常リンク、空フォームコンテナー、添付ファイルを処理。描画外観の欠落や壊れた注釈はページエラーにします。CLI hayro は描画が必要なフォーム・注釈と `BrotliDecode` ストリームを拒否するため、デスクトップ PDF.js を使ってください。
- 回転、可視切り抜き領域、物理ページ寸法に対応。非標準フォントが未埋め込みなら変換を拒否します。複雑なページは目視確認が必要で、hayro はあらゆる商用 PDF との互換性を保証しません。
- パーサーが相互参照を復旧する場合があり、すべての不正 PDF を厳密に拒否するわけではありません。個別制限はありますが、全メモリ確保をカバーする予算は未実装です。
- 通常のキャンセルは終了した worker の一時ディレクトリを清掃。強制終了・停電後の起動時復旧は未実装です。
- OCR、PDF 編集、目標サイズ保証、商用アクティベーション機構はありません。

## ベンチマークと技術記録

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

大容量サンプルはキャンセル・大きなファイルの試験向けに約 138 MB・24 ページの PDF を追加生成します。合成サンプルの圧縮率は実際の文書を予測しません。Poppler、pypdf、Pillow、NumPy は開発用独立検証ツールで、実行依存ではありません。

詳細記録は現在中国語です。[検証](../VALIDATION.md)と[計画](../../PLAN.md)を参照してください。Jpegli と Brotli 出力は実験であり、製品依存・出力オプションではありません。

## 外部コンポーネント

ロックファイルで依存関係を固定します。`npm run notices` はライセンス本文、hayro NOTICE、PDF.js フォント/WASM 表示、QuickJS/Mozilla MIT 表示、未変更 MPL ソース、Liberation 1.07.4 ソースと実配布フォントの編集可能 TTX ソースを収集します。`third-party/` に同梱し、一覧には保守的にビルド依存も含みます。[ソースと再構築の案内](../../licenses/SOURCE-CODE.md)、[ライセンス記録](../LICENSING-0.1.22.md)を参照してください。

`npm run test:licenses` は Windows/macOS CI で本文・アセット/ソースのハッシュ・MPL 完全性を検査します。PDF.js や資産を変更するときはライセンスと対応ソースを再確認してください。`scripts/make-font-sources.py` の FontTools 4.66.1 は開発ツールです。Liberation はフォント例外付き GPL、ほかの構成要素も元のライセンスを維持します。

PDF 入出力は [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/)、描画は [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) または [hayro](https://github.com/LaurenzV/hayro)、画像デコードは image、JPEG は MozJPEG、G4 は MIT の `fax 0.3.0`、制限付き CID TrueType サブセットは MIT/Apache-2.0 の `subsetter 0.2.6` を使用します。開発試験フォントは配布しません。独自コードは 0.1.23 から Apache-2.0 で、外部資料のライセンスは変更しません。
