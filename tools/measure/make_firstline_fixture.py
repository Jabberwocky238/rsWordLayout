#!/usr/bin/env python3
"""首行基线夹具：判每页第一条基线的形式。

前三批把问题收敛到了这一处：

- 栅格已确立（1080 条基线零例外，三份独立夹具）；
- 「游标走更细的整数单位」已排除（cursor-unit R2，得分随单位变粗单调下降）；
- 「以某条基线为锚、常数步长累加」已排除（cursor-unit 的退化对照，
  周期对、相位差 4，指向锚丢掉了亚栅格余数）；
- 而**首行那一项**从头到尾没对过：accumulation 的 Q2 是 9/12。

`accumulation` 那份预注册在**采集之前**就写下了下一个待判形式：

    b₀ = quantize(正文区顶 + natural − 量化后的 descent)

Q2 确实判假了，所以现在判它——这不是看了数据才想出来的假设。

设计：分母是**页数**，所以每组独占一页、组数尽量多（6 字体 × 4 字号 = 24 页）。
每组只要 3 行：判首行用第一条，后两条顺带给累加律采数（**不作预测**）。
字体与字号与前三批无一重合（§7.2）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, para, run, sect_pr

FONT_DIR = "/System/Library/Fonts/Supplemental/"
# 族名是 **Word 自己列出来的那个**，不是文件名——上一批「Big Caslon」
# 就是栽在这上面（Word 叫它 Big Caslon Medium），采前核查逮到的。
FONTS = {
    "Impact": FONT_DIR + "Impact.ttf",                                   # upem 2048
    "Microsoft Sans Serif": FONT_DIR + "Microsoft Sans Serif.ttf",       # upem 2048
    "Luminari": FONT_DIR + "Luminari.ttf",                               # upem 1000
    "Herculanum": FONT_DIR + "Herculanum.ttf",                           # upem 1000
    "Apple Chancery": FONT_DIR + "Apple Chancery.ttf",                   # upem 2048
    "Bodoni 72 Smallcaps Book": FONT_DIR + "Bodoni 72 Smallcaps Book.ttf",  # upem 1000
}
# 半点。9.5 / 11.5 / 19 / 21 pt——前三批一个都没用过。
SIZES_HALF_POINTS = [19, 23, 38, 42]
LINES_PER_GROUP = 3


def build_body() -> tuple[str, list[dict]]:
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqrstuvwx"
    i = 0
    for family in FONTS:
        for hp in SIZES_HALF_POINTS:
            tag = tags[i]
            i += 1
            for k in range(LINES_PER_GROUP):
                parts.append(
                    para(run(f"{tag}{k}", hp, family), hp, family,
                         page_break=(k == 0 and len(parts) > 0))
                )
            probes.append({"tag": tag, "kind": "first-line", "sizeHalfPoints": hp,
                           "family": family, "lines": LINES_PER_GROUP})
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
    total = sum(p["lines"] for p in probes)
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"探针 {len(probes)} 组（= {len(probes)} 页），共 {total} 条基线")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
