"""配对器（方法 §3）。

**这是整个方法的关键，也是唯一需要动脑的地方。**

三条纪律钉在这里：

1. **不靠 Unicode 身份配对**（§3.1）。PDF 里的字形可能没有 ToUnicode 映射。
   身份只作配对之后的核对手段。
2. **按读序配对**（§3.2）。同一行内 PDF 字形按内容流出现顺序编号 0..G−1，
   源字符按顺序编号 0..C−1，同序号相配。这条**前提必须显式写进验收定义**，
   不能默认——见 `PREMISES`。
3. **字形归行按行盒纵向包含，不按 y 聚类**（§3.3）。有抬升 run 的行会被 y 聚类拆成两簇，
   实测 Δy = 4.08pt 就足够，且 Windows 与 Mac 都复现。

平台差异（§6.6）：Mac 的 AppleScript 桥**拿不到行盒**，所以 §3.3 的盒包含在 Mac 上做不了。
Mac 走 `DERIVED_FROM_LINE_NUMBERS` 归行——那是**推算**不是测量，代价写在 `PREMISES` 里。
"""

from __future__ import annotations

import unicodedata

from dataclasses import dataclass, field

from . import FAIL, OK, UNDECIDABLE
from . import counting

# 验收定义里必须显式承认的前提。报告会把它们原样印出来，
# 让下游没法只抄结论不抄限定（§7.6）。
PREMISES = {
    "CONTENT_STREAM_ORDER": (
        "PDF 内容流的绘制顺序等于源字符顺序。这是一条新引入的前提（§3.2），"
        "本量具未独立验证它，只在配对失败时把它列为候选原因。"
    ),
    "BOX_CONTAINMENT": (
        "字形归行按行盒纵向包含（§3.3）。八份采集实测 outsideBox 与重叠命中全为 0。"
        "仅在采集提供行盒时可用（Windows COM 通道）。"
    ),
    "DERIVED_FROM_LINE_NUMBERS": (
        "**Mac 专用、推算而非测量。** Mac 拿不到行盒（§6.6），改用 first character line number "
        "把源字符分到行，再按每行预期字形数累加切分 PDF 字形。"
        "代价：计数模型错了会静默串行。缓解：切分前先核**整页**预期总数与实测总数相等，"
        "不等即判整页不可判，不猜。"
    ),
}

# §3.2 的两种配对模式。
MODE_ALL = "ALL_CHARS"
MODE_NON_WHITESPACE = "NON_WHITESPACE"

# 第三种模式，**超出 §3.2 原文**：按 §4 的逐字符计数模型配对。
#
# 由来（照 §7.5 标清楚）：§3.2 只给了「全字符」与「非空白字符」两种序列，
# 而实测存在两者皆非的行——一份 11 页夹具里 7 行如此（G=10，C=11，C'=9）。
# 差额来自**画 0 个字形的源字符**：分节符、段中分页符、行内对象。
# §4 本来就逐条给了这些字符画几个，把它用于配对是这条既有约定的直接后果。
#
# **纪律**：这条模式是在看过 PRECHECK_NEITHER 之后才成形的，所以
# 只靠它配上的行标 `backtest=True`，**不当独立检验**（§7.5）。
MODE_COUNT_MODEL = "COUNT_MODEL"
MODE_COUNT_MODEL_PROVENANCE = (
    "超出 §3.2 原文的第三种模式：按 §4 逐字符计数模型配对，画 0 个字形的源字符不占字形序号。"
    "在看过 PRECHECK_NEITHER 读数之后才成形，按 §7.5 标『回测』，不当独立检验。"
)


@dataclass
class LinePairing:
    """一行的配对结果。"""

    page: int
    line: int
    state: str
    source_text: str
    source_start: int
    glyph_count: int
    expected: int | None
    mode: str | None
    pairs: list[tuple[int, int]] = field(default_factory=list)  # (源字符下标, 字形下标)
    reasons: list[str] = field(default_factory=list)
    identity_checked: int = 0
    identity_mismatched: int = 0
    # 只靠 MODE_COUNT_MODEL 才配上的行：回测，不当独立检验（§7.5）。
    backtest: bool = False
    # 每个字形由 §4 的哪条约定产生（与 glyphs 等长）。
    # 用途是让验收定义能**按约定显式排除**某一类字形，而不是靠放宽容差蒙混过去。
    glyph_rules: list[str] = field(default_factory=list)


def precheck(glyph_count: int, text: str) -> tuple[str, list[str]]:
    """§3.2 的前置检查：定这一行按哪种序列配对。

    同一份文档里三种情形可能混着出现，**按行记，不按文档记**。
    """
    all_chars = len(text)
    non_ws = len([c for c in text if not c.isspace()])
    if glyph_count == all_chars:
        return MODE_ALL, []
    if glyph_count == non_ws:
        return MODE_NON_WHITESPACE, []
    return UNDECIDABLE, [
        "PRECHECK_NEITHER: G=%d，全字符数 C=%d，非空白字符数 C'=%d，两者皆非，不猜（§3.2）"
        % (glyph_count, all_chars, non_ws)
    ]


