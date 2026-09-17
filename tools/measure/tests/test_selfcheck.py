"""轨迹自检的检验。

这一层不碰 Word，所以测试也不需要采集包。断言钉的是**它该抓什么、不该抓什么**——
一个只会说「通过」的自检没有价值，一个乱报的自检会把人引到错的地方去。
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wordmeasure import FAIL, OK, UNDECIDABLE, selfcheck


def line(index, start, end, terminator="WRAP", expected=0, glyphs=0, glyph_id=1):
    return {
        "index": index,
        "sourceStart": start,
        "sourceEnd": end,
        "terminator": terminator,
        "terminatorExpectedGlyphs": expected,
        "glyphs": [{"origin": [0.0, 0.0], "glyphId": glyph_id} for _ in range(glyphs)],
    }


def trace(lines, pages=1):
    if pages == 1:
        return {"pages": [{"index": 0, "lines": lines}]}
    per = (len(lines) + pages - 1) // pages
    return {
        "pages": [
            {"index": i, "lines": lines[i * per : (i + 1) * per]} for i in range(pages)
        ]
    }


# ---- 源字符区间 ----


def test_document_global_offsets_pass():
    t = trace([line(0, 0, 10, glyphs=10), line(1, 10, 20, glyphs=10)])
    assert selfcheck.check_source_offsets(t)["state"] == OK


def test_paragraph_relative_offsets_fail():
    """每段都从 0 重来 → 段内偏移。

    逼出这条的读数：当前 main 上 5 份夹具全中，`sourceStart` 回到 0 的次数
    正好等于段落数（mr1 15 次、complex 42 次），而最大 `sourceEnd` 远小于篇长。
    """
    t = trace([line(0, 0, 10, glyphs=10), line(1, 0, 8, glyphs=8), line(2, 0, 5, glyphs=5)])
    result = selfcheck.check_source_offsets(t)
    assert result["state"] == FAIL
    assert result["startsAtZero"] == 3
    assert "段内" in result["reason"]


def test_no_ranges_is_undecidable():
    """一条区间都没有 ≠ 区间是对的。"""
    t = trace([line(0, None, None, glyphs=1)])
    assert selfcheck.check_source_offsets(t)["state"] == UNDECIDABLE


# ---- 终止符自报 vs 实画 ----


def test_terminator_conflict_is_caught():
    """记录说该终止符画 0 个，同一条记录里却画了 1 个——不必等 Word 判。"""
    t = trace([
        line(0, 0, 21, terminator="WRAP", glyphs=21),
        line(1, 21, 22, terminator="PAGE_BREAK_MID_PARAGRAPH", expected=0, glyphs=1, glyph_id=2323),
    ])
    result = selfcheck.check_terminator_glyphs(t)
    assert result["state"] == FAIL
    assert result["conflicts"][0]["declaredExpected"] == 0
    assert result["conflicts"][0]["actuallyEmitted"] == 1
    assert result["conflicts"][0]["glyphIds"] == [2323]


def test_terminator_agreement_passes():
    t = trace([line(0, 0, 1, terminator="PARAGRAPH_MARK", expected=1, glyphs=1)])
    assert selfcheck.check_terminator_glyphs(t)["state"] == OK


def test_wrap_lines_are_not_conflicts():
    """`WRAP` 表示源侧**没有**终止符字符，整行都是正文。

    拿它来判会把「1 个字符的正文行」误报成矛盾——第一版就是这么在 `complex` 上
    一口气误报 125 行的。这条回归钉住那个修正。
    """
    t = trace([line(0, 5, 6, terminator="WRAP", expected=0, glyphs=1)])
    result = selfcheck.check_terminator_glyphs(t)
    assert result["state"] == UNDECIDABLE, "单字符正文行不该被当成终止符矛盾"


def test_multichar_lines_are_skipped():
    """行里还有正文时分不出哪个字形属于终止符——跳过，不猜。"""
    t = trace([line(0, 0, 10, terminator="PARAGRAPH_MARK", expected=1, glyphs=10)])
    assert selfcheck.check_terminator_glyphs(t)["state"] == UNDECIDABLE


# ---- 行粒度 ----


def test_contiguous_records_flagged_as_run_granularity():
    """同页内源区间首尾相接的连排记录 → 记账粒度是 run 不是行。"""
    t = trace([line(0, 0, 10, glyphs=10), line(1, 10, 11, glyphs=1), line(2, 11, 16, glyphs=5)])
    result = selfcheck.check_line_granularity(t)
    assert result["state"] == FAIL
    assert result["contiguousRecords"] == 2


def test_line_granularity_undecidable_when_offsets_are_broken():
    """源区间本身就不对时，接续关系无从判断——先修那条，不要连带乱报。"""
    t = trace([line(0, 0, 10, glyphs=10), line(1, 0, 8, glyphs=8)])
    assert selfcheck.check_line_granularity(t)["state"] == UNDECIDABLE


# ---- 字形层覆盖 ----


def test_no_glyphs_is_undecidable_not_pass():
    """没接整形器 ≠ 这些行没有字形。「未核」与「核过无发现」分两栏（§7.4）。"""
    t = trace([line(0, 0, 10, glyphs=0), line(1, 10, 20, glyphs=0)])
    result = selfcheck.check_glyph_coverage(t)
    assert result["state"] == UNDECIDABLE
    assert "没覆盖" in result["reason"]


# ---- 顶层 ----


def test_top_level_takes_the_worst_and_keeps_undecidable_distinct():
    # 有 FAIL 就是 FAIL。
    bad = trace([line(0, 0, 10, glyphs=10), line(1, 0, 8, glyphs=8)])
    assert selfcheck.run(bad)["state"] == FAIL
    # 只有判不了就是判不了，不能折叠成 FAIL，也不能当成通过（§7.3）。
    unknown = trace([line(0, 0, 10, glyphs=0)])
    assert selfcheck.run(unknown)["state"] == UNDECIDABLE
    # 全对才是 OK。
    good = trace([
        line(0, 0, 10, terminator="WRAP", glyphs=10),
        line(1, 20, 21, terminator="PARAGRAPH_MARK", expected=1, glyphs=1),
    ])
    assert selfcheck.run(good)["state"] == OK
