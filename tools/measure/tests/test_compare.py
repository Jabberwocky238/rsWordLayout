"""比较器与配对器的检验。

重点是两个**会静默毁掉结果**的坑，它们都必须由代码挡住，不是由文档挡住：

- §6.5：两份不同文档之间，行级差值可能是 0。报告必须先读 `state` 再读 `maxAbs`。
- §7.3：三态出口。「判不了」不能被折叠进 FAIL，也不能被当成通过。
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wordmeasure import FAIL, OK, UNDECIDABLE, compare, pairing


def glyph(x, y, text=None):
    return {"origin": [x, y], "advance": [6.0, 0.0], "text": text}


def doc(pages):
    return {"pages": [{"index": i, "lines": lines} for i, lines in enumerate(pages)]}


def line(glyphs, **kw):
    return {"index": 0, "glyphs": glyphs, **kw}


def test_identical_documents_compare_zero():
    a = doc([[line([glyph(72.0, 83.04, "B"), glyph(80.0, 83.04, "0")])]])
    result = compare.compare(a, a)
    assert result.state == OK
    assert result.max_abs == 0.0


def test_geometry_difference_fails_at_zero_tolerance():
    """噪声底是 0.0000pt（§5），所以默认容差是 0。

    引擎对 Word 出现任何非零差，要么是引擎错，要么是配对错，不可能是量得不准。
    """
    a = doc([[line([glyph(72.0, 83.04)])]])
    b = doc([[line([glyph(72.5, 83.04)])]])
    result = compare.compare(a, b)
    assert result.state == FAIL
    assert result.max_abs == 0.5
    assert any(f["code"] == "GEOMETRY_EXCEEDS_TOLERANCE" for f in result.failures)
    # 结构全程对上、只是几何超容差：这时 maxAbs 是整体读数，摘要里照给。
    assert result.structurally_sound is True
    assert "maxAbs=0.5000pt" in result.summary()


def test_structural_failure_is_not_readable_as_agreement():
    """§6.5 的难例，逐字复现。

    两份文档计数层完全相同、只是行长排列不同：3 行结构失败，
    **剩下 1 行 8 个字形距离 0.0000pt**。只读 maxAbs 会把配对失败读成完全一致。
    """
    same = [glyph(72.0 + 8 * i, 83.04) for i in range(8)]
    a = doc([[
        {"index": 0, "glyphs": same},
        {"index": 1, "glyphs": [glyph(72.0, 97.0)] * 5},
        {"index": 2, "glyphs": [glyph(72.0, 111.0)] * 5},
        {"index": 3, "glyphs": [glyph(72.0, 125.0)] * 5},
    ]])
    b = doc([[
        {"index": 0, "glyphs": same},
        {"index": 1, "glyphs": [glyph(72.0, 97.0)] * 3},
        {"index": 2, "glyphs": [glyph(72.0, 111.0)] * 7},
        {"index": 3, "glyphs": [glyph(72.0, 125.0)] * 4},
    ]])
    result = compare.compare(a, b)

    # 那 8 个字形确实一对一地 0.0000pt——但整份不是「完全一致」。
    assert result.max_abs == 0.0
    assert result.state == FAIL
    assert sum(1 for f in result.failures if f["code"] == "GLYPH_COUNT_MISMATCH") == 3

    # 输出契约：state 必须排在 maxAbs 前面，摘要里也先说 state（§6.5）。
    keys = list(result.to_dict())
    assert keys[0] == "state"
    assert keys.index("state") < keys.index("maxAbs")
    assert result.summary().startswith("state=FAIL")
    assert result.structurally_sound is False
    assert "maxAbs 不可作整体读数" in result.summary()


def test_page_count_mismatch_stops_before_geometry():
    """任何一层数目对不上即结构失败，**不修配对再比**（§9.6）。"""
    a = doc([[line([glyph(72.0, 83.04)])]])
    b = doc([[line([glyph(72.0, 83.04)])], [line([glyph(72.0, 83.04)])]])
    result = compare.compare(a, b)
    assert result.state == FAIL
    assert result.failures[0]["code"] == "PAGE_COUNT_MISMATCH"
    assert result.max_abs is None  # 没比几何，就不该有几何读数。


def test_undecidable_is_its_own_exit():
    """判不了不是 FAIL，也不是通过（§7.3）。"""
    a = {"pages": [{"index": 0, "state": UNDECIDABLE, "reason": "行号扫描与 PDF 页数不符", "lines": []}]}
    b = doc([[line([glyph(72.0, 83.04)])]])
    result = compare.compare(a, b)
    assert result.state == UNDECIDABLE
    assert any(f["code"] == "PAGE_UNDECIDABLE" for f in result.failures)


def test_no_pairs_is_undecidable_not_pass():
    """一对都没配上不能当成通过——「未发现反例」不等于「通过」（§7.3）。"""
    a = doc([[]])
    b = doc([[]])
    result = compare.compare(a, b)
    assert result.state == UNDECIDABLE
    assert any(f["code"] == "NO_PAIRS" for f in result.failures)


def test_nonzero_tolerance_is_flagged():
    """给非零容差要在输出里标出来，不能悄悄放水。"""
    a = doc([[line([glyph(72.0, 83.04)])]])
    b = doc([[line([glyph(72.2, 83.04)])]])
    result = compare.compare(a, b, tolerance_pt=0.5)
    assert result.state == OK
    assert "**非零容差**" in result.to_dict()["toleranceNote"]


# ---- 配对器 ----


def test_precheck_all_chars():
    assert pairing.precheck(5, "AB CD")[0] == pairing.MODE_ALL


def test_precheck_non_whitespace():
    assert pairing.precheck(4, "AB CD")[0] == pairing.MODE_NON_WHITESPACE


def test_precheck_neither_is_undecidable():
    """两者皆非 → 该行判不了，记下 G 与 C，**不猜**（§3.2）。"""
    mode, reasons = pairing.precheck(3, "AB CD")
    assert mode == UNDECIDABLE
    assert "G=3" in reasons[0] and "C=5" in reasons[0]


def test_pair_line_records_identity_as_crosscheck_only():
    """身份不参与配对，只作配对之后的核对手段（§3.1）。"""
    glyphs = [
        {"glyphOrigin": [72.0, 83.0], "text": "A", "textStatus": "mapped"},
        {"glyphOrigin": [80.0, 83.0], "text": "B", "textStatus": "mapped"},
    ]
    result = pairing.pair_line(0, 0, "AB", 0, glyphs)
    assert result.state == OK
    assert result.identity_checked == 2
    assert result.identity_mismatched == 0


def test_pair_line_unmapped_glyphs_still_pair():
    """PDF 字形可能没有 ToUnicode 映射（合成字体、CJK 子集）——照样按读序配对。"""
    glyphs = [
        {"glyphOrigin": [72.0, 83.0], "text": None, "textStatus": "unmapped"},
        {"glyphOrigin": [80.0, 83.0], "text": None, "textStatus": "unmapped"},
    ]
    result = pairing.pair_line(0, 0, "中文", 0, glyphs)
    assert result.state == OK
    assert result.identity_checked == 0  # 没身份可核，但这不妨碍配对。


def test_assign_by_box_reports_outside():
    """§3.3：落在所有行盒之外的字形是信号，不能静默丢掉。"""
    boxes = [{"top": 80.0, "height": 14.0}]
    glyphs = [{"glyphOrigin": [72.0, 83.0]}, {"glyphOrigin": [72.0, 300.0]}]
    grouped, outside = pairing.assign_by_box(glyphs, boxes)
    assert len(grouped[0]) == 1
    assert len(outside) == 1
    assert outside[0]["assignment"] == "OUTSIDE_BOX"


def test_derived_assignment_refuses_on_page_count_mismatch():
    """Mac 推算归行的唯一闸门：整页预期数与实测数不等即不切分（不猜）。"""
    lines = [{"text": "AB\r"}, {"text": "CD\r"}]
    glyphs = [{"glyphOrigin": [0, 0]}] * 5  # 预期 6 个，实测 5 个
    grouped, info = pairing.assign_by_line_numbers(glyphs, lines)
    assert grouped is None
    assert info["reason"] == "PAGE_COUNT_MISMATCH"


def test_derived_assignment_splits_when_totals_agree():
    lines = [{"text": "AB\r"}, {"text": "CD\r"}]
    glyphs = [{"glyphOrigin": [i, 0]} for i in range(6)]
    grouped, info = pairing.assign_by_line_numbers(glyphs, lines)
    assert info["state"] == OK
    assert len(grouped[0]) == 3 and len(grouped[1]) == 3
    # 推算不是测量：前提必须随结果一起传下去。
    assert "推算而非测量" in info["premise"]
