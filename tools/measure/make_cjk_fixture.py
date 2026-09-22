#!/usr/bin/env python3
"""CJK 纯文本夹具：第一份能让量具读的中文夹具。

此前本仓库的 22 份采集包里**一个 CJK 字形都没有**（17231 个字形逐份查过）。
`fixtures/plain.docx` 是有的，但它的 `w:eastAsia` 写的是 **SimSun**，
而本机 Word 列不出该字体（`wm preflight` 实测 `SimSun: false`）——
采下去就是字体替换，而替换在几何上完全不可见（方法 §6.2）。所以换 **Songti SC**，
它在 Word 自报的 1614 个字体名里。

判据见 `docs/PREREG-2026-09-18-cjk-plain.md`。

## 为什么每组独占一页

与 `probe-metrics` 同一套办法：每页只有一种字体与一个字号，
于是「每页的首行都是一条已知设置的普通行」，行距可由相邻基线直接读。
首段不加 `pageBreakBefore`——Word 会忽略它，加了会多出一页（F-D）。

## 标签

CJK 正文用的字全部在 Songti SC 里（生成时 `assert_can_draw` 守着）。
标签用**汉字**不用数字：数字在某些字体里缺字形会被 Word 换回退字体，
`page-start` 那一批就是这么废掉的。
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from make_probe_fixture import W, R, para, run, sect_pr
from wordmeasure.fontcover import assert_can_draw

FAMILY = "Songti SC"
FONT_FILE = "/System/Library/Fonts/Supplemental/Songti.ttc"

# 半点。10.5pt（五号）是中文正文最常见的字号，必须在里面。
SIZES_HALF_POINTS = [21, 24, 32, 44]

# 每组的短段落标签：天干，全部是常用字。
LABELS = "甲乙丙丁戊己庚辛壬癸"

# 断行用的长段落。两段的差别只有一处：**乙段在换行点附近放了句读**，
# 用来看行首禁则把标点往前带。文本本身不含空格——CJK 断行走的是按字断，
# 与西文按词断不是同一条路（`font/linebreak.rs` 的注释写了这一点，但从未测过）。
WRAP_PLAIN = (
    "这是一段没有任何格式的中文正文用来看断行点落在哪里"
    "中文字与字之间没有空格所以断行点几乎落在每两个字之间"
    "引擎走的是按字断行的那条路和西文按词断行完全不是一回事"
    "这一段特意写得长一些好让它在页面上换行好几次便于逐行比对"
)
WRAP_KINSOKU = (
    "标点测试，逗号、顿号；分号：冒号。句号！叹号？问号）右括号】右方括号》右书名号"
    "行首禁则要求这些符号不能出现在一行的开头引擎必须把它前面的字一起带到下一行去"
    "所以只要把它们均匀撒在文本里就一定会有几个落在换行点附近，可以逐个查。"
)


def build_body() -> tuple[str, list[dict]]:
    parts: list[str] = []
    probes: list[dict] = []

    def new_page() -> bool:
        return len(parts) > 0

    # —— 第一部分：行距。每个字号一页，六个短段落，相邻基线之差即行距。
    for size in SIZES_HALF_POINTS:
        tags = []
        for i, ch in enumerate(LABELS[:6]):
            tag = f"{ch}{size}"
            parts.append(
                para(run(f"{ch}文", size, FAMILY), size, FAMILY,
                     page_break=(i == 0 and new_page()))
            )
            tags.append(tag)
        probes.append({"group": "pitch", "sizeHalfPoints": size, "family": FAMILY,
                       "paras": [f"{ch}文" for ch in LABELS[:6]]})

    # —— 第二部分：断行。两段长文各占一页，12pt。
    for name, text in (("wrapPlain", WRAP_PLAIN), ("wrapKinsoku", WRAP_KINSOKU)):
        parts.append(para(run(text, 24, FAMILY), 24, FAMILY, page_break=new_page()))
        probes.append({"group": "wrap", "name": name, "sizeHalfPoints": 24,
                       "family": FAMILY, "text": text})

    # —— 第三部分：段落标记。一页三段，每段两个字——
    #    Word 给每个段落标记画一个空格（§4 的 C1），在 CJK 字体上没测过。
    for ch in LABELS[6:9]:
        parts.append(para(run(f"{ch}末", 24, FAMILY), 24, FAMILY,
                          page_break=(ch == LABELS[6])))
    probes.append({"group": "mark", "sizeHalfPoints": 24, "family": FAMILY,
                   "paras": [f"{ch}末" for ch in LABELS[6:9]]})

    body = "".join(parts) + sect_pr("continuous")
    return body, probes


def write_docx(path: Path) -> list[dict]:
    chars = WRAP_PLAIN + WRAP_KINSOKU + "".join(LABELS) + "文末"
    assert_can_draw({FAMILY: FONT_FILE}, chars)

    body, probes = build_body()
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}</w:body></w:document>'
    )
    import zipfile

    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml",
                   '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
                   '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                   '<Default Extension="xml" ContentType="application/xml"/>'
                   '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
                   "</Types>")
        z.writestr("_rels/.rels",
                   '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
                   '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
                   "</Relationships>")
        z.writestr("word/document.xml", document)
    return probes


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="../../fixtures/cjk-plain.docx")
    args = ap.parse_args()
    out = Path(args.out)
    probes = write_docx(out)
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    meta = {"sha256": digest, "family": FAMILY, "fontFile": FONT_FILE, "probes": probes}
    out.with_suffix(".probes.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {out}  sha256={digest[:12]}…  探针组 {len(probes)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
