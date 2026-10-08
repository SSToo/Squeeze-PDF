# 第五轮：通用资源与图片输入优化

日期：2026-10-07。产品版本 0.1.15，比较基线为最终 0.1.14 包内 worker。本轮按常见 PDF 结构设计实现，自建样本覆盖新增分支；两份用户文档继续作为体积回归样本。保真原图片像素尺寸、既有 JPEG 质量 / 色度、各档 DPI 和矢量误差预算保持。

## 默认启用的改动

1. **扩大资源清理。** 在 XObject 外，分析 Tf、gs、sh、CS / cs、BDC / DP，清理可证明未使用的 Font、ExtGState、Shading、ColorSpace 和 Properties。内容数组连续分析；资源表为每个所有者复制，保持共享 / 继承调用。使用 named colour、XObject、Shading、ExtGState 或存在 DefaultRGB / Gray / CMYK 时，保留整个颜色表，避免间接颜色依赖丢失。图案、Type3、软遮罩、内联图片、未知操作、畸形表与超预算均保守跳过。批注页面保持资源；存在表单 / 编辑外观时，保留 Form 资源，包括当前未显示但未来编辑可能使用的字体。
2. **资源子图共享。** 从 Resources 出发，按内容建立递归指纹，引用编号不同但子对象相同的 Font / ICC / Form 等资源可共享。哈希后再逐项比较，流内容保持原字节，字典仅忽略流 Length；不是按 RGB 外观猜测等价。固定保留最早 ID。循环、深度 64、操作 / 哈希字节预算、标签 / MCID / 对象身份字段等阻止合并；含交互内容的文档不进入新增子图共享，继续使用原有策略。
3. **Indexed 原图。** 支持 1 / 2 / 4 / 8 位、奇数行宽、RGB / Gray 或默认范围 1 / 3 通道 ICC 基础色、字符串 / 流 lookup。精确展开索引后比较现有候选，保留基础 ICC；Decode、硬遮罩、异常 lookup / 索引和未支持颜色保留原编码。保真尺寸不变；均衡 / 更小只在既有调用证明与 DPI 规则下处理尺寸。
4. **既有 G4 原图。** 解码经过参数和完整行数检查的 1 位 DeviceGray / Group 4，覆盖 BlackIs1 极性，再比较原编码、G4、1 位 Flate 等候选。不对灰边文字二值化。G3、特殊 Decode、损坏 / 不完整或未支持参数保留原编码。
5. **CMYK 精确预测。** 8 位 DeviceCMYK / 默认范围 ICCBased N=4 的原始或单层 Flate 输入，在解码四通道数据不超过 4 MiB 时比较无损 Flate 预测。保持全部四通道样本、ICC、位深和尺寸，不经过 RGB 转换或 JPEG。CMYK JPEG、特殊 Decode / Range、遮罩、外部数据等继续跳过此路径。
6. **有界 Zopfli。** 对单名 FlateDecode 的编码流，包含 predictor 原字节，重新压缩后独立 inflate 核对，DecodeParms 保持。排除交互共享图片、附件 / 元数据 / 对象容器与外部流。最多检查 48 个候选、编码 6 次、单流 256 KiB、累计 1 MiB、2 次优化迭代；4 秒为尝试间检查的软时间预算，不是可抢占的硬超时。取消在尝试前后检查。新编码至少节省 24 字节才采用，完整 PDF 仍遵循更小才提交规则。

交互保护也进入解码样本去重：外观 / 字段共享图片不会被另一个较小编码替换。CLI/API 的严格 `lossless` 允许等价 Flate 重压缩和资源清理，不重新量化、降采样或修改图像样本。产品的「保真」仍是高保真策略，常规 JPEG 重编码及既有矢量精度优化可能改变像素 / 边缘，不能因此将任意文件称为严格无损。

