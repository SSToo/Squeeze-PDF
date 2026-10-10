"""Independent Poppler/CoreGraphics and pypdf QA for benchmark-general output.
Development only: numpy, Pillow, pypdf; PDFTOPPM/FONTCONFIG_FILE and optional QUARTZ.
"""
import hashlib,json,os,subprocess,sys,math
from pathlib import Path
import numpy as np
from PIL import Image
from pypdf import PdfReader
root=Path(sys.argv[1]).resolve();rows=json.loads((root/'reports.json').read_text())
render=root/'render';render.mkdir(exist_ok=True);cache={};file_hashes={}
readers={'poppler':os.environ['PDFTOPPM']}
text_cache={}
def text(path):
 key=str(path)
 if key not in text_cache:
  r=subprocess.run([os.environ['PDFTOTEXT'],'-layout',key,'-'],capture_output=True)
  assert r.returncode==0 and not r.stderr,(path,r.stderr)
  text_cache[key]=r.stdout
 return text_cache[key]
if os.environ.get('QUARTZ'): readers['quartz']=os.environ['QUARTZ']
reader_hashes={reader:hashlib.sha256(Path(executable).read_bytes()).hexdigest()[:12] for reader,executable in readers.items()}
def pixels(path,reader,page):
 path=Path(path).resolve();key=(str(path),reader,page)
 if key not in cache:
  if path not in file_hashes:file_hashes[path]=hashlib.sha256(path.read_bytes()).hexdigest()
  # Persistent cache includes source bytes, renderer bytes and geometry options.
  out=render/(path.stem+'-'+file_hashes[path][:16]+'-'+reader+'-'+reader_hashes[reader]+'-visible-v2-'+str(page)+'.png')
  geometry=PdfReader(path).pages[page-1];box=geometry.cropbox;unit=float(geometry.get('/UserUnit',1));width,height=float(box.width),float(box.height)
  if geometry.rotation%180:width,height=height,width
  width,height=math.ceil(width*unit*200/72),math.ceil(height*unit*200/72)
  args=([readers[reader],'-cropbox','-scale-dimension-before-rotation','-scale-to-x',str(width),'-scale-to-y',str(height),'-f',str(page),'-l',str(page),'-r','200','-singlefile','-png',str(path),str(out.with_suffix(''))] if reader=='poppler' else [readers[reader],str(path),str(out),str(page)])
  if not out.exists():
   r=subprocess.run(args,capture_output=True,text=True)
   assert r.returncode==0 and not r.stderr,(path,reader,r.stderr)
  assert Image.open(out).size==(width,height),(path,reader,page,Image.open(out).size,(width,height))
  cache[key]=np.asarray(Image.open(out).convert('RGB'),dtype=np.float32)
  if len(cache)>8: cache.pop(next(iter(cache)))
 return cache[key]
def rmse(a,b):
 assert a.shape==b.shape,(a.shape,b.shape)
 return float(np.sqrt(np.mean((a-b)**2)))
def tagged_relations(reader):
 root=reader.trailer['/Root'];tree=root.get('/StructTreeRoot')
 if tree is None: return []
 pages={(p.indirect_reference.idnum,p.indirect_reference.generation):i for i,p in enumerate(reader.pages)}
 result=[];pending=[tree.get_object().get('/K')]
 while pending:
  item=pending.pop();item=item.get_object() if hasattr(item,'get_object') else item
  if isinstance(item,list): pending.extend(item);continue
  if not isinstance(item,dict): continue
  if item.get('/Type')=='/MCR' and '/Stm' in item:
   pg=item.raw_get('/Pg');stm=item.raw_get('/Stm');index=pages[(pg.idnum,pg.generation)]
   contents=reader.pages[index].raw_get('/Contents');contents=contents.get_object()
   assert isinstance(contents,list), 'MCR stream must still belong to the original Contents array'
   stream_index=next((i for i,ref in enumerate(contents) if (ref.idnum,ref.generation)==(stm.idnum,stm.generation)),None)
   assert stream_index is not None,'MCR /Stm no longer belongs to page content'
   result.append((index,stream_index,item['/MCID']))
  elif '/K' in item: pending.append(item['/K'])
 return sorted(result)
