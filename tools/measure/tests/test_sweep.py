"""Offline sweeps must reject ambiguous source identity and missing fonts."""

import hashlib
import json
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wordmeasure import FAIL, OK, UNDECIDABLE
from wordmeasure.sweep import bind_inputs, font_names, main, run_bundle, summarize


def meta(sha="abc"):
    return {"fixture": {"before": {"sha256": sha}, "after": {"sha256": sha}, "unchanged": True},
            "preflight": {"requiredFamilies": {"Test Family": True}},
            "fontSubstitution": {"result": "PASS"}, "environment": {"platform": "mac"}}


def test_fixture_binding_uses_hash_instead_of_stale_capture_path(tmp_path):
    record = meta()
    record["fixture"]["before"]["path"] = "/old-machine/fixtures/wrong.docx"
    right = tmp_path / "same-bytes.docx"
    fonts = [{"path": "font.ttf", "names": ["Test Family"], "sha256": "font-sha"}]
    result = bind_inputs(record, {"abc": [right], "def": [tmp_path / "wrong.docx"]}, fonts)
    assert result["state"] == OK
    assert result["fixture"] == str(right)
    record["fixture"]["after"]["sha256"] = "def"
    assert bind_inputs(record, {"abc": [right]}, fonts)["reason"] == "FIXTURE_IDENTITY_UNVERIFIED"


def test_missing_font_is_undecidable_and_substrings_do_not_match(tmp_path):
    result = bind_inputs(meta(), {"abc": [tmp_path / "case.docx"]}, [
        {"path": "wrong.ttf", "names": ["Test Family Clone"], "sha256": "not-the-font"}
    ])
    assert result["state"] == UNDECIDABLE
    assert result["missingFamilies"] == ["Test Family"]


def name_table(text):
    encoded = text.encode("utf-16-be")
    return struct.pack(">HHH", 0, 1, 18) + struct.pack(">HHHHHH", 3, 1, 0x409, 4, len(encoded), 0) + encoded


def test_ttc_names_include_nonfirst_face_and_full_name(tmp_path):
    first, second = name_table("Other Face"), name_table("Test Family Book")
    headers_end = 20 + 2 * 28
    sfnt = struct.pack(">IHHHH", 0x10000, 1, 16, 0, 0)
    data = b"ttcf" + struct.pack(">IIII", 0x10000, 2, 20, 48)
    data += sfnt + struct.pack(">4sIII", b"name", 0, headers_end, len(first))
    data += sfnt + struct.pack(">4sIII", b"name", 0, headers_end + len(first), len(second))
    path = tmp_path / "collection.ttc"
    path.write_bytes(data + first + second)
    assert font_names(path) == {"Other Face", "Test Family Book"}


def test_failed_and_unrun_comparisons_have_separate_denominators():
    result = summarize([
        {"state": FAIL, "selfcheckState": OK, "comparisonState": FAIL,
         "structurallySound": True, "structure": {"referencePages": 1, "candidatePages": 1}},
        {"state": UNDECIDABLE, "reason": "REQUIRED_FONT_MISSING"},
    ])
    assert result["states"] == {OK: 0, FAIL: 1, UNDECIDABLE: 1}
    assert result["compared"] == 1
    assert result["notCompared"] == 1
    assert result["comparisonStates"] == {FAIL: 1, "NOT_RUN": 1}


def test_void_capture_never_invokes_engine(tmp_path):
    bundle = tmp_path / "capture"
    bundle.mkdir()
    (bundle / "META.json").write_text(json.dumps(meta()))
    (bundle / "VERDICT.json").write_text('{"verdict": "VOID"}')
    result = run_bundle(bundle, tmp_path / "out", binary=tmp_path / "no-engine",
                        fixtures={}, fonts=[], timeout=1)
    assert result["reason"] == "CAPTURE_VOID"
    assert result["state"] == UNDECIDABLE
    assert "traceRun" not in result


