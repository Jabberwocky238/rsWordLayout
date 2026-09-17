"""§4 计数约定的检验。

每条断言都钉在方法给出的**实测**上，不是钉在我们希望的行为上。
范围外的构造必须落到「判不了」，不能被悄悄算成某个数——
那正是 §7.3 说的「取消判不了这个出口，这套东西只会输出成立」。
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wordmeasure import OK, UNDECIDABLE, counting


def test_paragraph_mark_draws_one_space():
    # §4：段落标记画 1 个空格。
    assert counting.expected_glyphs("AB\r").expected == 3


def test_soft_return_draws_one():
    # §4：软回车 1 个（03d–03h 30/30）。
    assert counting.expected_glyphs("AB\x0b").expected == 3


def test_trailing_spaces_are_drawn():
    # §4：行尾／终止符前的尾随空格照画，个数不影响计数。
    assert counting.expected_glyphs("AB   \r").expected == 6


def test_page_break_before_mark_adds_one_space():
    """§4 的 Mac 侧实测：手动分页符紧跟段落标记 → 1 个空格，该行连同段落标记共 +2。

    原文给的直接证据：`'B01 break before mark  '` 是 21 字符加两个尾随空格。
    这里就照那条记录复算。
    """
    text = "B01 break before mark\x0c\r"
    assert len(text) == 23
    result = counting.expected_glyphs(text, page_break_kind="BEFORE_MARK")
    assert result.state == OK
    # 21 个可见字符 + 分页符 1 个空格 + 段落标记 1 个空格 = 23。
    assert result.expected == 23


def test_page_break_own_line_draws_nothing():
    # §4：行首独占、自成一条行记录 → 0 个（Mac 侧 4/4）。
    assert counting.expected_glyphs("\x0c", page_break_kind="OWN_LINE").expected == 0


def test_page_break_mid_paragraph_draws_nothing():
    # §4：段中（两侧都有文字）→ 0 个（Mac 侧 3/3）。
    assert counting.expected_glyphs("AB\x0cCD", page_break_kind="MID_PARAGRAPH").expected == 4


def test_section_break_draws_nothing():
    assert counting.expected_glyphs("\x0c", page_break_kind="SECTION").expected == 0


def test_unknown_break_kind_is_undecidable():
    """`\\x0c` 在 `Range.Text` 里分节符与手动分页符同形，源侧分不出。

    三种位置各画 0/1/0 个字形——判不出位置就**不猜**。
    """
    result = counting.expected_glyphs("AB\x0c\r")
    assert result.state == UNDECIDABLE
    assert any("BREAK_KIND_UNKNOWN" in r for r in result.reasons)


def test_leader_tab_is_undecidable_by_default():
    """前导符填充个数公式**已被证否**且没有替代（§8）。

    源侧分不出普通制表位与前导符制表位，所以默认判不了。
    这条如果退化成「按 1 个空格算」，就是拿未终态的约定去撑全称。
    """
    result = counting.expected_glyphs("A\tB")
    assert result.state == UNDECIDABLE
    assert any("LEADER_TAB_UNRESOLVED" in r for r in result.reasons)


def test_plain_tab_counts_one_when_caller_confirms():
    # §4：制表符各 1 个空格（09a 15/15 行）——但要调用方先确认不是前导符制表位。
    result = counting.expected_glyphs("A\tB", treat_tab_as_single_space=True)
    assert result.state == OK
    assert result.expected == 3


def test_inline_object_is_out_of_scope():
    # §4：行内对象 PDF 里 0 个字形，但 n=1，仍在范围外（§8）。
    result = counting.expected_glyphs("A\x01B")
    assert result.state == UNDECIDABLE


def test_classify_break_positions():
    assert counting.classify_break("AB\x0c\r", 2) == "BEFORE_MARK"
    assert counting.classify_break("\x0c", 0) == "OWN_LINE"
    assert counting.classify_break("AB\x0cCD", 2) == "MID_PARAGRAPH"


def test_rules_table_keeps_out_of_scope_marked():
    """范围外的条目必须在表里标出来，不能只写个数字。

    §7.6：结论带范围，范围与结论写在同一处，让下游没法只抄结论不抄限定。
    """
    table = {r["key"]: r for r in counting.rules_table()}
    assert table["LEADER_TAB"]["inScope"] is False
    assert table["AUTO_NUMBER_LABEL"]["inScope"] is False
    assert "证否" in table["LEADER_TAB"]["note"]
