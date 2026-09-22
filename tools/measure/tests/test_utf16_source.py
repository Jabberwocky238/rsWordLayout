"""Offset bookkeeping tests, not a Word glyph-count rule for complex scripts."""

import zipfile

import pytest

from wordmeasure import UNDECIDABLE, docxtext, wordmodel
from wordmeasure.source_text import InvalidSourceOffset, SourceText


def sweep(text, line_numbers, marks=None, paragraphs=None):
    return {
        "contentText": text,
        "endOfContent": len(line_numbers),
        "positions": [{"offset": i, "page": 1, "line": n}
                      for i, n in enumerate(line_numbers)],
        "marks": marks,
        "paragraphs": paragraphs,
    }


def test_docx_controls_and_paragraph_ranges_use_utf16(tmp_path):
    path = tmp_path / "source.docx"
    xml = ('<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
           '<w:body><w:p><w:r><w:t>A\U0001f642</w:t><w:br w:type="page"/></w:r></w:p>'
           '<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>\U00010400Z</w:t></w:r></w:p>'
           '</w:body></w:document>')
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("word/document.xml", xml)
    derived = docxtext.content_text(path)
    assert derived["endOfContent"] == 9
    assert [(p["start"], p["end"]) for p in derived["paragraphs"]] == [(0, 5), (5, 9)]
    assert derived["marks"] == {3: "PAGE_BREAK", 4: "PARAGRAPH_MARK", 8: "SECTION_BREAK"}


def test_sweep_slices_text_at_utf16_line_boundaries():
    data = sweep("A\U0001f642B\rZ\r", [1, 1, 1, 2, 2, 3, 3])
    lines = wordmodel.lines_from_sweep(data)
    assert [(l["start"], l["end"], l["text"]) for l in lines] == [
        (0, 3, "A\U0001f642"), (3, 5, "B\r"), (5, 7, "Z\r")]


def test_word_marks_are_converted_to_scalar_indices_for_counting():
    data = sweep("\U00010400\x0c\r", [1, 1, 1, 1],
                 {"2": "PAGE_BREAK", "3": "PARAGRAPH_MARK"},
                 [{"start": 0, "end": 4}])
    line = wordmodel.lines_from_sweep(data)[0]
    assert wordmodel.line_marks(data, line) == {
        1: "PAGE_BREAK_BEFORE_MARK", 2: "PARAGRAPH_MARK"}


@pytest.mark.parametrize("problem", ["split", "gap", "duplicate", "length", "mark"])
def test_invalid_offsets_make_the_model_undecidable(problem):
    data = sweep("\U00010400\r", [1, 1, 1], {"2": "PARAGRAPH_MARK"})
    if problem == "split":
        data["positions"][1]["line"] = 2
    elif problem == "gap":
        data["positions"].pop(1)
    elif problem == "duplicate":
        data["positions"][1]["offset"] = 0
    elif problem == "length":
        data["endOfContent"] = 2
    elif problem == "mark":
        data["marks"] = {"1": "PARAGRAPH_MARK"}
    model = wordmodel.build({"sweep": data, "glyphs": {"pages": [{}]}})
    assert model["state"] == UNDECIDABLE
    assert model["reason"].startswith("INVALID_SOURCE_OFFSETS:")
    assert model["pages"][0]["lines"] == []


def test_unpaired_surrogate_is_not_a_character_boundary():
    with pytest.raises(InvalidSourceOffset, match="UNPAIRED_SURROGATE"):
        SourceText("\ud800")


@pytest.mark.parametrize("text", ["AX\n", "AXZ\n"])
def test_page_break_annotation_cannot_turn_visible_text_into_a_control(text):
    data = sweep(text, [1] * len(text),
                 {"1": "PAGE_BREAK", str(len(text) - 1): "PARAGRAPH_MARK"},
                 [{"start": 0, "end": len(text)}])
    model = wordmodel.build({"sweep": data, "glyphs": {"pages": [{}]}})
    assert model["state"] == UNDECIDABLE
    assert "SOURCE_MARK_MISMATCH" in model["reason"]
