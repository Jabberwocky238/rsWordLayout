"""Items 5/6/7 of the capture queue (2026-10-04). """
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkfx import doc, run, CAL
from cmetrics import load, width
f = load(os.environ.get('RSWL_PHONE_CALIBRI', os.path.expanduser('~/.local/share/rswl/phonefonts/fonts-calibri.ttf')))
LINE = 10466
A = "0" * 40
base = width(A + " ", 12, f)
def pick(d):
    best = None
    for a in range(0, 50):
        for b in range(0, 40):
            e = base + width("0" * a + "i" * b, 12, f) - LINE
            if best is None or abs(e - d) < abs(best[0] - d): best = (e, a, b)
    return best
R = f'{CAL}<w:sz w:val="24"/>'
RED = f'{CAL}<w:color w:val="C00000"/><w:sz w:val="24"/>'
paras, rows = [], []
for n, d in enumerate([-45, -27, -10, 5, 15, 30]):
    e, a, b = pick(d)
    word = "0" * a + "i" * b
    rows.append((n + 1, d, round(e, 2), a, b))
    paras.append("<w:p>" + run(f"{n+1}" + A[1:] + " ", R) + run(word, RED) + run(" 000 000", R) + "</w:p><w:p/>")
for r in rows: print("para %d target %+d actual %+.2f  (%d zeros + %d i)" % r)
doc("hang-tol", "".join(paras))
# item 6: w:kern threshold above the run size; control without w:kern
COLORS = ["C00000", "00A000", "0000FF"]
def av(kern):
    k = f'<w:kern w:val="{kern}"/>' if kern else ""
    s = "AV" * 100
    return "<w:p>" + "".join(run(s[g:g+10], f'{CAL}<w:color w:val="{COLORS[(g//10)%3]}"/>{k}<w:sz w:val="24"/>') for g in range(0, 200, 10)) + "</w:p><w:p/>"
doc("kern-48", av(48) + av(None))
# item 6 redesign: one-line vs two-line readout. "AV"*41 kerned = 10399 twips (fits 10466), unkerned 11277.
def av41(kern, split=False):
    k = f'<w:kern w:val="{kern}"/>' if kern else ""
    s = "AV" * 41
    if not split:
        return "<w:p>" + run(s, f'{CAL}{k}<w:sz w:val="24"/>') + "</w:p><w:p/>"
    # runs break at every 10th letter (always a V|A boundary), alternating colors
    return "<w:p>" + "".join(run(s[g:g+10], f'{CAL}<w:color w:val="{COLORS[(g//10)%3]}"/>{k}<w:sz w:val="24"/>') for g in range(0, 82, 10)) + "</w:p><w:p/>"
doc("kern-48", av41(48) + av41(None) + av41(None, split=True) + av41(2))
