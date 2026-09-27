import hashlib
import json
from pathlib import Path
import subprocess

import pytest

import android_replay as replay
from wordmeasure.android import OFFSET_SPACE


@pytest.fixture
def case(tmp_path):
    analysis = tmp_path / "analysis"
    (analysis / "fixtures").mkdir(parents=True)
    (analysis / "reports/diff").mkdir(parents=True)
    fixture = analysis / "fixtures/sample.docx"
    fixture.write_bytes(b"fixture bytes")
    capture = analysis / "reports/diff/sample.word.narrow.jsonl"
    capture.write_text("\n".join(json.dumps(row) for row in [
        {"record": "meta", "schema": 1, "producer": "word",
         "docSha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
         "mode": "unknown", "cpSpace": "unknown", "cpUnit": "utf16"},
        {"record": "line", "story": "main", "para": 0, "line": 0,
         "cpFirst": 0, "cpLim": 4},
    ]), encoding="utf-8")
    binary = tmp_path / "layout-trace"
    binary.write_bytes(b"test binary")
    font = tmp_path / "calibri.ttf"
    font.write_bytes(b"test font")
    fallback = tmp_path / "fallback.ttc"
    fallback.write_bytes(b"test fallback")
    output = tmp_path / "out"
    return {"analysis": analysis, "fixture": fixture, "capture": capture, "binary": binary,
            "font": font, "fallback": fallback, "output": output}


def argv(case, *extra):
    return ["--analysis-root", str(case["analysis"]), "--trace-bin", str(case["binary"]),
            "--font", str(case["font"]), "--fallback-font", str(case["fallback"]) + "#2",
            "--output", str(case["output"]), *extra]


def read_report(case):
    return json.loads((case["output"] / "summary.json").read_text(encoding="utf-8"))


def fake_engine(monkeypatch, extra_line=False):
    calls = []

    def run(command, **kwargs):
        calls.append((command, kwargs))
        lines = [{"sourceStart": 0, "sourceEnd": 4}]
        if extra_line:
            lines.append({"sourceStart": 4, "sourceEnd": 5})
        Path(command[-1]).write_text(json.dumps({
            "schema": "rsword-layout-trace/1", "offsetSpace": OFFSET_SPACE,
            "pages": [{"lines": lines}],
        }), encoding="utf-8")
        return subprocess.CompletedProcess(command, 0)

    monkeypatch.setattr(replay.subprocess, "run", run)
    return calls


def test_conditional_replay_hashes_inputs_and_counts_extra_engine_lines(case, monkeypatch):
    monkeypatch.setenv("RSWORD_FALLBACK_FONT", "/uncontrolled.ttf")
    calls = fake_engine(monkeypatch, extra_line=True)
    assert replay.main(argv(case, "--assume-legacy-narrow")) == 1
    report = read_report(case)
    assert report["summary"]["counts"] == {"OK": 0, "FAIL": 1, "UNDECIDABLE": 0}
    assert report["summary"]["conditionalFixtures"] == 1
    assert report["summary"]["boundary"]["denominator"] == 2
    assert report["summary"]["boundary"]["matchedLines"] == 1
    assert report["engineBinary"]["sha256"] == replay.fingerprint(case["binary"])["sha256"]
    assert report["fallbackFonts"][0]["faceIndex"] == 2
    command, options = calls[0]
    assert command[command.index("--platform") + 1] == "android"
    assert command[command.index("--view") + 1] == "mobile"
    assert command[command.index("--content-width") + 1] == "5329"
    assert command[command.index("--require") + 1] == "Calibri"
    assert "RSWORD_FALLBACK_FONT" not in options["env"]
    assert options["timeout"] == 60
    assert (case["output"] / "sample/command.json").is_file()
    assert (case["output"] / "sample/stderr.log").is_file()
    assert "Conditional fixtures: 1" in (case["output"] / "summary.md").read_text()


def test_strict_mode_does_not_report_unknown_capture_as_match(case, monkeypatch):
    fake_engine(monkeypatch)
    assert replay.main(argv(case)) == 2
    summary = read_report(case)["summary"]
    assert summary["counts"]["UNDECIDABLE"] == 1
    assert summary["boundary"]["denominator"] == 0
    assert summary["boundary"]["unassessedFixtures"] == 1


@pytest.mark.parametrize("damage", ["missing", "mismatch"])
def test_missing_or_changed_fixture_remains_reported_without_running_engine(case, monkeypatch, damage):
    calls = fake_engine(monkeypatch)
    if damage == "missing":
        case["fixture"].unlink()
    else:
        case["fixture"].write_bytes(b"changed fixture")
    assert replay.main(argv(case, "--assume-legacy-narrow")) == 2
    assert not calls
    report = read_report(case)
    assert len(report["results"]) == 1
    assert report["summary"]["counts"]["UNDECIDABLE"] == 1


@pytest.mark.parametrize("failure", ["timeout", "nonzero"])
def test_engine_failures_are_reported_with_logs(case, monkeypatch, failure):
    def run(command, **kwargs):
        kwargs["stderr"].write(b"engine diagnostic")
        if failure == "timeout":
            raise subprocess.TimeoutExpired(command, kwargs["timeout"])
        return subprocess.CompletedProcess(command, 17)

    monkeypatch.setattr(replay.subprocess, "run", run)
    assert replay.main(argv(case)) == 2
    assert (case["output"] / "sample/stderr.log").read_bytes() == b"engine diagnostic"
    comparison = read_report(case)["results"][0]["comparison"]
    assert comparison["state"] == "UNDECIDABLE"
    assert comparison["boundary"] is None


def test_changed_font_invalidates_otherwise_matching_replay(case, monkeypatch):
    fake_engine(monkeypatch)
    original_run = replay.subprocess.run

    def run(command, **kwargs):
        process = original_run(command, **kwargs)
        case["font"].write_bytes(b"changed during run")
        return process

    monkeypatch.setattr(replay.subprocess, "run", run)
    assert replay.main(argv(case, "--assume-legacy-narrow")) == 2
    comparison = read_report(case)["results"][0]["comparison"]
    assert comparison["boundary"] is None
    assert "Input changed during replay" in comparison["reasons"][0]


def test_cannot_write_output_in_source_project_or_overwrite_existing(case):
    case["output"] = case["analysis"] / "new-output"
    with pytest.raises(SystemExit) as exc:
        replay.main(argv(case))
    assert exc.value.code == 2
    assert not case["output"].exists()
    case["output"] = case["analysis"].parent / "existing"
    case["output"].mkdir()
    with pytest.raises(SystemExit) as exc:
        replay.main(argv(case))
    assert exc.value.code == 2
