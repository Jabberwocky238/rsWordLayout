#!/usr/bin/env python3
"""纵向杂项第二批：抬升量的量化、同行多字号、分页符计数（V24 / V10 / C3）。

**V24 抬升量量化**：vmisc 的 P2 把分界摆得很清楚——`w:position = ±12` 半点
（±6pt = ±25 格，整格）两条全中，`±6` 半点（±3pt = **12.5 格**）两条全错，
偏半格。既然所有基线都在 0.24pt 栅格上（V1），抬升后的字形 y 也在栅格上，
抬升量**必然**是 0.24 的整数倍。本批判**量化之后**的形式，取值刻意半数落在整格上、
半数落不上。

**V10 同行多字号**：前十九批每行只有一个字号。这一项不必等 V19——
判的是「同一行内不同字号的字形**共用一条基线**」，一行之内 A 不参与。

**C3 分页符三分**：`w:br w:type="page"` 按位置画 0 或 1 个字形（量具方法 §4）。
三种位置各造一组，判量具的计数模型与实测字形数对得上。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, esc, para, rpr, run, sect_pr
from wordmeasure.fontcover import assert_can_draw

FONT_DIR = "/System/Library/Fonts/Supplemental/"
FONTS = {"Luminari": FONT_DIR + "Luminari.ttf"}
SIZE_HALF_POINTS = 26
LINE_TWIPS = 320
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz"

# `w:position` 半点。24/48 是整格（12pt=50 格、24pt=100 格），
# 6/18/30 不是（3pt=12.5、9pt=37.5、15pt=62.5 格）。
POSITIONS = [6, 18, 24, 30, 48]
# 同一行里混用的字号，半点。
MIXED_SIZES = [(20, 40), (26, 52), (16, 60)]


def page_break_run() -> str:
    """一个只含手动分页符的 run。"""
    return f"<w:r>{rpr(SIZE_HALF_POINTS, next(iter(FONTS)))}<w:br w:type=\"page\"/></w:r>"


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqrstuvwx"
    i = 0

    # A 组：抬升量量化。
    for pos in POSITIONS:
        tag = tags[i]; i += 1
        parts.append(para(
            run(f"{tag}nn", SIZE_HALF_POINTS, family)
            + run("rr", SIZE_HALF_POINTS, family, f'<w:position w:val="{pos}"/>'),
            SIZE_HALF_POINTS, family, page_break=len(parts) > 0,
            line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "rise-quantised", "positionHalfPoints": pos,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family, "lines": 1})

    # B 组：同一行内两种字号。
    for small, big in MIXED_SIZES:
        tag = tags[i]; i += 1
        parts.append(para(
            run(f"{tag}ss", small, family) + run("bb", big, family)
            + run("tt", small, family),
            SIZE_HALF_POINTS, family, page_break=True,
            line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "mixed-size", "small": small, "big": big,
                       "family": family, "lines": 1})

    # C 组：分页符三种位置（量具方法 §4 的三分）。
    # 1) 独占一段：整段只有一个分页符。
    tag = tags[i]; i += 1
    parts.append(para(page_break_run(), SIZE_HALF_POINTS, family, line=LINE_TWIPS,
                      line_rule="exact"))
    parts.append(para(run(f"{tag}after", SIZE_HALF_POINTS, family), SIZE_HALF_POINTS,
                      family, line=LINE_TWIPS, line_rule="exact"))
    probes.append({"tag": tag, "kind": "break-own-line", "family": family})

    # 2) 紧跟段落标记：文字 + 分页符，然后段落结束。
    tag = tags[i]; i += 1
    parts.append(para(run(f"{tag}before", SIZE_HALF_POINTS, family) + page_break_run(),
                      SIZE_HALF_POINTS, family, line=LINE_TWIPS, line_rule="exact"))
    parts.append(para(run(f"{tag}next", SIZE_HALF_POINTS, family), SIZE_HALF_POINTS,
                      family, line=LINE_TWIPS, line_rule="exact"))
    probes.append({"tag": tag, "kind": "break-before-mark", "family": family})

    # 3) 段中：文字 + 分页符 + 文字。
    tag = tags[i]; i += 1
    parts.append(para(run(f"{tag}head", SIZE_HALF_POINTS, family) + page_break_run()
                      + run("tail", SIZE_HALF_POINTS, family),
                      SIZE_HALF_POINTS, family, line=LINE_TWIPS, line_rule="exact"))
    probes.append({"tag": tag, "kind": "break-mid", "family": family})

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
    for kind in ("rise-quantised", "mixed-size", "break-own-line",
                 "break-before-mark", "break-mid"):
        print(f"  {kind:<20}{sum(1 for p in probes if p['kind']==kind)} 组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
