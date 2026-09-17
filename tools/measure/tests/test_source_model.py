"""源侧推导与计数模型配对的检验。

这些断言是**实测逼出来的**，不是设想出来的。每条都注明是哪条读数逼出来的，
免得日后有人把它们当成可以随手放宽的内部约定。
"""

import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wordmeasure import OK, UNDECIDABLE, counting, docxtext, pairing

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def make_docx(tmp_path, paragraphs):
    """最小 docx。`paragraphs` 是 (文本, 是否带 sectPr, 是否带分页符) 三元组。"""
    body = []
    for text, sect, page_break in paragraphs:
        pPr = '<w:pPr>%s</w:pPr>' % ('<w:sectPr/>' if sect else '')
        runs = '<w:r><w:t xml:space="preserve">%s</w:t></w:r>' % text
        if page_break:
            runs += '<w:r><w:br w:type="page"/></w:r>'
        body.append("<w:p>%s%s</w:p>" % (pPr, runs))
    xml = (
        '<?xml version="1.0"?><w:document xmlns:w="%s"><w:body>%s</w:body></w:document>'
        % (W, "".join(body))
    )
    path = tmp_path / "case.docx"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("word/document.xml", xml)
    return path


def test_section_paragraph_terminates_with_section_break(tmp_path):
    """带 `w:sectPr` 的段落，终止符是**分节符**而不是段落标记。

    逼出这条的读数：一份 11 页夹具，整页 §4 预期 62 个字形、实测 60 个。
    差额正是两段带 `w:sectPr` 的终止符——按段落标记算各画 1 个空格，
    按分节符算各画 0 个。**两者都只占 1 个字符位**，所以总长与段边界核查发现不了。
    """
    docx = make_docx(tmp_path, [("B09 section nextPage", True, False)])
    derived = docxtext.content_text(docx)
    assert derived["paragraphs"][0]["terminator"] == "SECTION_BREAK"
    assert derived["text"].endswith("\x0c")
    assert derived["marks"][len(derived["text"]) - 1] == "SECTION_BREAK"

    line = derived["text"]
    marks = {i: m for i, m in derived["marks"].items()}
    # 20 个可见字符 + 分节符 0 个 = 20。按段落标记算会得 21。
    assert counting.expected_glyphs(line, marks=marks).expected == 20


def test_plain_paragraph_terminates_with_paragraph_mark(tmp_path):
    docx = make_docx(tmp_path, [("B13 super", False, False)])
    derived = docxtext.content_text(docx)
    assert derived["paragraphs"][0]["terminator"] == "PARAGRAPH_MARK"
    assert derived["text"].endswith("\r")
    marks = dict(derived["marks"])
    # 9 个可见字符 + 段落标记 1 个空格 = 10。
    assert counting.expected_glyphs(derived["text"], marks=marks).expected == 10


def test_verify_declares_what_it_cannot_check(tmp_path):
    """长度与段边界核查**分不出** `\\r` 与 `\\x0c`——这一条必须写在结果里。

    「未核」与「核过无发现」分两栏；长得像，意思相反（§7.4）。
    """
    docx = make_docx(tmp_path, [("A", False, False)])
    derived = docxtext.content_text(docx)
    record = docxtext.verify(derived, end_of_content=2, paragraphs=derived["paragraphs"])
    assert record["state"] == OK
    assert any("终止符身份" in item for item in record["doesNotCheck"])


def test_verify_refuses_on_length_mismatch(tmp_path):
    docx = make_docx(tmp_path, [("A", False, False)])
    derived = docxtext.content_text(docx)
    record = docxtext.verify(derived, end_of_content=99, paragraphs=None)
    assert record["state"] == UNDECIDABLE
    assert any("END_OF_CONTENT_MISMATCH" in r for r in record["reasons"])


def test_page_break_position_needs_paragraph_and_line_context():
    """§4 的三分要同时看段落与行。

    逼出这条的读数：行文本 `'B04 before\\x0c'`（分页符在行尾、段落在下一页继续）。
    只看行内上下文会判成 UNKNOWN——而它其实是「段中，两侧都有文字」，画 0 个字形。
    """
    para = "B04 before\x0cafter\r"
    assert counting.resolve_page_break(para, 10, alone_on_line=False) == "PAGE_BREAK_MID_PARAGRAPH"
    # 自成一条行记录 → 行首独占，0 个字形。
    assert counting.resolve_page_break("\x0cB07 leading break\r", 0, alone_on_line=True) == "PAGE_BREAK_OWN_LINE"
    # 紧跟段落标记 → 1 个空格。
    assert counting.resolve_page_break("B01 mark\x0c\r", 8, alone_on_line=False) == "PAGE_BREAK_BEFORE_MARK"


def test_count_model_mode_is_marked_as_backtest():
    """§3.2 两种序列都不对、但 §4 模型给出确定总数时，走计数模型配对。

    逼出这条的读数：`'B04 before\\x0c'` 这一行 G=10、C=11、C'=9——两者皆非。
    差额来自分页符画 0 个字形，而 §3.2 的两种模式都表达不了「某个源字符画 0 个」。

    **这条模式是看过数据之后才成形的，所以必须标回测，不当独立检验（§7.5）。**
    """
    text = "B04 before\x0c"
    marks = {10: "PAGE_BREAK_MID_PARAGRAPH"}
    glyphs = [{"glyphOrigin": [i * 6.0, 83.0], "text": c, "textStatus": "mapped"}
              for i, c in enumerate("B04 before")]

    assert pairing.precheck(len(glyphs), text)[0] == UNDECIDABLE

    result = pairing.pair_line(0, 0, text, 0, glyphs, marks=marks)
    assert result.state == OK
    assert result.mode == pairing.MODE_COUNT_MODEL
    assert result.backtest is True
    assert any("回测" in r for r in result.reasons)
    # 画 0 个字形的源字符不占字形序号：最后一个字形配的是 'e'，不是分页符。
    assert result.pairs[-1] == (9, 9)
    assert result.identity_mismatched == 0


def test_count_model_mode_still_refuses_when_model_undecidable():
    """模型自己判不了的时候，计数模型模式**不能**顶上去当救火队。"""
    text = "A\tB"
    glyphs = [{"glyphOrigin": [0, 0]}] * 3
    result = pairing.pair_line(0, 0, text, 0, glyphs, marks={1: "TAB"})
    assert result.state == UNDECIDABLE
    assert any("LEADER_TAB_UNRESOLVED" in r for r in result.reasons)


def test_glyph_rules_are_attached_for_scope_exclusion():
    """每个字形要能说出自己由哪条约定产生，验收范围才能按约定显式排除（§8）。"""
    text = "AB\r"
    glyphs = [{"glyphOrigin": [i * 6.0, 83.0], "text": c, "textStatus": "mapped"}
              for i, c in enumerate("AB ")]
    result = pairing.pair_line(0, 0, text, 0, glyphs, marks={2: "PARAGRAPH_MARK"})
    assert result.state == OK
    assert result.glyph_rules == ["LITERAL", "LITERAL", "PARAGRAPH_MARK"]
