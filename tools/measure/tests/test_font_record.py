import base64
import copy
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import font_record as cli
from wordmeasure import font_record_reference as ref


def metrics(**changes):
    # Measured TNR principal fields; other fields are explicit synthetic zeros.
    result = {name: 0 for name in ref.UNSIGNED + ref.SIGNED + ["hasTypographicMetrics"]}
    result.update(designUnitsPerEm=2048, ascent=1825, descent=443, lineGap=87)
    result.update(changes)
    return result


def project(**changes):
    arguments = dict(initialized=metrics(), requested=metrics(ascent=1920, descent=512, lineGap=128),
                     lf_height=-16, xavg_width=1024, width_scale=1.0, escapement=0)
    arguments.update(changes)
    return ref.project_face1(**arguments)


@pytest.mark.parametrize("value,expected", [
    (0.5, 1), (-0.5, -1), (0.4999999701976776, 0), (-0.4999999701976776, 0),
    (0.5000000596046448, 1), (-0.5000000596046448, -1), (65535.5, 65536),
    (-32768.5, -32769), (2147483520.0, 2147483520), (2147483648.0, ref.MAX_I32),
    (-2147483648.0, ref.MIN_I32), (-2147483904.0, ref.MAX_I32),
    (float("inf"), ref.MAX_I32), (-float("inf"), ref.MAX_I32), (float("nan"), ref.MAX_I32),
])
def test_scalar_halfway_neighbors_and_overflow_sentinel(value, expected):
    assert ref.convert_scalar(value) == expected


def test_double_ulp_push_differs_from_mathematical_nearest():
    below = math.nextafter(0.5, 0.0)
    assert below < 0.5
    assert ref.round_leaf(below) == 1
    assert ref.round_leaf(-below) == -1


@pytest.mark.parametrize("height,requested,expected", [
    (-16, metrics(ascent=1920, descent=512, lineGap=128), [19, 15, 4, 3, 1, 8, 8]),
    (-50, metrics(ascent=1843, descent=492, lineGap=82), [57, 45, 12, 7, 2, 25, 25]),
    (-2048, None, [2268, 1825, 443, 220, 87, 1024, 1024]),
])
def test_principal_measured_inputs_have_hand_calculated_projection(height, requested, expected):
    result = project(lf_height=height, requested=requested)
    assert result["tPrefixWords"] == expected
    assert result["initialMWords"] == [expected[1], expected[2], expected[0], expected[4], expected[5], 0]


@pytest.mark.parametrize("saved_flag,returned_flag,expected", [
    (1, 0, [20, 16, 4, 4, 0]), (0, 1, [19, 15, 4, 3, 1]),
])
def test_gate_comes_from_initialized_q_not_temporary_api_flag(saved_flag, returned_flag, expected):
    result = project(initialized=metrics(hasTypographicMetrics=saved_flag),
                     requested=metrics(ascent=1920, descent=512, lineGap=128, hasTypographicMetrics=returned_flag))
    assert result["tPrefixWords"][:5] == expected
    assert result["temporaryMetrics"]["hasTypographicMetrics"] == returned_flag


def test_copy_preserves_defined_fields_without_scaling_or_tail_fabrication():
    initial = metrics(glyphBoxBottom=-628, subscriptPositionY=-293)
    result = project(initialized=initial, requested=None, lf_height=-2048, xavg_width=-7)
    assert result["branch"] == "copy50"
    assert result["temporaryMetrics"] == initial
    assert result["halfwordConversions"] == []
    assert result["temporaryExtraI16"] == -7
    assert result["tPrefixWords"][5:] == [1, 1]


def test_halfword_stores_wrap_and_signed_reload_precedes_width_clamp():
    q = metrics(designUnitsPerEm=2)
    positive = project(initialized=q, requested=metrics(designUnitsPerEm=2, capHeight=1), lf_height=-131071)
    assert positive["temporaryMetrics"]["capHeight"] == 0  # H(65535.5)=65536, STRH=0.
    negative = project(initialized=q, requested=metrics(designUnitsPerEm=2, lineGap=-1), lf_height=-65537)
    assert negative["temporaryMetrics"]["lineGap"] == 32767  # -32769 becomes 0x7fff.
    wrapped = project(initialized=q, requested=q, lf_height=-4, xavg_width=16384)
    assert wrapped["temporaryExtraI16"] == -32768
    assert wrapped["tPrefixWords"][5:] == [1, 1]


def test_scaling_leaves_glyph_box_and_upm_but_scales_subscript():
    result = project(requested=metrics(glyphBoxLeft=-1164, subscriptPositionY=-256))
    assert result["temporaryMetrics"]["glyphBoxLeft"] == -1164
    assert result["temporaryMetrics"]["designUnitsPerEm"] == 2048
    assert result["temporaryMetrics"]["subscriptPositionY"] == -2
    assert result["halfwordConversions"][8]["field"] == "hostExtraI16"


@pytest.mark.parametrize("angle", [900, 2700, 899, 901, 0])
def test_angle_adjustment_changes_only_the_first_four_words(angle):
    result = project(lf_height=-50, requested=metrics(ascent=1843, descent=492, lineGap=82), escapement=angle)
    expected = [59, 46, 13, 9] if angle in (900, 2700) else [57, 45, 12, 7]
    assert result["tPrefixWords"] == expected + [2, 25, 25]