def pair_line(
    page: int,
    line: int,
    text: str,
    source_start: int,
    glyphs: list[dict],
    *,
    marks: dict[int, str] | None = None,
    page_break_kind: str | None = None,
) -> LinePairing:
    """把一行的源字符与 PDF 字形按读序配对，并逐行报三态（§9.5）。"""
    g = len(glyphs)
    result = LinePairing(
        page=page,
        line=line,
        state=UNDECIDABLE,
        source_text=text,
        source_start=source_start,
        glyph_count=g,
        expected=None,
        mode=None,
    )

    # 先按 §4 算预期数，再按 §3.2 定配对模式。两者分开记：
    # 计数模型与配对模式是两件事，混起来会让「计数错」与「配对错」分不开。
    predicted = counting.expected_glyphs(text, marks=marks, page_break_kind=page_break_kind)
    result.expected = predicted.expected
    if predicted.state == UNDECIDABLE:
        # 计数模型说判不了，这一行就是判不了——**不管 §3.2 的前置检查是否碰巧对上**。
        #
        # 碰巧对上很容易发生：`'A\tB'` 有 3 个源字符，若该制表位恰好画出 3 个字形
        # （前导符填充），MODE_ALL 就会「成立」。但前导符的个数公式已被证否（§8），
        # 我们并不知道那 3 个是不是这么来的。拿巧合当判定，就是把「判不了」
        # 这个出口悄悄取消掉（§7.3），也正是证否条件 F4 说的那件事。
        result.reasons.extend(predicted.reasons)
        result.state = UNDECIDABLE
        return result

    mode, reasons = precheck(g, text)
    if mode == UNDECIDABLE:
        # 预期字形序列为空的行：0 个字形记通过（空过，§4 末行）。
        if g == 0 and predicted.expected == 0:
            result.state = OK
            result.mode = MODE_ALL
            result.reasons = ["EMPTY_LINE_PASS: 预期字形序列为空，0 个字形记通过（§4）"]
            return result
        # §3.2 的两种序列都不对，但 §4 的逐字符模型给出了确定的总数且与实测相等：
        # 走计数模型配对，并标回测（§7.5）。模型本身判不了就还是判不了。
        if predicted.expected == g:
            result.mode = MODE_COUNT_MODEL
            result.backtest = True
            result.reasons.append(
                "PRECHECK_NEITHER_USED_COUNT_MODEL: %s；%s" % (reasons[0], MODE_COUNT_MODEL_PROVENANCE)
            )
            indices = [i for i, k in enumerate(predicted.per_char) for _ in range(k)]
        else:
            result.reasons.extend(reasons)
            result.state = UNDECIDABLE
            return result
    else:
        result.reasons.extend(reasons)
        result.mode = mode
        indices = [i for i, c in enumerate(text) if mode == MODE_ALL or not c.isspace()]

    if len(indices) != g:
        result.state = UNDECIDABLE
        result.reasons.append(
            "PRECHECK_INCONSISTENT: 模式 %s 给出 %d 个源字符，字形 %d 个" % (result.mode, len(indices), g)
        )
        return result

    result.pairs = list(zip(indices, range(g)))

    # 逐字形标注产生它的约定。`applied` 与源字符一一对应（state 为 OK 时没有 refuse），
    # 某个字符画 k 个字形，这 k 个就都记它那条。
    if len(predicted.applied) == len(text):
        result.glyph_rules = [
            predicted.applied[i] for i, k in enumerate(predicted.per_char) for _ in range(k)
        ]
    else:
        result.glyph_rules = ["UNKNOWN"] * g

    # 身份核对（§3.1）：**配对之后**的核对手段，不参与配对，不改 state。
    for src_i, gi in result.pairs:
        glyph = glyphs[gi]
        if glyph.get("textStatus") != "mapped" or glyph.get("text") is None:
            continue
        result.identity_checked += 1
        if glyph["text"] != text[src_i]:
            # 空格类等价：Word 把段落标记、软回车、制表符都画成空格（§4）。
            #
            # 两侧都要认，不能只认 PDF 侧解出 `" "` 的情形：子集字体的 ToUnicode
            # 本身就不可靠（§3.1 明说这条对 CJK 子集无效，只能当配对之后的核对手段）。
            # 实测 AppleMyungjo 的子集把段落标记那个空格字形解成 `"\t"`——
            # 5 份采集包、**442 行**全部是这一形，零例外（font-free 40、noise-floor 21、
            # page-start 61、recursive 160、shift-floor 160），而不含该字体的 17 份一条都没有。
            # 旧写法把它们全判成身份不符，于是预注册的证否条件 **F3 命中 442 次**。
            # 那不是读序前提出了问题，是拿一份已知不可靠的映射去核对空白字符的类别。
            #
            # 二、**同形等价**：子集字体的 ToUnicode 反查不是单射。CJK 里汉字与它的
            # 康熙部首共用同一个字形，反查会挑到部首那个码位——实测 Songti SC 的
            # `文 U+6587` 解成 `⽂ U+2F8B`、`行` 解成 `⾏`、`一` 解成 `⼀`……
            # cjk-plain 那份采集里 **68 处身份不符全部是这一形，零例外**，
            # 而且全部是「本身就是部首」的那些字，其余汉字一个都没错。
            # Unicode 自己定义了这层等价（康熙部首的兼容分解），所以按 NFKC 比，
            # 不是自己编一张表。实测 14/14 对上，且 文/丈、己/已、日/曰 不会被弄混。
            same_space = glyph["text"].isspace() and text[src_i].isspace()
            same_shape = unicodedata.normalize("NFKC", glyph["text"]) == unicodedata.normalize(
                "NFKC", text[src_i]
            )
            if not (same_space or same_shape):
                result.identity_mismatched += 1

    if predicted.expected != g:
        result.state = FAIL
        result.reasons.append(
            "COUNT_MISMATCH: §4 预期 %d 个字形，实测 %d 个" % (predicted.expected, g)
        )
        return result

    result.state = OK
    return result


