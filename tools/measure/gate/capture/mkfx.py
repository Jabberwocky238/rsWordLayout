"""Fixtures for the 2026-10-04 phone readings ."""
import zipfile, re
import os
OUT = os.environ.get("RSWL_FX_OUT", "/tmp/rswl-cap/fx"); os.makedirs(OUT, exist_ok=True)
WA = os.path.join(os.environ.get("WORD_ANALYSE", os.path.expanduser("~/code/word_analyse")), "fixtures")
CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
SECT = ('<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
        'w:header="720" w:footer="720" w:gutter="0"/>{grid}</w:sectPr>')
def doc(name, body, grid=""):
    xml = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
           + body + SECT.format(grid=grid) + '</w:body></w:document>')
    with zipfile.ZipFile(f"{OUT}/{name}.docx", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT); z.writestr("_rels/.rels", RELS); z.writestr("word/document.xml", xml)
def run(text, rpr=""):
    return f'<w:r>{"<w:rPr>" + rpr + "</w:rPr>" if rpr else ""}<w:t xml:space="preserve">{text}</w:t></w:r>'
CAL = '<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/>'
COLORS = ["C00000", "00A000", "0000FF"]
def colored(ch, n, size, group=10):
    runs = []
    for g in range(0, n, group):
        runs.append(run(ch * min(group, n - g), f'{CAL}<w:color w:val="{COLORS[(g // group) % 3]}"/><w:sz w:val="{size}"/>'))
    return "<w:p>" + "".join(runs) + "</w:p>"
DIG = "0123456789"
# default font/size: no rPr at all
doc("dig-nofont", "<w:p>" + run(DIG * 20) + "</w:p>")
# item 3: narrow pixel widths
for ch, size in [("i", 19), ("i", 23), ("i", 27)]:
    doc(f"px-i-{size}", colored(ch, 300, size))
doc("px-0-29", "<w:p>" + run(DIG * 15, f"{CAL}<w:sz w:val=\"29\"/>") + "</w:p>")
doc("px-M-25", colored("M", 120, 25, group=5))
# item 9: ind-left in true print view, digits instead of zeros (Calibri digits are tabular)
doc("ind-left-dig", '<w:p><w:pPr><w:ind w:left="720"/></w:pPr>' + run(DIG * 10, f'{CAL}<w:sz w:val="24"/>') + "</w:p>")
# item 4: beforeLines variants of word_analyse before-lines.docx
src = zipfile.ZipFile(f"{WA}/before-lines.docx").read("word/document.xml").decode()
print("before-lines sample:", re.findall(r"<w:p>.*?</w:p>", src)[0][:300])
def variant(name, transform):
    x = transform(src)
    with zipfile.ZipFile(f"{OUT}/{name}.docx", "w", zipfile.ZIP_DEFLATED) as z:
        for n in zipfile.ZipFile(f"{WA}/before-lines.docx").namelist():
            z.writestr(n, x if n == "word/document.xml" else zipfile.ZipFile(f"{WA}/before-lines.docx").read(n))
variant("before-lines-sz48", lambda x: re.sub(r"<w:r>(?!<w:rPr>)", '<w:r><w:rPr><w:sz w:val="48"/></w:rPr>', x))
variant("before-lines-grid312", lambda x: x.replace("</w:pgMar>", "</w:pgMar>").replace("<w:pgMar", "<w:pgMar", 1).replace("</w:sectPr>", '<w:docGrid w:type="lines" w:linePitch="312"/></w:sectPr>'))
