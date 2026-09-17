#!/usr/bin/env python3
"""第三批杂项：分节边界、`atLeast` 的首行、字符缩放（V11 / V23 / H4b）。

三条又都能用**差值或比值**绕开尚未解出的 A：

- **V11 分节边界**：有分节与无分节两组，同设置，比**基线差之差**。
- **V23 `atLeast` 首行**：同 `line` 下 `atLeast` 与 `exact` 的**首行之差**。
- **H4b 字符缩放 `w:w`**：同一文本不同缩放，比**相邻字形 x 差的比值**，
  原始推进量在相除时消掉。

`w:w` 的单位是**百分比**（`val=200` 即 200%）。
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
LINE_TWIPS = 320
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz"

AT_LEAST_LINES = [340, 460, 580]      # 与 vmisc 的 400/520/640、第一版的 360/440/560 均不重合
SCALES = [100, 120, 180, 66]          # `w:w` 百分比，100 是对照；与第一版的 150/200/50 不重合


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqrstuvwx"
    i = 0

    # A 组：分节边界。两组三段，一组中间插 `continuous` 分节。
    for has_sect in (False, True):
        tag = tags[i]; i += 1
        for k in range(3):
            sect = sect_pr("continuous") if (has_sect and k == 1) else ""
            parts.append(para(run(f"{tag}{'abc'[k]}", SIZE_HALF_POINTS, family),
                              SIZE_HALF_POINTS, family, sect=sect,
                              page_break=(k == 0 and len(parts) > 0),
                              line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "section", "hasSection": has_sect,
                       "lineTwips": LINE_TWIPS, "family": family, "lines": 3})

    # B 组：`atLeast` 首行。
    for ln in AT_LEAST_LINES:
        for rule in ("atLeast", "exact"):
            tag = tags[i]; i += 1
            for k in range(3):
                parts.append(para(run(f"{tag}{'abc'[k]}", SIZE_HALF_POINTS, family),
                                  SIZE_HALF_POINTS, family, page_break=(k == 0),
                                  line=ln, line_rule=rule))
            probes.append({"tag": tag, "kind": "at-least-first", "rule": rule,
                           "lineTwips": ln, "family": family, "lines": 3})

    # C 组：字符缩放 `w:w`。
    for sc in SCALES:
        tag = tags[i]; i += 1
        extra = f'<w:w w:val="{sc}"/>' if sc != 100 else ""
        parts.append(para(run(f"{tag}mnopqrst", SIZE_HALF_POINTS, family, extra),
                          SIZE_HALF_POINTS, family, page_break=True,
                          line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "scale", "scalePercent": sc,
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
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    for kind in ("section", "at-least-first", "scale"):
        print(f"  {kind:<18}{sum(1 for p in probes if p['kind']==kind)} 组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
