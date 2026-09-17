#!/usr/bin/env python3
"""字号无关性夹具：判 `exact` 行距下基线位置是不是也**不看字号**。

font-free 确立了：`exact` 下基线与**字体**无关（40/40，六族 `ascent/upem`
差 2.4 倍仍逐格相同）。于是 A 只能是 `w:line` 的函数——**前提是字号也不参与**。
那一批六个族全用 13pt，**字号这一维没变过**，正是 §7.2 警告的情形。

本批固定一个族，变四个字号（8 / 13 / 20 / 30pt），判同一 `w:line` 下
各字号的基线是否逐格相同。

30pt 在最小的行距（218 twips = 10.9pt）下，字比行高大得多——
font-free 已确立 `exact` 不被内容撑开，所以这里也该落在同一格上。

**成立** → A 是纯粹的 `f(w:line)`，形式可解；
**判假** → 字号参与，A 里得有字号项，而那会把 font-free 的结论限死在同字号内。
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
LINES_TWIPS = [218, 226, 248, 284]      # 与前几批无一重合
# 半点。8 / 13 / 20 / 30 pt——30pt 在最小行距 10.9pt 下字比行高大得多。
SIZES_HALF_POINTS = [16, 26, 40, 60]
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
            probes.append({"tag": tag, "kind": "size-free", "lineTwips": line,
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
