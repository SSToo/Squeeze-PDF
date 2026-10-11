# Squeeze PDF — Licenses and source code

Original Squeeze PDF code is licensed under Apache-2.0. The accompanying
`PROJECT-LICENSE.txt` and `PROJECT-NOTICE.txt` contain the full project
license and attribution. Project repository: https://github.com/SSToo/Squeeze-PDF

The third-party components retain their own copyrights and licenses. The
product's own terms do not replace the rights granted by those licenses.
In particular, recipients may obtain, inspect and modify the MPL component
sources and the Liberation font sources under their respective licenses.

## Where to find the accompanying materials

- macOS: in Finder, right-click `Squeeze PDF.app`, choose **Show Package
  Contents**, then open `Contents/Resources/third-party/`.
- Windows: open `third-party/` in the installed application's directory.
- For a standalone worker or CLI distribution, provide this entire
  `third-party/` directory alongside the executable.

`NOTICES.txt` contains copyright notices and license texts, including the
Liberation font exceptions, the IJG acknowledgement, and both the QuickJS
engine and Mozilla sandbox MIT notices. `dependencies.json` lists packages
and separately licensed assets. `pdfjs-assets.json` records asset provenance
and checksums. Source code is included in this distribution, without a
separate download or payment:

- `mpl-sources/`: complete copies of the unmodified MPL dependency sources,
  including their licenses and build manifests. These requirements apply
  to those components, not to every separate file in Squeeze PDF.
- `font-sources/liberation-fonts-1.07.4.tar.gz`: complete unmodified upstream
  Liberation 1.07.4 SFD source release, with COPYING, License.txt, README,
  Makefile and FontForge build scripts.
- `font-sources/pdfjs-liberation-1.07.4-ttx.tar.gz`: complete editable XML
  representations of the four actual PDF.js fonts, with their license,
  original hashes, a reconstruction script and build requirements.

The font binaries are taken from PDF.js without changes. The upstream
1.07.4 binary release contains some additional alternate glyphs; matching
version strings do not establish byte identity. The additional TTX source
archive preserves the actual PDF.js glyph order, outlines, composites,
instructions, metrics, character maps, names and layout tables.

## Extracting and rebuilding font sources

Extract either `.tar.gz` archive with an archive tool. To build the upstream
SFD fonts, install FontForge and `make`, then run `make` inside the extracted
`liberation-fonts-1.07.4` directory. Read its README and Makefile for details.

To rebuild the actual PDF.js fonts from editable TTX, extract the second
archive and follow its README: create a Python virtual environment, install
the pinned FontTools requirement, then run `rebuild.py <output-directory>`.
FontTools and FontForge are source-rebuild tools only; Squeeze PDF does not
require them at runtime. Compiler output may differ in table packing and
padding; bit-identical reconstruction is not promised.

Upstream source: https://releases.pagure.org/liberation-fonts/liberation-fonts-1.07.4.tar.gz

## 中文说明

第三方组件各自的许可证继续有效，本产品自身条款不限制这些许可证赋予的权利。
源码和许可材料已经随本次分发提供，无需另行付费或联网下载。

Squeeze PDF 自有代码采用 Apache-2.0；完整正文及署名见同目录的
`PROJECT-LICENSE.txt` 和 `PROJECT-NOTICE.txt`。第三方字体及依赖继续适用各自许可证。

macOS：右键应用，选择“显示包内容”，打开
`Contents/Resources/third-party/`。Windows：打开软件安装目录中的 `third-party/`。
单独分发 worker 或 CLI 时，应同时提供整个 `third-party/` 目录。

`NOTICES.txt` 包含版权及许可正文；`mpl-sources/` 提供未修改 MPL 依赖的完整源码；
`font-sources/` 提供 Liberation 1.07.4 完整上游源码和实际随包四款字体的可编辑
TTX 源码、重建脚本及构建说明。原字体保持 PDF.js 提供的字节；重新构建可能改变
表布局、填充和校验和，不承诺重建文件逐字节相同。源码及其修改适用各自的许可证。
