import argparse
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import dwrite_metrics as dm


@pytest.fixture
def inputs(tmp_path, monkeypatch):
    font, framework = tmp_path / "font.ttf", tmp_path / "Frameworks/dwrite.framework/Versions/A/dwrite"
    framework.parent.mkdir(parents=True)
    font.write_bytes(b"synthetic font input")
    framework.write_bytes(b"synthetic library, never loaded")
    monkeypatch.setattr(dm, "FRAMEWORK", framework)
    monkeypatch.setattr(dm, "FRAMEWORK_SHA256", hashlib.sha256(framework.read_bytes()).hexdigest())
    monkeypatch.setattr(dm.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(dm.platform, "machine", lambda: "arm64")
    return font


def transcript(bound, sizes):
    """A complete synthetic receipt; these values are not Word expectations."""
    rows = [dict(stage="inputs", bindings=bound, emSizes=sizes, **dm.OPTIONS),
            dict(stage="beforeLoadLibrary"), dict(stage="LoadLibrary")]

    def call(name, **values):
        rows.extend([dict(stage="before" + name), dict(stage=name, **values)])

    for name in ("CreateFactory", "CreateFontFileReference", "Analyze"):
        call(name, hresult=0)
    rows.append(dict(stage="analysis", supported=1, fileType=2, faceType=1, count=1))
    for name in ("CreateFontFace", "QueryInterfaceFace1"):
        call(name, hresult=0)
    call("GetMetrics1")
    metrics = {name: 0 for name, _ in dm.FIELDS}
    metrics.update(designUnitsPerEm=2048, ascent=1800, descent=400, lineGap=-1)
    rows.append(dict(stage="metrics", metrics=metrics.copy()))
    for size in sizes:
        call("GetGdiCompatibleMetrics1", hresult=0)
        rows.append(dict(stage="gdiMetrics", emSize=size, metrics=metrics.copy()))
    for name in ("face1", "face", "file", "factory"):
        rows.extend([dict(stage="beforeRelease", interface=name), dict(stage="Release", interface=name, remaining=0)])
    return rows + [dict(stage="complete", status="MEASURED")]


def encode(rows):
    return ("\n".join(json.dumps(row) for row in rows) + "\n").encode()


@pytest.mark.parametrize("value", ["0", "-0", "-2", "nan", "inf", "-inf", "1e100", "1e-100", "bad", None])
def test_rejects_invalid_binary32_size(value):
    with pytest.raises(argparse.ArgumentTypeError):
        dm.em_size(value)


def test_binary32_rounding_is_recorded():
    assert dm.em_size("0.1") == 0.10000000149011612
    assert dm.em_size("1.401298464324817e-45") > 0
    assert dm.em_size("3.4028234663852886e38") == 3.4028234663852886e38


def test_success_keeps_sizes_binding_output_and_isolated_command(inputs, monkeypatch):
    sizes = [dm.em_size("0.1"), 16.0, 16.0]
    raw = encode(transcript(dm.bindings(inputs), sizes))
    seen = {}

    def run(command, **kwargs):
        seen.update(command=command, **kwargs)
        return subprocess.CompletedProcess(command, 0, raw, b"diagnostic\xff")

    monkeypatch.setenv("DYLD_INSERT_LIBRARIES", "/not-loaded")
    monkeypatch.setattr(dm.subprocess, "run", run)
    report = dm.capture(inputs, sizes)
    assert report["status"] == "MEASURED"
    assert report["inputsBefore"] == report["inputsAfter"] == dm.bindings(inputs)
    assert report["emSizes"] == sizes
    assert report["factoryType"] == 1 and report["pixelsPerDip"] == 1.0 and report["transform"] is None
    assert report["records"][-1] == dict(stage="complete", status="MEASURED")
    assert base64.b64decode(report["stdoutBase64"]) == raw
    assert base64.b64decode(report["stderrBase64"]) == b"diagnostic\xff"
    assert seen["command"][0:4] == [sys.executable, "-u", str(dm.SOURCE), "--worker"]
    assert "DYLD_INSERT_LIBRARIES" not in seen["env"]
    assert seen["env"]["DYLD_FRAMEWORK_PATH"] == str(dm.FRAMEWORK.parents[3])
    assert report["startedUtc"] <= report["finishedUtc"]


@pytest.mark.parametrize("mutation", ["null", "list", "scalar", "bool-em", "bool-option", "negative-hr",
                                      "unsigned-hr", "bool-hr", "missing-hr", "bool-metric", "range-metric",
                                      "missing-metric", "zero-upm", "bad-analysis", "missing-release",
                                      "swapped-release", "wrong-size", "wrong-binding", "missing-complete"])
def test_invalid_success_protocol_is_a_failure_receipt(inputs, monkeypatch, mutation):
    rows = copy.deepcopy(transcript(dm.bindings(inputs), [1.0]))
    at = lambda stage: next(row for row in rows if isinstance(row, dict) and row["stage"] == stage)
    if mutation in ("null", "list", "scalar"):
        rows.insert(2, {"null": None, "list": [], "scalar": 42}[mutation])
    elif mutation == "bool-em":
        at("gdiMetrics")["emSize"] = True
    elif mutation == "bool-option":
        rows[0]["factoryType"] = True
    elif mutation.endswith("hr"):
        if mutation == "missing-hr":
            del at("CreateFactory")["hresult"]
        else:
            at("CreateFactory")["hresult"] = {"negative-hr": -2147467259, "unsigned-hr": 2147500037, "bool-hr": False}[mutation]
    elif mutation in ("bool-metric", "range-metric", "zero-upm"):
        at("metrics")["metrics"]["designUnitsPerEm"] = {"bool-metric": True, "range-metric": 65536, "zero-upm": 0}[mutation]
    elif mutation == "missing-metric":
        del at("metrics")["metrics"]["ascent"]
    elif mutation == "bad-analysis":
        at("analysis")["count"] = True
    elif mutation == "missing-release":
        rows.remove(at("Release"))
    elif mutation == "swapped-release":
        at("Release")["interface"] = "factory"
    elif mutation == "wrong-size":
        at("gdiMetrics")["emSize"] = 2.0
    elif mutation == "wrong-binding":
        rows[0]["bindings"]["font"]["sha256"] = "0" * 64
    else:
        rows.pop()
    raw = encode(rows)
    monkeypatch.setattr(dm.subprocess, "run", lambda *a, **k: subprocess.CompletedProcess(a, 0, raw, b""))
    report = dm.capture(inputs, [1.0])
    assert report["status"] == "FAILED" and report["errors"]
    assert base64.b64decode(report["stdoutBase64"]) == raw


@pytest.mark.parametrize("damage", ["truncated", "malformed", "duplicate", "nan", "overflow-number", "deep-json", "invalid-utf8"])
def test_corrupt_protocol_never_claims_success(inputs, monkeypatch, damage):
    raw = encode(transcript(dm.bindings(inputs), [16.0]))
    if damage == "truncated":
        raw = raw[:-1]
    elif damage == "malformed":
        raw += b"{\n"
    elif damage == "duplicate":
        raw = raw.replace(b'"hresult": 0', b'"hresult": 0, "hresult": 0', 1)
    elif damage == "nan":
        raw = raw.replace(b'"hresult": 0', b'"hresult": NaN', 1)
    elif damage == "overflow-number":
        raw = raw.replace(b'"stage": "beforeLoadLibrary"', b'"stage": "beforeLoadLibrary", "extra": 1e999', 1)
    elif damage == "deep-json":
        raw += b"[" * 2000 + b"0" + b"]" * 2000 + b"\n"
    else:
        raw += b"\xff\n"
    monkeypatch.setattr(dm.subprocess, "run", lambda *a, **k: subprocess.CompletedProcess(a, 0, raw, b""))
    report = dm.capture(inputs, [16.0])
    assert report["status"] == "FAILED"
    assert base64.b64decode(report["stdoutBase64"]) == raw
    json.dumps(report, allow_nan=False)


@pytest.mark.parametrize("kind", ["hresult", "crash", "timeout"])
def test_failed_real_subprocess_produces_publishable_receipt(inputs, monkeypatch, tmp_path, kind):
    real_run = subprocess.run
    code = "import sys,os,signal,time;print('beforeNativeCall',flush=True);print('native stderr',file=sys.stderr,flush=True);"
    code += {"hresult": "print('{\"hresult\":-2147467259}',flush=True);sys.exit(1)",
             "crash": "os.kill(os.getpid(),signal.SIGTERM)", "timeout": "time.sleep(5)"}[kind]
    monkeypatch.setattr(dm.subprocess, "run", lambda command, **kwargs: real_run([sys.executable, "-u", "-c", code], **kwargs))
    report = dm.capture(inputs, [16.0], timeout=0.3 if kind == "timeout" else 5)
    assert report["status"] == "FAILED" and "beforeNativeCall" in report["stdout"]
    assert "native stderr" in report["stderr"]
    assert report["timedOut"] == (kind == "timeout")
    assert report["exitCode"] == {"hresult": 1, "crash": -15, "timeout": None}[kind]
    target = tmp_path / (kind + ".json")
    dm.publish(target, report)
    assert json.loads(target.read_text())["status"] == "FAILED"


@pytest.mark.parametrize("kind", ["platform", "hash", "missing-font", "spawn-error", "changed-font"])
def test_preflight_and_binding_failures(inputs, monkeypatch, kind):
    called = []

    def run(command, **kwargs):
        called.append(command)
        if kind == "spawn-error":
            raise OSError("cannot spawn")
        raw = encode(transcript(dm.bindings(inputs), [16.0]))
        inputs.write_bytes(b"changed")
        return subprocess.CompletedProcess(command, 0, raw, b"")

    monkeypatch.setattr(dm.subprocess, "run", run)
    if kind == "platform":
        monkeypatch.setattr(dm.platform, "machine", lambda: "x86_64")
    elif kind == "hash":
        monkeypatch.setattr(dm, "FRAMEWORK_SHA256", "0" * 64)
    elif kind == "missing-font":
        inputs.unlink()
    report = dm.capture(inputs, [16.0])
    assert report["status"] == "FAILED" and report["errors"]
    assert report["inputsBefore"]["tool"]["sha256"] == hashlib.sha256(dm.SOURCE.read_bytes()).hexdigest()
    assert bool(called) == (kind in ("spawn-error", "changed-font"))


@pytest.mark.parametrize("existing", ["file", "link", "dangling-link", "directory"])
def test_atomic_publish_never_overwrites(tmp_path, existing):
    target, referent = tmp_path / "out.json", tmp_path / "referent"
    referent.write_bytes(b"keep")
    if existing == "file":
        target.write_bytes(b"keep")
    elif existing == "directory":
        target.mkdir()
    else:
        target.symlink_to(referent if existing == "link" else tmp_path / "missing")
    with pytest.raises(FileExistsError):
        dm.publish(target, {"status": "FAILED"})
    assert referent.read_bytes() == b"keep"
    assert not list(tmp_path.glob(".dwrite-*"))
    if existing == "file":
        assert target.read_bytes() == b"keep"
    elif "link" in existing:
        assert target.is_symlink()


def test_nonfinite_report_never_creates_output(tmp_path):
    target = tmp_path / "out.json"
    with pytest.raises(ValueError):
        dm.publish(target, {"invalid": float("nan")})
    assert not os.path.lexists(target)
    assert not list(tmp_path.glob(".dwrite-*"))


def test_cli_nonoverwrite_precedes_any_measurement(tmp_path, monkeypatch):
    target = tmp_path / "out.json"
    target.symlink_to(tmp_path / "missing")
    monkeypatch.setattr(dm, "capture", lambda *a: pytest.fail("measurement must not start"))
    with pytest.raises(SystemExit) as exc:
        dm.main(["--font", "font.ttf", "--em-size", "16", "--out", str(target)])
    assert exc.value.code == 2 and target.is_symlink()


def test_cli_records_binary32_and_requires_out(tmp_path, monkeypatch):
    received = []
    monkeypatch.setattr(dm, "capture", lambda font, sizes: received.append((font, sizes)) or {"status": "FAILED"})
    with pytest.raises(SystemExit):
        dm.main(["--font", "font.ttf", "--em-size", "16"])
    assert not received
    target = tmp_path / "out.json"
    assert dm.main(["--font", "font.ttf", "--em-size", "0.1", "--em-size", "16", "--out", str(target)]) == 1
    assert received[0][1] == [0.10000000149011612, 16.0]
    assert json.loads(target.read_text()) == {"status": "FAILED"}
