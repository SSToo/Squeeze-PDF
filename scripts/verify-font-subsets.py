"""Independent fontTools validation of PDF CID-to-GID subset results."""
import io,json,sys,hashlib
from pathlib import Path
from pypdf import PdfReader
from fontTools.ttLib import TTFont
root=Path(sys.argv[1]);rows=json.loads((root/'reports.json').read_text());results=[]
for row in rows:
 if row['version']!='latest' or not row.get('advanced_optimization',{}).get('fonts',{}).get('fonts_changed'):continue
 source=PdfReader(row['input']);target=PdfReader(row['output'])
 def fonts(reader):
  result={};todo=[p['/Resources'] for p in reader.pages];seen=set()
  while todo:
   r=todo.pop()
   for n,f in r.get('/Font',{}).items():
    f=f.get_object()
    if f.get('/Subtype')=='/Type0':result[n]=f
   for x in r.get('/XObject',{}).values():
    key=(x.idnum,x.generation)
    if key in seen:continue
    seen.add(key);x=x.get_object()
    if x.get('/Subtype')=='/Form' and '/Resources'in x:todo.append(x['/Resources'])
  return result
 old,new=fonts(source),fonts(target)
 for name,fo in old.items():
  if name not in new:continue
  fn=new[name];co=fo['/DescendantFonts'][0].get_object();cn=fn['/DescendantFonts'][0].get_object()
  a=co['/FontDescriptor']['/FontFile2'].get_data();b=cn['/FontDescriptor']['/FontFile2'].get_data()
  if a==b:continue
  assert len(b)<len(a)
  aa=TTFont(io.BytesIO(a));bb=TTFont(io.BytesIO(b))
  for tag in ['gasp','OS/2','fpgm','prep','cvt ']:
   if tag in aa:assert tag in bb and aa.getTableData(tag)==bb.getTableData(tag),(row['name'],tag)
  om=co['/CIDToGIDMap'].get_data();nm=cn['/CIDToGIDMap'].get_data();checked=0;composites=0
  for cid in range(len(nm)//2):
   ng=int.from_bytes(nm[cid*2:cid*2+2],'big')
   if not ng:continue
   og=int.from_bytes(om[cid*2:cid*2+2],'big');on=aa.getGlyphName(og);nn=bb.getGlyphName(ng)
   ga=aa['glyf'][on];gb=bb['glyf'][nn]
   ca,ea,fa=ga.getCoordinates(aa['glyf']);cb,eb,fb=gb.getCoordinates(bb['glyf'])
   assert list(ca)==list(cb) and list(ea)==list(eb) and list(fa)==list(fb)
   assert aa['hmtx'][on]==bb['hmtx'][nn]
   pa=ga.program.getBytecode() if hasattr(ga,'program') else b'';pb=gb.program.getBytecode() if hasattr(gb,'program') else b'';assert pa==pb
   checked+=1;composites+=int(ga.isComposite())
  assert checked>20 and composites>0
  assert fo['/ToUnicode'].get_data()==fn['/ToUnicode'].get_data()
  assert co['/W']==cn['/W']
  results.append({'name':row['name'],'preset':row['preset'],'resource':str(name),'glyphs_checked':checked,'composite_glyphs':composites,'original_bytes':len(a),'subset_bytes':len(b),'hints_gasp_os2_widths_unicode_exact':True})
(root/'font-proofs.json').write_text(json.dumps(results,indent=2));print(json.dumps({'font_cases':len(results),'glyphs_checked':sum(r['glyphs_checked']for r in results),'composite_glyphs':sum(r['composite_glyphs']for r in results)}))