资源分析、候选选择、保护和 PDF 重建由本项目 Rust 实现；Zopfli 使用 Apache-2.0 的 [zopfli 0.8.3](https://docs.rs/crate/zopfli/0.8.3)，新 G4 解码使用 MIT / Apache-2.0 的 [hayro-ccitt 0.3.0](https://docs.rs/crate/hayro-ccitt/0.3.0)，编码继续使用既有 fax。已更新依赖锁与随包 notices。

## 自建语料与阅读器验证

`make_general_samples` 生成 17 类样本：4 种 Indexed 位深、灰度调色板、G4 两种极性、CMYK 原始 / TIFF / PNG predictor / ICC、等价 Form 子图、未使用资源、默认颜色表、带结构身份的 Form、继承资源 Form、Flate 绘制流。CMYK ICC 使用本机系统配置仅作开发验证，不随产品分发。

保留模式比较严格无损 / 保真 / 均衡 / 更小，图片模式比较无损 / 均衡 / 更小，共 119 种组合；新旧版本共 238 次处理全部成功，完整文件均未退步。pypdf 独立检查页数、页面框和文字；图片模式每页恰好一张图且无可选择文字，源文件哈希保持。Poppler 26.05.0 与 macOS CoreGraphics 各检查全部 119 个新结果；保留模式的保真 / 严格无损 34 页与原样本像素相同，图片模式 51 页与基线像素相同。PDF.js 6.3.289 独立核对这 34 页的画面像素和文字 / 位置相同。验证原样本不是空白页，并目视检查代表性图像 / 文本页。

CoreGraphics 开发工具本轮改为显式缩放 CGContext，以确保实际 200 DPI 采样；重新运行本轮检查。此前仅扩大画布、由页面适配变换缩放的记录不能用于声称实际 200 DPI 的画质验证。

部分保真输出的整份字节如下。样本刻意包含可压缩的规律像素及冗余资源，收益不能推算到普通照片、已优化 PDF 或任意用户文件。

| 自建样本            |  0.1.14 | 0.1.15 |
| ------------------- | ------: | -----: |
| Indexed 4 位        | 100,728 |  1,413 |
| G4 黑白图           |   4,439 |    845 |
| CMYK TIFF predictor |  61,432 |  2,724 |
| CMYK ICC 原始图     | 836,721 | 38,724 |
| 重复 Form 资源子图  |   9,303 |    971 |
| 未使用资源          |   5,277 |    607 |
| 绘制流与资源清理    |  26,318 | 16,762 |

最后一行同时包括资源清理，不能把整份收益都归于 Zopfli。该样本在常规 Flate 后的绘制编码 21,071 → 16,184 字节，Zopfli 单独节省 4,887 字节，约 0.45 秒；解码字节完全相同。对已有 JPEG 和无法证明有收益的流，不增加无效重编码。

88 项不同 Rust 测试通过，包括资源继承、未知 / 超预算、表单外观未来字体、特殊共享图片原编码、G4 奇数行 / 极性 / 截断、调色板 / ICC 样本与 CMYK 四通道 golden vectors；工作区全部目标测试、严格 clippy / fmt、TypeScript、八语言与前端渲染检查通过。新增诊断有八语言文本。PDF.js 的表单、批注、动态字段与损坏外观检查继续通过。

## 用户文档回归

下表来自相同原件、选项及新旧 worker。原件哈希保持，两种文件三档均未退步；并行与其他验证任务的耗时不能作为单文件速度结论。

| 文件     | 档位 |     0.1.14 |     0.1.15 | 再节省 |
| -------- | ---- | ---------: | ---------: | -----: |
| Talvory  | 保真 |  6,966,388 |  6,966,379 |      9 |
| Talvory  | 均衡 |  4,105,066 |  4,105,057 |      9 |
| Talvory  | 更小 |  3,003,320 |  3,003,311 |      9 |
| Orqavion | 保真 | 40,138,199 | 40,079,726 | 58,473 |
| Orqavion | 均衡 | 23,619,417 | 23,560,944 | 58,473 |
| Orqavion | 更小 | 16,228,470 | 16,169,997 | 58,473 |

两份保真结果另在 PDF.js 200 DPI 比较 11 个重点页面，新旧像素全部相同。上述结果表明，本轮主要扩大能优化的文档类型；已经优化充分的输入可能只有很小收益。

## 复测与交付

```sh
cargo build -p squeeze-cli --release --locked
cargo run -p squeeze-core --example make_general_samples --locked -- tmp/general-fixtures /path/to/cmyk.icc
node scripts/benchmark-general.mjs tmp/general-fixtures tmp/general-results /path/to/0.1.14-worker
# Python 开发环境需要 numpy / Pillow / pypdf；设置 PDFTOPPM 和 FONTCONFIG_FILE。
swiftc scripts/render-quartz.swift -o tmp/render-quartz
QUARTZ="$PWD/tmp/render-quartz" python3 scripts/verify-general-corpus.py tmp/general-results
node --experimental-strip-types scripts/verify-general-pdfjs.mjs tmp/general-results
```

过程记录在忽略的 `tmp/round5-0.1.15/`；产品不需要 Python、Poppler 或 Swift 编译器。最终 macOS arm64 App / DMG 已构建，包内 worker / App 严格 ad-hoc 签名及磁盘校验通过。最终包内 worker 重新处理全部 119 种语料组合均成功，并与旧版比较没有体积退步；115 份与已验证的开发输出逐字节相同。默认颜色表保护补丁的 4 个保留输出由独立阅读器重新核对。包内两份真实保真输出与开发结果逐字节相同，原件哈希不变；测试时另有语料 / 渲染任务，约 33 秒 / 89 秒不能作独立性能结论。DMG SHA-256：`bd90a0b0f791cc493c10d09b8b387ae0f22a53d741b5602480026ecb621da236`。

通用 CID 字体使用收集与重新子集化、无损 JBIG2、CMYK JPEG 的安全系数域优化、G3 / 内联图片、单图页面免渲染复用仍未成为默认路径；Brotli / JP2 继续遵循前轮兼容性结论。Windows / macOS Intel 实机、Developer ID 签名 / 公证及本轮原生界面点击未验证。
