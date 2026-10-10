"""Owned scale, vector, transparency and CMYK fixtures; development only."""
import io,sys,zlib,json,subprocess
from pathlib import Path
import numpy as np
from PIL import Image
from pypdf import PdfWriter,PdfReader
from pypdf.generic import ArrayObject as A,DictionaryObject as D,NameObject as N,NumberObject as I,FloatObject as F,BooleanObject as B,EncodedStreamObject
root=Path(sys.argv[1]);root.mkdir(parents=True,exist_ok=True)
def dic(**kw):return D({N('/'+k):v for k,v in kw.items()})
def arr(v):return A([I(x) if isinstance(x,int) else F(x) for x in v])
def stream(w,data,**kw):
 s=EncodedStreamObject();s._data=data;s.update(dic(**kw));return w._add_object(s)
def save(w,name):w.pdf_header=b'%PDF-1.7';w.write(root/(name+'.pdf'))
def page(w,body,resources=None):
 p=w.add_blank_page(240,180);p[N('/Resources')]=resources or D();p[N('/Contents')]=stream(w,body);return p
# Row-bounded paths beyond the previous 4 MiB decoded cap.
for bits,width,height in [(8,1800,1100),(16,1800,1100)]:
 w=PdfWriter();y,x=np.indices((height,width));values=np.stack([x%256,y%256,(x+y)%256],axis=2)
 raw=values.astype('uint8').tobytes() if bits==8 else (values*257).astype('>u2').tobytes()
 im=stream(w,raw,Type=N('/XObject'),Subtype=N('/Image'),Width=I(width),Height=I(height),BitsPerComponent=I(bits),ColorSpace=N('/DeviceRGB'))
 page(w,b'q 220 0 0 150 10 10 cm /I Do Q',dic(XObject=dic(I=im)));save(w,f'large-rgb{bits}')
# A source with >64 MiB encoded payloads, four independent pages/images.
w=PdfWriter();width,height=2000,3000;y,x=np.indices((height,width))
for i in range(4):
 raw=np.stack([(x+i*13)%256,(y+i*7)%256,(x+y+i*29)%256],axis=2).astype('uint8').tobytes()
 im=stream(w,raw,Type=N('/XObject'),Subtype=N('/Image'),Width=I(width),Height=I(height),BitsPerComponent=I(8),ColorSpace=N('/DeviceRGB'))
 page(w,b'q 220 0 0 150 10 10 cm /I Do Q',dic(XObject=dic(I=im)))
save(w,'large-four-pages-68m')
# Exact state reduction and a safe simple Form; negative transparent-group wrapper.
for kind in ['state','simple-form','group-form']:
 w=PdfWriter();body=b'0.2 0.4 0.7 rg 2 w '+b'0.2 0.4 0.7 rg 2 w '*100+b'20 20 180 130 re B\nq 0.8 g 0.8 g 10 10 50 50 re f Q 0.2 0.4 0.7 rg\n'
 resources=D()
 if kind!='state':
  attrs=dict(Type=N('/XObject'),Subtype=N('/Form'),FormType=I(1),BBox=arr([30,30,190,145]),Matrix=arr([1,0.1,-0.1,1,10,2]),Resources=D())
  if kind=='group-form':attrs['Group']=dic(S=N('/Transparency'),I=B(True))
  form=stream(w,body,**attrs);body=b'/V Do';resources=dic(XObject=dic(V=form))
 page(w,body,resources);save(w,'vector-'+kind)
