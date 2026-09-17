#!/usr/bin/env python3
"""固定行距夹具：把「字体度量」从行距里摘出去，单独判**累加机制**。

前六批都卡在同一处：首行基线对不上，而每一个候选形式都要先算 `natural`，
也就是先经过字体度量。于是「累加机制错了」和「natural 算错了」这两件事
**从来没分开过**。

`w:lineRule="exact"` 把它们分开：行距是**固定值**（`w:line` twips），
**与字体度量完全无关**。于是

    第 n 条基线 = quantize(b₀ + n × line/20 pt)

里只剩累加机制本身。成立，累加机制就确立了，问题全在 `natural`；
判假，那错的是累加机制，而且与字体无关。

设计要点：

- `line` 值刻意**避开栅格的整数倍**。0.24pt = 4.8 twips，`line` 若是 4.8 的倍数，
  每行都正好落在栅格上，「细累加」与「逐行量化」同解，白采。
  选的六个值除以 4.8 的小数部分都在 0.08 ~ 0.92 之间。
- 每组 20 行，独占一页——累加要够深才显得出漂移（cursor-unit 的教训）。
- 字体在这一层上**不是变量**（`exact` 下 step 与字体无关），所以复用第六批的族
  不违反 §7.2；用三个不同的族，反倒能顺带看「字体确实不影响 exact」。
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
    "Brush Script MT": FONT_DIR + "Brush Script.ttf",
    "Silom": FONT_DIR + "Silom.ttf",
    "Lao Sangam MN": FONT_DIR + "Lao Sangam MN.ttf",
}
# twips。除以 4.8（= 0.24pt）的小数部分依次是
# .083 / .417 / .583 / .083 / .417 / .917——没有一个落在栅格上。
LINES_TWIPS = [250, 290, 310, 370, 410, 470]
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
            probes.append({"tag": tag, "kind": "exact-spacing", "lineTwips": line,
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
