"""Regenerate editable PDF.js font sources with FontTools 4.66.1 (development only).

The PDF.js version and font hashes must already have been reviewed in
licenses/pdfjs-assets.json. Review the resulting archive hash separately;
this script does not change that manifest or the distributed fonts.
"""

import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile
import textwrap

import fontTools
from fontTools.ttLib import TTFont


ROOT = Path(__file__).resolve().parent.parent
if fontTools.__version__ != "4.66.1":
    raise SystemExit("Use FontTools 4.66.1 to reproduce this source archive.")

manifest = json.loads((ROOT / "licenses/pdfjs-assets.json").read_text())
pdfjs = ROOT / "node_modules/pdfjs-dist"
if json.loads((pdfjs / "package.json").read_text())["version"] != manifest["pdfjs_version"]:
    raise SystemExit("Review the PDF.js version before regenerating sources.")
component = next(c for c in manifest["components"] if c["name"] == "Liberation Sans")
entries = {}
hashes = {}
for item in component["files"]:
    path = pdfjs / item["path"]
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != item["sha256"]:
        raise SystemExit(f"Review changed font bytes first: {path.name}")
    hashes[path.name] = digest
    font = TTFont(path, recalcTimestamp=False, recalcBBoxes=False)
    xml = io.StringIO()
    font.saveXML(xml)
    entries[path.with_suffix(".ttx").name] = xml.getvalue().encode("utf-8")

entries["rebuild.py"] = textwrap.dedent('''\
    #!/usr/bin/env python3
    """Rebuild editable PDF.js Liberation font sources. Requires FontTools 4.66.1."""
    from pathlib import Path
    import argparse
    from fontTools.ttLib import TTFont
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for source in sorted(Path(__file__).parent.glob("*.ttx")):
        font = TTFont(recalcTimestamp=False, recalcBBoxes=False)
        font.importXML(source)
        # Preserve the historical range recorded in the editable source.
        # FontTools otherwise changes usFirstCharIndex from 33 to 32.
        font["OS/2"].updateFirstAndLastCharIndex = lambda _font: None
        font.save(args.output / source.with_suffix(".ttf").name)
    ''').encode("utf-8")
entries["README.txt"] = textwrap.dedent('''\
    PDF.js Liberation Sans 1.07.4 — editable sources
    ================================================

    These full TTX XML files describe the four actual fonts distributed in
    pdfjs-dist 6.4.299. They retain glyph order, outlines, composite references,
    TrueType instructions, font-wide hinting, metrics, character maps, naming,
    and OpenType layout tables. The original copyright and license remain in
    force; see LICENSE.txt (GPLv2 with the Liberation font exceptions).
    Squeeze PDF does not modify the distributed font binaries.

    The adjacent liberation-fonts-1.07.4.tar.gz is the complete, unmodified
    upstream SFD source release, including FontForge build scripts, Makefile,
    COPYING, License.txt, AUTHORS and README. Its binary release contains a few
    additional alternate glyphs, compared with the PDF.js fonts; retained glyph
    outlines, metrics and instructions have been checked against that release.
    Do not assume that equal version numbers mean identical font binaries.

    The TTX files are an additional editable representation of the exact PDF.js
    fonts, exported with FontTools 4.66.1. To rebuild them:

        python3 -m venv font-build
        font-build/bin/python -m pip install -r requirements.txt
        font-build/bin/python rebuild.py rebuilt

    On Windows use font-build\\Scripts\\python.exe instead. FontTools is a build
    tool only, and is not a runtime dependency of Squeeze PDF. Its MIT-licensed
    source is at https://github.com/fonttools/fonttools . The compiler may change
    table packing, checksums, and padding; bit-identical TTF reconstruction is
    not promised. All supplied source files may be inspected and modified under
    the font license. The application continues to distribute the existing PDF.js
    font bytes, not these locally rebuilt fonts.

    PDF.js font provenance and glyph-order constraints:
    https://github.com/mozilla/pdf.js/blob/v6.4.299/external/standard_fonts/README.md
    Upstream SFD source:
    https://releases.pagure.org/liberation-fonts/liberation-fonts-1.07.4.tar.gz
    ''').encode("utf-8")
entries["requirements.txt"] = b"fonttools==4.66.1\n"
entries["LICENSE.txt"] = (ROOT / "licenses/liberation-1.07.4-LICENSE.txt").read_bytes()
entries["original-font-hashes.json"] = (json.dumps(hashes, indent=2) + "\n").encode()
archive = ROOT / "licenses/font-sources/pdfjs-liberation-1.07.4-ttx.tar.gz"
with archive.open("wb") as raw:
    with gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, filename="") as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as output:
            for name, data in sorted(entries.items()):
                info = tarfile.TarInfo("pdfjs-liberation-1.07.4-ttx/" + name)
                info.size = len(data)
                info.mode = 0o644
                output.addfile(info, io.BytesIO(data))
print(archive.name, archive.stat().st_size, hashlib.sha256(archive.read_bytes()).hexdigest())
