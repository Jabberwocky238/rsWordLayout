"""Page evidence must not turn missing provenance or a width into print truth."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys

import pytest

MEASURE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MEASURE))

from wordmeasure.android_pages import compare_page_count, parse_page_log, trace_page_count

SHA = "a" * 64


def enter(n=0, width="28e2"):
    return f"ENTER n={n} tid=123 w2=0 w3={width} sp=abcdef\n"


def page(n, count=1, index=0):
    return f"PGIDX n={n} count={count:x} index={index:x}\n"


def audit(text=None):
    return parse_page_log((text or enter() + page(0) + page(1)).encode())


def binding(log):
    return {"schema": "android-page-binding/1", "docSha256": SHA,
            "logSha256": log["logSha256"], "mode": "print",
            "modeSource": "archived fixture name and capture recipe"}


def test_all_counts_runs_and_width_context_are_preserved_without_channel_inference():
    text = (enter(width="14d1") + page(0) + enter(1) + page(1) + page(2, 2)
            + page(3, 2, 1) + enter(2, "2342"))
    log = audit(text)
    assert [p["count"] for p in log["pgidx"]] == [1, 1, 2, 2]
    assert [(r["count"], r["readings"]) for r in log["countRuns"]] == [(1, 2), (2, 2)]
    assert log["finalCount"] == 2
    assert log["widths"] == [{"widthTwips": 5329, "readings": 1},
                             {"widthTwips": 9026, "readings": 1},
                             {"widthTwips": 10466, "readings": 1}]
    assert log["lastPgidxContext"]["precedingEnter"]["widthTwips"] == 10466
    assert log["lastPgidxContext"]["followingWidths"] == [{"widthTwips": 9026, "readings": 1}]
    assert log["identity"]["pgidxChannel"] == "unrecorded"
    assert log["logSha256"] == hashlib.sha256(text.encode()).hexdigest()


def test_strict_remains_undecidable_even_with_matching_hash_sidecar_and_paper_width():
    log = audit()
    result = compare_page_count(log, 1, SHA, binding(log))
    assert result["state"] == "UNDECIDABLE"
    assert result["conditional"] is False
    assert "PGIDX_DOCUMENT_THREAD_CHANNEL_AND_COMPLETION_UNVERIFIED" in result["problems"]


def test_legacy_comparison_states_all_three_provenance_assumptions():
    result = compare_page_count(audit(), 1, SHA, assume_legacy_print=True)
    assert result["state"] == "OK"
    assert result["conditional"] is True
    assert len(result["assumptions"]) == 3
    assert result["modeSource"] == "explicit --assume-legacy-print"
    assert result["fixtureSha256"] == SHA


def test_last_count_is_used_instead_of_first_or_max_count():
    log = audit(enter() + page(0, 3) + page(1, 3) + page(2, 2) + page(3, 2))
    assert compare_page_count(log, 2, SHA, assume_legacy_print=True)["state"] == "OK"
    assert compare_page_count(log, 3, SHA, assume_legacy_print=True)["state"] == "FAIL"


@pytest.mark.parametrize(("key", "value", "reason"), [
    ("docSha256", "b" * 64, "FIXTURE_HASH_MISMATCH"),
    ("logSha256", "b" * 64, "LOG_HASH_MISMATCH"),
    ("mode", "mobile", "CAPTURE_MODE_INCOMPATIBLE"),
    ("modeSource", "", "CAPTURE_MODE_SOURCE_MISSING"),
    ("schema", "wrong", "INVALID_CAPTURE_BINDING"),
])
def test_assumptions_cannot_override_binding_conflicts(key, value, reason):
    log = audit()
    meta = {**binding(log), key: value}
    result = compare_page_count(log, 1, SHA, meta, assume_legacy_print=True)
    assert result["state"] == "UNDECIDABLE"
    assert reason in result["problems"]


@pytest.mark.parametrize("suffix", [
    enter(1, "14d1"),
    enter(1, "14d1") + enter(2),
])
def test_mobile_width_after_last_page_reading_cannot_be_overridden(suffix):
    result = compare_page_count(audit(enter() + page(0) + suffix), 1, SHA,
                                assume_legacy_print=True)
    assert result["state"] == "UNDECIDABLE"
    assert "TERMINAL_WIDTH_CONFLICTS_WITH_LEGACY_PRINT_RECIPE" in result["problems"]


def test_mobile_width_before_last_page_reading_is_a_conflict_too():
    log = audit(enter() + page(0) + enter(1, "14d1") + page(1))
    assert compare_page_count(log, 1, SHA, assume_legacy_print=True)["state"] == "UNDECIDABLE"


@pytest.mark.parametrize(("raw", "reason"), [
    (b"nothing\n", "NO_PGIDX_READINGS"),
    (b"PGIDX n=0 count=z index=0\n", "MALFORMED_PGIDX"),
    (b"PGIDX n=0 count=1 index=0 garbage\n", "MALFORMED_PGIDX"),
    (b"PGIDX n=0 count=1 index=0", "TRUNCATED_OR_UNTERMINATED_LOG"),
    (b"PGIDX n=0 count=1 index=0\nPGI", "TRUNCATED_OR_UNTERMINATED_LOG"),
    (b"PGIDX n=0 count=1 index=1\n", "INVALID_PAGE_INDEX"),
    (b"PGIDX n=0 count=0 index=0\n", "NO_POSITIVE_FINAL_PAGE_COUNT"),
    (b"PGIDX n=0 count=1 index=0\n\xff\n", "INVALID_LOG_ENCODING"),
    (b"ENTER n=0 tid=1 w2=0\nPGIDX n=0 count=1 index=0\n", "MALFORMED_ENTER"),
    ((page(0) + page(0)).encode(), "PGIDX_SEQUENCE_DISCONTINUITY"),
    ((page(0) + page(2)).encode(), "PGIDX_SEQUENCE_DISCONTINUITY"),
    (page(1).encode(), "PGIDX_PREFIX_MISSING"),
])
def test_incomplete_or_invalid_logs_are_not_conditional_passes(raw, reason):
    result = compare_page_count(parse_page_log(raw), 1, SHA, assume_legacy_print=True)
    assert result["state"] == "UNDECIDABLE"
    assert reason in result["problems"]


def test_probe_sample_limit_prevents_claiming_settled_count_even_with_complete_lines():
    log = audit(enter() + "".join(page(i) for i in range(80)) + enter(1))
    assert log["sampling"] == {"pgidxLimit": 80, "limitReached": True,
                              "source": "word_analyse/tools/layout-probe/lineprobe.c: PGIDX n < 80"}
    assert log["finalCount"] == 1
    result = compare_page_count(log, 1, SHA, assume_legacy_print=True)
    assert result["state"] == "UNDECIDABLE"
    assert "PGIDX_SAMPLE_LIMIT_REACHED" in result["problems"]


@pytest.mark.parametrize("count", [None, 0, -1, True, 1.0, float("nan"), "1"])
def test_engine_count_must_be_positive_integer(count):
    result = compare_page_count(audit(), count, SHA, assume_legacy_print=True)
    assert result["state"] == "UNDECIDABLE"


def test_trace_count_uses_pages_and_checks_optional_count_and_mode():
    trace = {"schema": "rsword-layout-trace/1", "pages": [{}, {}]}
    assert trace_page_count(trace)[1] == ["ENGINE_MODE_UNVERIFIED"]
    count, problems, assumptions = trace_page_count(trace, assume_legacy_print=True)
    assert (count, problems, len(assumptions)) == (2, [], 1)
    trace.update(pageCount=2, mode="print")
    assert trace_page_count(trace) == (2, [], [])
    trace["mode"] = "mobile-consumption"
    assert trace_page_count(trace, assume_legacy_print=True)[1] == ["ENGINE_MODE_INCOMPATIBLE"]


@pytest.mark.parametrize("declared_mode", [None, "print"])
@pytest.mark.parametrize(("platform", "overflow"), [("android", "不挂出"), ("mac", "按 w:overflowPunct 挂出")])
def test_legacy_trace_mobile_metrics_suffix_cannot_be_overridden(declared_mode, platform, overflow):
    trace = {"schema": "rsword-layout-trace/1", "pages": [{}],
             "metrics": f"RealMetrics；平台 {platform}，视图 mobile；行末标点 {overflow}"}
    if declared_mode is not None:
        trace["mode"] = declared_mode
    count, problems, _ = trace_page_count(trace, assume_legacy_print=True)
    assert count is None
    assert "ENGINE_METRICS_MODE_INCOMPATIBLE" in problems


@pytest.mark.parametrize("declared", [None, 0, -1, True, float("nan"), "2", 1, 3])
def test_trace_page_count_field_cannot_disagree_or_have_wrong_type(declared):
    trace = {"schema": "rsword-layout-trace/1", "pages": [{}, {}], "mode": "print",
             "pageCount": declared}
    assert trace_page_count(trace)[1] == ["ENGINE_PAGE_COUNT_MISMATCH_OR_INVALID"]


@pytest.mark.parametrize("trace", [[], {}, {"schema": "rsword-layout-trace/1", "pages": []},
                                   {"schema": "rsword-layout-trace/1", "pages": [None]}])
def test_malformed_traces_are_not_page_evidence(trace):
    assert trace_page_count(trace, assume_legacy_print=True)[0] is None


def test_cli_records_user_supplied_count_without_claiming_engine_execution(tmp_path):
    log, fixture, output = [tmp_path / n for n in ("word.log", "test.docx", "audit.json")]
    log.write_text(enter() + page(0))
    fixture.write_bytes(b"fixture bytes")
    command = [sys.executable, str(MEASURE / "android_pages.py"), str(log),
               "--fixture", str(fixture), "--engine-page-count", "1",
               "--assume-legacy-print", "--output", str(output)]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stderr
    report = json.loads(output.read_text())
    assert report["comparison"]["state"] == "OK"
    assert report["candidate"]["source"] == "user-supplied count"
    assert report["engineExecuted"] is False
    assert report["comparison"]["fixtureSha256"] == hashlib.sha256(fixture.read_bytes()).hexdigest()
    assert subprocess.run(command, capture_output=True, check=False).returncode == 2


def test_cli_rejects_contradictory_trace_mode_even_with_legacy_flag(tmp_path):
    log, fixture, trace, output = [tmp_path / n for n in ("word.log", "f.docx", "trace.json", "out.json")]
    log.write_text(enter() + page(0))
    fixture.write_bytes(b"fixture bytes")
    trace.write_text(json.dumps({"schema": "rsword-layout-trace/1", "pages": [{}], "mode": "mobile"}))
    result = subprocess.run([sys.executable, str(MEASURE / "android_pages.py"), str(log),
                             "--fixture", str(fixture), "--trace", str(trace),
                             "--assume-legacy-print", "--output", str(output)],
                            capture_output=True, text=True, check=False)
    assert result.returncode == 2, result.stderr
    assert json.loads(output.read_text())["comparison"]["reason"] == "ENGINE_MODE_INCOMPATIBLE"


def test_cli_trace_hash_mismatch_is_not_overridden(tmp_path):
    log, fixture, trace, output = [tmp_path / n for n in ("word.log", "f.docx", "trace.json", "out.json")]
    log.write_text(enter() + page(0))
    fixture.write_bytes(b"fixture bytes")
    trace.write_text(json.dumps({"schema": "rsword-layout-trace/1", "pages": [{}],
                                 "mode": "print", "docSha256": SHA}))
    result = subprocess.run([sys.executable, str(MEASURE / "android_pages.py"), str(log),
                             "--fixture", str(fixture), "--trace", str(trace),
                             "--assume-legacy-print", "--output", str(output)],
                            capture_output=True, text=True, check=False)
    assert result.returncode == 2, result.stderr
    assert json.loads(output.read_text())["comparison"]["reason"] == "ENGINE_FIXTURE_HASH_MISMATCH"


def test_invalid_nonfinite_binding_source_leaves_a_valid_undecidable_report(tmp_path):
    log, fixture, sidecar, output = [tmp_path / n for n in ("word.log", "f.docx", "meta.json", "out.json")]
    raw = (enter() + page(0)).encode()
    log.write_bytes(raw)
    fixture.write_bytes(b"fixture bytes")
    meta = binding(parse_page_log(raw))
    meta.update(docSha256=hashlib.sha256(fixture.read_bytes()).hexdigest(), modeSource=float("nan"))
    sidecar.write_text(json.dumps(meta))
    result = subprocess.run([sys.executable, str(MEASURE / "android_pages.py"), str(log),
                             "--fixture", str(fixture), "--binding", str(sidecar),
                             "--engine-page-count", "1", "--assume-legacy-print", "--output", str(output)],
                            capture_output=True, text=True, check=False)
    assert result.returncode == 2, result.stderr
    report = json.loads(output.read_text())
    assert report["comparison"]["reason"] == "CAPTURE_MODE_SOURCE_MISSING"
    assert "NaN" not in output.read_text()
