#!/usr/bin/env python3
"""进位规则夹具：判 `quantize` 在**恰好半格**处往哪边走。

exact-spacing 把自由度逼到了这一处。那一批的式子是

    第 n 条基线 = quantize(b₀ + n × step)

其中 `b₀` 实测、`step = w:line/20` 精确且不过字体、`n` 整数——**唯一还能错的
只有 `quantize` 自己**。而且三个字体逐组得分一字不差，字体确实不参与。

`b₀` 落在栅格上（栅格七批成立），所以 `(b₀ + n × step) / 0.24` 的小数部分
只由 `n × line/4.8` 决定——**完全可控**。于是：

- 小数部分 **= 0** 的步：正落在栅格点上，四种进位规则同解。**这是前提检验**：
  它若也不中，说明错的不是进位规则，是式子本身。
- 小数部分 **≠ 0** 的步：规则不同则读数差一格。**这是判据。**

`line` 值刻意取**八种不同的小数部分**（.0833 / .25 / .4167 / .5 / .5833 /
.75 / .9167，含两个 .5）。**只取 .5 是不够的**——半格处「半数进位」与「向上」
永远同解，那样的夹具分不开这两个。采集前核过分离度（8 个 line × 19 步 = 152 步）：

| 两两 | 能分开的步数 |
| --- | ---: |
| 进位 / 取偶 | 15 |
| 进位 / 截断 | 81 |
| 进位 / 向上 | 49 |
| 取偶 / 截断 | 66 |
| 取偶 / 向上 | 64 |
| 截断 / 向上 | 130 |

四种两两都分得开。

字体在这一层上不是变量（`exact` 下行距与字体无关，exact-spacing 已实测确立），
所以复用同三个族不违反 §7.2。
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
# `exact` 下行距与字体无关（exact-spacing 实测：三族逐组一字不差），
# 所以字体在这一层不是变量，两个族够了——省下的行数给 `line` 的多样性。
FONTS = {
    "Brush Script MT": FONT_DIR + "Brush Script.ttf",
    "Silom": FONT_DIR + "Silom.ttf",
}
# twips。除以 4.8（= 0.24pt）的小数部分依次是
# .083 / .417 / .583 / .083 / .417 / .917——没有一个落在栅格上。
# 除以 4.8 的小数部分覆盖 .0833 / .25 / .4167 / .5 / .5833 / .75 / .9167，
# **不只取 .5**——半格处「进位」与「向上」同解，只取 .5 就分不开那两个。
LINES_TWIPS = [242, 246, 250, 254, 258, 300, 310, 324]
SIZE_HALF_POINTS = 24          # 12pt：exact 下字号不参与行距，取个常见值即可
LINES_PER_GROUP = 20
LABEL_CHARS = "abcdefghijklmnopqr0123456789"


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqr"
    i = 0
    for family in FONTS:
        for line in LINES_TWIPS:
            tag = tags[i]
            i += 1
            for k in range(LINES_PER_GROUP):
                parts.append(
                    para(run(f"{tag}{k:02d}", SIZE_HALF_POINTS, family),
                         SIZE_HALF_POINTS, family,
                         page_break=(k == 0 and len(parts) > 0),
                         line=line, line_rule="exact")
                )
            probes.append({"tag": tag, "kind": "rounding", "lineTwips": line,
                           "sizeHalfPoints": SIZE_HALF_POINTS, "family": family,
                           "lines": LINES_PER_GROUP})
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
    print(f"探针 {len(probes)} 组 × {LINES_PER_GROUP} 行 = "
          f"{len(probes) * LINES_PER_GROUP} 条基线，"
          f"{len(probes) * (LINES_PER_GROUP - 1)} 步")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "linesPerGroup": LINES_PER_GROUP,
                    "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
