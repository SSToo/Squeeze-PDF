# Squeeze PDF

面向 Windows 和 macOS 的离线 PDF 压缩 MVP。Tauri 2 + React 界面，Rust 实现压缩策略、图片处理、页面重建与文件保护；PDF 读写使用 lopdf，桌面整页渲染使用本地 PDF.js，CLI 默认渲染使用 hayro，图片解码使用 image，JPEG 编码通过 Rust 封装调用静态链接的 MozJPEG。运行时无需 qpdf、Ghostscript 或 Python。

## 两种输出模式

| 模式         | 输出内容                                                   | 适用情况                                           |
| ------------ | ---------------------------------------------------------- | -------------------------------------------------- |
| 保留原始内容 | 原有文字、字体和矢量类型保留，优化路径、结构及受支持的图片 | 合同、报告、图纸、需要文字复制或放大查看的文档     |
| 整页转图片   | 每页渲染为一张独立图片，再组成 PDF                         | 需要统一图片页面、愿意放弃文字选择和矢量内容的文档 |

输出模式与画质档位独立：

| 档位        | 保留原始内容                                      | 整页转图片                         |
| ----------- | ------------------------------------------------- | ---------------------------------- |
| 保真 / 清晰 | 保留原图片分辨率，JPEG 95，路径精度与重复轮廓优化 | 300 DPI，页面像素用 Flate 无损编码 |
| 均衡        | 路径精度与重复轮廓优化，JPEG 85，最高 200 DPI     | 宽 1400 px，JPEG 质量 72           |
| 更小        | 路径精度与重复轮廓优化，JPEG 50，最高 100 DPI     | 宽 1000 px，JPEG 质量 60           |

“清晰”只指栅格图片像素无损编码，文字与矢量已经栅格化。图片模式不保留可选择文字、矢量路径、链接和书签。桌面模式将可渲染的可见表单与批注合并到图片，隐藏字段不会显示，批注弹窗文字不保留；嵌入附件保留原文件数据。

保留原始内容模式只在候选 PDF 更小且校验通过时导出。图片模式会导出用户选择的页面格式，即使体积变大，并明确显示增加量。原件只读，已有输出不覆盖；处理中原件发生变化会阻止提交。

桌面保留模式的三个档位均会精简独立页面流与可证明调用尺度的嵌套 Form 中的路径小数，按所有已知变换限制每轴坐标误差至约 0.0005 点。Form 分析累计 Matrix、cm 和 UserUnit；内容数组只用于连续状态分析，字形、图案、软遮罩、外观、循环及未知调用环境保持保守。文字定位和颜色参数不参与舍入，极细边缘仍可能变化。保真保留图片原像素尺寸，以 JPEG 95 和完整色度采样重编码受支持图片；仅采用更小的图片候选，图片像素可能变化，保真不表示整个 PDF 严格无损。同时将平移后相同的闭合、纯色填充轮廓复用为矢量 Form，精确平移坐标，不转换成图片。复用覆盖独立页面流及带自身资源的嵌套 Form，保留原 Form 的 Matrix、BBox 和透明组；核对所有已知调用点的继承颜色空间，未知调用环境仅处理明确设置纯色的路径。相交轮廓保持一组以保留字形空洞；图案 / 渐变、裁剪、开放路径和需要保真策略的交互文档不参与复用，只有计算收益为正才提交；后续流建立缓存后，会在预算内重新评估此前未采用的候选。新增共享矢量资源按对象号固定写入顺序，避免随机哈希顺序造成输出体积波动。其他结构优化包括：合并字典及内容相同的数据流，清除无法从文档引用到的对象。JPEG 使用 MozJPEG 的 Trellis、渐进扫描优化、去振铃与优化 Huffman；质量 90 以上采用完整色度采样。桌面图片模式使用本地 PDF.js 按最终尺寸直接渲染，Rust 逐页编码和重建 PDF，避免 JPEG 中间文件及二次缩放。CLI 的默认独立渲染仍采用 hayro 和预算内的 2 倍超采样。受支持的原图与整页图片比较原编码（保留模式）、JPEG、无损 Flate 五种行预测及固定 / 自适应策略、精确 Indexed 调色板、真实黑白 1 位打包与 CCITT G4 无损候选；成本包含字典、预测参数及调色板。自动灰度仅合并本来就完全相同的 RGB 通道，不对灰色边缘二值化。透明遮罩仅作 8 位无损 Flate 重编码，保持透明度样本、尺寸及颜色空间。原有常规 RGB / Gray JPEG 另尝试系数域 Huffman 与顺序 / 渐进扫描优化，量化表、系数、采样和标记保持；携带 ICC 的 JPEG 可走此路径，继续禁止像素重编码。每份文件可缓存最多 32 MB / 256 份相同像素的 JPEG 编码，完整区分尺寸、通道与质量；两种模式共享策略，输出字节不变。高色数 RGB 快速跳过 Flate / 调色板；纯色字节流避免重复预测尝试。小文件沿用对象流 100 / 等级 6，优化后至少 1000 个对象的文档使用分组 1000 / 等级 9；写入通过缓冲器完成，提交前检查刷新与往返解析。

