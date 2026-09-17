#!/usr/bin/env python3
"""纵向杂项夹具：段落间距、`w:position`、`atLeast`（清单 V9 / V12 / V8b）。

纵向的 A（V19）还没解出来，但这三项**都能用差值绕开它**：

- **段落间距**：同设置两段的首行基线之差，有间距时比无间距时多的那一截，
  就是 `w:spacing/@after + @before`。A 在相减时消掉。
- **`w:position`**：同一行内，抬升 run 与普通 run 的字形 y 之差就是抬升量。
  一行之内，A 根本不参与。
- **`atLeast`**：`line` 取得足够大时（大于自然行高），`atLeast` 应当与 `exact`
  给出**同一条基线序列**。比的是两种规则的输出，不必知道 A。

行距一律 `exact`（已确立与字体、字号无关），好让「行高」这一项是已知量。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, para, run, sect_pr
from wordmeasure.fontcover import assert_can_draw

FONT_DIR = "/System/Library/Fonts/Supplemental/"
FONTS = {"Luminari": FONT_DIR + "Luminari.ttf"}
SIZE_HALF_POINTS = 26
LINE_TWIPS = 320                  # 16pt，`exact`
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz"

SPACES = [0, 120, 240, 480]       # 段后间距 twips：0 / 6 / 12 / 24 pt
POSITIONS = [0, 6, 12, -6, -12]   # `w:position` 半点，含负值（下沉）
AT_LEAST_LINES = [400, 520, 640]  # atLeast 的 line，远大于 13pt 的自然行高


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqrstuvwx"
    i = 0

    # A 组：段落间距。每组三段，量第 1→2、2→3 的首行基线差。
    for sp in SPACES:
        tag = tags[i]; i += 1
        for k in range(3):
            parts.append(para(run(f"{tag}{'abc'[k]}", SIZE_HALF_POINTS, family),
                              SIZE_HALF_POINTS, family,
                              page_break=(k == 0 and len(parts) > 0),
                              line=LINE_TWIPS, line_rule="exact", space_after=sp))
        probes.append({"tag": tag, "kind": "space-after", "spaceAfter": sp,
                       "lineTwips": LINE_TWIPS, "sizeHalfPoints": SIZE_HALF_POINTS,
                       "family": family, "lines": 3})

    # B 组：`w:position`。一行里普通 run + 抬升 run。
    for pos in POSITIONS:
        tag = tags[i]; i += 1
        extra = f'<w:position w:val="{pos}"/>' if pos else ""
        parts.append(para(
            run(f"{tag}nn", SIZE_HALF_POINTS, family)
            + run("rr", SIZE_HALF_POINTS, family, extra),
            SIZE_HALF_POINTS, family, page_break=True,
            line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "position", "positionHalfPoints": pos,
                       "lineTwips": LINE_TWIPS, "sizeHalfPoints": SIZE_HALF_POINTS,
                       "family": family, "lines": 1})

    # C 组：`atLeast` vs `exact`，同 line 值各排 6 行。
    for ln in AT_LEAST_LINES:
        for rule in ("atLeast", "exact"):
            tag = tags[i]; i += 1
            for k in range(6):
                parts.append(para(run(f"{tag}{'abcdef'[k]}", SIZE_HALF_POINTS, family),
                                  SIZE_HALF_POINTS, family,
                                  page_break=(k == 0), line=ln, line_rule=rule))
            probes.append({"tag": tag, "kind": "at-least", "rule": rule,
                           "lineTwips": ln, "sizeHalfPoints": SIZE_HALF_POINTS,
                           "family": family, "lines": 6})

    return f"<w:body>{''.join(parts)}{sect_pr('continuous')}</w:body>", probes


def write_docx(path: Path) -> list[dict]:
    body, probes = build_body()
    document = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
                f'<w:document xmlns:w="{W}" xmlns:r="{R}">{body}</w:document>')
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>")
    rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
            "</Relationships>")
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in (("[Content_Types].xml", content_types),
                           ("_rels/.rels", rels), ("word/document.xml", document)):
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, data)
    return probes


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("output")
    args = ap.parse_args()
    path = Path(args.output)
    probes = write_docx(path)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    for kind in ("space-after", "position", "at-least"):
        print(f"  {kind:<14}{sum(1 for p in probes if p['kind']==kind)} 组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
