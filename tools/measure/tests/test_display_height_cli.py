import hashlib
import json
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import display_height as cli


EXAMPLE = Path(__file__).resolve().parents[1] / "examples/display-height-explicit.json"


def test_cli_binds_inputs_code_and_distinct_direct_stores(tmp_path):
    out = tmp_path / "result.json"
    assert cli.main(["--input", str(EXAMPLE), "--out", str(out)]) == 0
    result = json.loads(out.read_bytes())
    assert result["schema"] == "rsword-display-height-reference/1"
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["inputs"] == json.loads(EXAMPLE.read_bytes())
    assert result["source"]["sha256"] == hashlib.sha256(EXAMPLE.read_bytes()).hexdigest()
    assert result["result"]["conversion"]["requestedI32"] == 0
    adjustment = result["result"]["adjustment"]
    assert adjustment["forwardedHeights"] == [0, 0, 0, 0]
    assert [item["offset"] for item in adjustment["directStores"]] == ["bc", "b4", "1c0"]
    for name, module in [("cli", cli), ("model", cli.model), ("muldiv", cli.muldiv),
                         ("publisher", cli.dwrite_metrics)]:
        assert result["codeSha256"][name] == hashlib.sha256(Path(module.__file__).read_bytes()).hexdigest()


@pytest.mark.parametrize("change", [
    lambda data: data.update(schema="MEASURED"),
    lambda data: data.update(unknown=0),
    lambda data: data.pop("hasLine"),
    lambda data: data.update(hasLine=1),
    lambda data: data.update(conversion=[]),
    lambda data: data["conversion"].update(source_scale=True),
    lambda data: data["conversion"].pop("origin"),
    lambda data: data["conversion"].update(unknown=0),
    lambda data: data.update(record=None),
    lambda data: data["record"].update(b4=1 << 31),
    lambda data: data["record"].pop("bc"),
    lambda data: data["record"].update(b8=0),
])
def test_invalid_inputs_do_not_publish(tmp_path, change):
    data = json.loads(EXAMPLE.read_bytes())
    change(data)
    source, out = tmp_path / "input.json", tmp_path / "out.json"
    source.write_text(json.dumps(data))
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(source), "--out", str(out)])
    assert error.value.code == 2
    assert not out.exists()


@pytest.mark.parametrize("raw", ['{"schema":1,"schema":2}',
                                '{"record":{"b4":1,"b4":2}}', 'NaN', '1e999', '1.0', 'null', '[{}]', '{'])
def test_malformed_or_ambiguous_json_is_rejected(tmp_path, raw):
    source = tmp_path / "input.json"
    source.write_text(raw)
    with pytest.raises((ValueError, TypeError)):
        cli.project_file(source)


@pytest.mark.parametrize("symlink", [False, True])
def test_existing_output_is_never_replaced(tmp_path, symlink):
    out = tmp_path / "out.json"
    if symlink:
        out.symlink_to(tmp_path / "missing.json")
    else:
        out.write_text("original")
    with pytest.raises(SystemExit) as error:
        cli.main(["--input", str(EXAMPLE), "--out", str(out)])
    assert error.value.code == 2
    assert out.is_symlink() if symlink else out.read_text() == "original"