## 桌面功能

- 拖入或选择多个 PDF，去重后串行处理。
- 输出模式与三档画质选择，实时阶段与文件完成数。
- 前后大小、节省或增加比例、处理说明；失败原因直接显示在文件行中。
- 原件与结果双栏预览，页码切换与缩放；预览资源全部本地打包。
- 输出目录选择、同名自动编号、取消、重试、打开结果和所在目录。
- 记住模式、档位、目录和主题；文件队列仅保存在本次会话。
- 明暗切换右侧的语言图标可弹出菜单，支持 English、中文、Français、Español、한국어、日本語、Deutsch、Português。首次跟随受支持的系统语言，否则使用英文；后续记住选择。
- 深色主题使用炭灰背景、灰色面板和明亮的操作按钮，插画及选中状态一起适配。
- 界面、任务阶段、错误、处理说明及预览控件随语言立即更新，切换不影响正在运行的任务。文件名、路径和底层未知技术错误保留原值。中文目前为简体。

## 开发运行

MozJPEG 由构建脚本编译进 worker，发布包无需外部 DLL 或工具。当前关闭可选汇编 SIMD，因此构建不依赖 NASM 或 CMake；需要平台 C 编译器。

需要 Node.js 22.13+、Rust 1.92，以及对应平台的 Tauri 编译环境。仓库的 `rust-toolchain.toml` 固定 Rust 1.92.0。macOS 需要 Xcode Command Line Tools；Windows 使用 MSVC、C++ Build Tools 和 WebView2。

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` 只启动前端服务；桌面文件功能通过 `tauri dev` 使用。开发与发布脚本会准备当前架构的 worker、PDF.js 本地资源和第三方许可通知。

## 编译与检查

```sh
cargo test -p squeeze-core -p squeeze-cli --locked

node scripts/prepare-worker.mjs
npm run notices
npm run check
npm run test:render
npm run test:i18n
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# macOS 应用；额外添加 ,dmg 可调用 Tauri 的 DMG 打包器
npm run tauri -- build --bundles app

# macOS 应用及简洁 DMG，不依赖 Finder 自动排版
npm run package:mac

