#!/usr/bin/env python3
"""平移-截断夹具：判「位置 = 某个连续量取整」这个模型本身。

闭式与递推 × 四种取整规则 = 8 个候选，recursive 那一批**全判假**，
而且最优规则在两批之间换人（截断 168→256、取偶 182→240），
说明位置不是「精确累加 + 某一种固定取整规则」。

但这八个候选其实没穷尽该穷尽的东西。**所有取整规则的差别，都等价于给被取整的
量加一个常数**——四舍五入就是 `floor(x + 0.5)`，取偶是 `floor(x + 0.5)` 在平局处
的微调，向上是 `floor(x + 1 - ε)`。于是整族写成一个式子：

    基线(n) 的格数 = ⌊A + n × step⌋        A 为待定实数

判的不是「A 等于多少」，而是**「存不存在这样的 A」**：每条基线给出 A 的一个
半开区间 `[c(n) − n×step, c(n)+1 − n×step)`，取交集非空即存在。
**二值、精确、不拟合**——它不挑「最好的 A」，只问有没有。

成立 → 「位置 = 某连续量截断」这个模型立住，A 的形式是下一题；
判假 → **这个模型整个倒掉**，位置根本不是由一个连续量取整来的。

有限精度（V17）**采集前用算术排除**，不必再采：精确位置是 48 分之几格的有理数，
它与整数的最小非零距离是 1/48 ≈ 0.021 格；double 在这量级的误差约 1e-13 格，
float32 也才 4e-5。**比翻转所需的量小十个数量级**，不可能是原因。
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
    "AppleMyungjo": FONT_DIR + "AppleMyungjo.ttf",
}
# twips。除以 4.8（= 0.24pt）的小数部分依次是
# .083 / .417 / .583 / .083 / .417 / .917——没有一个落在栅格上。
# 与前三批无一重合；小数部分覆盖 .1667 / .25 / .3333 / .5 / .5833 / .75 / .8333。
LINES_TWIPS = [228, 232, 236, 262, 268, 282, 286, 294]
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
            probes.append({"tag": tag, "kind": "shift-floor", "lineTwips": line,
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
