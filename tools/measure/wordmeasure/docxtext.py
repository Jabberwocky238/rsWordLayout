"""从 `word/document.xml` 推出 Word 偏移空间里的正文文本。

为什么需要它：计数约定（§4）的输入是**源字符**，而 Word 的 `Range.Text` 通道并非总能拿到
（离线复算既有采集包时就没有）。所以这里从 `document.xml` 推一份。

**推出来的东西必须被核过才能用。** §6.4 的教训是「不要从源侧推排版结果」——
正文字符序列是源侧可定的（与「跨不跨页」不同，那是 Word 排出来的），所以推它合法；
但推完要拿采集侧的两个独立读数对：

1. `endOfContent` —— 正文总字符数；
2. `Document.Paragraphs` 的逐段区间 —— 段边界。

两个都对上才算数，对不上就判不了，不猜（`verify()` 返回三态）。

映射（对应 §4 的约定表）：

| `document.xml` | `Range.Text` |
| --- | --- |
| `<w:t>` 文本 | 原样 |
| `<w:tab/>` | `\\t` |
| `<w:br w:type="page"/>` | `\\x0c` |
| `<w:br/>`、`<w:br w:type="textWrapping"/>` | `\\x0b` |
| 段末（`</w:p>`） | `\\r` |
| `<w:drawing>` / `<w:object>` | `\\x01`（行内对象占位符） |
"""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

from . import OK, UNDECIDABLE

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


# 每个控制字符**是什么**，直接标出来，不让下游从字符本身去猜。
# 这一条是实测逼出来的：分节符与段落标记在 `Range.Text` 里都占 1 个字符，
# 但前者画 0 个字形、后者画 1 个空格（§4）。只比长度的核查区分不出这两者——
# 检验必须在**被判的那一层**上有差别（§7.2）。
MARK_PARAGRAPH = "PARAGRAPH_MARK"
MARK_SECTION = "SECTION_BREAK"
MARK_PAGE_BREAK = "PAGE_BREAK"
MARK_SOFT_RETURN = "SOFT_RETURN"
MARK_TAB = "TAB"
MARK_INLINE_OBJECT = "INLINE_OBJECT"


def _paragraph_text(para: ET.Element) -> tuple[str, dict[int, str]]:
    """段落正文与逐控制字符的标注。返回 (文本, {段内下标: 标记})。"""
    out: list[str] = []
    marks: dict[int, str] = {}

    def push(text: str, mark: str | None = None):
        if mark is not None:
            marks[len("".join(out))] = mark
        out.append(text)

    for node in para.iter():
        tag = node.tag
        if tag == W + "t":
            push(node.text or "")
        elif tag == W + "tab":
            push("\t", MARK_TAB)
        elif tag == W + "br":
            kind = node.get(W + "type")
            if kind in ("page", "column"):
                push("\x0c", MARK_PAGE_BREAK)
            else:
                push("\x0b", MARK_SOFT_RETURN)
        elif tag in (W + "drawing", W + "object", W + "pict"):
            push("\x01", MARK_INLINE_OBJECT)
        elif tag == W + "cr":
            push("\x0b", MARK_SOFT_RETURN)
    return "".join(out), marks


