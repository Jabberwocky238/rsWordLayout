"""Number the 'P000 line' labels of the two beforeLines follow-ups (capture queue item 4, 2026-10-04)
so each paragraph is identifiable on a screenshot. Run after mkfx.py; writes <name>-n.docx."""
import zipfile, re, itertools
from mkfx import OUT
for name in ["before-lines-sz48", "before-lines-grid312"]:
    z = zipfile.ZipFile(f"{OUT}/{name}.docx"); files = {n: z.read(n) for n in z.namelist()}
    x = files["word/document.xml"].decode(); c = itertools.count()
    x = re.sub(r"P000 line", lambda m: f"P{next(c):03d} line", x)
    files["word/document.xml"] = x.encode()
    with zipfile.ZipFile(f"{OUT}/{name}-n.docx", "w", zipfile.ZIP_DEFLATED) as o:
        for n, d in files.items(): o.writestr(n, d)
    print(name, "paragraphs", next(c))
