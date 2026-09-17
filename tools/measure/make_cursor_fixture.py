#!/usr/bin/env python3
"""游标单位夹具：判「Word 的纵向游标走在哪个整数单位上」。

上一批（`PREREG-2026-09-17-accumulation.md`）的结果把问题逼到了这一层：

- Q0 成立（240/240），1/300 英寸的**显示栅格**已确立；
- Q1「细累加、落位量化」只拿到 193/228，**35 处全部差恰好一格**，且可复现；
- 而且已经用算术排除了两种解释——不是读错度量表（hhea 与 OS/2 winAscent
  完全相同，sTypo 差 6%，那会处处出错），也不是进位规则（35 处不对的小数部分
  全 >0.5，可对上的里也有 67 步 >0.5）。

剩下的形状指向：游标不是在**实数**上累加，而是在某个**比 0.24pt 更细的整数单位**
上走。这一份就是判那个单位。

设计要点：

1. **步数尽量多。** 单位的差别是**累计**出来的：一步之内几乎所有候选单位都同解，
   几十步之后才分家。所以用小字号（6 / 7.5pt）把一页塞满，每组走到七八十步，
   而不是上一批的 19 步。
2. **换字体，而且刻意挑 upem 各不相同的**（905 / 1000 / 2048 / 2100 / 2560）。
   单位判的是「自然行高被取整到哪」，而 upem 直接决定自然行高落在哪——
   upem 全是 2048 的一批字体，对这个问题近乎只有一个实例。
3. **留一个退化对照**：Andale Mono 6pt 的自然行高正好是 6.7500pt，是每个候选
   单位的整数倍，所有候选在它上面同解。它判不出单位——**这正是要它的理由**：
   若它也出现不对，那说明错的不是单位，是模型的形状。
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
FONTS = {
    "Chalkduster": FONT_DIR + "Chalkduster.ttf",   # upem 905
    "Big Caslon": FONT_DIR + "BigCaslon.ttf",      # upem 1000，lineGap 18
    "Andale Mono": FONT_DIR + "Andale Mono.ttf",   # upem 2048
    "Ayuthaya": FONT_DIR + "Ayuthaya.ttf",         # upem 2100
    "Krungthep": FONT_DIR + "Krungthep.ttf",       # upem 2560
}
SIZES_HALF_POINTS = [12, 15]        # 6pt / 7.5pt
CONTENT_HEIGHT_PT = 841.89 - 144.0  # A4 减上下边距
MAX_LINES = 80


def lines_that_fit(family: str, size_pt: float) -> int:
    """这一组能排多少行——**留一行的余量**，宁可少一行也不要跨页。

    跨页的组按预注册的排除项判不了，等于白采。"""
    m = hhea(Path(FONTS[family]))
    nat = natural_pt(m, size_pt)
    asc = (m["ascent"] + m["lineGap"]) / m["upem"] * size_pt
    return max(2, min(MAX_LINES, int((CONTENT_HEIGHT_PT - asc) / nat) - 1))


def build_body() -> tuple[str, list[dict]]:
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghij"
    i = 0
    for family in FONTS:
        for hp in SIZES_HALF_POINTS:
            size = hp / 2.0
            n = lines_that_fit(family, size)
            tag = tags[i]
            i += 1
            for k in range(n):
                parts.append(
                    para(run(f"{tag}{k:02d}", hp, family), hp, family,
                         page_break=(k == 0 and len(parts) > 0))
                )
            probes.append({"tag": tag, "kind": "cursor", "sizeHalfPoints": hp,
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    args = parser.parse_args()
    path = Path(args.output)
    probes = write_docx(path)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    total = sum(p["lines"] for p in probes)
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"探针 {len(probes)} 组，共 {total} 条基线，{total - len(probes)} 步")
    for p in probes:
        print(f"    {p['tag']}  {p['family']:<13}{p['sizeHalfPoints']/2:>5}pt  {p['lines']:>3} 行")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
