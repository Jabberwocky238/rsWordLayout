#!/usr/bin/env python3
"""A 的形式夹具：判基线是不是落在行内的**固定比例**处。

font-free 与 size-free 合起来确立：`exact` 行距下基线位置**只由 `w:line` 决定**，
字体与字号都不参与。shift-floor 则确立位置是 `⌊A + n × step⌋`。
于是 V19 只剩一件事：A 长什么样。

`exact` 下 `step` 就是行高的格数 `L = w:line × 10 / 48`。若基线固定落在行内的
某个**比例** k 处，那么

    A = 300 + k × L        （300 格 = 72pt = 正文区顶）
    c(n) = ⌊300 + (k + n) × L⌋

k 的物理意义是「基线在一行之内的相对高度」——一个纯粹的排版常数，
不含任何字体量。这与前两批的结论是一路的。

判的仍是**存在性**：存在不存在这样一个 k，而不是「最合适的 k 是多少」。
每条基线把 k 限到一个半开区间，全部取交集，非空即成立——二值、精确、不拟合。

十六个 `w:line` 值与前五批**无一重合**。
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
# 字体在这一层不是变量（font-free 已确立 `exact` 下基线与字体无关），固定一个。
FONTS = {"Luminari": FONT_DIR + "Luminari.ttf"}
# 十六个值，与前五批无一重合。A 只依赖 `w:line`，所以把组数全给它。
LINES_TWIPS = [206, 210, 214, 238, 240, 260, 264, 270,
               272, 280, 288, 296, 298, 302, 304, 308]
SIZES_HALF_POINTS = [26]               # 13pt；字号已确立不参与（size-free）
LINES_PER_GROUP = 10
# **只用字母**——Zapfino / Herculanum 没有 0–5 的字形。
GROUP_TAGS = "abcdefghijklmnopqrstuvwx"
ROW_TAGS = "abcdefghij"
LABEL_CHARS = GROUP_TAGS + ROW_TAGS


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    parts: list[str] = []
    probes: list[dict] = []
    i = 0
    family = next(iter(FONTS))
    for hp in SIZES_HALF_POINTS:
        for line in LINES_TWIPS:
            tag = GROUP_TAGS[i]
            i += 1
            for k in range(LINES_PER_GROUP):
                parts.append(
                    para(run(f"{tag}{ROW_TAGS[k]}", hp, family), hp, family,
                         page_break=(k == 0 and len(parts) > 0),
                         line=line, line_rule="exact")
                )
            probes.append({"tag": tag, "kind": "a-form", "lineTwips": line,
                           "sizeHalfPoints": hp, "family": family,
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
    print(f"探针 {len(probes)} 组 × {LINES_PER_GROUP} 行 = {len(probes)*LINES_PER_GROUP} 条基线")
    print(f"每个 `line` 下 {len(SIZES_HALF_POINTS)} 个字号，"
          f"共 {len(LINES_TWIPS)*LINES_PER_GROUP} 个位置要比")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "linesPerGroup": LINES_PER_GROUP,
                    "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
