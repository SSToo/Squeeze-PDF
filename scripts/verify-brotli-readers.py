"""Development-only compatibility matrix; Pillow/NumPy and local renderers.

Outputs stay in the experiment directory. Pixel comparisons always use the
same reader's matched Flate baseline, never different renderers' pixels.
"""
import argparse
import json
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image


def command(args, timeout=120):
    try:
        result = subprocess.run(
            [str(a) for a in args], capture_output=True, text=True, timeout=timeout
        )
        return {
            "exit": result.returncode,
            "stderr": result.stderr,
            "stdout": result.stdout,
        }
    except subprocess.TimeoutExpired:
        return {"exit": None, "stderr": "timeout", "stdout": ""}


def compare(a, b):
    x = np.asarray(Image.open(a).convert("RGB"), dtype=np.int16)
    y = np.asarray(Image.open(b).convert("RGB"), dtype=np.int16)
    if x.shape != y.shape:
        return {"equal": False, "error": "dimensions differ"}
    return {
        "equal": bool(np.array_equal(x, y)),
        "rmse": float(np.sqrt(np.mean((x.astype(float) - y) ** 2))),
        "changed_pixels": int(np.any(x != y, axis=2).sum()),
        "baseline_ink_pixels": int(np.any(x < 250, axis=2).sum()),
    }


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
parser.add_argument("--poppler", required=True, type=Path)
parser.add_argument("--quartz", required=True, type=Path)
parser.add_argument("--worker", required=True, type=Path)
args = parser.parse_args()
root = args.directory.resolve()
cases = {
    "document-a": [1, 8, 52],
    "document-b": [1, 3, 8, 12, 41, 54, 56, 57],
    "font": [1],
    "raster": [1],
    "binary": [1],
    "interactive": [1],
}
rows = []


def record(row):
    rows.append(row)
    (root / "readers-verification.json").write_text(
        json.dumps(rows, indent=2) + "\n"
    )
    print(json.dumps({k: row[k] for k in ("name", "reader", "passed")}), flush=True)


for reader in ("Poppler", "CoreGraphics"):
    for name, pages in cases.items():
        folder = root / name / reader
        folder.mkdir(exist_ok=True)
        row = {"name": name, "reader": reader, "dpi": 200, "pages": []}
        for page in pages:
            item = {"number": page, "runs": {}}
            outputs = []
            for variant in ("baseline", "brotli"):
                source = root / name / f"{variant}.pdf"
                output = folder / f"{variant}-{page}.png"
                output.unlink(missing_ok=True)
                if reader == "Poppler":
                    cmd = [
                        args.poppler, "-f", page, "-l", page, "-r", 200,
                        "-singlefile", "-png", source, output.with_suffix(""),
                    ]
                else:
                    cmd = [args.quartz, source, output, page]
                item["runs"][variant] = command(cmd)
                outputs.append(output)
            if all(p.exists() for p in outputs):
                item.update(compare(*outputs))
            else:
                item.update(equal=False, error="no rendered output")
            item["passed"] = item["equal"] and all(
                r["exit"] == 0 for r in item["runs"].values()
            )
            row["pages"].append(item)
        row["passed"] = all(p["passed"] for p in row["pages"])
        record(row)

# Small targeted pages test the CLI's actual hayro path; large documents have
# already been covered by the independent complete PDF.js page walk.
for name in ("font", "raster", "binary", "interactive"):
    folder = root / name / "hayro"
    folder.mkdir(exist_ok=True)
    row = {"name": name, "reader": "CLI/hayro", "dpi": 72, "runs": {}}
    outputs = []
    for variant in ("baseline", "brotli"):
        target = folder / f"{variant}.pdf"
        target.unlink(missing_ok=True)
        row["runs"][variant] = command([
            args.worker, "compress", root / name / f"{variant}.pdf", "-o", target,
            "--output-mode", "raster", "--preset", "lossless", "--dpi", 72, "--json",
        ])
        output = folder / f"{variant}.png"
        output.unlink(missing_ok=True)
        if row["runs"][variant]["exit"] == 0 and target.exists():
            row["runs"][variant]["render"] = command([
                args.poppler, "-r", 72, "-singlefile", "-png", target,
                output.with_suffix(""),
            ])
        outputs.append(output)
    if all(p.exists() for p in outputs):
        row.update(compare(*outputs))
    else:
        row.update(equal=False, error="no complete raster output")
    row["passed"] = row["equal"] and all(r["exit"] == 0 for r in row["runs"].values())
    if row["runs"]["baseline"]["exit"] != 0:
        row["status"] = "baseline_already_unsupported"
    elif "BrotliDecode" in row["runs"]["brotli"]["stderr"]:
        row["status"] = "unsupported_extension_rejected"
    else:
        row["status"] = "compatible" if row["passed"] else "render_mismatch"
    record(row)
