"""Follow-ups to item 8 (2026-10-04): both-phase conflict and root-related styles."""
import os, zipfile, shutil, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mkcs
from mkcs import OUT, W, SECT, RELS, DRELS, CT
N = 60
# both-phase replicas: before300 after100 exact400, even paragraphs contextual
def para_bp(i, explicit_off):
    ctx = i % 2 == 0
    c = '<w:contextualSpacing/>' if ctx else ('<w:contextualSpacing w:val="0"/>' if explicit_off else '')
    return (f'<w:p><w:pPr><w:pStyle w:val="ProbeA"/>{c}<w:spacing w:before="300" w:after="100" w:line="400" w:lineRule="exact"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr><w:t>P{i:03d}</w:t></w:r></w:p>')
def write(name, body, styles=True, doc_rel=True):
    with zipfile.ZipFile(f"{OUT}/{name}.docx", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT)
        if styles and not doc_rel:
            z.writestr("_rels/.rels", RELS.replace('</Relationships>',
                '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="word/styles.xml"/></Relationships>'))
        else:
            z.writestr("_rels/.rels", RELS)
        if styles and doc_rel:
            z.writestr("word/_rels/document.xml.rels", DRELS)
        if styles:
            z.writestr("word/styles.xml", STY)
        z.writestr("word/document.xml", f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}{SECT}</w:body></w:document>')
STY = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W}>'
       '<w:style w:type="paragraph" w:customStyle="1" w:styleId="ProbeA"><w:name w:val="ProbeA"/></w:style>'
       '<w:style w:type="paragraph" w:customStyle="1" w:styleId="Big"><w:name w:val="Big"/>'
       '<w:pPr><w:spacing w:before="0" w:after="0" w:line="720" w:lineRule="exact"/></w:pPr></w:style></w:styles>')
write("bp-explicit", "".join(para_bp(i, True) for i in range(N)))
write("bp-absent", "".join(para_bp(i, False) for i in range(N)))
shutil.copy(f"{mkcs.WA}/context-both-phase.docx", f"{OUT}/bp-original.docx")
big = "".join(f'<w:p><w:pPr><w:pStyle w:val="Big"/></w:pPr><w:r><w:t>P{i:03d}</w:t></w:r></w:p>' for i in range(N))
write("sr-root", big, doc_rel=False)
write("sr-doc", big, doc_rel=True)
# predictions for both-phase under own-side + max (top before kept)
def page1(gT_F, gF_T, top=300, line=400, H=15398):
    y, n = top, 0
    for i in range(N):
        if i: y += gT_F if (i - 1) % 2 == 0 else gF_T
        if y + line > H: break
        y += line; n += 1
    return n
print("both-phase own-side+max:", page1(300, 100), " old-report model:", page1(0, 300), "/", page1(300, 0))
