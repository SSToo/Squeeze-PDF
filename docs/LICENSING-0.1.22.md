# 0.1.22 第三方许可与源码分发补齐

日期：2026-10-10。对应上次商用依赖审核发现的字体源码分发和 QuickJS 通知缺口。范围是许可材料、构建检查与安装包资源；压缩引擎代码和画质参数保持 0.1.21。

本记录描述 0.1.22 当时的授权状态；后续 0.1.23 已确定自有代码采用 Apache-2.0，见 [开源准备记录](OPEN-SOURCE.md)。第三方原许可保持。

## 本次材料

| 组件 | 实际分发内容 | 补齐方式 |
| --- | --- | --- |
| Liberation Sans 1.07.4 | PDF.js 的四款 TTF 字体，GPLv2 加 Liberation 字体例外 | 完整上游 SFD 源码、实际四款字体的完整可编辑 TTX、还原脚本、构建要求、许可正文及原字体哈希随包提供 |
| QuickJS / PDF.js QuickJS 沙箱 | `quickjs-eval.js`、`quickjs-eval.wasm` | 补充 Bellard / Gordon 和 Mozilla 两份完整 MIT 版权与许可声明，写入分发清单 |
| 五个 MPL-2.0 Rust 依赖 | selectors、cssparser、cssparser-macros、dtoa-short、option-ext | 继续附带未修改的完整 crate 源码，增加通用 MPL 正文、明确获取说明及逐文件哈希检查 |

`licenses/pdfjs-assets.json` 固定 PDF.js 6.4.299、六个字体 / QuickJS 文件及许可 / 源码材料的 SHA-256。QuickJS 的链接固定许可文本来源修订，不据此声称已证明 WASM 的精确构建提交。字体例外适用范围以原许可为准，不能只按 npm 包顶层 Apache-2.0 许可判断全部 PDF.js 资产。

## 字体源码对应关系

