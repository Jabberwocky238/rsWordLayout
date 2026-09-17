#!/usr/bin/env python3
"""噪声底夹具：量「本该完全相同的读数，实测差多少」（清单 H5）。

量具方法 §5 给的噪声底是 **0.0000pt**，本项目一路的容差都按它取 0。
但 hbox 与 hbox2 两批里，本该精确成立的横向关系稳定差
**0.0005 / 0.000384 pt**——量级一致、方向一致、跨夹具重现。

那可能根本不是 Word 与预测的差，而是**读数本身的量化**。
若真如此，「容差 0」在亚千分之一磅这一层就不成立，
而**所有横向判据都建在它上面**。

这一批不判 Word，判**量具**。办法是造一批**本该逐位相同**的读数：

1. **行间**：同一段文本重复 20 行，左对齐、同字体同字号、行距固定。
   第 i 个字形的 x 在 20 行之间应当**完全一样**。
2. **行内**：一行里把同一个两字符组重复 12 遍，
   每一遍的内部间隔应当**完全一样**。

两处若都逐位相同，噪声底就是 0，此前的容差站得住；
若有差，差多少就是噪声底，此前所有横向判据都得按它重读。
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
# 三个族，看噪声是不是与字形有关。
FONTS = {
    "Luminari": FONT_DIR + "Luminari.ttf",
    "Trattatello": FONT_DIR + "Trattatello.ttf",
    "AppleMyungjo": FONT_DIR + "AppleMyungjo.ttf",
}
SIZE_HALF_POINTS = 26
LINES_PER_GROUP = 20
REPEAT_TEXT = "mnopqrst"          # 行间重复用
INLINE_UNIT = "xy"                # 行内重复用
INLINE_REPEATS = 12
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz"


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghi"
    i = 0

    # A 组：行间重复。20 行完全一样的文本，各组独占一页。
    for family in FONTS:
        tag = tags[i]
        i += 1
        for k in range(LINES_PER_GROUP):
            parts.append(para(run(f"{tag}{REPEAT_TEXT}", SIZE_HALF_POINTS, family),
                              SIZE_HALF_POINTS, family,
                              page_break=(k == 0 and len(parts) > 0),
                              line=300, line_rule="exact"))
        probes.append({"tag": tag, "kind": "between-lines", "family": family,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "lines": LINES_PER_GROUP,
                       "text": REPEAT_TEXT})

    # B 组：行内重复。一行里同一个两字符组重复 12 遍。
    for family in FONTS:
        tag = tags[i]
        i += 1
        parts.append(para(run(f"{tag}{INLINE_UNIT * INLINE_REPEATS}",
                              SIZE_HALF_POINTS, family),
                          SIZE_HALF_POINTS, family, page_break=True,
                          line=300, line_rule="exact"))
        probes.append({"tag": tag, "kind": "within-line", "family": family,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "lines": 1,
                       "unit": INLINE_UNIT, "repeats": INLINE_REPEATS})

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
    print(f"  行间重复 {sum(1 for p in probes if p['kind']=='between-lines')} 组 "
          f"× {LINES_PER_GROUP} 行")
    print(f"  行内重复 {sum(1 for p in probes if p['kind']=='within-line')} 组 "
          f"× {INLINE_REPEATS} 遍")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