# Windows EXE 安装包，在 Windows 上运行
npm run tauri -- build --bundles nsis
```

应用包位于 `target/release/bundle/`。默认生成当前机器架构的包。传入 `--target` 时，同时设置 `CARGO_BUILD_TARGET`，确保 worker 架构一致。Windows 安装包配置为包含 WebView2 离线安装程序。

已配置 Windows x64、macOS arm64 与 Intel 的原生 CI 构建；本地验证环境为 macOS arm64。Windows 和 Intel 尚未执行安装回归。macOS 默认使用 ad-hoc 签名供本地试用；公开分发的 Windows 签名、macOS Developer ID 签名和公证尚未配置。正式发布时通过 `APPLE_SIGNING_IDENTITY` 设置发布身份，参考 [Tauri 官方签名说明](https://v2.tauri.app/distribute/sign/macos/)。

## CLI 与引擎

```sh
cargo run -p squeeze-cli -- inspect "原件.pdf"
cargo run -p squeeze-cli -- compress "原件.pdf" -o "保真结果.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "原件.pdf" -o "结果.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "原件.pdf" -o "图片版.pdf" --output-mode raster --preset small --json
```

`--output-mode` 支持 `preserve`（默认）与 `raster`；`--preset` 支持 `lossless`、`fidelity`、`balanced`、`small`，可加 `--quality 85 --dpi 200`。保留模式可加 `--compact-paths` 开启路径精度优化，`--reuse-paths` 开启重复矢量轮廓复用；CLI 默认关闭这两项。`lossless` 不进行有损图片重编码和降采样，允许精确预测／调色板／G4、JPEG 系数域优化及等价资源共享，并忽略这两项矢量优化。`fidelity` 对应桌面保真：图片不降采样，默认 JPEG 95；可与两项矢量优化一起使用，不接受目标 DPI。桌面三个保留档位都启用两项矢量优化；图片模式的清晰档仍采用像素无损编码。Ctrl+C 请求取消；桌面端使用独立 worker，取消会终止渲染或 worker。页面渲染和 worker 编码分别限制为 120 秒。

核心入口为 `squeeze_core::inspect_file`、`squeeze_core::audit_file` 与 `squeeze_core::compress`。`squeeze audit input.pdf` 输出只读体积分类；压缩 JSON 增加 `size_before` / `size_after`、逐阶段流字节与耗时、每张图片的 `encoding` / `candidates` / `elapsed_ms` / `jpeg_cached` 及写入配置。流分类只计一次，文件语法与字典开销不等于可删冗余；图片候选成本包含相同的间接对象边界，不是实际整份 PDF 大小。`inspect` 预检，`usage` 计算图片绘制尺寸，`images` 优化受支持图片，`jpeg` 编码 JPEG，`jpeg_lossless` 在系数域优化已有 JPEG，`image_encoding` 比较精确图片编码，`audit` 记录体积与阶段，`resources` 按所有者清理已证明未使用的 XObject，并比较样本及渲染语义合并同像素异编码图片；标记图片保留独立身份，颜色 / Decode / 遮罩等字典须相同。`structure` 精简内容数字并合并重复资源，`lossless` 重压缩流，`page_image` 选择整页图像编码，`raster` 提供 CLI 默认渲染，`raster_input` 校验桌面渲染计划与逐页像素并重建图片页面，`form_precision` 证明所有 Form 调用的物理尺度上界，`paths` 优化矢量路径精度，`reusable_paths` 精确复用闭合矢量轮廓，`validate` 校验写入对象及引用。

生成两种模式、三档画质的十二组报告：

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

原 `make_sample --large` 命令额外生成约 138 MB 的 24 页文件，供取消及大文件流程测试。生成样本的高压缩率不能代表真实扫描件或商业 PDF 的表现。`benchmark-round1.mjs` 对相同输入比较新旧 worker、两种模式和三档；`scripts/verify-codec-benchmark.py` 用开发期 Poppler / pypdf / Pillow / NumPy 独立检查，详见 [第一轮结果](docs/PDF-OPTIMIZATION-ROUND1.md)。第二轮的 Form、JPEG 参数与 Jpegli 对照见 [第二轮结果](docs/PDF-OPTIMIZATION-ROUND2.md)；Jpegli 仅用于开发期实验，未成为产品依赖。`jpeg_profiles` 示例及 `benchmark-jpeg-profiles.py` 保存相同像素下的编码 / 局部指标，`verify-vector-pages.mjs` 用 PDF.js 以 200 DPI 检查指定页面。

第三轮的每图片保护、资源清理、样本去重、G4 与受限字体 / JP2 实验见 [第三轮结果](docs/PDF-OPTIMIZATION-ROUND3.md)。第四轮 [Brotli 格式实验](docs/PDF-OPTIMIZATION-ROUND4.md)完成体积与阅读器矩阵；兼容性未通过，没有加入产品编码选项。可用 `cargo run --release -p squeeze-core --example brotli_experiment --locked -- compressed.pdf tmp/brotli-probe` 复测单文件流候选；测试文件只在开发目录保存。

第五轮扩大通用输入与资源优化，见 [第五轮结果](docs/PDF-OPTIMIZATION-ROUND5.md)：除原有 XObject 外，清理可证明未使用的 Font / ColorSpace / ExtGState / Shading / Properties；合并引用编号不同但子图等价的资源；补充 Indexed、既有 G4 与 CMYK 原图路径。Zopfli 在工作量和软时间预算内无损重压缩部分 Flate，解码字节不变，保留 DecodeParms。CLI/API 的 `lossless` 不重新量化图片或改变分辨率，可以改变等价的 Flate 编码；交互共享图片保持原编码。

第六轮实现受限原始颜色分量编码、明确默认 Decode 识别和完整文件布局竞争，并让严格 Lossless 使用精确图片候选；见 [第六轮结果](docs/PDF-OPTIMIZATION-ROUND6.md)。`native_samples` 核对原始分量与映射，`writing` 在预算内比较原布局、压紧编号和引用频率布局；JSON 的 `writer.layout`／`writer.candidates` 记录实际选择与候选完整字节。31 类样本、217 对组合均未变大；真实文件本轮改善有限，不把自建渐变的压缩比例推广到一般文档。

## 支持范围与限制

- 每文件最多 256 MiB，单流解压最多 64 MiB，单张图片最多 2400 万像素；队列串行执行。
- 原始内容模式的常规图片通路支持 8 位 RGB / Gray、默认范围的 1 / 3 通道 ICCBased、Flate 预测器与 JPEG，以及受限 1 位 Gray / G4、Indexed 输入；常规灰度软遮罩保持无损。原始分量通路另覆盖受限 raw / 单层 Flate 的 1 / 2 / 4 / 8 / 16 位 DeviceGray / RGB / CMYK、CalGray / CalRGB、Lab、ICCBased、Indexed、Separation / DeviceN，不转换 RGB，保持原宽高、颜色空间、位深、ICC Range、非默认 Decode 和遮罩。每张新增分量工作最多 4 MiB、每份新增样本预算 16 MiB，另有软时间预算。CMYK JPEG、JPEG2000、JBIG2、G3 / 特殊 CCITT、未知或多层过滤器仍保留原图片编码；单名 Flate 的精确重压缩另受 Zopfli 预算与交互保护约束。只归一化确实默认的设备颜色 Decode；含 DefaultGray / RGB / CMYK 的文档继续保留显式映射。
- 根据实际绘制尺寸处理继承资源、共享图片、Form Matrix 和 UserUnit；按每张图片记录未知调用；分析不完整或超过预算时只禁用相关图片的降采样，继续安全重编码。保真始终保持原图片尺寸。
- 原始内容模式保留表单、批注及附件，并保护外观 / 字段默认资源及其共享图片；不相关的普通页面图片按所选档位优化。图案 / Type3 等未知图片调用保持原尺寸。附件不再关闭普通页面优化；表单与外观批注的两项有损矢量优化仍采用整份保守保护。空的批注数组不算交互内容。加密、数字签名、PDF/A、XFA、组合包均跳过。
- 桌面图片模式支持普通超链接、空表单容器、可渲染的表单与批注，并保留嵌入附件。核对批注解析及绘制结果；缺少可绘制外观的便签 / 附件图标或损坏批注会按页报错，避免导出缺内容的页面。CLI 的 hayro 路径仍拒绝需要绘制表单或批注的文件；另拒绝含 BrotliDecode 数据流的文件，避免静默遗漏，需改用桌面 PDF.js 图片模式。
- 图片模式处理旋转、可见裁剪区域和物理页面尺寸；缺少已嵌入的非标准字体时拒绝转换，避免静默丢字。复杂页面仍需前后人工检查。
- hayro 仍处于快速迭代阶段；采用固定版本不代表具备完整商业 PDF 兼容性。图片模式的渲染与颜色表现需要更多真实样本回归。
- 底层解析器可能容错重建交叉引用；尚未实现对所有畸形 PDF 的严格拒绝。
- 已限制每流、像素和分析操作数，但尚未实现覆盖所有分配的总内存预算；导入预检在后台线程，压缩在独立进程。
- 正常取消会清理已终止 worker 的临时目录。系统强制杀死应用或断电后的启动恢复清理尚未实现。
- 不包含 OCR、PDF 编辑、目标大小保证或商业激活系统。

详细实测见 [docs/VALIDATION.md](docs/VALIDATION.md)，实施及后续计划见 [PLAN.md](PLAN.md)。

## 第三方依赖

锁定文件固定构建依赖。`npm run notices` 收集依赖清单、原许可文本、hayro 上游 NOTICE、PDF.js 字体及 WASM 通知，并附带未修改 MPL 依赖源码；构建时放入应用资源的 `third-party/`。生成清单保守包含构建依赖，正式商业发布前应按实际分发内容复核。

自研范围包括压缩策略、PDF 图片分析、降采样、页面重建、文件保护与验证；标准格式解析及解码使用 Rust 基础库，桌面整页渲染使用 PDF.js，CLI 默认渲染使用 hayro，JPEG 编码使用静态链接的 MozJPEG C 库，精确黑白图的 G4 编码使用 MIT 许可的纯 Rust `fax 0.3.0`。开发期 CID 字体子集实验使用 `subsetter 0.2.6`，未接入默认引擎；实验不会随软件分发测试字体。参考 [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/)、[hayro](https://github.com/LaurenzV/hayro)、[PDF.js](https://mozilla.github.io/pdf.js/getting_started/) 的官方资料。

项目按闭源商业产品开发；自身最终授权条款和发布主体尚未定义，第三方许可分别适用。
