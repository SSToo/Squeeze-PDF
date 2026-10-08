"""Development-only exact binary and reversible JP2 comparison fixtures.
Requires Pillow with OpenJPEG and pypdf; never thresholds existing user pages.
"""
from pathlib import Path
import argparse
p=argparse.ArgumentParser();p.add_argument("font");p.add_argument("directory");args=p.parse_args();root=Path(args.directory);root.mkdir(parents=True,exist_ok=True)
from PIL import Image,ImageDraw,ImageFont,features
from pypdf import PdfWriter
from pypdf.generic import DictionaryObject as D,NameObject as N,NumberObject as I,ArrayObject as A,DecodedStreamObject
import random,json,io
w,h=1729,2201;im=Image.new('1',(w,h),1);d=ImageDraw.Draw(im);font=ImageFont.truetype(args.font,24)
r=random.Random(7)
for y in range(35,h-40,35):
 text=' '.join(str(r.randrange(100000,999999)) for _ in range(13))
 d.text((25,y),text,font=font,fill=0)
 for x in [18,w-18]:d.line((x,y,x,y+30),fill=0)
raw=im.convert('L').tobytes();packed=im.tobytes()
for bits,data in [(8,raw),(1,packed)]:
 wr=PdfWriter();p=wr.add_blank_page(w*.72,h*.72)
 s=DecodedStreamObject();s.set_data(data);s.update({N('/Type'):N('/XObject'),N('/Subtype'):N('/Image'),N('/Width'):I(w),N('/Height'):I(h),N('/BitsPerComponent'):I(bits),N('/ColorSpace'):N('/DeviceGray')});ref=wr._add_object(s)
 p[N('/Resources')]=D({N('/XObject'):D({N('/I'):ref})})
 c=DecodedStreamObject();c.set_data(f'q {w*.72} 0 0 {h*.72} 0 0 cm /I Do Q'.encode());p[N('/Contents')]=wr._add_object(c)
 wr.write(root/f'scan-{bits}bit.pdf')
# Development-only JPEG2000 reversible transform, same grayscale samples.
b=io.BytesIO();im.convert('L').save(b,format='JPEG2000',irreversible=False,no_jp2=False)
encoded=b.getvalue();assert Image.open(io.BytesIO(encoded)).tobytes()==raw
wr=PdfWriter();p=wr.add_blank_page(w*.72,h*.72);s=DecodedStreamObject();s.set_data(encoded);s.update({N('/Type'):N('/XObject'),N('/Subtype'):N('/Image'),N('/Width'):I(w),N('/Height'):I(h),N('/BitsPerComponent'):I(8),N('/ColorSpace'):N('/DeviceGray'),N('/Filter'):N('/JPXDecode')});p[N('/Resources')]=D({N('/XObject'):D({N('/I'):wr._add_object(s)})});c=DecodedStreamObject();c.set_data(f'q {w*.72} 0 0 {h*.72} 0 0 cm /I Do Q'.encode());p[N('/Contents')]=wr._add_object(c);wr.write(root/'scan-jpx.pdf')
json.dump({'width':w,'height':h,'raw_bytes':len(raw),'jpx_bytes':len(encoded),'openjpeg':features.version_codec('jpg_2000')},open(root/'jpx-experiment.json','w'))
