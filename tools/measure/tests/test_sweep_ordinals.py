"""Reject proven ordinal reversals without repairing archived line assignment."""

import copy
import hashlib
import json
from pathlib import Path

import pytest

from wordmeasure import OK, UNDECIDABLE, compare, wordmodel

ROOT = Path(__file__).resolve().parents[3]


def bundle(readings, paragraphs, *, text=None, marks=None, omitted_glyphs=()):
    text = text or "abcdefghijklmnopqrstuvwxyz"[:len(readings)]
    positions = [{"offset": i, "page": page, "line": line}
                 for i, (page, line) in enumerate(readings)]
    pages = []
    for page in sorted({p for p, _ in readings}):
        glyphs = [
            {"glyphOrigin": [float(i), 10.0], "advanceVector": [1.0, 0.0],
             "text": ch, "textStatus": "mapped", "fontName": "Test", "effectiveSizePt": 12,
             "rise": 0}
            for i, ch in enumerate(text)
            if readings[i][0] == page and i not in omitted_glyphs
        ]
        pages.append({"width": 100, "height": 100, "glyphs": glyphs})
    return {"path": "synthetic", "META": {},
            "sweep": {"contentText": text, "endOfContent": len(readings), "positions": positions,
                      "paragraphs": paragraphs, "marks": marks},
            "glyphs": {"pages": pages}}


def assert_ordinal_rejected(data, rejected_pages):
    original = copy.deepcopy(data)
    model = wordmodel.build(data)
    assert data == original
    assert model["state"] == UNDECIDABLE
    assert len(model["pages"]) == len(data["glyphs"]["pages"])
    for index, page in enumerate(model["pages"]):
        if index in rejected_pages:
            assert page["state"] == UNDECIDABLE
            assert page["reason"].startswith("SOURCE_LINE_ORDINAL_DECREASE:")
            assert page["lines"] == []
        else:
            assert page["state"] == OK
    return model


@pytest.mark.parametrize("before,after", [(3, 1), (7, 1), (2, 1)])
def test_same_paragraph_and_page_decrease_is_not_a_line_count_failure(before, after):
    data = bundle([(1, before), (1, before), (1, after), (1, after)], [{"start": 0, "end": 4}])
    model = assert_ordinal_rejected(data, {0})
    result = compare.compare(model, {"pages": [{"lines": []}]}).to_dict()
    assert result["state"] == UNDECIDABLE
    assert [f["code"] for f in result["failures"]] == ["PAGE_UNDECIDABLE"]


@pytest.mark.parametrize("readings", [
    [(1, 1), (1, 2), (2, 1), (2, 2)],  # Page-local restart.
    [(1, 1), (1, 2), (2, 3), (2, 4)],  # Global line ordinals.
    [(1, 3), (1, 3), (1, 7), (1, 7)],  # Gaps do not prove a bad reading.
    [(1, 3), (1, 3), (1, 3), (1, 3)],
])
def test_legacy_capture_without_stability_keeps_valid_page_and_line_sequences(readings):
    data = bundle(readings, [{"start": 0, "end": 4}])
    assert "sweepStability" not in data["META"]
    assert wordmodel.build(data)["state"] == OK


@pytest.mark.parametrize("paragraphs", [None, [], [{"start": 0, "end": 2}, {"start": 2, "end": 4}],
                                          [{"start": 0, "end": 1}, {"start": 3, "end": 4}]])
def test_unknown_or_different_paragraph_ownership_does_not_prove_a_reversal(paragraphs):
    data = bundle([(1, 3), (1, 3), (1, 1), (1, 1)], paragraphs)
    assert wordmodel.build(data)["state"] == OK


def test_section_terminator_and_next_recorded_paragraph_allow_same_page_restart():
    data = bundle([(1, 3), (1, 3), (1, 1), (1, 1)],
                  [{"start": 0, "end": 2}, {"start": 2, "end": 4}],
                  text="A\x0cBC", marks={"1": "SECTION_BREAK"}, omitted_glyphs=(1,))
    assert wordmodel.build(data)["state"] == OK


def test_previous_segment_may_cross_a_paragraph_boundary_before_the_actual_decrease():
    data = bundle([(1, 3), (1, 3), (1, 3), (1, 1)],
                  [{"start": 0, "end": 1}, {"start": 1, "end": 4}])
    assert_ordinal_rejected(data, {0})


@pytest.mark.parametrize("paragraphs", [
    [{"start": 0, "end": 4}, {"start": 1, "end": 3}],
    [{"start": False, "end": 4}],
    [{"start": 0, "end": 5}],
])
def test_ambiguous_or_malformed_recorded_paragraphs_do_not_establish_ownership(paragraphs):
    data = bundle([(1, 3), (1, 3), (1, 1), (1, 1)], paragraphs)
    assert wordmodel.build(data)["state"] == OK


@pytest.mark.parametrize("name,fixture_sha", [
    ("kinsoku", "37abbd1186bc9711679816ae0addec1e4c45d23c835d5c621b188f171583fead"),
    ("kinsoku2", "57e52d52a70650529b7166f6a5fd379fc5dd91a1ca5d362814f0fc8ef106f8ec"),
])
def test_archived_kinsoku_bad_pages_are_undecidable_without_changing_the_capture(name, fixture_sha):
    path = ROOT / "captures" / f"{name}-2026-09-18"
    files = [path / f"{kind}.json" for kind in ("META", "sweep", "glyphs")]
    before = {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    data = {p.stem: json.loads(p.read_text()) for p in files}
    data["path"] = str(path)
    assert data["META"]["fixture"]["before"]["sha256"] == fixture_sha
    model = assert_ordinal_rejected(data, {1, 2, 3})
    # Identical readable page 0 plus arbitrary line counts on rejected pages
    # must not turn an untrusted ordinal partition into a structural failure.
    candidate = {"pages": [copy.deepcopy(model["pages"][0]),
                           {"lines": [{}, {}]}, {"lines": [{}, {}]}, {"lines": [{}, {}]}]}
    result = compare.compare(model, candidate).to_dict()
    assert result["state"] == UNDECIDABLE
    assert [f["code"] for f in result["failures"]] == ["PAGE_UNDECIDABLE"] * 3
    assert before == {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in files}


@pytest.mark.parametrize("problem", ["split", "gap", "duplicate", "length"])
def test_existing_invalid_source_offset_errors_keep_their_original_reason(problem):
    data = bundle([(1, 3)] * 4, [{"start": 0, "end": 4}], text="abcd")
    if problem == "split":
        data["sweep"]["contentText"] = "A\U0001f642B"
        data["sweep"]["positions"][2]["line"] = 1
    elif problem == "gap":
        data["sweep"]["positions"].pop(1)
    elif problem == "duplicate":
        data["sweep"]["positions"][1]["offset"] = 0
    else:
        data["sweep"]["endOfContent"] = 5
    model = wordmodel.build(data)
    assert model["state"] == UNDECIDABLE
    assert model["reason"].startswith("INVALID_SOURCE_OFFSETS:")