[PDF.js 字体说明](https://github.com/mozilla/pdf.js/blob/v6.4.299/external/standard_fonts/README.md) 标示字体版本为 1.07.4，并明确字形顺序参与其字宽及缩放映射。实际核对发现，[上游 1.07.4 发布](https://releases.pagure.org/liberation-fonts/)的 TTF 包 Regular / Bold 各 682 个字形、Italic / BoldItalic 各 687 个，PDF.js 四款各 681 个。保留字形的轮廓、指令和字宽已与上游版本核对，不能仅凭版本号声称整个字体二进制相同。

因此同时提供完整上游 SFD 源码和实际 PDF.js 字体导出的完整 TTX。TTX 包含轮廓、复合字形引用、字体及字形提示、字宽、字符映射、命名、布局表与还原脚本。软件继续使用 PDF.js 原始字体字节。重建字体可有表打包、填充及校验和差异，不承诺逐字节相同。

| 源码归档 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `liberation-fonts-1.07.4.tar.gz` | 2,937,949 | `ad98b7498dc2992f7f0868f79b65ce4a720a3acdb63ab3f1f1cb6881117a5406` |
| `pdfjs-liberation-1.07.4-ttx.tar.gz` | 820,775 | `2750c1446ea31839af039a531a6fb52d112516acc0be6166187b1b7d08872e09` |

TTX 归档由 `scripts/make-font-sources.py` 和 FontTools 4.66.1 可重复生成。归档内 `rebuild.py` 保留源码中的历史 OS/2 字符范围，避免编译器自动修正它；原始字体二进制不被覆盖。

## 用户如何取得源码

随包的双语 `third-party/SOURCE-CODE.md` 明确位置及重建方法，源码已经本地提供，无需额外付费或下载：

- macOS：右键应用 → 显示包内容 → `Contents/Resources/third-party/`。
- Windows：安装目录的 `third-party/`。
- 单独分发 CLI / worker 时，应将整个 `third-party/` 一并提供。

`NOTICES.txt` 包含原许可及版权声明，`mpl-sources/` 和 `font-sources/` 包含对应源码。自定义产品条款不能收回第三方许可授予的权利；MPL 的说明及更大闭源作品规则见 [Mozilla 官方 FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)。产品自身最终授权条款及发布主体仍由产品方确定。

## 自动检查

构建前检查实际 PDF.js 版本、字体 / QuickJS 字节和许可 / 源码归档；发现变化或新增相关资产会阻止构建，要求先更新审核材料。生成通知后检查完整许可正文、源码说明、源归档和逐文件 MPL 源码哈希。生成目录每次清理重建，避免遗留旧版本材料。

`npm run test:licenses` 覆盖正常材料、未审核版本、变更字体、新增 QuickJS 文件、漏带字体源码、删除版权声明、漏带 MPL 源文件。Windows / macOS CI 已加入这一检查。

## 本机验证与交付

本机为 macOS arm64。已完成：

- `npm run notices` 生成 358 条依赖 / 资产记录，五个 MPL 源码目录共 71 个文件逐字节校验；`npm run test:licenses` 七项全部通过。TypeScript、相关脚本 Prettier / Node 语法检查通过。提交时的差异检查仅排除两份按上游原字节保存的许可正文 `licenses/MPL-2.0.txt`、`licenses/liberation-1.07.4-LICENSE.txt`：其中自带行尾空格，不改写正文或已审核哈希；其余文件差异检查通过。
- 从实际随包 TTX 归档解压并执行其 `rebuild.py`，四款字体各 681 个字形顺序一致。每款 18 个非偏移表的规范化 XML 相同，包括轮廓、字宽、提示、字符映射及 GPOS / GSUB；仅排除编译器计算的 `head.checkSumAdjustment` 和记录偏移的 `loca`。四款生产字体与两个 QuickJS 资产的构建输出哈希仍与清单一致。
- `npm run package:mac` 生成 0.1.22 App / DMG。App 中完整许可、两份字体源码归档、获取说明及 MPL 源码校验通过；以只读方式实际挂载 DMG 后重复验证通过。DMG 根目录另附 `Third-party licenses and sources.txt`，内容与随包获取说明相同。
- App 和 worker 的严格 ad-hoc 签名、DMG 磁盘校验通过；DMG 内两个可执行文件与构建 App 的哈希一致，worker 和应用版本均为 0.1.22。
- 最终包内 worker 重新执行八类样本的 38 个组合：保留模式 32 个、CLI 图片模式六个。字体子集、可编辑 / 外观保护、CMYK、透明、大图及矢量样本的输出与 0.1.21 已验收结果逐字节相同，输入哈希保持。未重跑上一轮 141 项 Rust 测试或全部 488 个组合；引擎源码及依赖版本未变化。

交付文件：`target/release/bundle/dmg/Squeeze PDF_0.1.22_aarch64.dmg`，16,037,900 字节。

| 文件 | SHA-256 |
| --- | --- |
| 0.1.22 DMG | `e05314b96046ea1d73bfaf94fdcfb6969f7e2478b11de4583b242d87948ff0b7` |
| DMG 内 `squeeze-desktop` | `439c1af695d1539c03eec0b7ed2d77f6267c7e925d9d8cabe89cbdea636ae5f2` |
| DMG 内 `squeeze-worker` | `ccfdea7337bb7923492138b205f508f19484b00356d2492e4c22295145d94da4` |

原 0.1.21 DMG 保留，SHA-256 仍为 `1ff18a143a2c4f1edbca9385aeb0acafdcf2d0e2df7c986b6f4f5a83929658e4`。本轮原始记录保存在忽略的 `tmp/license-completion/`，包括 `package-proof.json`、`font-source-check/results.json` 和 `packaged-0.1.22/results.json`。

Windows / Intel 的相同分发规则及 CI 检查已接入，本轮没有生成或实机验收它们的 0.1.22 安装包。正式开发者签名、公证和上一轮尚未完成的真实桌面连续任务验收保持原状态。本次补齐已发现的许可 / 源码分发材料，不替代最终产品授权条款的确定。
