import base64
import copy
import hashlib
import json
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import font_vertical as cli
from test_dwrite_metrics import encode, transcript
from test_font_record import sfnt


def inputs():
    return {"schema": "rsword-font-vertical-input/1",
            "font": {"lf_height": -2048, "width_scale": 1.0, "escapement": 0},
            "vertical": {"tail_p0": 0, "tail_p13": 8, "metric_byte38": 0,
                         "input_size": 24, "dy": 2048, "scale_y": 294912,
                         "correction_p0": 0, "metric_byte37": 0}}


@pytest.fixture
def sources(tmp_path):
    # Synthetic protocol/font records exercise the real parser; no native library is loaded.
    font = tmp_path / "fixture.ttf"
    font.write_bytes(sfnt())
    bound = {"font": {"path": str(font), "sha256": hashlib.sha256(font.read_bytes()).hexdigest()},
             "framework": {"path": "/synthetic", "sha256": cli.dwrite_metrics.FRAMEWORK_SHA256},
             "tool": {"path": "/synthetic", "sha256": "0" * 64}}
    rows = transcript(bound, [2048.0])
    report = {"schema": "rsword-dwrite-metrics/1", "status": "MEASURED", "exitCode": 0,
              "timedOut": False, "errors": [], "inputsBefore": bound, "inputsAfter": copy.deepcopy(bound),
              "emSizes": [2048.0], "stdoutBase64": base64.b64encode(encode(rows)).decode()}
    measured = tmp_path / "measured.json"
    measured.write_text(json.dumps(report))
    config = tmp_path / "input.json"
    config.write_text(json.dumps(inputs()))
    return config, measured, font, report, rows


def test_cli_connects_validated_protocol_to_three_vertical_words(sources, tmp_path):
    config, measured, _, _, _ = sources
    out = tmp_path / "out.json"
    assert cli.main(["--input", str(config), "--measurements", str(measured), "--out", str(out)]) == 0
    result = json.loads(out.read_bytes())
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["fontRecord"]["result"]["tPrefixWords"] == [2200, 1800, 400, 152, -1, 1024, 1024]
    assert result["result"]["updatedMetricWords"] == [43176, 9600, 52776]
    assert result["source"]["sha256"] == hashlib.sha256(config.read_bytes()).hexdigest()
    assert result["fontRecord"]["measurement"]["sha256"] == hashlib.sha256(measured.read_bytes()).hexdigest()
    assert result["codeSha256"]["model"] == hashlib.sha256(Path(cli.model.__file__).read_bytes()).hexdigest()
    assert result["codeSha256"]["adjustmentModel"] == hashlib.sha256(Path(cli.adjustment.__file__).read_bytes()).hexdigest()


def test_shared_bits_and_later_correction_use_distinct_explicit_checkpoints(sources):
    config, measured, _, _, _ = sources
    data = inputs()
    data["vertical"].update(tail_p0=1 << 16, correction_p0=0, metric_byte37=1)
    config.write_text(json.dumps(data))
    result = cli.project_file(config, measured)
    # Subtract 152, suppress external -1: (1800-152)*24, not the later correction gate.
    assert result["result"]["updatedMetricWords"] == [39552, 9600, 49152]
    data["vertical"].update(correction_p0=1 << 16)
    config.write_text(json.dumps(data))
    result = cli.project_file(config, measured)
    assert result["result"]["updatedMetricWords"] == [47744, 9600, 57344]


@pytest.mark.parametrize("change", [
    lambda value: value.update(schema="MEASURED"),
    lambda value: value.update(unexpected=0),
    lambda value: value.update(font=[]),
    lambda value: value["font"].update(lf_height=-16),  # Exact size is absent; no interpolation.
    lambda value: value["font"].update(width_scale=True),
    lambda value: value["font"].update(extra=0),
    lambda value: value.update(vertical=[]),
    lambda value: value["vertical"].update(t_prefix_words=[1] * 7),
    lambda value: value["vertical"].update(pre_scale={}),
    lambda value: value["vertical"].update(tail_p13=0),  # Unsupported alternate tail.
    lambda value: value["vertical"].update(tail_p13=True),
    lambda value: value["vertical"].update(input_size=24.0),
    lambda value: value["vertical"].update(metric_byte38=128),
    lambda value: value["vertical"].pop("correction_p0"),
])
def test_invalid_or_unsupported_input_never_publishes(sources, tmp_path, change):
    config, measured, _, _, _ = sources
    data = inputs()
    change(data)
    config.write_text(json.dumps(data))
    out = tmp_path / "out.json"
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(config), "--measurements", str(measured), "--out", str(out)])
    assert error.value.code == 2
    assert not out.exists()


@pytest.mark.parametrize("damage", ["font", "protocol", "binding", "status"])
def test_revalidates_measurement_instead_of_trusting_derived_results(sources, damage):
    config, measured, font, report, rows = sources
    report["records"] = [{"stage": "fake", "metrics": {"ascent": 42}}]
    if damage == "font":
        font.write_bytes(sfnt(-1))
    elif damage == "protocol":
        rows[-1]["status"] = "FAILED"
        report["stdoutBase64"] = base64.b64encode(encode(rows)).decode()
    elif damage == "binding":
        report["inputsAfter"]["font"]["sha256"] = "f" * 64
    else:
        report["status"] = "FAILED"
    measured.write_text(json.dumps(report))
    with pytest.raises(ValueError):
        cli.project_file(config, measured)


@pytest.mark.parametrize("raw", ['{"schema":1,"schema":2}', 'NaN', '1e999', 'null', '[{}]', '{'])
def test_invalid_json_is_rejected(sources, raw):
    config, measured, _, _, _ = sources
    config.write_text(raw)
    with pytest.raises((ValueError, TypeError)):
        cli.project_file(config, measured)


@pytest.mark.parametrize("symlink", [False, True])
def test_existing_output_is_preserved_before_reading_inputs(tmp_path, monkeypatch, symlink):
    out = tmp_path / "out.json"
    if symlink:
        out.symlink_to(tmp_path / "missing")
    else:
        out.write_text("original")
    monkeypatch.setattr(cli, "project_file", lambda *args: pytest.fail("must not read inputs"))
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", "missing", "--measurements", "missing", "--out", str(out)])
    assert error.value.code == 2
    assert out.is_symlink() if symlink else out.read_text() == "original"
