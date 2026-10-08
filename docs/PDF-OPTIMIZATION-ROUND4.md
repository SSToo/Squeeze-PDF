# 第四轮：Brotli 与新格式的兼容性实验

验证日期：2026-10-07。基线为第三轮 0.1.13；产品修补版本为 0.1.14。Brotli 没有进入默认输出或新增界面选项，原有 JPEG、DPI、矢量误差与第三轮压缩策略保持。

## 结论

Brotli 能进一步减小部分 Flate 绘制 / 字体等数据，但现有阅读器支持不一致。本机 PDF.js 通过，Poppler 与 macOS CoreGraphics 出现缺失内容，CLI 原有 hayro 路径还会静默导出空白字体页。因此未达到计划 K 的产品兼容性门槛，保留为开发实验。

0.1.14 的实际产品改动是给 CLI 原生渲染增加明确的 BrotliDecode 拒绝检查，避免报告成功却丢失内容。桌面 PDF.js 渲染及外部像素清单路径继续可用；没有启用 Brotli 编码。

## 实验范围与格式声明

PDF Association 于 2026-08-19 发布 Brotli 扩展，其注册表定义了 PDF 2.0、前缀 PDFa、ExtensionLevel 1、ExtensionRevision 2026 及 BrotliDecode；数据使用 RFC 7932 Brotli，不加额外包装。[发布公告](https://pdfa.org/no-excuses-brotli-compression-advances-pdf/)，[官方扩展注册表](https://pdfa.org/extensions/)。这确认存在已公布扩展，不能推导任意阅读器都能显示。

开发工具 `brotli_experiment` 对同一解码流比较质量 6 / 9 / 11、窗口 22 的候选，逐字节核对解码。限定单一 FlateDecode、没有 DecodeParms / 外部流，排除附件与元数据；最多 64 个流、单流 4 MiB、累计 16 MiB。JPEG、G4 和其他已编码图片不再套 Brotli。流候选预留字典成本，最终仍记录整份文件及扩展声明开销，微小候选不保证整份更小。

输出声明 PDF 2.0 及注册的 PDFa 扩展；Catalog 所在对象流、交叉引用流、对象容器仍采用常规 Flate，方便发现格式声明。保留其他有效扩展，遇到已有 PDFa 条目拒绝实验而非盲目覆盖。没有把 Filter 数组当作编码回退。Brotli 带 predictor、大窗口、多重 Filter 与 Brotli 对象容器不在本轮编码范围；完整规范下载本次访问受限，未声称已验收这些情况。

`brotli 8.0.4` 为直接开发依赖，产品原有 lopdf 等已间接使用它；新增编码仅存在于示例工具，不成为运行时策略。加密、签名、PDF/A、XFA 等仍由现有预检拒绝。

## 体积与成本

输入为第三轮已经压缩的结果。读取后重新序列化会改变对象容器 ID，所以先写出同一写入配置的 Flate 基线，再与 Brotli 变体比较；两份真实基线比产品文件多 8 字节。下面均为整份文件精确字节，包含格式声明成本。

| 输入                     | 匹配 Flate 基线 | Brotli 实验 |                变化 | 采用流数 |
| ------------------------ | --------------: | ----------: | ------------------: | -------: |
| Talvory 保真结果         |       6,966,396 |   6,775,073 | 减少 191,323，2.75% |       64 |
| Orqavion 保真结果        |      40,138,207 |  39,860,340 | 减少 277,867，0.69% |       63 |
| CID 字体子集测试页       |          16,973 |      15,938 |   减少 1,035，6.10% |        3 |
| JPEG 图片测试页          |         120,274 |     120,350 |             增加 76 |        0 |
| G4 黑白扫描页            |          77,980 |      78,058 |             增加 78 |        0 |
| 批注 / 字段 / 附件测试页 |       6,053,109 |   6,053,119 |             增加 10 |        1 |

Talvory 采用流的编码数据 923,326 → 731,870 字节，Orqavion 为 1,226,119 → 948,108 字节；流部分收益不能写成整个 PDF 的收益。三个质量候选及解码核对共约 3.91 秒 / 4.50 秒，质量 11 占大部分时间；这不是从原件压缩的总耗时，也没有证明全部候选的时间最优。本轮未测峰值内存或所有剩余流的收益。

无候选的 JPEG / G4 对照仍写出 PDF 2.0 声明，以检验容器版本变化本身是否影响渲染；它们并不含 Brotli 数据。产品如果将来采用候选，仍必须比较完整文件并保留原策略回退。

## 阅读器矩阵

每种阅读器只比较自身的 Flate 与 Brotli 画面，不跨阅读器比较抗锯齿像素。PDF.js 全部 113 页包括真实文件 109 页、四个测试页；真实文件 100 DPI，测试页 200 DPI。Poppler 与 CoreGraphics 分别检查两份真实文件 11 个重点页面和四个测试页，共 15 页。

| 实现 / 环境                     | 含 Brotli 数据的结果                 | JPEG / G4 无 Brotli 对照 | 验证范围                                                                        |
| ------------------------------- | ------------------------------------ | ------------------------ | ------------------------------------------------------------------------------- |
| lopdf 0.45.0                    | 131 个采用流全部解码字节相同         | 解析通过                 | 仅结构 / 解码核对，不代表画面正确                                               |
| PDF.js 6.3.289                  | 全部像素与提取文字相同               | 相同                     | 全部 113 页；字体页另核对实际像素导出                                           |
| Poppler 26.05.0                 | 不通过，报告 Unknown filter 并缺内容 | 相同                     | 200 DPI，15 页                                                                  |
| macOS 27.0.1 CoreGraphics       | 不通过，字体 / 绘制内容缺失          | 相同                     | 固定渲染设置，200 DPI，15 页                                                    |
| CLI / hayro 0.7.1，修补前       | 字体页返回成功但输出空白，不通过     | 相同                     | 72 DPI 字体、JPEG、G4 测试页                                                    |
| CLI / hayro，0.1.14             | 字体页明确拒绝、不提交输出           | 相同                     | 最终包内 worker；批注混合页两版本均因原有批注限制拒绝，不能用作 Brotli 支持证据 |
| Windows / Acrobat / PDFium 实机 | 未验证                               | 未验证                   | 本机无对应可用环境                                                              |

PDF.js 检查对象解码失败、页面数、尺寸、全部 RGB 像素和提取文字；不仅看程序退出码。Poppler 即使退出码为零，也输出了未知过滤器警告和错误画面。CoreGraphics 字体测试页与基线 RMSE 约 27.40；Poppler 同页约 72.03，属于明显内容缺失，不能解释成正常抗锯齿差异。

CLI 拒绝检查扫描流过滤器，覆盖普通内容、嵌套 Form / 字体及过滤器数组；保守检查全部数据流，含未绘制 Brotli 流的文件也可能被拒绝。检查仅进入 hayro 真正渲染路径，不进入桌面几何计划 / 像素编码路径。回归验证页面 / 嵌套 Form 两位置与单名 / 数组两种形式共四种情况，确认无输出、原件不变且桌面计划仍成功。

桌面路径另使用 PDF.js 实际渲染的 Brotli 字体页，交给最终包内 worker 的像素清单接口，在 200 DPI 输出一页一图；独立 pypdf / Pillow 核对导出的图像组件样本与清单 RGB 完全相同、非空白、无可选择文字。这是接口验证，当前锁屏，未完成原生应用内点击操作。

第五轮审查发现，早期 CoreGraphics 工具扩大了画布，但页适配变换没有确保上采样。上面的 CoreGraphics 数值保留为历史兼容性记录，不能用于声称实际 200 DPI 的画质；第五轮已改为显式 CGContext 缩放并重新验证常规输出。第四轮 Brotli 缺内容的兼容性失败结论保持。

## 新图片格式的处置

第三轮可逆 JP2 实验在 Poppler 正常，但本机 PDF.js 出现错误画面，且此黑白样本远大于 G4，继续不作为默认候选。[第三轮记录](PDF-OPTIMIZATION-ROUND3.md)。AVIF / JPEG XL 本轮没有 PDF 输出实现，也未取得其完整规范与跨阅读器验证，不列作已经完成的优化。JBIG2、通用字体使用收集及更广泛特殊颜色仍为原计划后续工作。

第四轮的实验与兼容性判定已完成，产品兼容性验收未通过；这两种状态必须区分。现有常规 PDF 输出继续采用已验证的策略。

## 复测与交付

```sh
cargo run --release -p squeeze-core --example brotli_experiment --locked -- compressed.pdf tmp/brotli-probe
# verify-brotli-pdfjs / readers 工具读取含六个固定测试目录的批量实验根目录。
node --experimental-strip-types scripts/verify-brotli-pdfjs.mjs tmp/round4-brotli
swiftc scripts/render-quartz.swift -o tmp/render-quartz
python3 scripts/verify-brotli-readers.py tmp/round4-brotli --poppler /path/to/pdftoppm --quartz tmp/render-quartz --worker /path/to/squeeze-worker
```

开发 QA 的 Python 需要 NumPy、Pillow、pypdf，产品不需要 Python、Swift 编译器或 Poppler。字体文件只用于本机测试，不随产品分发。原始数据保存在忽略的 `tmp/round4-brotli/`。

11 项 raster 回归、工作区严格 clippy、fmt、开发脚本语法与文档格式检查通过；第三轮已经完成 77 项不同 Rust 测试及八语言 / 前端渲染验证。第四轮只修改原生渲染入口，补测该入口的全部回归。

最终 macOS arm64 0.1.14 App / DMG 构建、磁盘校验及 App / worker 严格 ad-hoc 签名通过。包内 worker 实测拒绝 Brotli 字体页，未生成文件；JPEG / G4 对照继续正确。再从两份用户原件压缩保真，Talvory 6,966,388 字节、Orqavion 40,138,199 字节，与第三轮整份字节相同，原件哈希不变，分别约 23 秒 / 41 秒。DMG SHA-256：`e625dc123e2a037be6c12b45b7398d9c4d445844a2d89e2dc28555e72dd44c6b`。

受 Mac 锁屏影响，原生应用内点击回归未完成；Windows / macOS Intel 实机、Developer ID 签名与公证仍未验证。格式实验的通过结论只限上述实际阅读器与版本。
