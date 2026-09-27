import hashlib
import json
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import font_tail as cli


def inputs():
    return {"schema": "rsword-font-tail-input/1", "project": {
        "v_prefix_words": [1000, 800, 200, 0, 75, 500, 500],
        "tail_p0": 0, "tail_p8": 100, "tail_p13": 8,
        "metric_byte38": 134, "mode": 2, "incoming_scale": 1440, "dy": 2048}}


def test_cli_preserves_separate_stack_output_and_provenance(tmp_path):
    source, out = tmp_path / "input.json", tmp_path / "output.json"
    source.write_text(json.dumps(inputs()))
    assert cli.main(["--input", str(source), "--out", str(out)]) == 0
    result = json.loads(out.read_text())
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["schema"] == "rsword-font-tail-reference/1"
    assert result["result"]["finalFields"] == {"c0": 800, "c4": 350, "c8": 950, "d0": 0}
    assert result["result"]["stackResult"] == 0
    assert "cc" not in result["result"]["finalFields"]
    assert "updatedMetricWords" not in result["result"]
    assert result["inputs"] == inputs()
    assert result["source"]["sha256"] == hashlib.sha256(source.read_bytes()).hexdigest()
    assert result["codeSha256"]["model"] == hashlib.sha256(Path(cli.model.__file__).read_bytes()).hexdigest()
    assert result["codeSha256"]["integerArithmetic"] == hashlib.sha256(Path(cli.arithmetic.__file__).read_bytes()).hexdigest()


def test_non_special_default_selector_keeps_split_before_adding_gap(tmp_path):
    data = inputs()
    data["project"].update(metric_byte38=1, tail_p13=20)
    source = tmp_path / "default.json"
    source.write_text(json.dumps(data))
    result = cli.project_file(source)["result"]
    assert result["finalFields"] == {"c0": 800, "c4": 200, "c8": 875, "d0": 0}
    assert result["stackResult"] == 0


def mode2_inputs():
    data = inputs()
    data["schema"] = "rsword-font-tail-mode2-input/1"
    data["project"].pop("mode")
    data["project"].pop("incoming_scale")
    data["project"].update(pre_scale_cc=17, input_size=24, dy=1000,
                           scale_y=6000, correction_p0=0, metric_byte37=0)
    return data


def test_mode2_cli_preserves_tail_cc_and_stage_provenance(tmp_path):
    source, out = tmp_path / "input.json", tmp_path / "output.json"
    data = mode2_inputs()
    source.write_text(json.dumps(data))
    assert cli.main(["--input", str(source), "--out", str(out)]) == 0
    report = json.loads(out.read_text())
    assert report["schema"] == "rsword-font-tail-mode2-reference/1"
    assert report["status"] == "ARITHMETIC_REFERENCE"
    assert report["result"]["updatedMetricWords"] == [950, 350, 1300]
    assert report["result"]["scaledFields"]["cc"] == 17
    assert "cc" not in report["result"]["tail"]["finalFields"]
    assert report["result"]["tail"]["stackResult"] == 0
    assert report["inputs"] == data
    assert report["codeSha256"]["verticalModel"] == hashlib.sha256(
        Path(cli.vertical.__file__).read_bytes()).hexdigest()


@pytest.mark.parametrize("mutate", [
    lambda data: data["project"].pop("pre_scale_cc"),
    lambda data: data["project"].update(pre_scale_cc=True),
    lambda data: data["project"].update(mode=2),
    lambda data: data["project"].update(incoming_scale=0),
    lambda data: data["project"].update(metric_byte38=0),
    lambda data: data["project"].update(scale_y=6000.0),
    lambda data: data.update(schema="rsword-font-tail-input/1"),
    lambda data: data.update(schema=[]),
])
def test_mode2_schema_requires_complete_distinct_contract(tmp_path, mutate):
    data = mode2_inputs()
    mutate(data)
    source, out = tmp_path / "input.json", tmp_path / "output.json"
    source.write_text(json.dumps(data))
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(source), "--out", str(out)])
    assert error.value.code == 2
    assert not out.exists()


@pytest.mark.parametrize("mutate", [
    lambda data: data.update(schema="MEASURED"),
    lambda data: data.update(extra=0),
    lambda data: data.update(project=[]),
    lambda data: data["project"].update(extra=1),
    lambda data: data["project"].pop("dy"),
    lambda data: data["project"].update(mode=True),
    lambda data: data["project"].update(mode=4),
    lambda data: data["project"].update(tail_p8=100.0),
    lambda data: data["project"].update(metric_byte38=256),
    lambda data: data["project"].update(metric_byte38=0),  # This selector chooses the simple tail.
    lambda data: data["project"].update(v_prefix_words=[1000, 800]),
])
def test_invalid_or_simple_tail_input_does_not_publish(tmp_path, mutate):
    data = inputs()
    mutate(data)
    source, out = tmp_path / "input.json", tmp_path / "output.json"
    source.write_text(json.dumps(data))
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(source), "--out", str(out)])
    assert error.value.code == 2
    assert not out.exists()


@pytest.mark.parametrize("raw", [
    '{"schema": 1, "schema": 2}', 'NaN', '1e999', 'null', '[{}]', '{',
])
def test_invalid_json_never_publishes(tmp_path, raw):
    source, out = tmp_path / "input.json", tmp_path / "output.json"
    source.write_text(raw)
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(source), "--out", str(out)])
    assert error.value.code == 2
    assert not out.exists()


@pytest.mark.parametrize("kind", ["file", "dangling-symlink", "directory"])
def test_existing_destination_is_preserved_before_reading_input(tmp_path, monkeypatch, kind):
    out = tmp_path / "output.json"
    if kind == "file":
        out.write_text("original")
    elif kind == "dangling-symlink":
        out.symlink_to(tmp_path / "missing")
    else:
        out.mkdir()
    monkeypatch.setattr(cli, "project_file", lambda _: pytest.fail("must not read inputs"))
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", "missing", "--out", str(out)])
    assert error.value.code == 2
    if kind == "file":
        assert out.read_text() == "original"
    elif kind == "dangling-symlink":
        assert out.is_symlink()
    else:
        assert out.is_dir()
