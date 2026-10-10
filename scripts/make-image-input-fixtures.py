"""Owned reference PDFs for image-entry and multi-page QA; no user inputs."""
import base64, io, json, sys, zlib
from pathlib import Path
import numpy as np
from PIL import Image
from pypdf import PdfWriter
from pypdf.generic import ArrayObject, BooleanObject, DecodedStreamObject, DictionaryObject, EncodedStreamObject, NameObject, NullObject, NumberObject
root=Path(sys.argv[1]);root.mkdir(parents=True,exist_ok=True)
N=NameObject
D=lambda **items: DictionaryObject({N('/'+k):v for k,v in items.items()})
A=lambda values: ArrayObject([NumberObject(i) for i in values])
def stream(writer,data,**attrs):
 s=EncodedStreamObject();s._data=data;s.update(D(**attrs));return writer._add_object(s)
def save(writer,name):
 writer.pdf_header=b'%PDF-1.7'
 with (root/(name+'.pdf')).open('wb') as f:writer.write(f)
def page(writer,images,n=1):
 p=writer.add_blank_page(120,90)
 font=writer._add_object(D(Type=N('/Font'),Subtype=N('/Type1'),BaseFont=N('/Helvetica')))
 p[N('/Resources')]=D(Font=D(F=font),XObject=D(**images))
 p[N('/Contents')]=stream(writer,f'BT /F 8 Tf 5 80 Td (Page {n}) Tj ET\nq 110 0 0 65 5 5 cm /I Do Q\n'.encode()+b'% owned reference comment\n'*100)
 return p
w,h=360,240;y,x=np.indices((h,w));raw=np.stack([x%256,y%256,(x+y)%256],axis=2).astype('uint8').tobytes()
attrs=dict(Type=N('/XObject'),Subtype=N('/Image'),Width=NumberObject(w),Height=NumberObject(h),ColorSpace=N('/DeviceRGB'),BitsPerComponent=NumberObject(8))
# >256 exact colours, high correlation; native JPEG never supplies these original samples.
for kind in ['raw','flate','hex-flate','runlength','indirect-predictor']:
 writer=PdfWriter();a=attrs.copy();data=raw
 if kind in ['flate','hex-flate']:a['Filter']=N('/FlateDecode');data=zlib.compress(raw,1)
 if kind=='hex-flate':a['Filter']=ArrayObject([N('/ASCIIHexDecode'),N('/FlateDecode')]);a['DecodeParms']=ArrayObject([NullObject(),NullObject()]);data=data.hex().encode()+b'>'
 if kind=='runlength':
  data=b''.join(bytes([len(raw[i:i+128])-1])+raw[i:i+128] for i in range(0,len(raw),128))+b'\x80';a['Filter']=N('/RunLengthDecode')
 if kind=='indirect-predictor':
  data=b''.join(b'\x02'+(np.frombuffer(raw[j*w*3:(j+1)*w*3],dtype='uint8')-(np.frombuffer(raw[(j-1)*w*3:j*w*3],dtype='uint8') if j else np.zeros(w*3,dtype='uint8'))).tobytes() for j in range(h))
  data=zlib.compress(data,1)
  a['Filter']=writer._add_object(ArrayObject([writer._add_object(N('/FlateDecode'))]))
  a['DecodeParms']=writer._add_object(ArrayObject([writer._add_object(D(Predictor=writer._add_object(NumberObject(12)),Columns=NumberObject(w),Colors=NumberObject(3),BitsPerComponent=writer._add_object(NumberObject(8))))]))
  a['BitsPerComponent']=writer._add_object(NumberObject(8));a['Width']=writer._add_object(NumberObject(w));a['ColorSpace']=writer._add_object(N('/DeviceRGB'))
 image=stream(writer,data,**a);page(writer,{'I':image});save(writer,'rgb-'+kind)
# Deterministic photographic/noise negative control.
rng=np.random.default_rng(102030);noise=rng.integers(0,256,(h,w,3),dtype='uint8').tobytes()
writer=PdfWriter();page(writer,{'I':stream(writer,noise,**attrs)});save(writer,'rgb-noise')
# JPEG terminal is identical between wrapper/property representations.
buf=io.BytesIO();Image.frombytes('RGB',(w,h),raw).save(buf,format='JPEG',quality=94)
for kind in ['jpeg','jpeg-indirect']:
 writer=PdfWriter();a=attrs.copy();a['Filter']=N('/DCTDecode');data=buf.getvalue()
 if kind=='jpeg-indirect':
  a['Filter']=writer._add_object(ArrayObject([N('/ASCII85Decode'),writer._add_object(N('/DCTDecode'))]));a['DecodeParms']=writer._add_object(ArrayObject([NullObject(),writer._add_object(DictionaryObject())]));a['BitsPerComponent']=writer._add_object(NumberObject(8));data=base64.a85encode(data)+b'~>'
 page(writer,{'I':stream(writer,data,**a)});save(writer,'rgb-'+kind)
# Shared resources across every page; last-page geometry cannot be ignored.
for count in [10,100]:
 writer=PdfWriter();image=stream(writer,zlib.compress(raw,1),Filter=N('/FlateDecode'),**attrs)
 for n in range(count):
  p=page(writer,{'I':image},n+1)
  if n==count-1:
   p[N('/CropBox')]=A([5,5,115,85]);p[N('/Rotate')]=NumberObject(90);p[N('/UserUnit')]=NumberObject(2)
 save(writer,f'shared-{count}-pages')
print(json.dumps({'created':10,'pages':[1,10,100],'rgb_colors':len(set(tuple(p) for p in np.frombuffer(raw,dtype='uint8').reshape(-1,3)))}))
