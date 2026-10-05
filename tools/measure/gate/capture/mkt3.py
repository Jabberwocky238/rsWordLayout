"""talltable-080 and footnote follow-ups (2026-10-04)."""
import zipfile, re, shutil
import os
OUT = os.environ.get("RSWL_FX_OUT", "/tmp/rswl-cap/fx"); os.makedirs(OUT, exist_ok=True)
WA = os.path.join(os.environ.get("WORD_ANALYSE", os.path.expanduser("~/code/word_analyse")), "fixtures")
src = zipfile.ZipFile(f"{WA}/talltable-080.docx")
doc = src.read("word/document.xml").decode()
def variant(name, f):
    x = f(doc)
    assert x != doc or name == "tt-orig"
    with zipfile.ZipFile(f"{OUT}/{name}.docx", "w", zipfile.ZIP_DEFLATED) as z:
        for n in src.namelist():
            z.writestr(n, x if n == "word/document.xml" else src.read(n))
cellsp = lambda x: x.replace('<w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:line="480" w:lineRule="exact"/>',
                             '<w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/>')
nob = lambda x: re.sub(r'<w:tblBorders>.*?</w:tblBorders>', '<w:tblBorders>' + ''.join(f'<w:{s} w:val="nil"/>' for s in ["top","left","bottom","right","insideH","insideV"]) + '</w:tblBorders>', x)
zm = lambda x: x.replace('</w:tblBorders></w:tblPr>', '</w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>')
variant("tt-orig", lambda x: x)
variant("tt-sp0", cellsp)
variant("tt-sp0-nob", lambda x: nob(cellsp(x)))
variant("tt-sp0-nob-m0", lambda x: zm(nob(cellsp(x))))
# body paragraphs without any spacing in a plain document: is there a default space after?
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
CT0 = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{extra}</Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
SECT = ('<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>')
def write(name, body, footnotes=None):
    with zipfile.ZipFile(f"{OUT}/{name}.docx", "w", zipfile.ZIP_DEFLATED) as z:
        extra = '<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>' if footnotes else ''
        z.writestr("[Content_Types].xml", CT0.format(extra=extra)); z.writestr("_rels/.rels", RELS)
        if footnotes:
            z.writestr("word/_rels/document.xml.rels", '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>')
            z.writestr("word/footnotes.xml", f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {W}>{footnotes}</w:footnotes>')
        z.writestr("word/document.xml", f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}{SECT}</w:body></w:document>')
write("sp-default", "".join(f'<w:p><w:pPr><w:spacing w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>P{i:03d}</w:t></w:r></w:p>' for i in range(60)))
# footnotes: body rows exact 240, 10pt labels; footnote reference(s) on the first row(s)
def body(refs):
    return "".join(f'<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>P{i:03d}</w:t></w:r>'
                   + (f'<w:r><w:footnoteReference w:id="{refs.index(i)+1}"/></w:r>' if i in refs else '') + '</w:p>' for i in range(90))
def sep(sp=""):
    return (f'<w:footnote w:type="separator" w:id="-1"><w:p>{sp}<w:r><w:separator/></w:r></w:p></w:footnote>'
            '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>')
def fn(i, lines=1, line=480):
    ps = "".join(f'<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="{line}" w:lineRule="exact"/></w:pPr><w:r><w:t>Footnote {i} line {k}.</w:t></w:r></w:p>' for k in range(lines))
    return f'<w:footnote w:id="{i}">{ps}</w:footnote>'
write("fn-base", body([0]), sep() + fn(1))
write("fn-two-lines", body([0]), sep() + fn(1, lines=2))
write("fn-line240", body([0]), sep() + fn(1, line=240))
write("fn-sep20", body([0]), sep('<w:pPr><w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/></w:pPr>') + fn(1))
write("fn-two-notes", body([0, 1]), sep() + fn(1) + fn(2))
write("fn-none", body([]), sep() + fn(1))
