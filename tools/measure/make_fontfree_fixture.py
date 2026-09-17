#!/usr/bin/env python3
"""字体无关性夹具：判 `exact` 行距下基线位置是不是**完全不看字体**。

shift-floor 的回测撞见一件事：同一 `w:line` 下换字体，基线**逐格相同**，
连首行都一样（8 / 8）。那两个族的 ascent 是 1820/2048 em 与 891/1025 em，
差得不算小。但那是从已见数据看出来的，得独立检验，而且该用差得**更极端**的族。

本批的六个族，`ascent / upem` 从 **0.795 到 1.875**，差 2.4 倍：

| 族 | upem | ascent | ascent/upem |
| --- | ---: | ---: | ---: |
| Zapfino | **400** | 750 | **1.875** |
| Trattatello | 1000 | 1150 | 1.150 |
| Khmer Sangam MN | 2048 | 2294 | 1.120 |
| Luminari | 1000 | 983 | 0.983 |
| AppleMyungjo | 1025 | 891 | 0.869 |
| Herculanum | 1000 | 795 | **0.795** |

Zapfino 在 13pt 下 ascent 有 24.4pt，比最大的行距（14.6pt）还高——
若字体真的不参与，连它也该落在同一格上。

**标签只用字母**：Zapfino 与 Herculanum **没有数字 0–5 的字形**
（读 `cmap` 查出来的），用数字做标签会被 Word 换成回退字体，整批作废——
page-start 那一批就是这么废掉的。`assert_can_draw` 在生成时守着这一条。
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
FONTS = {
    "Zapfino": FONT_DIR + "Zapfino.ttf",
    "Trattatello": FONT_DIR + "Trattatello.ttf",
    "Khmer Sangam MN": FONT_DIR + "Khmer Sangam MN.ttf",
    "Luminari": FONT_DIR + "Luminari.ttf",
    "AppleMyungjo": FONT_DIR + "AppleMyungjo.ttf",
    "Herculanum": FONT_DIR + "Herculanum.ttf",
}
LINES_TWIPS = [222, 244, 256, 292]      # 与前几批无一重合
SIZE_HALF_POINTS = 26                   # 13pt
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
    for family in FONTS:
        for line in LINES_TWIPS:
            tag = GROUP_TAGS[i]
            i += 1
            for k in range(LINES_PER_GROUP):
                parts.append(
                    para(run(f"{tag}{ROW_TAGS[k]}", SIZE_HALF_POINTS, family),
                         SIZE_HALF_POINTS, family,
                         page_break=(k == 0 and len(parts) > 0),
                         line=line, line_rule="exact")
                )
            probes.append({"tag": tag, "kind": "font-free", "lineTwips": line,
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
    print(f"探针 {len(probes)} 组 × {LINES_PER_GROUP} 行 = {len(probes)*LINES_PER_GROUP} 条基线")
    print(f"每个 `line` 下 {len(FONTS)} 个字体，共 {len(LINES_TWIPS)*LINES_PER_GROUP} 个位置要比")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "linesPerGroup": LINES_PER_GROUP,
                    "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
