"""contextualSpacing capacity fixtures (capture queue item 8, 2026-10-04).
Each paragraph is one line 'Pnnn', exact 480, Calibri 12pt; reading = paragraphs on print page 1."""
import zipfile
import os
OUT = os.environ.get("RSWL_FX_OUT", "/tmp/rswl-cap/fx"); os.makedirs(OUT, exist_ok=True)
WA = os.path.join(os.environ.get("WORD_ANALYSE", os.path.expanduser("~/code/word_analyse")), "fixtures")
H = 15398  # body height
CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
      '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
DRELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
         '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>')
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
STY = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W}>'
       + "".join(f'<w:style w:type="paragraph" w:customStyle="1" w:styleId="Probe{x}"><w:name w:val="Probe{x}"/>'
                 '<w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr></w:style>' for x in "AB")
       + '</w:styles>')
SECT = ('<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>')
def para(i, style, ctx, before, after):
    c = '<w:contextualSpacing/>' if ctx else '<w:contextualSpacing w:val="0"/>'
    return (f'<w:p><w:pPr><w:pStyle w:val="Probe{style}"/>{c}<w:spacing w:before="{before}" w:after="{after}" w:line="480" w:lineRule="exact"/>'
            f'<w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr><w:t>P{i:03d}</w:t></w:r></w:p>')
def write(name, specs):
    body = "".join(para(i, *s) for i, s in enumerate(specs))
    with zipfile.ZipFile(f"{OUT}/{name}.docx", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT); z.writestr("_rels/.rels", RELS)
        z.writestr("word/_rels/document.xml.rels", DRELS); z.writestr("word/styles.xml", STY)
        z.writestr("word/document.xml", f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}{SECT}</w:body></w:document>')
N = 70
CASES = {
    # all contextual, same style, after only: is a paragraph's own after dropped?
    "cs-own-after":  [("A", True, 0, 240)] * N,
    # all contextual, same style, before only: own before dropped?
    "cs-own-before": [("A", True, 240, 0)] * N,
    # only the FOLLOWING paragraph is contextual: does it drop the previous one's after?
    "cs-next-flag":  [("A", i % 2 == 1, 0, 240 if i % 2 == 0 else 0) for i in range(N)],
    # only the PRECEDING paragraph is contextual: does it drop the next one's before?
    "cs-prev-flag":  [("A", i % 2 == 0, 240 if i % 2 == 1 else 0, 0) for i in range(N)],
    # all contextual, after only, alternating styles: style identity required?
    "cs-diff-style": [("AB"[i % 2], True, 0, 240) for i in range(N)],
}
def gap(p, q, model):
    sp, cp, _, ap = p; sq, cq, bq, _ = q
    same = sp == sq
    if model == "own":      # each paragraph's flag drops its own side
        a = 0 if (cp and same) else ap; b = 0 if (cq and same) else bq
    elif model == "either":  # either flag drops both sides
        drop = (cp or cq) and same
        a = 0 if drop else ap; b = 0 if drop else bq
    elif model == "none":
        a, b = ap, bq
    return max(a, b)  # Android: before/after collapse to the larger
def page1(specs, model):
    y = specs[0][2]  # page-top before counts
    n = 0
    for i, s in enumerate(specs):
        if i:
            y += gap(specs[i-1], s, model)
        if y + 480 > H: break
        y += 480; n += 1
    return n
if __name__ == "__main__":
    for name, specs in CASES.items():
        write(name, specs)
        print(f"{name:14s}", "  ".join(f"{m}={page1(specs, m)}" for m in ["own", "either", "none"]))
