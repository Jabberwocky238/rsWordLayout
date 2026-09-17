"""字形计数约定（方法 §4）。

**这是最可直接复用的产物**：Word 为每个看不见的字符画什么。

范围限定，抄在这里免得下游只抄结论不抄限定（§7.6）：**拉丁文本、简单 TrueType 字体。**
仍在范围外的：制表符、跨页的表格行、自动编号、行内对象、合成字体（含 CJK 位图化）。

每条约定都带来源与分母。`checked` 写的是原文的检验情况，`platform` 写这条在哪个平台上观测到——
Mac 与 Windows **不能互相替代**（§6.6），但拉丁文本的字形计数类结论目前两平台一致。
"""

from __future__ import annotations

from dataclasses import dataclass, field

from . import OK, UNDECIDABLE

# Word `Range.Text` 里的控制字符。
PARAGRAPH_MARK = "\r"
SOFT_RETURN = "\x0b"  # w:br type="textWrapping"
PAGE_OR_SECTION_BREAK = "\x0c"  # 分节符与手动分页符在 Range.Text 里都是 \x0c
TAB = "\t"
INLINE_OBJECT = "\x01"  # 行内对象占位符


@dataclass(frozen=True)
class Rule:
    """一条计数约定。"""

    name: str
    glyphs: int | None  # None = 范围外，判不了
    checked: str
    platform: str
    note: str = ""


# §4 表，逐条照搬。顺序即原表顺序。
RULES: dict[str, Rule] = {
    "PARAGRAPH_MARK": Rule(
        "段落标记（\\r）", 1, "多份", "windows", "画 1 个空格"
    ),
    "SOFT_RETURN": Rule(
        "软回车（w:br type=textWrapping，\\x0b）", 1, "03d–03h 30/30", "windows"
    ),
    "SECTION_BREAK": Rule(
        "分节符（段内 w:sectPr）", 0, "88 站点实评 76、纯不符 0", "windows",
        "检验实例与设计实例同构，**结构性检验 Windows 侧为 0**（§6.3）",
    ),
    "PAGE_BREAK_OWN_LINE": Rule(
        "手动分页符 · 行首独占、自成一条行记录", 0, "同上，另 Mac 侧 4/4", "windows+mac"
    ),
    "PAGE_BREAK_BEFORE_MARK": Rule(
        "手动分页符 · 紧跟段落标记", 1, "Mac 侧 3/3", "mac",
        "该行连同段落标记共 +2；由字形文本直接确认",
    ),
    "PAGE_BREAK_MID_PARAGRAPH": Rule(
        "手动分页符 · 段中（两侧都有文字）", 0, "Mac 侧 3/3", "mac"
    ),
    "TRAILING_SPACE": Rule(
        "行尾／终止符前的尾随空格", 1, "11a 74/75、13a 16/16、03d/f/h 24/24", "windows",
        "照画，个数不影响计数",
    ),
    "TAB": Rule(
        "制表符（普通、竖线、连续、小数点）", 1, "09a 15/15 行", "windows", "各 1 个空格"
    ),
    "LEADER_TAB": Rule(
        "前导符制表位（dot/hyphen/underscore/heavy/middleDot）", None,
        "09a 30/30（步距）；**个数公式未知**", "windows",
        "把填充字符画成一串普通文字字形，另加 1 个空格。"
        "步距 = 该填充字符在字体里的 advance（30/30，最大差 0.0002pt）；"
        "个数公式 n = floor(跨度÷步距) **已被证否**（两个候选各 3/30），没有替代（§8）",
    ),
    "AUTO_NUMBER_LABEL": Rule(
        "自动编号标签", None, "09a T8 3/3", "windows",
        "画字形，但**源文本里没有对应字符**（Range.Text 里不存在）；"
        "源候选是 Paragraph.Range.ListFormat.ListString",
    ),
    "INLINE_OBJECT": Rule(
        "行内对象（图片等）", 0, "n=1（60pt 图片对应 59.996pt 偏移）", "windows",
        "Range.Text 里 1 个占位字符，PDF 里 0 个字形，后续字形按对象宽度偏移",
    ),
}

