import copy
import hashlib
import json
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import font_adjustment as cli


def inputs():
    return {"schema": "rsword-font-adjustment-input/1", "project": {
        "pre_scale": dict(bc=300, c0=1800, c4=400, c8=1900, cc=200, d0=0,
                          **{"180": 500, "184": 7, "28c": 100, "290": 200, "294": 300}),
        "scale_x": 1440, "scale_y": 2880, "h2": 24, "dx": 2000, "dy": 2000,
        "initial_m": [1, 2, 3, 11, 5, 13], "c8_correction": False,
    }, "h2Source": {"raw0": 2200, "raw_c": 200, "input_size": 24, "denominator": 2000}}


def save(tmp_path, data):
    path = tmp_path / "input.json"
    path.write_text(json.dumps(data))
    return path


def test_cli_replays_explicit_checkpoints_with_hashes_and_preserved_words(tmp_path):
    path = save(tmp_path, inputs())
    out = tmp_path / "out.json"
    assert cli.main(["--input", str(path), "--out", str(out)]) == 0
    result = json.loads(out.read_bytes())
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["result"]["updatedMWords"] == [456, 96, 552, 11, 7, 13]
    assert result["h2Result"]["h2"] == 24
    assert result["source"]["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
    assert result["codeSha256"]["model"] == hashlib.sha256(Path(cli.model.__file__).read_bytes()).hexdigest()


def test_h2_source_is_optional_but_not_inferred(tmp_path):
    data = inputs()
    del data["h2Source"]
    result = cli.project_file(save(tmp_path, data))
    assert result["h2Result"] is None
    assert result["inputs"] == data


@pytest.mark.parametrize("change", [
    lambda value: value.update(schema="MEASURED"),
    lambda value: value.update(unexpected=0),
    lambda value: value.update(project=[]),
    lambda value: value.update(h2Source=None),
    lambda value: value["h2Source"].update(input_size=25),
    lambda value: value["project"].update(c8_correction=1),
    lambda value: value["project"].update(unknown=1),
    lambda value: value["project"].update(scale_x=True),
])
def test_invalid_inputs_never_publish_success(tmp_path, change):
    data = copy.deepcopy(inputs())
    change(data)
    source = save(tmp_path, data)
    out = tmp_path / "out.json"
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(source), "--out", str(out)])
    assert error.value.code == 2
    assert not out.exists()


@pytest.mark.parametrize("raw", [
    '{"schema":1,"schema":2}', 'NaN', '1e999', 'null', '[{}]', '{',
])
def test_invalid_json_is_rejected(tmp_path, raw):
    source = tmp_path / "bad.json"
    source.write_text(raw)
    with pytest.raises((ValueError, TypeError)):
        cli.project_file(source)


@pytest.mark.parametrize("symlink", [False, True])
def test_existing_output_is_preserved_including_dangling_symlink(tmp_path, symlink):
    source = save(tmp_path, inputs())
    out = tmp_path / "out.json"
    if symlink:
        out.symlink_to(tmp_path / "missing.json")
    else:
        out.write_text("original")
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(source), "--out", str(out)])
    assert error.value.code == 2
    assert out.is_symlink() if symlink else out.read_text() == "original"