quality=[]
for row in rows:
 assert 'error' not in row,row
 source=Path(row['input']);assert hashlib.sha256(source.read_bytes()).hexdigest()==row['input_sha256']
 output=Path(row['output']) if row['output'] else source
 a,b=PdfReader(source),PdfReader(output)
 assert len(a.pages)==len(b.pages)==row['pages']
 for pa,pb in zip(a.pages,b.pages):
  if row['mode']=='preserve':
   assert list(pa.mediabox)==list(pb.mediabox)
   assert list(pa.cropbox)==list(pb.cropbox)
   assert pa.get('/Rotate',0)==pb.get('/Rotate',0)
   assert pa.get('/UserUnit',1)==pb.get('/UserUnit',1)
  else:
   assert len(pb.images)==1,row
   assert not pb.extract_text(),row
 if row['mode']=='preserve':
  assert text(source)==text(output),row
  assert tagged_relations(a)==tagged_relations(b),row
 if row['version']=='latest':
  # The JSON describes output identities and bytes, not stale pre-dedup attempts.
  for image in row.get('output_images',[]):
   oid,gen=map(int,image['object'].split())
   obj=b.get_object(__import__('pypdf').generic.IndirectObject(oid,gen,b))
   assert len(obj._data)==image['bytes']
   assert [obj['/Width'],obj['/Height']]==image['dimensions']
 if row['version']=='latest':
  old=next(r for r in rows if r['name']==row['name'] and r['mode']==row['mode'] and r['preset']==row['preset'] and r['version']=='baseline')
  assert row['result_bytes']<=old['result_bytes'],(row['name'],row['mode'],row['preset'],row['result_bytes'],old['result_bytes'])
  result=dict(name=row['name'],mode=row['mode'],preset=row['preset'],bytes=row['result_bytes'],baseline_bytes=old['result_bytes'])
  exact_rgb_pages=[]
  if row['mode']=='raster' and row.get('image_optimization',{}).get('selected'):
   previous_reader=PdfReader(Path(old['output']) if old['output'] else source)
   proofs=row.get('rgb_flate',{}).get('sample_proofs',[])
   for index,(pa,pb) in enumerate(zip(previous_reader.pages,b.pages)):
    ia=pa['/Resources']['/XObject']['/PageImage'];ib=pb['/Resources']['/XObject']['/PageImage']
    if ia._data==ib._data:continue
    data=ib.get_data();digest=hashlib.sha256(data).hexdigest()
    assert any(p['sha256']==digest and p['bytes']==len(data) and p['dimensions']==[ib['/Width'],ib['/Height']] for p in proofs),(row['name'],index,'missing fixed-input sample proof')
    exact_rgb_pages.append(index+1)
  result['exact_rgb_pages']=exact_rgb_pages
  result['pages']=[]
  for page in range(1,row['pages']+1):
   measurements=dict(page=page)
   for reader in readers:
    actual,ref=pixels(output,reader,page),pixels(source,reader,page)
    previous=Path(old['output']) if old['output'] else source
    baseline=pixels(previous,reader,page)
    measurements[reader+'_source_rmse']=rmse(actual,ref)
    measurements[reader+'_baseline_source_rmse']=rmse(baseline,ref)
    measurements[reader+'_baseline_rmse']=rmse(actual,baseline)
    if row['mode']=='preserve' and row['preset']=='lossless':
     assert measurements[reader+'_source_rmse']==0,(row['name'],page,row['preset'],reader,measurements)
    newly_eligible_lossy = row['mode']=='preserve' and row['preset'] in ('balanced','small') and (row.get('image_preparation',{}).get('changed',0)>0 or row.get('advanced_optimization',{}).get('transparency',{}).get('changed',0)>0)
    measurements['newly_eligible_lossy']=newly_eligible_lossy
    if measurements[reader+'_baseline_rmse'] != 0 and not newly_eligible_lossy and page not in exact_rgb_pages:
     assert measurements[reader+'_source_rmse']<=measurements[reader+'_baseline_source_rmse']+1e-6,(row['name'],page,row['mode'],row['preset'],reader,measurements)
   result['pages'].append(measurements)
  quality.append(result)
(root/'quality.json').write_text(json.dumps(quality,indent=2)+'\n')
print(json.dumps({'runs':len(rows),'latest_checked':len(quality),'pages_checked':sum(len(r['pages']) for r in quality),'readers':list(readers)},indent=2))
