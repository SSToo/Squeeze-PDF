"""Development-only wrapped G4 and tagged /Stm reference fixtures.
Run after make_general_samples / make_stream_samples; never reads user PDFs.
"""
import base64
import sys
from pathlib import Path
from pypdf import PdfReader, PdfWriter
from pypdf.generic import (
    ArrayObject, BooleanObject, DecodedStreamObject, DictionaryObject,
    NameObject, NullObject, NumberObject,
)
root = Path(sys.argv[1])
N = NameObject
D = lambda **items: DictionaryObject({N('/' + k): v for k, v in items.items()})
for kind in ['hex', 'ascii85']:
    writer = PdfWriter()
    writer.clone_document_from_reader(PdfReader(root / 'g4.pdf'))
    for reference in writer.pages[0]['/Resources']['/XObject'].values():
        stream = reference.get_object()
        if stream.get('/Subtype') != '/Image':
            continue
        assert stream['/Filter'] == '/CCITTFaxDecode'
        payload, params = stream._data, stream.get('/DecodeParms', NullObject())
        stream[N('/Filter')] = ArrayObject([
            N('/ASCIIHexDecode' if kind == 'hex' else '/ASCII85Decode'), N('/CCITTFaxDecode'),
        ])
        stream[N('/DecodeParms')] = ArrayObject([NullObject(), params])
        stream._data = payload.hex().encode() + b'>' if kind == 'hex' else base64.a85encode(payload) + b'~>'
    with (root / (kind + '-g4-image.pdf')).open('wb') as file:
        writer.write(file)
writer = PdfWriter()
page = writer.add_blank_page(320, 240)
font = writer._add_object(D(Type=N('/Font'), Subtype=N('/Type1'), BaseFont=N('/Helvetica')))
page[N('/Resources')] = D(Font=D(F=font))
parts = []
for data in [
    b'/P << /MCID 0 >> BDC BT /F 12 Tf 10 170 Td (Tagged stream identity) Tj ET EMC\n',
    b'q 0.2 0.4 0.6 rg 10 10 80 80 re f Q\n',
]:
    stream = DecodedStreamObject()
    stream.set_data(data)
    parts.append(writer._add_object(stream))
page[N('/Contents')] = ArrayObject(parts)
page[N('/StructParents')] = NumberObject(0)
tree = D(Type=N('/StructTreeRoot'))
tree_ref = writer._add_object(tree)
element = writer._add_object(D(
    Type=N('/StructElem'), S=N('/P'), P=tree_ref, Pg=page.indirect_reference,
    K=D(Type=N('/MCR'), Pg=page.indirect_reference, Stm=parts[0], MCID=NumberObject(0)),
))
parents = writer._add_object(D(Nums=ArrayObject([NumberObject(0), ArrayObject([element])])))
tree.update(D(K=element, ParentTree=parents, ParentTreeNextKey=NumberObject(1)))
writer.root_object.update(D(StructTreeRoot=tree_ref, MarkInfo=D(Marked=BooleanObject(True))))
with (root / 'tagged-stm-contents.pdf').open('wb') as file:
    writer.write(file)
