"""Backtests against archived Mac Word control lines; no new capture claims."""

from pathlib import Path

import pytest

from wordmeasure import OK, capture, wordmodel


@pytest.mark.parametrize("spelling", ["\r", "\n"])
def test_source_mark_identifies_a_break_only_paragraph(spelling):
    sweep = {"contentText": "\x0c" + spelling,
             "paragraphs": [{"start": 0, "end": 2}],
             "marks": {"0": "PAGE_BREAK", "1": "PARAGRAPH_MARK"}}
    line = {"start": 0, "end": 2, "text": sweep["contentText"]}
    assert wordmodel.line_marks(sweep, line) == {
        0: "PAGE_BREAK_BEFORE_MARK", 1: "PARAGRAPH_MARK"}


def test_break_only_line_with_later_text_still_draws_zero():
    sweep = {"contentText": "\x0cZ\n",
             "paragraphs": [{"start": 0, "end": 3}],
             "marks": {"0": "PAGE_BREAK", "2": "PARAGRAPH_MARK"}}
    line = {"start": 0, "end": 1, "text": "\x0c"}
    assert wordmodel.line_marks(sweep, line) == {0: "PAGE_BREAK_OWN_LINE"}


@pytest.mark.parametrize("name,line_count,glyph_count", [
    ("breaks-sections-2026-09-17", 33, 90),
])
def test_archived_control_capture_counts_are_complete(name, line_count, glyph_count):
    root = Path(__file__).resolve().parents[3]
    bundle = capture.load_bundle(root / "captures" / name)
    model = wordmodel.build(bundle)
    assert model["state"] == OK
    lines = [line for page in model["pages"] for line in page["lines"]]
    assert len(lines) == line_count
    assert all(line["state"] == OK for line in lines)
    assert sum(len(line["glyphs"]) for line in lines) == glyph_count
    assert sum(line["identityMismatched"] for line in lines) == 0