def test_trace_failure_cannot_be_reported_as_comparison(tmp_path, monkeypatch):
    bundle = tmp_path / "capture"
    bundle.mkdir()
    fixture, font = tmp_path / "case.docx", tmp_path / "font.ttf"
    fixture.write_bytes(b"fixture")
    font.write_bytes(b"font")
    sha = hashlib.sha256(fixture.read_bytes()).hexdigest()
    (bundle / "META.json").write_text(json.dumps(meta(sha)))
    for name in ("sweep.json", "glyphs.json"):
        (bundle / name).write_text("{}")
    monkeypatch.setattr("wordmeasure.sweep.invoke", lambda *a, **kw: {"exitCode": 1})
    result = run_bundle(bundle, tmp_path / "out", binary=tmp_path / "engine",
                        fixtures={sha: [fixture]}, fonts=[{"path": str(font), "names": ["Test Family"],
                        "sha256": hashlib.sha256(font.read_bytes()).hexdigest()}], timeout=1)
    assert result["state"] == UNDECIDABLE
    assert result["reason"] == "TRACE_FAILED"
    assert "comparisonState" not in result


def test_source_binding_failure_is_not_an_admitted_comparison(tmp_path, monkeypatch):
    bundle = tmp_path / "capture"
    bundle.mkdir()
    fixture, font = tmp_path / "case.docx", tmp_path / "font.ttf"
    fixture.write_bytes(b"fixture")
    font.write_bytes(b"font")
    sha = hashlib.sha256(fixture.read_bytes()).hexdigest()
    (bundle / "META.json").write_text(json.dumps(meta(sha)))
    for name in ("sweep.json", "glyphs.json"):
        (bundle / name).write_text("{}")

    def invoke(command, output, **kwargs):
        if "--metrics" in command:
            Path(command[-1]).write_text("{}")
            return {"exitCode": 0}
        if "compare" in command:
            assert command[command.index("--source-docx") + 1] == str(fixture)
            result = {"state": UNDECIDABLE, "reference": {
                "sourceAnnotation": {"state": UNDECIDABLE, "reason": "CONTENT_TEXT_MISMATCH"}}}
        else:
            result = {"state": OK}
        Path(command[-1]).write_text(json.dumps(result))
        return {"exitCode": 2 if "compare" in command else 0}

    monkeypatch.setattr("wordmeasure.sweep.invoke", invoke)
    result = run_bundle(bundle, tmp_path / "out", binary=tmp_path / "engine",
                        fixtures={sha: [fixture]}, fonts=[{"path": str(font), "names": ["Test Family"],
                        "sha256": hashlib.sha256(font.read_bytes()).hexdigest()}], timeout=1)
    assert result["state"] == UNDECIDABLE
    assert result["reason"] == "SOURCE_ANNOTATION_UNVERIFIED"
    assert result["sourceAnnotation"]["reason"] == "CONTENT_TEXT_MISMATCH"
    assert summarize([result])["compared"] == 0


def test_sweep_keeps_one_engine_revision_when_cargo_replaces_the_binary(tmp_path, monkeypatch):
    binary = tmp_path / "engine"
    binary.write_bytes(b"original engine")
    out = tmp_path / "results"
    observed = []

    def replay(bundle, destination, **kwargs):
        destination.mkdir()
        observed.append(kwargs["binary"].read_bytes())
        binary.write_bytes(b"rebuilt engine")
        return {"bundle": str(bundle), "state": UNDECIDABLE}

    monkeypatch.setattr("wordmeasure.sweep.font_inventory", lambda _: ([], []))
    monkeypatch.setattr("wordmeasure.sweep.fixture_index", lambda _: {})
    monkeypatch.setattr("wordmeasure.sweep.run_bundle", replay)
    assert main(["--trace-bin", str(binary), "--font-dir", str(tmp_path),
                 "--output", str(out), "--bundle", str(tmp_path / "one"),
                 "--bundle", str(tmp_path / "two")]) == 2
    assert observed == [b"original engine", b"original engine"]
    provenance = json.loads((out / "provenance.json").read_text())
    assert provenance["traceBinarySha256"] == hashlib.sha256(b"original engine").hexdigest()
