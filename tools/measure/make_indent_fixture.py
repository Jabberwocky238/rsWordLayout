#!/usr/bin/env python3
"""缩进与对齐夹具（清单 H3）。

纵向的账已经查了十四批，横向一直只有一条结论（H1：推进量取自字体、`w:kern`
默认关，Δx 中位 0.0000pt）。缩进与对齐**一次都没变过**——前十四批一律
左对齐、零缩进，正是 §7.2 警告的那种「这一维只有一个实例」。

本批判两件事，**都不依赖字体度量**：

1. **缩进**：左对齐时，行首字形的 x = 正文左边距 + `w:ind/@left`（首行再加
   `@firstLine`）。与字宽无关，算得出绝对值。

2. **对齐**：同一段文本在左 / 居中 / 右三种对齐下，行宽 `w` 是同一个数，
   而 `x_center = x_left + (avail − w)/2`、`x_right = x_left + (avail − w)`，
   于是

       x_right − x_left = 2 × (x_center − x_left)

   **`w` 自己消掉了**——不必知道行宽，也就不必算字形推进量。
   这是本批能绕开「预测行宽」的关键。

段落一律短到不换行，所以每段就是一行。
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
FONTS = {"Luminari": FONT_DIR + "Luminari.ttf"}
SIZE_HALF_POINTS = 26                     # 13pt
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz"

# (左缩进, 首行缩进) twips。含悬挂（首行为负）。
INDENTS = [(0, 0), (360, 0), (720, 0), (1080, 0),
           (0, 360), (360, 720), (720, -360), (1440, -720)]
# 对齐组：四段长短不同的文本，各排三种对齐。
ALIGN_TEXTS = ["ab", "abcdefgh", "abcdefghijklmnop", "abcdefghijklmnopqrstuvwx"]
ALIGNS = ["left", "center", "right"]


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []

    # A 组：缩进。每组两行（第 2 行用来分开「首行缩进」与「左缩进」）。
    for i, (left, first) in enumerate(INDENTS):
        tag = f"i{i}"
        for k in range(2):
            parts.append(
                para(run(f"{tag}{'ab'[k]}", SIZE_HALF_POINTS, family),
                     SIZE_HALF_POINTS, family,
                     page_break=(k == 0 and len(parts) > 0),
                     align="left", ind_left=left, ind_first=first)
            )
        probes.append({"tag": tag, "kind": "indent", "indLeft": left,
                       "indFirst": first, "sizeHalfPoints": SIZE_HALF_POINTS,
                       "family": family, "lines": 2})

    # B 组：对齐。同一文本三种对齐，各占一页，免得互相影响。
    for j, text in enumerate(ALIGN_TEXTS):
        for al in ALIGNS:
            tag = f"j{j}{al[0]}"
            parts.append(
                para(run(f"{tag}{text}", SIZE_HALF_POINTS, family),
                     SIZE_HALF_POINTS, family,
                     page_break=len(parts) > 0, align=al)
            )
            probes.append({"tag": tag, "kind": "align", "align": al,
                           "textIndex": j, "sizeHalfPoints": SIZE_HALF_POINTS,
                           "family": family, "lines": 1})

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
    ind = [p for p in probes if p["kind"] == "indent"]
    al = [p for p in probes if p["kind"] == "align"]
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"缩进组 {len(ind)} 个 × 2 行 = {len(ind)*2} 个 x 读数")
    print(f"对齐组 {len(al)} 段（{len(ALIGN_TEXTS)} 种文本 × {len(ALIGNS)} 种对齐）"
          f" = {len(ALIGN_TEXTS)} 个三元组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
