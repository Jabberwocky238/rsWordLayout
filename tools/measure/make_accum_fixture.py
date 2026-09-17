#!/usr/bin/env python3
"""累加律夹具：判「Word 量化的是基线位置，还是行的高度」。

上一批（`docs/PREREG-2026-09-17-probe-metrics.md`）作废在证否条件 F-E 上：
同一设置连排三段**完全相同**的段落，两个行距不相等，差**恰好一格**（0.24pt）。
F-E 当初按「量具自检」写，假设读数不等 = 测量不稳；真相是**被测的量本来就不是常数**。
于是那批五条预测的共同前提（「每种设置有一个行距」）塌了，五条一起废。

这一份改判**位置**，不判**行距**。设计上与上一份差三处，每一处都是上一批教的：

1. **每组 20 段，不是 3 段。** 三段只给两个行距读数，分不出「每行各自取整」与
   「细累加、落位取整」——两者在两步内常常同解。二十段给十九步，累加的漂移藏不住。
2. **换字体族。** 上一批全部是 Liberation 三兄弟加 Carlito；模型正是从那批数据看出来的，
   再拿同一批字体判就是拿设计实例当检验实例（量具方法 §7.2）。
3. **绕开度量兼容克隆。** 不用 Times New Roman（= Liberation Serif）、不用 Arial
   （= Liberation Sans）、**不用 Courier New**——它的 hhea 与 Liberation Mono
   一字不差（1705 / 615 / 0）。名字不同不等于实例不同，判的是度量就得按度量算。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from make_probe_fixture import W, R, para, run, sect_pr

# 五个族，两两度量不同，且都不是上一批任何一个的克隆。
# 括号里是 12pt 自然行高 ÷ 0.24 的小数部分——它决定这一族会不会显出交替。
FAMILIES = [
    "Georgia",        # .812
    "Verdana",        # .767
    "Tahoma",         # .352
    "Comic Sans MS",  # .678
    "Trebuchet MS",   # .057 —— 几乎落在栅格上，**预期几乎不交替**，是反向对照
]

# 字号（半点），刻意避开上一批用过的 8/9/10/11/12/14/16/18/24pt。
SIZES_HALF_POINTS = [17, 21, 26, 30, 34, 40, 44]  # 8.5 / 10.5 / 13 / 15 / 17 / 20 / 22 pt

# 每组多少段。二十步足够让「每行取整」与「细累加」分家，且 22pt 下仍装得下一页。
LINES_PER_GROUP = 20


def build_body() -> tuple[str, list[dict]]:
    parts: list[str] = []
    probes: list[dict] = []

    def group(tag: str, size: int, family: str, kind: str):
        """一组 `LINES_PER_GROUP` 段同设置的短段落，独占一页。

        独占一页是必须的：累加从每页顶重新开始，混在一页里就分不清是累加还是分页。
        """
        for i in range(LINES_PER_GROUP):
            parts.append(
                para(run(f"{tag}{i:02d}", size, family), size, family,
                     page_break=(i == 0 and len(parts) > 0))
            )
        probes.append({"tag": tag, "kind": kind, "sizeHalfPoints": size,
                       "family": family, "lines": LINES_PER_GROUP})

    # A 字号扫描（Georgia）。
    for size in SIZES_HALF_POINTS:
        group(f"A{size:02d}", size, "Georgia", "size-sweep")

    # B 字体扫描（13pt）。
    for i, family in enumerate(FAMILIES):
        group(f"B{i}", 26, family, "family-sweep")

    return f"<w:body>{''.join(parts)}{sect_pr('continuous')}</w:body>", probes


def write_docx(path: Path) -> list[dict]:
    body, probes = build_body()
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W}" xmlns:r="{R}">{body}</w:document>'
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    )
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    args = parser.parse_args()
    path = Path(args.output)
    probes = write_docx(path)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"探针 {len(probes)} 组 × {LINES_PER_GROUP} 行 = {len(probes) * LINES_PER_GROUP} 条基线")
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "linesPerGroup": LINES_PER_GROUP, "probes": probes},
                   ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