def assign_by_box(glyphs: list[dict], line_boxes: list[dict]) -> tuple[dict[int, list[dict]], list[dict]]:
    """§3.3：按行盒纵向包含把字形归行（Windows 通道）。

    把字形原点的 y（已翻成页顶向下）落在 `top` 到 `top + height` 之间即归该行。
    **不要按基线 y 做聚类**——有抬升 run 的行会被拆成两簇。

    返回 (按行下标分组的字形, 落在所有行盒之外的字形)。八份采集实测 outsideBox 为 0，
    非 0 就是信号，不要静默丢掉。
    """
    grouped: dict[int, list[dict]] = {i: [] for i in range(len(line_boxes))}
    outside: list[dict] = []
    for glyph in glyphs:
        y = glyph["glyphOrigin"][1]
        hit = [i for i, box in enumerate(line_boxes) if box["top"] <= y <= box["top"] + box["height"]]
        if len(hit) == 1:
            grouped[hit[0]].append(glyph)
        elif not hit:
            outside.append(dict(glyph, assignment="OUTSIDE_BOX"))
        else:
            outside.append(dict(glyph, assignment="OVERLAPPING_BOXES", candidates=hit))
    return grouped, outside


def assign_by_line_numbers(
    glyphs: list[dict],
    lines: list[dict],
    *,
    marks: dict[int, dict[int, str]] | None = None,
    page_break_kinds: dict[int, str] | None = None,
) -> tuple[dict[int, list[dict]] | None, dict]:
    """Mac 通道的归行：按每行预期字形数累加切分（推算，不是测量）。

    切分**前**先核整页：Σ 各行预期 == 该页实测字形总数。不等即整页不可判，不猜——
    这是防止计数模型出错后静默串行的唯一闸门。
    """
    kinds = page_break_kinds or {}
    per_line_marks = marks or {}
    per_line = []
    for i, line in enumerate(lines):
        predicted = counting.expected_glyphs(
            line["text"], marks=per_line_marks.get(i), page_break_kind=kinds.get(i)
        )
        per_line.append(predicted)

    if any(p.state != OK for p in per_line):
        return None, {
            "state": UNDECIDABLE,
            "reason": "PAGE_COUNT_MODEL_INCOMPLETE",
            # 同一条理由在一行里可能出现多次（一行有几个制表位就几条），
            # 全列出来只会把别的理由淹掉；这里保序去重。
            "detail": [
                {"line": i, "reasons": sorted(set(p.reasons))}
                for i, p in enumerate(per_line)
                if p.state != OK
            ],
        }

    total = sum(p.expected for p in per_line)
    if total != len(glyphs):
        return None, {
            "state": UNDECIDABLE,
            "reason": "PAGE_COUNT_MISMATCH",
            "detail": "整页 §4 预期 %d 个字形，实测 %d 个；不切分，不猜（推算归行的前提不成立）"
            % (total, len(glyphs)),
            "expected": total,
            "actual": len(glyphs),
        }

    grouped: dict[int, list[dict]] = {}
    at = 0
    for i, predicted in enumerate(per_line):
        grouped[i] = glyphs[at : at + predicted.expected]
        at += predicted.expected
    return grouped, {
        "state": OK,
        "method": "DERIVED_FROM_LINE_NUMBERS",
        "premise": PREMISES["DERIVED_FROM_LINE_NUMBERS"],
        "pageExpected": total,
        "pageActual": len(glyphs),
    }