# 范围外构造：出现即让该行判不了，不猜。
OUT_OF_SCOPE = {
    TAB: "TAB",  # 普通制表符有约定，但前导符制表位没有，源侧分不出来 → 保守判不了
    INLINE_OBJECT: "INLINE_OBJECT",
}


@dataclass
class LineCount:
    """一行的预期字形数。"""

    state: str  # OK | UNDECIDABLE
    expected: int | None
    text: str
    reasons: list[str] = field(default_factory=list)
    applied: list[str] = field(default_factory=list)
    # 逐源字符画几个字形。`state == OK` 时 sum(per_char) == expected。
    # 配对器用它把字形映回源字符：画 0 个的字符（分节符、段中分页符、行内对象）
    # 直接跳过，不占字形序号。
    per_char: list[int] = field(default_factory=list)


def expected_glyphs(
    text: str,
    *,
    marks: dict[int, str] | None = None,
    page_break_kind: str | None = None,
    treat_tab_as_single_space: bool = False,
) -> LineCount:
    """按 §4 算一行源字符对应的预期字形数。

    `marks` 是**逐字符的构造标注**（行内下标 → `RULES` 的键），由源侧给出。
    给了就照它算，不从字符本身猜——因为字符本身分不出来：

    - 分节符与段落标记都只占 1 个字符位，前者画 **0** 个字形、后者画 **1** 个空格；
    - `\x0c` 在 `Range.Text` 里分节符与手动分页符同形，而手动分页符还分三种位置
      （行首独占 0 / 紧跟段落标记 1 / 段中 0）。

    实测就是在这两条上把计数打偏的：一份 11 页夹具里，四段带 `w:sectPr` 的终止符
    被当成段落标记，整页预期比实测多 2。

    没给 `marks` 时退回按字符猜，猜不出的一律判不了——**不猜**（§7.3）。
    `page_break_kind` 是没有 `marks` 时给 `\x0c` 用的位置提示。

    `treat_tab_as_single_space` 只在调用方能确认该制表位**不是前导符制表位**时才置真——
    前导符的填充个数公式**未知**（§8），默认保守判不了。
    """
    marks = marks or {}
    count = 0
    reasons: list[str] = []
    applied: list[str] = []
    per_char: list[int] = []

    def take(n: int, key: str):
        nonlocal count
        count += n
        per_char.append(n)
        applied.append(key)

    def refuse(reason: str):
        per_char.append(0)
        reasons.append(reason)

    for i, ch in enumerate(text):
        mark = marks.get(i)

        if mark is not None:
            if mark == "PAGE_BREAK":
                refuse(
                    "PAGE_BREAK_POSITION_UNRESOLVED: 手动分页符的位置类型未定；"
                    "行首独占 0 / 紧跟段落标记 1 / 段中 0（§4），调用方须先定位置"
                )
                continue
            if mark == "TAB" and not treat_tab_as_single_space:
                refuse(
                    "LEADER_TAB_UNRESOLVED: 前导符填充个数公式已被证否且无替代（§8）；"
                    "源侧分不出普通制表位与前导符制表位"
                )
                continue
            rule = RULES.get(mark)
            if rule is None:
                refuse("UNKNOWN_MARK: %s" % mark)
                continue
            if rule.glyphs is None:
                refuse("OUT_OF_SCOPE: %s（§4 表末「仍在范围外」）" % mark)
                continue
            take(rule.glyphs, mark)
            continue

        # 没有标注：只能按字符猜，猜不出就判不了。
        if ch == PARAGRAPH_MARK:
            take(RULES["PARAGRAPH_MARK"].glyphs, "PARAGRAPH_MARK")
        elif ch == SOFT_RETURN:
            take(RULES["SOFT_RETURN"].glyphs, "SOFT_RETURN")
        elif ch == PAGE_OR_SECTION_BREAK:
            key = {
                "OWN_LINE": "PAGE_BREAK_OWN_LINE",
                "BEFORE_MARK": "PAGE_BREAK_BEFORE_MARK",
                "MID_PARAGRAPH": "PAGE_BREAK_MID_PARAGRAPH",
                "SECTION": "SECTION_BREAK",
            }.get(page_break_kind or "")
            if key is None:
                refuse(
                    "BREAK_KIND_UNKNOWN: \\x0c 在 Range.Text 里分节符与手动分页符同形，"
                    "源侧分不出；三种位置各画 0/1/0 个字形（§4）"
                )
                continue
            take(RULES[key].glyphs, key)
        elif ch == TAB:
            if treat_tab_as_single_space:
                take(RULES["TAB"].glyphs, "TAB")
            else:
                refuse(
                    "LEADER_TAB_UNRESOLVED: 前导符填充个数公式已被证否且无替代（§8）；"
                    "源侧分不出普通制表位与前导符制表位"
                )
        elif ch in OUT_OF_SCOPE:
            refuse("OUT_OF_SCOPE: %s（§4 表末「仍在范围外」）" % OUT_OF_SCOPE[ch])
        else:
            # 普通可见字符与空格：各 1 个字形。尾随空格照画，个数不影响计数。
            take(1, "LITERAL")

    if reasons:
        return LineCount(UNDECIDABLE, None, text, reasons, applied, per_char)
    return LineCount(OK, count, text, [], applied, per_char)


