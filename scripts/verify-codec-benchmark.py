"""Development QA; needs Poppler on PATH, Pillow, NumPy and pypdf.
Run against benchmark-round1.mjs output; never ships in the desktop runtime.
"""
import hashlib, json, os, shutil, subprocess, sys
from pathlib import Path
import numpy as np
from PIL import Image
from pypdf import PdfReader

root = Path(sys.argv[1]).resolve()
poppler = os.environ.get('PDFTOPPM') or shutil.which('pdftoppm')
if not poppler:
    raise SystemExit('Set PDFTOPPM to your local Poppler pdftoppm executable')
render = root / 'render'; render.mkdir(exist_ok=True)
rows = json.loads((root / 'reports.json').read_text())
cache = {}
def pixels(pdf):
    pdf = Path(pdf)
    if str(pdf) not in cache:
        prefix = render / pdf.stem
        r = subprocess.run([poppler, '-r', '100', '-singlefile', '-png', str(pdf), str(prefix)], capture_output=True, text=True)
        assert r.returncode == 0 and not r.stderr, (pdf, r.stderr)
        cache[str(pdf)] = np.asarray(Image.open(prefix.with_suffix('.png')).convert('RGB'), dtype=np.float32)
    return cache[str(pdf)]
results = []
for row in rows:
    source = Path(row['input'])
    assert hashlib.sha256(source.read_bytes()).hexdigest() == row['input_sha256']
    output = Path(row['output']) if row['output'] else source
    reader = PdfReader(output)
    assert len(reader.pages) == row['pages'] == 1
    if row['output_mode'] == 'raster':
        assert len(reader.pages[0].images) == 1
        assert not reader.pages[0].extract_text()
    image, reference = pixels(output), pixels(source)
    assert image.shape == reference.shape
    rmse = float(np.sqrt(np.mean((image-reference)**2)))
    if row['version'] == 'latest' and row['output_mode'] == 'preserve' and row['preset'] == 'fidelity':
        assert rmse == 0., (row['name'], rmse)
    results.append(dict(name=row['name'], version=row['version'], mode=row['output_mode'], preset=row['preset'], bytes=row['result_bytes'], source_rmse=rmse))
for row in results:
    if row['version'] != 'latest': continue
    old = next((r for r in results if r['name']==row['name'] and r['mode']==row['mode'] and r['preset']==row['preset'] and r['version']=='baseline'), None)
    if old:
        assert row['bytes'] <= old['bytes'], (row,old)
        row['baseline_bytes'] = old['bytes']
        row['baseline_rmse'] = old['source_rmse']
(root / 'quality.json').write_text(json.dumps(results, indent=2)+'\n')
print(json.dumps([r for r in results if r['version']=='latest' and r['preset'] in ['fidelity','lossless']], indent=2))