def test_width_float32_multiply_and_overflow_are_not_combined_with_integer_clamp():
    result = project(lf_height=-2048, requested=None, xavg_width=45, width_scale=0.7)
    assert result["tPrefixWords"][5] == 32
    result = project(lf_height=-2048, requested=None, xavg_width=32767, width_scale=3.4028234663852886e38)
    assert result["tPrefixWords"][5] == ref.MAX_I32


@pytest.mark.parametrize("change", [
    {"lf_height": 0}, {"lf_height": ref.MIN_I32}, {"lf_height": True},
    {"width_scale": True}, {"width_scale": float("nan")}, {"width_scale": 10**400},
    {"escapement": False}, {"xavg_width": 32768}, {"xavg_width": True},
    {"initialized": metrics(designUnitsPerEm=0)}, {"requested": metrics(designUnitsPerEm=1000)},
    {"requested": metrics(ascent=True)}, {"requested": None}, {"lf_height": -2048},
])
def test_unsupported_or_ambiguous_inputs_are_rejected(change):
    with pytest.raises(ValueError):
        project(**change)


def sfnt(width=1024, length=78):
    data = bytearray(28 + length)
    struct.pack_into(">IHHHH", data, 0, 0x10000, 1, 0, 0, 0)
    struct.pack_into(">4sIII", data, 12, b"OS/2", 0, 28, length)
    struct.pack_into(">h", data, 30, width)
    return bytes(data)


def test_os2_reader_preserves_signedness_and_enforces_host_length():
    assert cli.xavg_from_truetype(sfnt(-17))["xAvgCharWidthI16"] == -17
    for data in (sfnt(length=77), sfnt()[:-1], b"ttcf" + sfnt()[4:], sfnt()[:20]):
        with pytest.raises(ValueError):
            cli.xavg_from_truetype(data)


@pytest.fixture
def measurement(tmp_path, monkeypatch):
    # Adapter tests isolate the already separately tested native protocol parser.
    font = tmp_path / "fixture.ttf"
    font.write_bytes(sfnt())
    bound = {"font": {"path": str(font), "sha256": hashlib.sha256(font.read_bytes()).hexdigest()},
             "framework": {"path": "/synthetic", "sha256": cli.dwrite_metrics.FRAMEWORK_SHA256},
             "tool": {"path": "/synthetic", "sha256": "0" * 64}}
    records = [dict(stage="analysis", fileType=2, faceType=1, count=1), dict(stage="metrics", metrics=metrics()),
               dict(stage="gdiMetrics", emSize=2048.0, metrics=metrics()),
               dict(stage="gdiMetrics", emSize=16.0, metrics=metrics(ascent=1920, descent=512, lineGap=128))]
    monkeypatch.setattr(cli.dwrite_metrics, "protocol", lambda raw, bindings, sizes: copy.deepcopy(records))
    report = dict(schema="rsword-dwrite-metrics/1", status="MEASURED", exitCode=0, timedOut=False, errors=[],
                  inputsBefore=bound, inputsAfter=copy.deepcopy(bound), emSizes=[2048.0, 16.0],
                  stdoutBase64=base64.b64encode(b"synthetic; parser isolated").decode())
    path = tmp_path / "input.json"
    return path, report, records, font


def test_adapter_binds_real_font_bytes_and_only_reports_reference(measurement):
    path, report, _, _ = measurement
    path.write_text(json.dumps(report))
    result = cli.project_report(path, lf_height=-16, width_scale=1.0, escapement=0)
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["result"]["initialMWords"] == [15, 4, 19, 1, 8, 0]
    assert result["fontTable"]["xAvgCharWidthI16"] == 1024
    assert result["measurement"]["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()


@pytest.mark.parametrize("damage", ["failed", "changed-font", "missing-n", "missing-D", "conflicting-repeat", "CFF", "changed-hash"])
def test_adapter_does_not_interpolate_or_accept_incompatible_measurements(measurement, damage):
    path, report, records, font = measurement
    if damage == "failed":
        report["status"] = "FAILED"
    elif damage == "changed-font":
        font.write_bytes(sfnt(-1))
    elif damage == "missing-n":
        records.pop()
    elif damage == "missing-D":
        records.pop(2)
    elif damage == "conflicting-repeat":
        records.append(dict(stage="gdiMetrics", emSize=16.0, metrics=metrics(ascent=1)))
    elif damage == "CFF":
        records[0]["faceType"] = 0
    else:
        report["inputsAfter"]["font"]["sha256"] = "f" * 64
    path.write_text(json.dumps(report))
    with pytest.raises(ValueError):
        cli.project_report(path, lf_height=-16, width_scale=1.0, escapement=0)


def test_cli_refuses_existing_output_before_reading_measurement(tmp_path, monkeypatch):
    out = tmp_path / "out.json"
    out.symlink_to(tmp_path / "missing")
    monkeypatch.setattr(cli, "project_report", lambda *a, **k: pytest.fail("must not read source"))
    with pytest.raises(SystemExit) as exc:
        cli.main(["--measurements", "missing", "--lf-height", "-16", "--width-scale", "1", "--escapement", "0", "--out", str(out)])
    assert exc.value.code == 2 and out.is_symlink()
