"""Independent identity counterexamples: pixels alone cannot detect aliasing."""
import json, subprocess, sys
from pathlib import Path
from pypdf import PdfReader,PdfWriter
from pypdf.generic import ArrayObject,BooleanObject,DecodedStreamObject,DictionaryObject,NameObject,NumberObject,TextStringObject
root=Path(sys.argv[1]).resolve();root.mkdir(parents=True,exist_ok=True);baseline=Path(sys.argv[2]).resolve();latest=Path(sys.argv[3]).resolve();N=NameObject
D=lambda **items:DictionaryObject({N('/'+k):v for k,v in items.items()})
A=lambda v:ArrayObject([NumberObject(i) for i in v])
def stream(w,raw,**attr):
 s=DecodedStreamObject();s.set_data(raw);s.update(D(**attr));return w._add_object(s)
def write(w,name):
 w.pdf_header=b'%PDF-1.7';p=root/(name+'.pdf')
 with p.open('wb') as f:w.write(f)
 return p
w=PdfWriter();p=w.add_blank_page(150,100);font=w._add_object(D(Type=N('/Font'),Subtype=N('/Type1'),BaseFont=N('/Helvetica')));p[N('/Resources')]=D(Font=D(F=font))
parts=[stream(w,b'/P << /MCID 0 >> BDC BT /F 10 Tf 5 65 Td (Tagged identity) Tj ET EMC\n'+b'% repeated safe comment\n'*150) for _ in range(2)];p[N('/Contents')]=ArrayObject(parts);p[N('/StructParents')]=NumberObject(0)
tree=D(Type=N('/StructTreeRoot'));tree_ref=w._add_object(tree);elements=[w._add_object(D(Type=N('/StructElem'),S=N('/P'),P=tree_ref,Pg=p.indirect_reference,K=D(Type=N('/MCR'),Pg=p.indirect_reference,Stm=part,MCID=NumberObject(0)))) for part in parts]
tree[N('/K')]=ArrayObject(elements);w.root_object.update(D(StructTreeRoot=tree_ref,MarkInfo=D(Marked=BooleanObject(True))));tagged=write(w,'same-mcid-distinct-stm')
w=PdfWriter();p=w.add_blank_page(150,100);p[N('/Contents')]=stream(w,b'q 0.9 g 0 0 150 100 re f Q\n'+b'% repeated safe comment\n'*150)
fields=[]
for i in range(2):
 ap=stream(w,b'0.2 0.4 0.6 rg 0 0 35 15 re f\n',Type=N('/XObject'),Subtype=N('/Form'),BBox=A([0,0,35,15]))
 fields.append(w._add_object(D(Type=N('/Annot'),Subtype=N('/Widget'),FT=N('/Tx'),T=TextStringObject('Field'+str(i)),V=TextStringObject('Value'),Rect=A([10+i*60,30,45+i*60,45]),P=p.indirect_reference,AP=D(N=ap))))
p[N('/Annots')]=ArrayObject(fields);w.root_object[N('/AcroForm')]=D(Fields=ArrayObject(fields));appearance=write(w,'independent-appearances')
def relation(path,name):
 r=PdfReader(path)
 if name.startswith('same-mcid'):
  contents=r.pages[0].raw_get('/Contents');ids=[(o.idnum,o.generation) for o in contents]
  mcrs=[e.get_object()['/K'] for e in r.trailer['/Root']['/StructTreeRoot']['/K']]
  stm=[(m.raw_get('/Stm').idnum,m.raw_get('/Stm').generation) for m in mcrs]
  assert stm==ids
  return len(set(ids))
 fields=r.trailer['/Root']['/AcroForm']['/Fields'];refs=[f.get_object()['/AP'].raw_get('/N') for f in fields]
 assert [(f.get_object()['/T'],f.get_object()['/V']) for f in fields]==[('Field0','Value'),('Field1','Value')]
 return len(set((o.idnum,o.generation) for o in refs))
results=[]
for source in [tagged,appearance]:
 for preset in ['lossless','fidelity','balanced','small']:
  for version,worker in [('baseline',baseline),('latest',latest)]:
   output=root/f'{source.stem}-{preset}-{version}.pdf';args=[str(worker),'compress',str(source),'-o',str(output),'--preset',preset,'--json']
   if preset!='lossless':args+=['--compact-paths','--reuse-paths']
   report=json.loads(subprocess.check_output(args));count=relation(Path(report['output'] or source),source.stem)
   assert count==(2 if version=='latest' else 1),(source,preset,version,count)
   results.append(dict(name=source.stem,preset=preset,version=version,distinct_identities=count,bytes=report['result_bytes']))
(root/'results.json').write_text(json.dumps(results,indent=2));print('16 runs: frozen baseline aliases both counterexamples; all current results retain two identities and field values')
