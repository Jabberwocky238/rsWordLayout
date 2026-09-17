#!/usr/bin/env python3
"""分节符与分页符夹具：V11 与 C3，量具补完之后重判。

这两项此前都**卡在量具**：`sweep` 里没有构造标注，`counting` 只能按字符猜，
而 `\\x0c` 在 `Range.Text` 里分节符与手动分页符**同形**，猜不出来就报判不了，
整页归行跟着废掉（vmisc2 丢了三页分页符、vmisc3 丢了分节页）。

采集现在会写入**经 `verify()` 核对过**的构造标注，两项因此解锁。

- **C3 分页符三分**（§4）：独占一段 0 个字形、紧跟段落标记 1 个、段中 0 个。
- **V11 分节边界**：跨过 `continuous` 分节那一步，与无分节的同设置相比差多少。

分节与分页各造**三组**，比前几批的一组结实。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, para, rpr, run, sect_pr
from wordmeasure.fontcover import assert_can_draw

FONT_DIR = "/System/Library/Fonts/Supplemental/"
FONTS = {"Luminari": FONT_DIR + "Luminari.ttf"}
SIZE_HALF_POINTS = 26
LINE_TWIPS = 340
LABEL_CHARS = "abcdefghijklmnopqrstuvwxyz"


def page_break_run() -> str:
    return f"<w:r>{rpr(SIZE_HALF_POINTS, next(iter(FONTS)))}<w:br w:type=\"page\"/></w:r>"


def build_body() -> tuple[str, list[dict]]:
    assert_can_draw(FONTS, LABEL_CHARS)
    family = next(iter(FONTS))
    parts: list[str] = []
    probes: list[dict] = []
    tags = "abcdefghijklmnopqrstuvwx"
    i = 0

    # A 组：分节边界。三对（有 / 无分节），每对同设置。
    for rep in range(3):
        for has_sect in (False, True):
            tag = tags[i]; i += 1
            for k in range(3):
                sect = sect_pr("continuous") if (has_sect and k == 1) else ""
                parts.append(para(run(f"{tag}{'abc'[k]}", SIZE_HALF_POINTS, family),
                                  SIZE_HALF_POINTS, family, sect=sect,
                                  page_break=(k == 0 and len(parts) > 0),
                                  line=LINE_TWIPS, line_rule="exact"))
            probes.append({"tag": tag, "kind": "section", "hasSection": has_sect,
                           "pairIndex": rep, "family": family, "lines": 3})

    # B 组：分页符三分，各三组。
    for rep in range(3):
        # 1) 独占一段。
        tag = tags[i]; i += 1
        parts.append(para(page_break_run(), SIZE_HALF_POINTS, family,
                          line=LINE_TWIPS, line_rule="exact"))
        parts.append(para(run(f"{tag}x", SIZE_HALF_POINTS, family), SIZE_HALF_POINTS,
                          family, line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "break-own-line", "pairIndex": rep,
                       "family": family})
        # 2) 紧跟段落标记。
        tag = tags[i]; i += 1
        parts.append(para(run(f"{tag}y", SIZE_HALF_POINTS, family) + page_break_run(),
                          SIZE_HALF_POINTS, family, line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "break-before-mark", "pairIndex": rep,
                       "family": family})
        # 3) 段中。
        tag = tags[i]; i += 1
        parts.append(para(run(f"{tag}z", SIZE_HALF_POINTS, family) + page_break_run()
                          + run("w", SIZE_HALF_POINTS, family),
                          SIZE_HALF_POINTS, family, line=LINE_TWIPS, line_rule="exact"))
        probes.append({"tag": tag, "kind": "break-mid", "pairIndex": rep,
                       "family": family})

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
    for kind in ("section", "break-own-line", "break-before-mark", "break-mid"):
        print(f"  {kind:<20}{sum(1 for p in probes if p['kind']==kind)} 组")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
