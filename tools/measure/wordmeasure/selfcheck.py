"""引擎轨迹的自检：不需要 Word 采集也能查的那些。

为什么要单独有这一层：Word 采集很贵（要真 Word、要授权、要核字体），
而**有一类错在轨迹自己身上就能看出来**——契约说了什么、轨迹又填了什么，两者对不上。
这类错必须先清掉，否则拿去和 Word 比，差值里混着契约错，分不出是布局错还是记账错。

三态出口与别处一致：成立／不成立／判不了（方法 §7.3）。
「判不了」在这里是真会出现的——比如没接整形器时字形序列为空，
那种轨迹不是「字形数为 0 这个事实」，而是「这份轨迹没覆盖字形层」。
"""

from __future__ import annotations

from . import FAIL, OK, UNDECIDABLE


def _lines(trace: dict):
    for page in trace.get("pages", []):
        for line in page.get("lines", []):
            yield page, line


def check_source_offsets(trace: dict) -> dict:
    """源字符区间必须是**全篇偏移**，不是段内偏移。

    契约写的是「与 Word 的 `Range.Start/End` 一致」。Word 的偏移是全篇连续的：
    各段文本依次拼接，每段末尾算一个终止符。段内偏移长得几乎一样——都是小整数、
    都单调——但它**对不上 Word**，而且对不上的方式很隐蔽：比较器会以为两边在说同一件事。

    判据：全篇 `sourceStart` 回到 0 的次数应当只有 1 次（第一行）。
    每段都回 0，说明记的是段内偏移。
    """
    starts = [line.get("sourceStart") for _, line in _lines(trace)]
    known = [s for s in starts if s is not None]
    if not known:
        return {
            "check": "源字符区间是全篇偏移",
            "state": UNDECIDABLE,
            "reason": "没有一行带源区间；这份轨迹不覆盖该层",
        }

    resets = sum(1 for s in known if s == 0)
    max_end = max((line.get("sourceEnd") or 0) for _, line in _lines(trace))
    record = {
        "check": "源字符区间是全篇偏移",
        "state": OK if resets <= 1 else FAIL,
        "linesWithRange": len(known),
        "linesWithoutRange": len(starts) - len(known),
        "startsAtZero": resets,
        "maxSourceEnd": max_end,
    }
    if resets > 1:
        record["reason"] = (
            "`sourceStart` 回到 0 共 %d 次（全篇应当只有 1 次），最大 `sourceEnd` 只有 %d——"
            "记的是**段内**偏移，不是全篇偏移。这样的区间与 Word 的 Range 对不上，"
            "配对没法用它校验。" % (resets, max_end)
        )
    return record


def check_terminator_glyphs(trace: dict) -> dict:
    """引擎自报的终止符应产出字形数，要和它自己画出来的对得上。

    轨迹里每行既有 `terminatorExpectedGlyphs`（按 §4 约定该画几个），
    又有实际画出来的字形。**这两个数出自同一份记录**，对不上就是引擎自相矛盾，
    不必等 Word 来判。

    只查终止符**独占一行**的情形——那时该行的字形应当恰好等于终止符该产出的数目。
    行里还有正文时分不出哪个字形属于终止符，跳过。

    `WRAP` 要整个跳过：它表示「自动换行，源侧没有对应字符」，
    那种行的源区间全是正文。拿它来判会把「1 个字符的正文行」误报成矛盾——
    这条是写完第一版后看数据才发现的，`complex` 上一口气误报了 125 行。
    """
    conflicts = []
    checked = 0
    for page, line in _lines(trace):
        expected = line.get("terminatorExpectedGlyphs")
        start, end = line.get("sourceStart"), line.get("sourceEnd")
        if expected is None or start is None or end is None:
            continue
        # WRAP 没有终止符字符，整行都是正文——这条查不了它。
        if line.get("terminator") == "WRAP":
            continue
        # 终止符独占一行：源区间只有 1 个字符。
        if end - start != 1:
            continue
        checked += 1
        actual = len(line.get("glyphs", []))
        if actual != expected:
            conflicts.append(
                {
                    "page": page["index"],
                    "line": line["index"],
                    "terminator": line.get("terminator"),
                    "declaredExpected": expected,
                    "actuallyEmitted": actual,
                    "glyphIds": [g.get("glyphId") for g in line.get("glyphs", [])],
                }
            )

    if checked == 0:
        return {
            "check": "自报的终止符字形数 vs 实际画出来的",
            "state": UNDECIDABLE,
            "reason": "没有终止符独占的行可查",
        }
    return {
        "check": "自报的终止符字形数 vs 实际画出来的",
        "state": OK if not conflicts else FAIL,
        "linesChecked": checked,
        "conflicts": conflicts,
        "reason": None
        if not conflicts
        else "%d 行自相矛盾：记录说该终止符产出 N 个字形，同一条记录里却画了 M 个。"
        "两个数出自同一份轨迹，不必等 Word 判。" % len(conflicts),
    }


