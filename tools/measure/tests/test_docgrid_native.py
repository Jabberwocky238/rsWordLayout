"""Arithmetic/reference-contract checks; these are not Word layout assertions."""

import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import pytest

import docgrid_native as native

CASES = Path(native.__file__).parent / "cases" / "docgrid-native-synthetic.json"


def inputs():
    return json.loads(CASES.read_text())


def run_case(**changes):
    case = inputs()["cases"][0]
    case.update(changes)
    return native.replay({"schema": native.INPUT_SCHEMA, "cases": [case]})[0]["result"]


def test_published_model_is_the_frozen_source_byte_for_byte():
    raw = Path(native.docgrid_reference.__file__).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == native.MODEL_SHA256


def test_sample_vectors_cover_split_parity_conversion_and_independent_outer_limit():
    document = inputs()
    before = copy.deepcopy(document)
    rows = native.replay(document)
    assert [row["result"]["tuple"] for row in rows] == [
        [57, 46, 16, 9, 9, 3735928559],
        [56, 46, 17, 9, 9, 4294967295],
        [58, 46, 28, 28, 28, 0],
        [57, 46, 38, 9, 9, 0],
    ]
    assert document == before
    # Converting 277*300/1440 to 57 before ceil changes the chosen multiple.
    assert rows[2]["result"]["convertedPeriod"] == 57
    assert rows[2]["result"]["chosen"] == 114
    assert rows[3]["result"]["convertedValue"] == 104
    assert rows[3]["result"]["extra"] == 29


def test_period_truncates_while_u16_value_rounds_half_up():
    result = run_case(components=[0, 0, 0, 0, 0, 0], tag=0,
                      valueU16=1, periodI32=1, scaleU32=720)
    assert result["convertedPeriod"] == 0
    assert result["convertedValue"] == 1
    assert result["tuple"] == [0, 0, 1, 0, 0, 0]
    bypass = run_case(tag=0, valueU16=500, scaleU32=300,
                      periodI32=360, useInputHeight=True)
    assert bypass["convertedValue"] == 57
    assert bypass["extra"] == 0


@pytest.mark.parametrize(("period", "scale"), [(0, 300), (75, 0)])
def test_zero_input_skips_period_math_without_losing_opaque_bits(period, scale):
    result = run_case(periodI32=period, scaleU32=scale)
    assert result["tuple"] == [57, 46, 7, 0, 0, 3735928559]
    assert result["convertedPeriod"] == period


def test_clamp_and_wrapping_are_distinct_operations():
    result = run_case(components=[57, 46, native.I32_MAX, 100, native.I32_MAX, 0])
    assert result["tuple"] == [57, 46, native.I32_MIN + 8, 75, native.I32_MIN + 8, 0]
    assert result["diagnostics"] == ["offset_c_outside_chosen"]
    lower = run_case(components=[57, 46, 0, -20, 0, 0])
    assert lower["tuple"][3] == 0
    assert lower["diagnostics"] == ["offset_c_outside_chosen"]


def test_overflow_does_not_silently_turn_into_an_unbounded_ceiling():
    result = run_case(components=[native.I32_MAX, 0, native.I32_MAX, 0, 0, 0],
                      periodI32=native.I32_MAX - 1)
    assert result["multipleCandidate"] == -4
    assert result["chosen"] == native.I32_MAX - 1
    assert result["delta"] == -1
    assert result["tuple"][2] == native.I32_MIN
    assert result["diagnostics"] == ["chosen_less_than_input"]


@pytest.mark.parametrize("period", [native.I32_MIN, native.I32_MAX])
def test_period_conversion_saturates_at_signed32_limits(period):
    result = run_case(periodI32=period, scaleU32=0xffffffff)
    assert result["convertedPeriod"] == period


@pytest.mark.parametrize(("field", "value"), [
    ("tag", True), ("tag", 1), ("tag", 3), ("tag", 0.0),
    ("valueU16", -1), ("ratioU16", 65536), ("scaleU32", 1 << 32),
    ("periodI32", 1 << 31), ("periodI32", False), ("useInputHeight", 0),
    ("components", [0, 0, 0, 0, 0]),
    ("components", [0, 0, 0, 0, 0, -1]),
    ("components", [1 << 31, 0, 0, 0, 0, 0]),
    ("components", [0, 0, False, 0, 0, 0]),
])
def test_invalid_native_inputs_are_rejected(field, value):
    with pytest.raises(ValueError):
        run_case(**{field: value})


@pytest.mark.parametrize("mutation", ["missing", "extra", "duplicate_id", "empty", "schema"])
def test_ambiguous_protocol_has_no_defaults(mutation):
    document = inputs()
    if mutation == "missing":
        del document["cases"][0]["scaleU32"]
    elif mutation == "extra":
        document["cases"][0]["dpi"] = 300
    elif mutation == "duplicate_id":
        document["cases"][1]["id"] = document["cases"][0]["id"]
    elif mutation == "empty":
        document["cases"] = []
    else:
        document["schema"] = "unknown"
    with pytest.raises(ValueError):
        native.replay(document)


def test_cli_records_inputs_model_and_scope_without_mutating_inputs(tmp_path):
    output = tmp_path / "report.json"
    before = (CASES.read_bytes(), CASES.stat().st_mtime_ns)
    result = subprocess.run([sys.executable, str(Path(native.__file__)), str(CASES),
                             "--output", str(output)], capture_output=True, text=True)
    assert result.returncode == 0, result.stderr
    report = json.loads(output.read_text())
    assert report["state"] == "ARITHMETIC_REFERENCE"
    assert report["modelSha256"] == native.MODEL_SHA256
    assert report["input"]["sha256"] == hashlib.sha256(before[0]).hexdigest()
    assert not report["nativeExecuted"] and not report["engineExecuted"]
    assert report["cases"] == native.replay(inputs())
    assert before == (CASES.read_bytes(), CASES.stat().st_mtime_ns)
    written = output.read_bytes()
    with pytest.raises(SystemExit):
        native.main([str(CASES), "--output", str(output)])
    assert output.read_bytes() == written
    assert list(tmp_path.iterdir()) == [output]


def test_cli_rejects_dangling_output_symlink_and_duplicate_keys(tmp_path):
    output, target = tmp_path / "report.json", tmp_path / "target.json"
    output.symlink_to(target)
    with pytest.raises(SystemExit):
        native.main([str(CASES), "--output", str(output)])
    assert output.is_symlink() and not target.exists()
    duplicate = tmp_path / "input.json"
    duplicate.write_text('{"schema":"x","schema":"y","cases":[]}')
    with pytest.raises(SystemExit):
        native.main([str(duplicate), "--output", str(target)])
    assert not target.exists()


def test_model_change_fails_before_publishing(tmp_path, monkeypatch):
    monkeypatch.setattr(native, "MODEL_SHA256", "0" * 64)
    output = tmp_path / "report.json"
    with pytest.raises(SystemExit):
        native.main([str(CASES), "--output", str(output)])
    assert not output.exists()


def test_publication_race_preserves_other_writer_and_removes_temporary(tmp_path, monkeypatch):
    output = tmp_path / "report.json"

    def competing_writer(source, destination):
        destination.write_bytes(b"other writer")
        raise FileExistsError(destination)

    monkeypatch.setattr(native.os, "link", competing_writer)
    with pytest.raises(SystemExit):
        native.main([str(CASES), "--output", str(output)])
    assert output.read_bytes() == b"other writer"
    assert list(tmp_path.iterdir()) == [output]
