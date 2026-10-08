"""Developer-only fixed-pixel codec experiment (Pillow/NumPy, built cjpegli).
Usage: python benchmark-jpeg-profiles.py SAMPLES OUTPUT MOZ_PROFILES CJPEGLI
No JPEG quality number or whole-page average is used as a quality equivalence proof.
"""
import json, subprocess, sys, time
from pathlib import Path
import numpy as np
from PIL import Image

samples, output, moz, jpegli = map(lambda s: Path(s).resolve(), sys.argv[1:5])
output.mkdir(parents=True, exist_ok=True)

def metrics(a, b):
    delta = (a-b)**2
    error = np.mean(delta, axis=2)
    edge = np.zeros(a.shape[:2], dtype=bool)
    edge[1:] |= np.max(np.abs(a[1:]-a[:-1]), axis=2)>24
    edge[:,1:] |= np.max(np.abs(a[:,1:]-a[:,:-1]), axis=2)>24
    edge[:-1] |= edge[1:]; edge[:,:-1] |= edge[:,1:]
    tiles = np.array([float(np.mean(error[y:y+32,x:x+32])) for y in range(0,a.shape[0],32) for x in range(0,a.shape[1],32)])
    h,w = a.shape[:2];h=h//8*8;w=w//8*8
    luma = lambda x: x[:h,:w]@np.array([.299,.587,.114],dtype=np.float32)
    x=luma(a).reshape(h//8,8,w//8,8);y=luma(b).reshape(h//8,8,w//8,8)
    mx=x.mean(axis=(1,3));my=y.mean(axis=(1,3));vx=x.var(axis=(1,3));vy=y.var(axis=(1,3));cov=(x*y).mean(axis=(1,3))-mx*my
    ssim=(2*mx*my+6.5025)*(2*cov+58.5225)/((mx**2+my**2+6.5025)*(vx+vy+58.5225))
    return dict(rmse=float(np.sqrt(error.mean())),edge_rmse=float(np.sqrt(error[edge].mean())) if edge.any() else 0.,ssim8_mean=float(ssim.mean()),worst_tile_rmse=float(np.sqrt(tiles.max())),tiles_mse=tiles.tolist())

rows=[]
for sample in json.loads((samples/'manifest.json').read_text()):
    source=samples/sample['file'];a=np.asarray(Image.open(source).convert('RGB'),dtype=np.float32)
    for quality in [95,85,72,60]:
        dest=output/source.stem/f'q{quality}';dest.mkdir(parents=True,exist_ok=True)
        subprocess.run([str(moz),str(source),str(dest),str(quality)],check=True)
        encoders=json.loads((dest/'encoders.json').read_text());baseline=(dest/'baseline.jpg').stat().st_size
        for sampling in ['444','422','420']:
            file=dest/f'jpegli-{sampling}.jpg';start=time.monotonic()
            subprocess.run([str(jpegli),str(source),str(file),f'--quality={quality}',f'--chroma_subsampling={sampling}'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            encoders.append(dict(name=f'jpegli-{sampling}',bytes=file.stat().st_size,elapsed_ms=round((time.monotonic()-start)*1000)))
        file=dest/'jpegli-same-size.jpg';start=time.monotonic()
        subprocess.run([str(jpegli),str(source),str(file),f'--target_size={baseline}','--chroma_subsampling=444'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        encoders.append(dict(name='jpegli-same-size',bytes=file.stat().st_size,elapsed_ms=round((time.monotonic()-start)*1000)))
        measured=[]
        for row in encoders:
            b=np.asarray(Image.open(dest/f'{row["name"]}.jpg').convert('RGB'),dtype=np.float32);assert b.shape==a.shape
            measured.append(dict(**row,**metrics(a,b),sample=source.stem,quality_setting=quality))
        reference=next(r for r in measured if r['name']=='baseline')
        for row in measured:
            # Strict experimental screen, not a general perceptual guarantee.
            row['no_worse_screen']=bool(row['ssim8_mean']>=reference['ssim8_mean'] and row['edge_rmse']<=reference['edge_rmse']+1e-6 and np.all(np.array(row['tiles_mse'])<=np.array(reference['tiles_mse'])+1e-6))
            row['bytes_ratio']=row['bytes']/baseline
        (dest/'metrics.json').write_text(json.dumps(measured,indent=2));rows.extend(measured)
        print(source.stem,quality,'strict smaller candidates',[(r['name'],round(r['bytes_ratio'],3)) for r in measured if r['no_worse_screen'] and r['bytes']<baseline],flush=True)
(output/'results.json').write_text(json.dumps(rows,indent=2)+'\n')
