"""Independent Poppler/CoreGraphics and pypdf QA for benchmark-general output.
Development only: numpy, Pillow, pypdf; PDFTOPPM/FONTCONFIG_FILE and optional QUARTZ.
"""
import hashlib,json,os,subprocess,sys
from pathlib import Path
import numpy as np
from PIL import Image
from pypdf import PdfReader
root=Path(sys.argv[1]).resolve();rows=json.loads((root/'reports.json').read_text())
render=root/'render';render.mkdir(exist_ok=True);cache={}
readers={'poppler':os.environ['PDFTOPPM']}
if os.environ.get('QUARTZ'): readers['quartz']=os.environ['QUARTZ']
def pixels(path,reader):
 path=Path(path).resolve();key=(str(path),reader)
 if key not in cache:
  out=render/(path.stem+'-'+hashlib.sha256(str(path).encode()).hexdigest()[:6]+'-'+reader+'.png')
  args=([readers[reader],'-r','200','-singlefile','-png',str(path),str(out.with_suffix(''))] if reader=='poppler' else [readers[reader],str(path),str(out),'1'])
  r=subprocess.run(args,capture_output=True,text=True)
  assert r.returncode==0 and not r.stderr,(path,reader,r.stderr)
  cache[key]=np.asarray(Image.open(out).convert('RGB'),dtype=np.float32)
  if len(cache)>8: cache.pop(next(iter(cache)))
 return cache[key]
def rmse(a,b):
 assert a.shape==b.shape
 return float(np.sqrt(np.mean((a-b)**2)))
quality=[]
for row in rows:
 assert 'error' not in row,row
 source=Path(row['input']);assert hashlib.sha256(source.read_bytes()).hexdigest()==row['input_sha256']
 output=Path(row['output']) if row['output'] else source
 a,b=PdfReader(source),PdfReader(output)
 assert len(a.pages)==len(b.pages)==row['pages']==1
 assert list(a.pages[0].mediabox)==list(b.pages[0].mediabox)
 if row['mode']=='preserve': assert a.pages[0].extract_text()==b.pages[0].extract_text(),row
 else:
  assert len(b.pages[0].images)==1,row
  assert not b.pages[0].extract_text(),row
 if row['version']=='latest':
  old=next(r for r in rows if r['name']==row['name'] and r['mode']==row['mode'] and r['preset']==row['preset'] and r['version']=='baseline')
  assert row['result_bytes']<=old['result_bytes'],(row['name'],row['mode'],row['preset'],row['result_bytes'],old['result_bytes'])
  result=dict(name=row['name'],mode=row['mode'],preset=row['preset'],bytes=row['result_bytes'],baseline_bytes=old['result_bytes'])
  for reader in readers:
   actual,ref=pixels(output,reader),pixels(source,reader)
   result[reader+'_source_rmse']=rmse(actual,ref)
   previous=Path(old['output']) if old['output'] else source
   result[reader+'_baseline_rmse']=rmse(actual,pixels(previous,reader))
   if row['mode']=='preserve' and row['preset'] in ('lossless','fidelity'):
    assert result[reader+'_source_rmse']==0,(row['name'],row['preset'],reader,result)
   if row['mode']=='raster': assert result[reader+'_baseline_rmse']==0,(row['name'],reader,result)
  quality.append(result)
(root/'quality.json').write_text(json.dumps(quality,indent=2)+'\n')
print(json.dumps({'runs':len(rows),'latest_checked':len(quality),'readers':list(readers),'max_fidelity_rmse':max(r['poppler_source_rmse'] for r in quality if r['mode']=='preserve' and r['preset']=='fidelity')},indent=2))
