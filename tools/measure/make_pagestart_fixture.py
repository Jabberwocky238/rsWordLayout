#!/usr/bin/env python3
"""起页方式夹具：判「强制分页起的首行」与「自然溢出起的首行」是否同一回事。

前五批**每一组都用 `w:pageBreakBefore` 起页**，于是所有首行读数都来自
「强制分页之后的第一行」。这一维从头到尾只有一个实例——正是量具方法 §7.2
警告的那种情形：检验实例与设计实例在被判的那一层上没有区别。

若 Word 对这两种首行处理不同，那前五批**所有**首行读数都带着同一个混杂因素，
而首行恰恰是唯一一直对不上的那一项。

**这个怀疑不是从残差来的**，是从「我自己的夹具在这一维上没有变化」来的。

判法不依赖任何公式：每组**同时**产生两种首行，直接比。

    第 1 段带 `w:pageBreakBefore`  → 本组第 1 页的首行 = 强制分页起的
    随后排满一页再多几行          → 本组第 2 页的首行 = 自然溢出起的

同组内字体、字号、行距设置完全相同，所以两者若不等，只能是**起页方式**造成的。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, para, run, sect_pr
from prereg_probe import hhea, natural_pt

FONT_DIR = "/System/Library/Fonts/Supplemental/"
# 族名是 Word 自己列出来的那个；与前五批无一重合。
FONTS = {
    "Brush Script MT": FONT_DIR + "Brush Script.ttf",     # upem 2048
    "Khmer Sangam MN": FONT_DIR + "Khmer Sangam MN.ttf",  # upem 2048, **gap 380**
    "Kokonor": FONT_DIR + "Kokonor.ttf",                  # upem **2600**, gap 42
    "Silom": FONT_DIR + "Silom.ttf",                      # upem 1000
    "Gurmukhi MN": FONT_DIR + "Gurmukhi.ttf",             # upem 2048
    "Lao Sangam MN": FONT_DIR + "Lao Sangam MN.ttf",      # upem 2048
}
SIZES_HALF_POINTS = [58, 66, 70]        # 29 / 33 / 35 pt，前五批都没用过
CONTENT_HEIGHT_PT = 841.89 - 144.0
TAIL_LINES = 3                          # 第 2 页至少要有这么多行


def lines_needed(family: str, size_pt: float) -> int:
    """排满一页再多 `TAIL_LINES` 行，好让第 2 页确实有首行可读。"""
    m = hhea(Path(FONTS[family]))
    nat = natural_pt(m, size_pt)
    asc = (m["ascent"] + m["lineGap"]) / m["upem"] * size_pt
    per_page = max(1, int((CONTENT_HEIGHT_PT - asc) / nat))
    return per_page + TAIL_LINES


def build_body() -> tuple[str, list[dict]]:
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqr"
    i = 0
    for family in FONTS:
        for hp in SIZES_HALF_POINTS:
            size = hp / 2.0
            n = lines_needed(family, size)
            tag = tags[i]
            i += 1
            for k in range(n):
                parts.append(
                    para(run(f"{tag}{k:02d}", hp, family), hp, family,
                         page_break=(k == 0 and len(parts) > 0))
                )
            probes.append({"tag": tag, "kind": "page-start", "sizeHalfPoints": hp,
                           "family": family, "lines": n})
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
    total = sum(p["lines"] for p in probes)
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"探针 {len(probes)} 组，共 {total} 段（每组跨 2 页）")
    for p in probes:
        print(f"    {p['tag']}  {p['family']:<17}{p['sizeHalfPoints']/2:>5}pt  {p['lines']:>3} 段")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
