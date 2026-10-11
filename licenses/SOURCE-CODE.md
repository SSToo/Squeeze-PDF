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
