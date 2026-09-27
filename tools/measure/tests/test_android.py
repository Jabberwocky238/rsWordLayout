"""Android replay must catch false agreement and preserve evidence limits."""

import json
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wordmeasure.android import OFFSET_SPACE, compare_capture, load_capture

SHA = "a" * 64


def capture(ranges=((0, 4), (4, 8)), *, legacy=False):
    return {
        "meta": {"schema": 1, "producer": "word", "docSha256": SHA, "cpUnit": "utf16",
                 "cpSpace": "unknown" if legacy else "document",
                 "mode": "unknown" if legacy else "mobile-consumption"},
        "lines": [{"story": "main", "para": 0, "line": i, "cpFirst": start,
                   "cpLim": end, "runs": []} for i, (start, end) in enumerate(ranges)],
    }


def trace(ranges=((0, 4), (4, 8))):
    return {"schema": "rsword-layout-trace/1", "offsetSpace": OFFSET_SPACE,
            "pages": [{"lines": [{"sourceStart": start, "sourceEnd": end}
                                  for start, end in ranges]}]}


def test_boundary_match_does_not_claim_run_geometry_or_page_agreement():
    result = compare_capture(capture(), trace(), SHA)
    assert result["state"] == "OK"
    assert result["boundary"]["matchedLines"] == 2
    assert all(result[key]["state"] == "UNDECIDABLE"
               for key in ("runSplits", "geometry", "pagination"))


def test_unknown_capture_metadata_requires_explicit_conditional_replay():
    assert compare_capture(capture(legacy=True), trace(), SHA)["state"] == "UNDECIDABLE"
    result = compare_capture(capture(legacy=True), trace(), SHA, True)
    assert result["state"] == "OK"
    assert result["conditional"] is True
    assert len(result["assumptions"]) == 2


@pytest.mark.parametrize(("key", "value"), [("cpUnit", "word-cp"), ("cpSpace", "story"),
                                            ("mode", "print"), ("docSha256", "b" * 64)])
def test_legacy_assumption_cannot_override_contradictory_evidence(key, value):
    word = capture(legacy=True)
    word["meta"][key] = value
    assert compare_capture(word, trace(), SHA, True)["state"] == "UNDECIDABLE"


@pytest.mark.parametrize("ranges", [((0, 4),), ((0, 4), (4, 8), (8, 9)),
                                     ((0, 4), (0, 4), (4, 8)), ((4, 8), (0, 4)),
                                     ((1, 4), (4, 8)), ()])
def test_missing_extra_duplicate_reordered_and_start_mismatches_fail(ranges):
    result = compare_capture(capture(), trace(ranges), SHA)
    assert result["state"] == "FAIL"
    assert result["boundary"]["mismatches"]
    assert result["boundary"]["comparedLines"] == max(2, len(ranges))


@pytest.mark.parametrize("ranges", [(), ((0, None),), ((0, 0),), ((1, 4),),
                                     ((0, 4), (3, 8)), ((0, 4), (5, 8))])
def test_missing_or_partial_reference_is_not_a_pass_or_engine_failure(ranges):
    assert compare_capture(capture(ranges), trace(), SHA)["state"] == "UNDECIDABLE"


def test_duplicate_word_line_identities_are_rejected():
    word = capture()
    word["lines"][1]["line"] = 0
    assert compare_capture(word, trace(), SHA)["state"] == "UNDECIDABLE"


def test_non_main_story_and_wrong_engine_coordinate_contract_are_rejected():
    word = capture()
    word["lines"][0]["story"] = "footnote"
    assert compare_capture(word, trace(), SHA)["state"] == "UNDECIDABLE"
    engine = trace()
    engine["offsetSpace"] = "bytes"
    assert compare_capture(capture(), engine, SHA)["state"] == "UNDECIDABLE"


def test_jsonl_requires_one_leading_meta_record(tmp_path):
    path = tmp_path / "word.jsonl"
    word = capture()
    records = [{"record": "meta", **word["meta"]}] + [
        {"record": "line", **line} for line in word["lines"]]
    path.write_text("\n".join(map(json.dumps, records)))
    assert compare_capture(load_capture(path), trace(), SHA)["state"] == "OK"
    path.write_text("\n".join(map(json.dumps, records + [records[0]])))
    with pytest.raises(ValueError, match="INVALID_CAPTURE_RECORDS"):
        load_capture(path)
