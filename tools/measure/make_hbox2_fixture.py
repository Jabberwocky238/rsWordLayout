#!/usr/bin/env python3
"""横向第二批：两端对齐与字符级字距（清单 H3d / H4a），重判。

hbox 那一批这两项都没判成，而且**两次都是我自己的问题**：

- **两端对齐**报「判不了」，因为判据按标签前缀找行，只找得到段落的第一行
  （已修：改按源区间归属，见 `prereg_probe.lines_by_paragraph`）；
- **字距**判假，因为预注册把 `w:spacing` 的单位写成了半点——
  它是 **twips（1/20 磅）**。实测 `val=20` 给出每个间隔多 1.0005pt，
  正是 twips 的解释。

两项都要**新的预注册与新的夹具**才能重判——改好判据回头重跑旧读数，
那是改判据迁就数据。本夹具因此换了文本与字距值。

字距值取 **15 / 30 / 60 twips**（0.75 / 1.5 / 3pt），与上一批的 20 / 40 / 80
无一重合；两端对齐的段落改用不同的词与长度。
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
SIZE_HALF_POINTS = 26
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz "

# 本批只判两端对齐与字距，右对齐（H3c）已在 hbox 判掉了。
RIGHT_TEXTS = []
# 两端对齐要换行。词与上一批不同。
JUSTIFY_WORDS = "omicron rho sigma tau upsilon phi chi psi omega nu xi mu "
# 字符级字距，**twips**（1/20 磅）——上一批把单位写成半点，那是错的。
# 取值与上一批的 20/40/80 无一重合。0 是对照。
SPACINGS = [0, 15, 30, 60]


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []

    # R 组：右对齐，各占一页。
    for j, text in enumerate(RIGHT_TEXTS):
        tag = f"r{j}"
        parts.append(para(run(f"{tag} {text}", SIZE_HALF_POINTS, family),
                          SIZE_HALF_POINTS, family,
                          page_break=len(parts) > 0, align="right"))
        probes.append({"tag": tag, "kind": "right", "textIndex": j,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family, "lines": 1})

    # J 组：两端对齐，长到必然换行。
    for j, reps in enumerate((4, 6, 9)):
        tag = f"t{j}"
        parts.append(para(run(f"{tag} " + JUSTIFY_WORDS * reps, SIZE_HALF_POINTS, family),
                          SIZE_HALF_POINTS, family,
                          page_break=True, align="both"))
        probes.append({"tag": tag, "kind": "justify", "reps": reps,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family})

    # S 组：字符级字距。同一文本排四遍，`0` 是对照。
    for sp in SPACINGS:
        tag = f"s{sp:02d}"
        extra = f'<w:spacing w:val="{sp}"/>' if sp else ""
        parts.append(para(run(f"{tag} mnopqrst", SIZE_HALF_POINTS, family, extra),
                          SIZE_HALF_POINTS, family, page_break=True, align="left"))
        probes.append({"tag": tag, "kind": "letter-spacing", "spacingHalfPoints": sp,
                       "sizeHalfPoints": SIZE_HALF_POINTS, "family": family, "lines": 1})

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
    for kind in ("right", "justify", "letter-spacing"):
        print(f"  {kind:<16}{sum(1 for p in probes if p['kind']==kind)} 组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
