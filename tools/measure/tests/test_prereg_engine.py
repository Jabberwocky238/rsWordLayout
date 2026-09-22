import pytest

from prereg_engine import _lines
from wordmeasure.source_text import InvalidSourceOffset


def trace(glyph, start=0, end=4):
    return {"pages": [{"index": 0, "lines": [{
        "sourceStart": start, "sourceEnd": end, "glyphs": [glyph],
    }]}]}


def test_precise_size_takes_precedence_and_offsets_are_utf16():
    glyph = {"origin": [72, 79.92], "sizeHalfPoints": 16, "sizeCentipoints": 792}
    line = _lines(trace(glyph, 2, 4), "\U0001f642X\r")[0]
    assert line["text"] == "X"
    assert line["sizePt"] == line["glyphs"][0]["sizePt"] == 7.92


def test_old_trace_half_point_size_is_still_readable():
    line = _lines(trace({"origin": [0, 0], "sizeHalfPoints": 24}), "abc\r")[0]
    assert line["sizePt"] == 12.0


def test_source_range_cannot_split_a_surrogate_pair():
    with pytest.raises(InvalidSourceOffset, match="INVALID_UTF16_BOUNDARY"):
        _lines(trace({"origin": [0, 0], "sizeHalfPoints": 24}, 1, 4), "\U0001f642X\r")
