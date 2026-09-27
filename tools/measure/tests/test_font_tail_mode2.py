import copy
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wordmeasure import font_tail_reference as ref
from wordmeasure import font_vertical_reference as vertical


def inputs(**changes):
    values = dict(v_prefix_words=[1000, 800, 200, 0, 75, 500, 500],
                  tail_p0=0, tail_p8=100, tail_p13=8, metric_byte38=134,
                  pre_scale_cc=17, input_size=24, dy=1000, scale_y=6000,
                  correction_p0=0, metric_byte37=0)
    values.update(changes)
    return values


def test_tail_connects_to_scaled_components_without_changing_inputs():
    arguments = inputs()
    before = copy.deepcopy(arguments)
    result = ref.project_alternate_mode2(**arguments)
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["mode"] == 2
    assert result["h2Result"]["h2"] == 24
    assert result["preScaleFields"] == dict(c0=800, c4=350, c8=950, cc=17, d0=0)
    assert result["scaledFields"] == result["preScaleFields"]
    assert result["updatedMetricWords"] == [950, 350, 1300]
    assert [row["offset"] for row in result["conversions"]] == list(vertical.VERTICAL)
    assert "updatedMWords" not in result
    assert arguments == before


def test_cc_is_neither_raw_vc_nor_stack_result_and_does_not_change_m():
    result = ref.project_alternate_mode2(**inputs(tail_p13=12, pre_scale_cc=-37))
    assert result["tail"]["stackResult"] == -150
    assert result["tail"]["finalFields"] == dict(c0=950, c4=200, c8=1100, d0=150)
    assert result["scaledFields"] == dict(c0=950, c4=200, c8=1100, cc=-37, d0=150)
    assert result["updatedMetricWords"] == [1100, 200, 1300]
    changed = ref.project_alternate_mode2(**inputs(tail_p13=12, pre_scale_cc=ref.MAX_I32))
    assert changed["updatedMetricWords"] == result["updatedMetricWords"]
    assert changed["tail"] == result["tail"]


def test_each_component_rounds_before_m_sum():
    result = ref.project_alternate_mode2(**inputs(
        v_prefix_words=[1000, 801, 199, 0, 75, 500, 500], input_size=1, scale_y=72000))
    assert result["preScaleFields"] == dict(c0=801, c4=349, c8=951, cc=17, d0=0)
    assert result["scaledFields"] == dict(c0=401, c4=175, c8=476, cc=9, d0=0)
    assert result["updatedMetricWords"] == [476, 175, 651]  # Rounding the combined 1300 gives 650.


@pytest.mark.parametrize("scale,increment", [(35, 0), (36, 1), (-35, 0), (-36, -1)])
def test_later_correction_keeps_its_own_read_point_and_signed_truncation(scale, increment):
    plain = ref.project_alternate_mode2(**inputs(scale_y=scale))
    corrected = ref.project_alternate_mode2(**inputs(
        scale_y=scale, correction_p0=1 << 16, metric_byte37=1))
    assert plain["tail"] == corrected["tail"]
    assert plain["scaledFields"] == corrected["scaledFields"]
    assert corrected["c8IncrementI32"] == increment
    assert corrected["updatedMetricWords"][0] == plain["updatedMetricWords"][0] + increment
    assert corrected["updatedMetricWords"][1] == plain["updatedMetricWords"][1]


def test_h2_uses_raw_vc_and_not_the_separate_cc_input():
    result = ref.project_alternate_mode2(**inputs(
        v_prefix_words=[1000, 800, 200, 2000, 75, 500, 500], input_size=1))
    assert result["h2Result"]["rawDifferenceI32"] == -1000
    assert result["h2Result"]["roundedI32"] == -1
    assert result["h2Result"]["h2"] == 3276
    assert result["preScaleFields"]["cc"] == 17


def test_zero_denominator_and_correction_preserve_native_wrap():
    result = ref.project_alternate_mode2(**inputs(
        dy=0, scale_y=36, correction_p0=1 << 16, metric_byte37=1))
    assert result["h2Result"]["h2"] == 3276
    assert set(result["scaledFields"].values()) == {ref.MAX_I32}
    assert result["updatedMetricWords"] == [ref.MIN_I32, ref.MAX_I32, -1]


def test_non_special_default_selector_keeps_original_split_through_scale():
    result = ref.project_alternate_mode2(**inputs(metric_byte38=1, tail_p13=20))
    assert result["updatedMetricWords"] == [875, 200, 1075]
    assert result["tail"]["stackResult"] == 0


@pytest.mark.parametrize("name", ["pre_scale_cc", "input_size", "dy", "scale_y",
                                  "correction_p0", "metric_byte37"])
@pytest.mark.parametrize("value", [True, 1.0, None])
def test_new_read_points_require_exact_integers(name, value):
    with pytest.raises(ValueError):
        ref.project_alternate_mode2(**inputs(**{name: value}))


@pytest.mark.parametrize("changes", [
    dict(pre_scale_cc=ref.MAX_I32 + 1), dict(pre_scale_cc=ref.MIN_I32 - 1),
    dict(input_size=-1), dict(input_size=65536), dict(dy=ref.MIN_I32 - 1),
    dict(scale_y=ref.MAX_I32 + 1), dict(correction_p0=-1), dict(correction_p0=1 << 32),
    dict(metric_byte37=-1), dict(metric_byte37=256), dict(metric_byte38=0),
])
def test_invalid_ranges_and_simple_tail_are_rejected(changes):
    with pytest.raises(ValueError):
        ref.project_alternate_mode2(**inputs(**changes))


@pytest.mark.parametrize("fields", [None, {}, {"c0": 1},
    dict(c0=1, c4=2, c8=3, cc=4, d0=True),
    dict(c0=1, c4=2, c8=3, cc=ref.MAX_I32 + 1, d0=5),
    dict(c0=1, c4=2, c8=3, cc=4, d0=5, extra=6)])
def test_shared_vertical_stage_rejects_missing_or_fabricated_fields(fields):
    with pytest.raises(ValueError):
        vertical.project_vertical_fields(pre_scale=fields, raw0=1000, raw_c=0,
            input_size=24, dy=1000, scale_y=6000, correction_p0=0, metric_byte37=0)
