# 参与 Squeeze PDF

欢迎提交问题和 Pull Request。项目自有代码采用 Apache-2.0；有意提交并纳入项目的贡献按同一许可证提供，除非提交时明确说明其他安排。请确认有权提交相关代码，保留第三方代码的原版权、来源和许可；不得将第三方材料直接改标为项目许可证。

## 开发环境

需要 Node.js 22.13+、Rust 1.92.0 和平台 C 编译器。macOS 安装 Xcode Command Line Tools；Windows 安装 MSVC C++ Build Tools 和 WebView2。完整构建及打包命令见 [README](README.md)。

```sh
npm ci
npm run tauri -- dev
```

`npm ci` 会准备固定版本的 PDF.js 资源。CLI、桌面 worker 和第三方材料由构建脚本生成。`package.json` 的 `private: true` 用于防止误发布到 npm，不影响项目采用开源许可证。

## 验证更改

引擎更改应覆盖相关模式、档位、回退和输入保护，并比较完整 PDF；不要仅凭中间编码流判断最终体积。涉及画质或内容的改动需要独立阅读器验证，报告清楚实际检查的范围。保留模式和图片模式各自的内容行为见 README。

```sh
node scripts/prepare-worker.mjs
npm run notices
npm run test:licenses
npm run check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

前端渲染或语言更改另运行 `npm run test:render` 和 `npm run test:i18n`。CI 在 Windows x64、macOS arm64 / Intel 上执行检查和打包；本机测试结果不能代替其他平台的实机验收。

自动发布或安装包收集逻辑更改需运行 `node --test scripts/test-release.mjs`。版本标签发布步骤见 [发布指南](docs/RELEASING.md)。

部分独立画质验证脚本需要 Python 和额外的 PDF / 图像工具。`desktop-raster-baseline.mjs` 默认使用 PATH 中的 `python3`，可用 `SQUEEZE_QA_PYTHON` 指定已装好验证依赖的解释器；这些不是产品运行依赖。

## Logo 与界面颜色

原始 Logo 位于 `public/logo.png`，窗口和网页图标、Windows / macOS 应用图标均由它生成。保留原图透明通道；替换后执行以下命令，包含默认图标生成器未覆盖的 64 px 资源：

```sh
npm run tauri -- icon public/logo.png
npm run tauri -- icon public/logo.png --png 64 --output tmp/icon-64
cp tmp/icon-64/64x64.png src-tauri/icons/64x64.png
cp src-tauri/icons/32x32.png public/favicon.png
```

图标生成器同时更新仓库已有的移动端尺寸，产品支持的平台仍为 Windows / macOS。

浅色和深色主题的品牌点缀统一放在 `src/styles.css` 的颜色变量中。红色按钮使用白色文字，深色主题保持炭灰背景；修改时检查文字、控件边界和键盘焦点的对比度。

## 依赖、样本和提交

- 中文 README 位于仓库根目录，其余七种语言位于 `docs/readme/README.{en,fr,es,ko,ja,de,pt}.md`。功能、档位、构建命令、支持边界或许可发生变化时，同步相关语言并保持八语言切换链接有效；技术验证报告可保留原始中文，链接处标明语言。
- 升级 PDF.js 或改变字体 / WASM 时，重新检查对应许可、源码和 `licenses/pdfjs-assets.json`。字体源归档的生成工具及固定要求见 `scripts/make-font-sources.py`；它是开发工具。
- 增加依赖时检查代码与资产的实际许可，并通过 `npm run notices` 保持分发材料完整。单独分发 CLI / worker 也要附带完整的生成 `third-party/` 目录。
- 优先使用 Rust examples 生成可复现样本。不要提交用户原始 PDF、密钥、签名证书或含个人文档内容的日志；需要复现问题时提供有权公开的最小样本。
- Pull Request 说明具体问题、最终行为、测试结果和兼容性边界。生成目录 `target/`、`dist/`、`third-party/` 及本地 `tmp/` 不纳入 Git。

公开提交中的测试记录使用自建样本或匿名编号，不包含用户真实文件名、本地路径、原件指纹或文档内容。
