import copy
import hashlib
import json
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import simple_fit as cli


def inputs():
    return {"schema": "rsword-simple-fit-input/1", "fit": {"u": 80, "v": 40, "s": 20, "limit": 110}}


def save(tmp_path, data):
    path = tmp_path / "input.json"
    path.write_text(json.dumps(data))
    return path


@pytest.mark.parametrize("allow,expected,advance", [(None, "NEEDS_OVERHANG", None),
                                                  (True, "KEEP", 120), (False, "CLIP", 110)])
def test_cli_preserves_unresolved_state_or_uses_explicit_overhang(tmp_path, allow, expected, advance):
    data = inputs()
    if allow is not None:
        data["allowOverhang"] = allow
    source = save(tmp_path, data)
    out = tmp_path / "out.json"
    assert cli.main(["--input", str(source), "--out", str(out)]) == 0
    result = json.loads(out.read_bytes())
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["classification"]["decision"] == "NEEDS_OVERHANG"
    assert result["result"]["decision"] == expected
    assert result["result"]["advance_i32"] == advance
    assert result["overhangResolved"] == (allow is not None)
    assert result["inputs"] == data
    assert result["source"]["sha256"] == hashlib.sha256(source.read_bytes()).hexdigest()
    for name, path in [("cli", Path(cli.__file__)), ("model", Path(cli.model.__file__)),
                       ("publisher", Path(cli.dwrite_metrics.__file__))]:
        assert result["codeSha256"][name] == hashlib.sha256(path.read_bytes()).hexdigest()


@pytest.mark.parametrize("s,expected", [(0, "KEEP"), (-20, "REJECT")])
def test_unneeded_overhang_does_not_override_classification(tmp_path, s, expected):
    data = inputs()
    data["fit"].update(v=20, s=s)
    data["allowOverhang"] = True
    result = cli.project_file(save(tmp_path, data))
    assert result["result"]["decision"] == expected
    assert result["result"]["allow_overhang"] is None
    assert result["overhangResolved"] is False


@pytest.mark.parametrize("change", [
    lambda value: value.update(schema="MEASURED"),
    lambda value: value.update(unexpected=0),
    lambda value: value.update(fit=[]),
    lambda value: value.update(allowOverhang=None),
    lambda value: value.update(allowOverhang=1),
    lambda value: value["fit"].update(u=True),
    lambda value: value["fit"].update(v=1 << 31),
    lambda value: value["fit"].update(s=1 << 30),
    lambda value: value["fit"].update(unknown=1),
    lambda value: value["fit"].pop("limit"),
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


@pytest.mark.parametrize("raw", ['{"schema":1,"schema":2}',
                                '{"fit":{"u":1,"u":2}}', 'NaN', '1e999', '1.0', 'null', '[{}]', '{'])
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