def check_line_granularity(trace: dict) -> dict:
    """一行只能有一条记录。

    实测踩过：`LayoutRecord::from_paint` 把每条 `DrawCmd::DrawGlyphs` 当一行，
    而绘制指令是**按 run** 出的——一行里有几个 run（换字体、上标、分页符）就出几条。
    于是「行」这一层被 run 切碎，比较器的行层配对必然对不上，
    而且失败看起来像「引擎少排了行」，其实是记账粒度错了。

    判据：同一行的记录不该出现「源区间接续上一条」的连排。
    段内偏移下查不了这条（每段都从 0 重来），此时记判不了。
    """
    if check_source_offsets(trace)["state"] != OK:
        return {
            "check": "一行一条记录",
            "state": UNDECIDABLE,
            "reason": "源区间不是全篇偏移，接续关系无从判断——先修那条",
        }

    runs = []
    previous = None
    for page, line in _lines(trace):
        start, end = line.get("sourceStart"), line.get("sourceEnd")
        if start is None or end is None:
            previous = None
            continue
        if previous is not None and previous[0] == page["index"] and previous[1] == start:
            runs.append({"page": page["index"], "line": line["index"], "continuesFrom": start})
        previous = (page["index"], end)

    return {
        "check": "一行一条记录",
        "state": OK if not runs else FAIL,
        "contiguousRecords": len(runs),
        "sample": runs[:6],
        "reason": None
        if not runs
        else "%d 条记录的源区间紧接上一条——很可能是按 run 出的记录被当成了行。" % len(runs),
    }


def check_glyph_coverage(trace: dict) -> dict:
    """字形层到底有没有覆盖。

    没接整形器时绘制指令不产字形序列，轨迹里只有行。那种轨迹**过不了比较器的字形层**，
    但它长得和「这些行确实没有字形」一模一样——必须分开说，否则下游会把
    「没量」读成「量过且为 0」（§7.4：未核与核过无发现分两栏）。
    """
    total_lines = sum(1 for _ in _lines(trace))
    with_glyphs = sum(1 for _, line in _lines(trace) if line.get("glyphs"))
    if total_lines == 0:
        return {"check": "字形层覆盖", "state": UNDECIDABLE, "reason": "轨迹里没有行"}
    if with_glyphs == 0:
        return {
            "check": "字形层覆盖",
            "state": UNDECIDABLE,
            "totalLines": total_lines,
            "linesWithGlyphs": 0,
            "reason": "一个字形都没有——多半是没注册整形器。"
            "这不等于「这些行没有字形」，是这份轨迹**没覆盖**字形层。",
        }
    return {
        "check": "字形层覆盖",
        "state": OK,
        "totalLines": total_lines,
        "linesWithGlyphs": with_glyphs,
    }


CHECKS = (
    check_glyph_coverage,
    check_source_offsets,
    check_terminator_glyphs,
    check_line_granularity,
)


def run(trace: dict) -> dict:
    """跑全部自检。顶层 state 按最坏的一项走，且判不了不折叠进不成立（§7.3）。"""
    results = [check(trace) for check in CHECKS]
    states = {r["state"] for r in results}
    if FAIL in states:
        state = FAIL
    elif UNDECIDABLE in states:
        state = UNDECIDABLE
    else:
        state = OK
    return {
        "schema": "rsword-layout-selfcheck/1",
        "state": state,
        "note": "这一层只查轨迹自身与契约对不对得上，**不涉及 Word**。"
        "这里不通过，就别拿去和 Word 比——差值里会混着记账错。",
        "checks": results,
    }
