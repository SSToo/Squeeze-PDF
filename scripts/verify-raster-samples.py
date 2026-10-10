"""Check changed desktop encodings against the exact production RGB staging."""
import json, sys
from pathlib import Path
from pypdf import PdfReader
old,new,directory=map(Path,sys.argv[1:]);a,b=PdfReader(old),PdfReader(new)
assert len(a.pages)==len(b.pages)
result=[]
for index,(pa,pb) in enumerate(zip(a.pages,b.pages)):
 ia=pa['/Resources']['/XObject']['/PageImage'];ib=pb['/Resources']['/XObject']['/PageImage']
 assert ia['/Width']==ib['/Width'] and ia['/Height']==ib['/Height']
 if ia._data==ib._data:
  assert ia.get('/Filter')==ib.get('/Filter') and ia.get('/DecodeParms')==ib.get('/DecodeParms')
  result.append({'page':index+1,'unchanged_image':True})
 else:
  assert ib['/ColorSpace']=='/DeviceRGB' and ib['/BitsPerComponent']==8 and ib['/Filter']=='/FlateDecode'
  assert ib.get_data()==(directory/f'{index}.rgb').read_bytes()
  result.append({'page':index+1,'exact_staged_rgb':True})
print(json.dumps(result))
