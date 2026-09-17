#!/usr/bin/env python3
"""固定距离夹具：判基线是不是在**行底上方固定距离**处。

a-form 判掉了「固定比例」：不存在跨全部 16 个 `w:line` 的单一 k
（`line=206` 要求 k ∈ [0.8019, 0.8039)，`line=210` 要求 k ≥ 0.8057，不交）。

但失败是有方向的——各组容许的 k **随 L 增大而增大**。固定比例给不出这个走向，
**固定距离**给得出：若基线在行底上方固定 `d`（格），则

    A = 300 + L − d        ⟹    c(n) = ⌊300 + (n+1) × L − d⌋

k = 1 − d/L 随 L 递增，正是实测的方向。d 与 k 一样不含字体量，
与 font-free / size-free 是一路的。

**十六个 `w:line` 值与前六批无一重合**，且刻意取得**跨度更大**
（150 ~ 460 twips，7.5 ~ 23pt）：固定比例与固定距离的差别随 L 的跨度放大，
a-form 那一批 206 ~ 308 的跨度只有 1.5 倍，这一批是 3 倍。
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
# 十六个值，与前六批无一重合，且跨度大（7.5 ~ 23pt）：
# 固定比例与固定距离的差别随 L 的跨度放大。
LINES_TWIPS = [150, 166, 178, 194, 202, 220, 230, 316,
               328, 340, 356, 388, 404, 422, 436, 460]
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
            probes.append({"tag": tag, "kind": "fixed-distance", "lineTwips": line,
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
