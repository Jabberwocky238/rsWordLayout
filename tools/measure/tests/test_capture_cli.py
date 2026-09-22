import pytest

from wordmeasure import cli


@pytest.mark.parametrize("usable,unchanged,expected", [
    ("OK", True, 0), ("UNDECIDABLE", True, 2), ("UNDECIDABLE", False, 1),
])
def test_capture_cli_propagates_unstable_measurements(monkeypatch, capsys, usable, unchanged, expected):
    meta = {
        "pageCount": 1, "glyphTotal": 2, "glyphCounts": [2],
        "fontSubstitution": {"result": "PASS"}, "fixture": {"unchanged": unchanged},
        "usability": usable, "usabilityReason": ["LINE_SWEEP_UNSTABLE"] if usable != "OK" else [],
    }
    monkeypatch.setattr(cli.capture_mod, "capture", lambda *args, **kwargs: meta)
    assert cli.main(["capture", "input.docx", "capture-output", "--no-slot"]) == expected
    if expected == 2:
        assert "UNDECIDABLE: LINE_SWEEP_UNSTABLE" in capsys.readouterr().err