# Premultiplied edges with and without /Matte; same soft mask shared elsewhere.
width,height=640,480;y,x=np.indices((height,width));alpha=np.clip((185-np.sqrt((x-320)**2+(y-240)**2))/30,0,1)
colour=np.stack([x/639,np.full_like(x,0.25,dtype=float),y/479],axis=2)
for matte in [False,True]:
 for background in ['white','dark']:
  w=PdfWriter();a=(alpha*255).round().astype('uint8').tobytes();mattrs=dict(Type=N('/XObject'),Subtype=N('/Image'),Width=I(width),Height=I(height),BitsPerComponent=I(8),ColorSpace=N('/DeviceGray'),Filter=N('/FlateDecode'))
  if matte:mattrs['Matte']=arr([1,1,1])
  mask=stream(w,zlib.compress(a,1),**mattrs)
  c=colour*alpha[:,:,None]+(1-alpha[:,:,None]) if matte else colour
  im=stream(w,zlib.compress((c*255).round().astype('uint8').tobytes(),1),Type=N('/XObject'),Subtype=N('/Image'),Width=I(width),Height=I(height),BitsPerComponent=I(8),ColorSpace=N('/DeviceRGB'),SMask=mask,Filter=N('/FlateDecode'),Interpolate=B(True))
  bg=b'0.05 0.08 0.12 rg 0 0 240 180 re f\n' if background=='dark' else b''
  page(w,bg+b'q 75 0 0 56.25 10 10 cm /I Do Q\nq 160 0 0 120 80 60 cm /A Do Q',dic(XObject=dic(I=im,A=mask)))
  save(w,f'alpha-{int(matte)}-{background}')
# Four-channel Adobe JPEG, inverted /Decode, transform zero and no Adobe APP14.
width,height=320,240;y,x=np.indices((height,width));raw=np.stack([x%256,y%256,(x+y)%256,np.full_like(x,20)],axis=2).astype('uint8').tobytes()
buf=io.BytesIO();Image.frombytes('CMYK',(width,height),raw).save(buf,'JPEG',quality=94,optimize=False);jpeg=buf.getvalue()
for kind in ['normal','inverse','ct0','no-adobe']:
 w=PdfWriter();attrs=dict(Type=N('/XObject'),Subtype=N('/Image'),Width=I(width),Height=I(height),BitsPerComponent=I(8),ColorSpace=N('/DeviceCMYK'),Filter=N('/DCTDecode'));data=jpeg
 if kind=='inverse':attrs['Decode']=arr([1,0]*4)
 if kind=='ct0':attrs['DecodeParms']=dic(ColorTransform=I(0))
 if kind=='no-adobe':
  pos=data.find(b'\xff\xee');length=int.from_bytes(data[pos+2:pos+4],'big');assert pos>=0;data=data[:pos]+data[pos+2+length:]
 im=stream(w,data,**attrs);page(w,b'q 220 0 0 150 10 10 cm /I Do Q',dic(XObject=dic(I=im)));save(w,'cmyk-jpeg-'+kind)
# Use an explicitly supplied local TTF; never put the development font in the app.
if len(sys.argv)>2:
 subprocess.run([sys.executable,str(Path(__file__).with_name('make-font-subset-fixture.py')),sys.argv[2],str(root/'font-data')],check=True)
 for kind in ['plain','scope','vertical','editable']:
  w=PdfWriter();w.clone_document_from_reader(PdfReader(root/'font-data/font-original.pdf'));p=w.pages[0];font=p['/Resources']['/Font']['/F'];original=p['/Contents'].get_data()
  if kind=='scope':
   # Shared CID child, unused alias, inherited Form scope, and continuous Contents state.
   alias=w._add_object(D(font));p['/Resources']['/Font'][N('/Unused')]=alias
   form=stream(w,original,Type=N('/XObject'),Subtype=N('/Form'),BBox=arr([0,0,600,800]))
   p['/Resources'][N('/XObject')]=dic(V=form)
   p[N('/Contents')]=A([stream(w,b'q 1 0 0 1 0 0 cm /V Do Q\nBT /F 22 Tf 36 30 Td\n'),stream(w,b'<012c0130> Tj ET\n')])
  if kind=='vertical':font[N('/Encoding')]=N('/Identity-V')
  if kind=='editable':w.root_object[N('/AcroForm')]=dic(Fields=A(),DA=__import__('pypdf').generic.TextStringObject('/F 12 Tf'))
  save(w,'cid-font-'+kind)
print(json.dumps({'fixtures':len(list(root.glob('*.pdf')))}))
