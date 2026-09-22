#!/usr/bin/env python3
"""kinsoku 的变体：把 `w:overflowPunct` 显式关掉，逼行首禁则动。

判据 `docs/PREREG-2026-09-18-kinsoku.md`。`cjk-plain` 实测 Songti SC 12pt 下
一行正好 37 个汉字，所以把标点放在**第 38 位**，让它必须落到下一行的行首——
除非 Word 用某条规则躲开。躲的方式有两种，给出的字数相反，这一批就判哪条先动。
"""
from __future__ import annotations
import argparse, hashlib, json, zipfile
from pathlib import Path
from make_probe_fixture import W, R, para, run, sect_pr
from wordmeasure.fontcover import assert_can_draw

FAMILY = "Songti SC"
FONT_FILE = "/System/Library/Fonts/Supplemental/Songti.ttc"
SIZE = 24                      # 半点 = 12pt

# 37 个汉字填满第一行，第 38 位放标点，后面再跟 24 个汉字（标点不在段末）。
HEAD = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥东南西北中上下左右前后内外天地"
TAIL = "春夏秋冬风雨雷电山川草木花鸟鱼虫日月星辰云霞露霜"
PUNCTS = {"K1": "。", "K2": "，", "K3": "）", "K4": "、"}

assert len(HEAD) == 37, len(HEAD)
assert len(TAIL) == 24, len(TAIL)  # 标点后面还有一整行的量，保证它不在段末


def build_body():
    parts, probes = [], []
    for i, (tag, p) in enumerate(PUNCTS.items()):
        text = HEAD + p + TAIL
        parts.append(para(run(text, SIZE, FAMILY), SIZE, FAMILY, page_break=(i > 0)))
        probes.append({"tag": tag, "punct": p, "text": text,
                       "punctIndex": len(HEAD), "sizeHalfPoints": SIZE, "family": FAMILY})
    return "".join(parts) + sect_pr("continuous"), probes


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="../../fixtures/kinsoku2.docx")
    a = ap.parse_args()
    out = Path(a.out)
    assert_can_draw({FAMILY: FONT_FILE}, HEAD + TAIL + "".join(PUNCTS.values()))
    body, probes = build_body()
    # `w:overflowPunct` 是段落属性，Word 默认开。关掉它，看轮到谁动。
    # 插在 `w:jc` 之前——`w:pPr` 的子元素有固定次序，位置错了 Word 会忽略整段属性。
    body = body.replace('<w:jc ', '<w:overflowPunct w:val="false"/><w:jc ')
    doc = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
           f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}</w:body></w:document>')
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml",
                   '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
                   '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                   '<Default Extension="xml" ContentType="application/xml"/>'
                   '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
        z.writestr("_rels/.rels",
                   '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
                   '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
        z.writestr("word/document.xml", doc)
    d = hashlib.sha256(out.read_bytes()).hexdigest()
    out.with_suffix(".probes.json").write_text(
        json.dumps({"sha256": d, "family": FAMILY, "fontFile": FONT_FILE, "probes": probes},
                   ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {out}  sha256={d[:12]}…  段 {len(probes)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