def resolve_page_break(paragraph_text: str, index: int, *, alone_on_line: bool) -> str:
    """把一个手动分页符定到 §4 的三种位置之一。

    判据要同时看**段落**与**行**，因为原表的三分就是这么划的：

    - 「行首独占、自成一条行记录」是**行**的性质 → 用 `alone_on_line`；
    - 「紧跟段落标记」「段中（两侧都有文字）」是**段落**里的位置。

    判不出返回 `UNKNOWN`——不猜。
    """
    if paragraph_text[index] != PAGE_OR_SECTION_BREAK:
        raise ValueError("不是 \\x0c")
    if alone_on_line:
        return "PAGE_BREAK_OWN_LINE"
    after = paragraph_text[index + 1 :]
    if after[:1] in (PARAGRAPH_MARK, PAGE_OR_SECTION_BREAK) or after == "":
        return "PAGE_BREAK_BEFORE_MARK"
    before = paragraph_text[:index]
    if before.strip() and after.strip(PARAGRAPH_MARK + PAGE_OR_SECTION_BREAK).strip():
        return "PAGE_BREAK_MID_PARAGRAPH"
    return "UNKNOWN"


def classify_break(text: str, index: int) -> str:
    """按 §4 的三分判断 `\\x0c` 的位置类型。

    只按**该行内**的上下文判，判不出返回 `UNKNOWN`——不猜。
    注意分节符与手动分页符在这一层分不开；本函数只处理手动分页符的三种位置，
    分节符要由调用方从 `document.xml` 侧另行认定。
    """
    if text[index] != PAGE_OR_SECTION_BREAK:
        raise ValueError("不是 \\x0c")
    before = text[:index]
    after = text[index + 1 :]
    if after.startswith(PARAGRAPH_MARK):
        return "BEFORE_MARK"
    if not before.strip("\r\x0b") and not after.strip("\r\x0b"):
        return "OWN_LINE"
    if before.strip() and after.strip():
        return "MID_PARAGRAPH"
    return "UNKNOWN"


def rules_table() -> list[dict]:
    """把约定表导出成数据，供报告与下游复用。"""
    return [
        {
            "key": key,
            "name": rule.name,
            "glyphs": rule.glyphs,
            "checked": rule.checked,
            "platform": rule.platform,
            "note": rule.note,
            "inScope": rule.glyphs is not None,
        }
        for key, rule in RULES.items()
    ]
