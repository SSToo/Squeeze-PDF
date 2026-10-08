"""Development-only CID mapping fixture; requires fontTools and pypdf.
Supply a local TrueType font. Generated font files stay in the chosen directory.
"""
import sys,json,io
from pathlib import Path
import argparse
a=argparse.ArgumentParser();a.add_argument("font");a.add_argument("directory");args=a.parse_args();root=Path(args.directory);root.mkdir(parents=True,exist_ok=True)
from fontTools.ttLib import TTFont
from pypdf import PdfWriter
from pypdf.generic import DictionaryObject as D,NameObject as N,NumberObject as I,ArrayObject as A,DecodedStreamObject,TextStringObject as S
font_path=args.font
f=TTFont(font_path);raw=open(font_path,'rb').read(); cmap=f.getBestCmap();order=f.getGlyphOrder();upem=f['head'].unitsPerEm
text='Squeeze PDF 0123456789 office café Ångström '
gids={ord(c):f.getGlyphID(cmap[ord(c)]) for c in text}
# Nonidentity CIDs prove the experiment must rewrite CIDToGIDMap.
rows={300+i:gid for i,gid in enumerate(sorted(set(gids.values())))};reverse={gid:cid for cid,gid in rows.items()}
writer=PdfWriter();page=writer.add_blank_page(600,800)
def addstream(data,extra={}):
 s=DecodedStreamObject();s.set_data(data);s.update(extra);return writer._add_object(s)
file=addstream(raw,{N('/Length1'):I(len(raw))})
head=f['head'];desc=writer._add_object(D({N('/Type'):N('/FontDescriptor'),N('/FontName'):N('/Arial'),N('/Flags'):I(32),N('/FontBBox'):A([I(getattr(head,x)*1000//upem) for x in ['xMin','yMin','xMax','yMax']]),N('/ItalicAngle'):I(0),N('/Ascent'):I(950),N('/Descent'):I(-250),N('/CapHeight'):I(700),N('/StemV'):I(80),N('/FontFile2'):file}))
mapdata=bytearray((max(rows)+1)*2)
for cid,gid in rows.items():mapdata[cid*2:cid*2+2]=gid.to_bytes(2,'big')
widths=A()
for cid,gid in rows.items():widths.extend([I(cid),A([I(round(f['hmtx'].metrics[order[gid]][0]*1000/upem))])])
cidfont=writer._add_object(D({N('/Type'):N('/Font'),N('/Subtype'):N('/CIDFontType2'),N('/BaseFont'):N('/Arial'),N('/CIDSystemInfo'):D({N('/Registry'):S('Adobe'),N('/Ordering'):S('Identity'),N('/Supplement'):I(0)}),N('/FontDescriptor'):desc,N('/CIDToGIDMap'):addstream(mapdata),N('/W'):widths}))
tounicode=['/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /ArialUCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange',str(len(gids))+' beginbfchar']
for code,gid in gids.items():tounicode.append(f'<{reverse[gid]:04X}> <{code:04X}>')
tounicode+=['endbfchar endcmap CMapName currentdict /CMap defineresource pop end end']
font=writer._add_object(D({N('/Type'):N('/Font'),N('/Subtype'):N('/Type0'),N('/BaseFont'):N('/Arial'),N('/Encoding'):N('/Identity-H'),N('/DescendantFonts'):A([cidfont]),N('/ToUnicode'):addstream('\n'.join(tounicode).encode())}))
page[N('/Resources')]=D({N('/Font'):D({N('/F'):font})})
body=b'BT /F 22 Tf 36 740 Td 32 TL\n'+b'\n'.join(b'<'+''.join(f'{reverse[gids[ord(c)]]:04X}' for c in text).encode()+b'> Tj T*' for _ in range(18))+b'\nET'
page[N('/Contents')]=addstream(body)
writer.write(root/'font-original.pdf')
json.dump(rows,open(root/'font-map.json','w'))
json.dump({'cid_font':cidfont.idnum,'gids':len(rows),'original_font_bytes':len(raw)},open(root/'font-fixture.json','w'))
print(cidfont.idnum)
