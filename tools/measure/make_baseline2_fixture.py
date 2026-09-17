#!/usr/bin/env python3
"""首行基线夹具（第二轮）：判「行底先落栅格」的两种量化次序。

first-line 那一批把两个候选都判掉了，而且它们其实是**同一个量的两种量化次序**：

    natural − descent = ascent + lineGap

所以「基线偏移 = ascent + lineGap」这个量，配上那两种次序，都不成立。

本轮换一个**排版上的说法**，不是从残差凑的：行盒从游标起、高 natural，
**行底也是一个位置**；而「Word 量化的是位置」是前几批已确立的方向。
若行底先落到栅格上，基线 = 行底 − descent，就得到两种新的次序（见预注册 §3）。

设计要点：

- 挑 `lineGap` **非零**的族（Plantagenet Cherokee 74、STIX Two Text 250）。
  `natural − descent = ascent + lineGap`，lineGap 越大，各种次序分得越开；
  前四批的族几乎清一色 lineGap = 0，等于在这一维上只有一个实例。
- 挑 `upem` 极端的族（Zapfino **400**、Sathu **1274**），upem 决定自然行高
  落在栅格的哪里。前四批只有 905 / 1000 / 2048 / 2100 / 2560。
- 字体与字号与前四批**无一重合**（§7.2）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, para, run, sect_pr

FONT_DIR = "/System/Library/Fonts/Supplemental/"
# 族名是 Word 自己列出来的那个。
FONTS = {
    "Skia": FONT_DIR + "Skia.ttf",                                    # upem 2048, gap 0
    "Sathu": FONT_DIR + "Sathu.ttf",                                  # upem 1274（罕见）, gap 0
    "Plantagenet Cherokee": FONT_DIR + "PlantagenetCherokee.ttf",     # upem 1000, **gap 74**
    "STIX Two Text": FONT_DIR + "STIXTwoText.ttf",                    # upem 1000, **gap 250**
    "Trattatello": FONT_DIR + "Trattatello.ttf",                      # upem 1000, gap 0
    "Zapfino": FONT_DIR + "Zapfino.ttf",                              # upem **400**, gap 0
}
# 半点。23 / 25 / 26 / 27 pt——前四批一个都没用过。
SIZES_HALF_POINTS = [46, 50, 52, 54]
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
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"探针 {len(probes)} 组（= {len(probes)} 页），共 {sum(p['lines'] for p in probes)} 条基线")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