def content_text(docx: Path) -> dict:
    """推出正文文本与逐段区间。

    只走主文档故事（`w:body` 的顶层 `w:p`）。表格里的段落**不并进来**——
    表格仍在范围外（§4 / §8），并进来只会让偏移悄悄错位。
    """
    with zipfile.ZipFile(Path(docx)) as archive:
        xml = archive.read("word/document.xml")
    root = ET.fromstring(xml)
    body = root.find(W + "body")
    if body is None:
        raise ValueError("document.xml 里没有 w:body")

    paragraphs = []
    text_parts = []
    marks: dict[int, str] = {}
    offset = 0
    tables = 0
    for child in body:
        if child.tag == W + "tbl":
            tables += 1
            continue
        if child.tag != W + "p":
            continue
        body_text, body_marks = _paragraph_text(child)
        for at, mark in body_marks.items():
            marks[offset + at] = mark

        # 段末终止符：段内带 `w:sectPr` 的是**分节符**（`\x0c`，画 0 个字形），
        # 不带的才是段落标记（`\r`，画 1 个空格）。两者都只占 1 个字符位，
        # 所以任何只核长度的检查都分不出它们——实测正是这条把计数打偏了。
        pPr = child.find(W + "pPr")
        has_sect = pPr is not None and pPr.find(W + "sectPr") is not None
        terminator = "\x0c" if has_sect else "\r"
        marks[offset + len(body_text)] = MARK_SECTION if has_sect else MARK_PARAGRAPH
        body_text += terminator

        paragraphs.append(
            {
                "index": len(paragraphs),
                "start": offset,
                "end": offset + len(body_text),
                "terminator": MARK_SECTION if has_sect else MARK_PARAGRAPH,
            }
        )
        text_parts.append(body_text)
        offset += len(body_text)

    return {
        "text": "".join(text_parts),
        "marks": marks,
        "paragraphs": paragraphs,
        "endOfContent": offset,
        "tablesSkipped": tables,
        "derivation": "推自 word/document.xml；使用前必须经 verify() 与采集读数对上（§6.4）",
    }


def verify(derived: dict, *, end_of_content: int, paragraphs: list[dict] | None) -> dict:
    """拿采集侧的独立读数核这份推导。

    两条都对上才算 `OK`。任一条对不上就是 `UNDECIDABLE`——
    那说明映射表漏了某个构造，此时拿它去算计数只会得到看着像对的错答案。
    """
    record = {"state": OK, "checks": {}, "reasons": []}

    # 先说清楚这组核查**不能**核什么：长度与段边界都分不出
    # 分节符（\x0c，0 个字形）与段落标记（\r，1 个空格）——两者都占 1 个字符位。
    # 能分出它们的是逐页字形数（见 wordmodel），不是这里。写出来，免得下游把
    # 「核过」读成「全都核过」（§7.4：未核与核过无发现分两栏）。
    record["doesNotCheck"] = [
        "终止符身份（\\r vs \\x0c）：两者都占 1 个字符位，长度与段边界核查对此不敏感（§7.2）",
    ]

    record["checks"]["endOfContent"] = {
        "derived": derived["endOfContent"],
        "captured": end_of_content,
        "agree": derived["endOfContent"] == end_of_content,
    }
    if not record["checks"]["endOfContent"]["agree"]:
        record["state"] = UNDECIDABLE
        record["reasons"].append(
            "END_OF_CONTENT_MISMATCH: 推出 %d 字符，采集读数 %d。"
            "`document.xml` → `Range.Text` 的映射漏了构造，不能拿它算计数（§6.4）。"
            % (derived["endOfContent"], end_of_content)
        )

    if paragraphs is None:
        record["checks"]["paragraphs"] = {"state": "NOT_CHECKED", "note": "采集包没给段落区间"}
        # 「未核」与「核过无发现」分两栏——长得像，意思相反（§7.4）。
        if record["state"] == OK:
            record["state"] = UNDECIDABLE
            record["reasons"].append("PARAGRAPHS_NOT_CHECKED: 只核了总长，段边界未核")
    else:
        derived_ranges = [(p["start"], p["end"]) for p in derived["paragraphs"]]
        captured_ranges = [(p["start"], p["end"]) for p in paragraphs]
        agree = derived_ranges == captured_ranges
        record["checks"]["paragraphs"] = {
            "derivedCount": len(derived_ranges),
            "capturedCount": len(captured_ranges),
            "agree": agree,
        }
        if not agree:
            record["state"] = UNDECIDABLE
            first = next(
                (i for i, (a, b) in enumerate(zip(derived_ranges, captured_ranges)) if a != b),
                min(len(derived_ranges), len(captured_ranges)),
            )
            record["checks"]["paragraphs"]["firstDivergence"] = first
            record["reasons"].append(
                "PARAGRAPH_RANGE_MISMATCH: 第 %d 段起区间不一致（推 %s，采 %s）"
                % (
                    first,
                    derived_ranges[first] if first < len(derived_ranges) else None,
                    captured_ranges[first] if first < len(captured_ranges) else None,
                )
            )
    return record
