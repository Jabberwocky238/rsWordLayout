#!/usr/bin/env python3
"""横向组合夹具：右对齐、两端对齐、字符级字距（清单 H3c / H3d / H4）。

三条预测都**绕开了字体度量**，各有各的绕法：

- **右对齐 / 两端对齐**判的是「行尾贴不贴右边界」。右边界是
  `72pt + 可用宽 = 523.3pt`，一个**常数**——不必知道行宽，
  只要末字形的右缘（实测 x + 实测 advance）落在那里。
- **字距**判的是**有无字距的差**：同一段文本排两遍，一遍带 `w:spacing`，
  一遍不带，相邻字形的 x 差之差应当恰好等于 `w:spacing`。
  字形本身的推进量在相减时消掉。

页面：A4 宽 11906 twips，左右边距各 1440 → 可用宽 9026 twips = **451.3pt**，
右边界 = 72 + 451.3 = **523.3pt**。
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
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz "

RIGHT_TEXTS = ["ab", "abcdefgh", "abcdefghijklmnop", "abcdefghijklmnopqrstuvwx"]
# 两端对齐要换行，用重复的词把行填满。
JUSTIFY_WORDS = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda "
# 字符级字距，半点。0 是对照。
SPACINGS = [0, 20, 40, 80]


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []

    # R 组：右对齐，各占一页。
    for j, text in enumerate(RIGHT_TEXTS):
        tag = f"r{j}"
        parts.append(para(run(f"{tag} {text}", SIZE_HALF_POINTS, family),
                          SIZE_HALF_POINTS, family,
                          page_break=len(parts) > 0, align="right"))
        probes.append({"tag": tag, "kind": "right", "textIndex": j,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family, "lines": 1})

    # J 组：两端对齐，长到必然换行。
    for j, reps in enumerate((3, 5)):
        tag = f"t{j}"
        parts.append(para(run(f"{tag} " + JUSTIFY_WORDS * reps, SIZE_HALF_POINTS, family),
                          SIZE_HALF_POINTS, family,
                          page_break=True, align="both"))
        probes.append({"tag": tag, "kind": "justify", "reps": reps,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family})

    # S 组：字符级字距。同一文本排四遍，`0` 是对照。
    for sp in SPACINGS:
        tag = f"s{sp:02d}"
        extra = f'<w:spacing w:val="{sp}"/>' if sp else ""
        parts.append(para(run(f"{tag} abcdefgh", SIZE_HALF_POINTS, family, extra),
                          SIZE_HALF_POINTS, family, page_break=True, align="left"))
        probes.append({"tag": tag, "kind": "letter-spacing", "spacingHalfPoints": sp,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family, "lines": 1})

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
    for kind in ("right", "justify", "letter-spacing"):
        print(f"  {kind:<16}{sum(1 for p in probes if p['kind']==kind)} 组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
