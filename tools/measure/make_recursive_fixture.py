#!/usr/bin/env python3
"""递推夹具：判游标是「从起点重算」还是「从上一行的落位继续」。

rounding 那一批把四种进位规则全判假了，而且推出一条更强的：
半数进位与半数取偶只在平局处不同，两者分别 160 / 182，差的正是平局步——
**连「取最近格」本身都不对**。

于是闭式那条路走完了。剩下的另一种机制是**递推**：

    闭式： y(n)   = quantize(y(0) + n × step)      ← 已判假
    递推： y(n+1) = quantize(y(n) + step)          ← 本批判它

差别在于误差往哪走：闭式每次从起点重算，递推每次从**已经落位**的那一格继续。
后者是排版实现里很自然的写法（游标落下去，下一行从落下去的地方起算）。

设计要点（采集前用算术核过）：

- **递推式下每步的小数部分是恒定的**（等于 `step` 的小数部分），因为每步都从
  整数格出发。所以平局只在 `step` 的小数恰为 ½ 时出现——`line` 里若一个这样的
  值都没有，「递推进位」与「递推取偶」**一步都分不开**。第一版选的八个 `line`
  正是如此（0/152），改掉了。
- 现在四个 `line` 的 `step` 小数恰为 ½（252 / 276 / 348 / 372，即 L ≡ 12 mod 24），
  另四个小数各异。八个候选（闭式/递推 × 四种进位规则）两两最少 25 步能分开。
- 字体在这一层不是变量（`exact` 下行距与字体无关，exact-spacing 已实测确立）。
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
    "Lao Sangam MN": FONT_DIR + "Lao Sangam MN.ttf",
    "AppleMyungjo": FONT_DIR + "AppleMyungjo.ttf",
}
# twips。除以 4.8（= 0.24pt）的小数部分依次是
# .083 / .417 / .583 / .083 / .417 / .917——没有一个落在栅格上。
# 前四个的 `step` 小数恰为 ½（L ≡ 12 mod 24）——递推式下平局只在这里出现，
# 少了它们就分不开「递推进位」与「递推取偶」。后四个小数各异。
LINES_TWIPS = [252, 276, 348, 372, 234, 266, 274, 278]
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
            probes.append({"tag": tag, "kind": "recursive", "lineTwips": line,
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
