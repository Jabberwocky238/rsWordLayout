"""Replay the independently captured native evidence without opening Word."""

import hashlib
import json
from pathlib import Path

import prereg_vertical_precision as probe
from wordmeasure import OK

ROOT = Path(__file__).resolve().parents[3]
CAPTURE = ROOT / "captures/vertical-precision-2026-09-22"


def test_archived_receipts_preserve_the_original_capture_bytes():
    hashes = json.loads((CAPTURE / "receipt-hashes.json").read_text())
    assert len(hashes) == 33
    for name, expected in hashes.items():
        assert hashlib.sha256((CAPTURE / name).read_bytes()).hexdigest() == expected, name


def test_native_capture_admits_all_conditions_and_retains_counterexamples():
    result = probe.evaluate(
        CAPTURE, ROOT / "fixtures/vertical-precision.probes.json",
        ROOT / "fixtures/vertical-precision.docx",
        ROOT / "fixtures/vertical-precision.font-inputs.json")
    assert result["state"] == OK, result["admission"]
    assert result["denominators"]["admittedCases"] == 21
    assert result["denominators"]["expectedConditions"] == 42
    assert all(metric["readable"] == 42 for metric in result["metrics"].values())
    assert result["metrics"]["paintX"]["candidates"]["absolute/ratio_0.66"] == {
        "conditions": 42, "ok": 6, "fail": 36, "undecidable": 0}
    assert result["metrics"]["offset"]["candidates"]["empirical_unquantized"] == {
        "conditions": 42, "ok": 7, "fail": 35, "undecidable": 0}
    assert all(candidate["fail"] == 42 for candidate in result["metrics"]["advance"]["candidates"].values())
