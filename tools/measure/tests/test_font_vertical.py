import copy
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wordmeasure import font_record_reference as record
from wordmeasure import font_vertical_reference as ref


def inputs(**changes):
    result = dict(t_prefix_words=[100, 70, 30, 20, 10, 5, 5], tail_p0=0,
                  tail_p13=8, metric_byte38=0, input_size=144, dy=80,
                  scale_y=80, correction_p0=0, metric_byte37=0)
    result.update(changes)
    return result


@pytest.mark.parametrize("p13,allowed", [
    (0, False), (4, False), (8, True), (9, True), (0x10, True),
    (0x11, True), (0x12, True), (0x13, True), (0x88, True),
    (0x90, True), (0x91, False), (0x92, True), (0x93, False),
    (0x1c, False), (0xff, False),
])
def test_selector_four_signed_byte_and_bit_zero_exception(p13, allowed):
    if allowed:
        result = ref.project_simple_mode2(**inputs(tail_p13=p13))
        assert result["tailGate"]["branch"] == "1003477e8"
    else:
        with pytest.raises(ValueError, match="unsupported alternate"):
            ref.project_simple_mode2(**inputs(tail_p13=p13))


def test_charset_gate_checks_all_byte_values_against_literal_membership():
    for byte in range(256):
        if byte in {0x80, 0x81, 0x86, 0x88}:
            with pytest.raises(ValueError, match="unsupported alternate"):
                ref.project_simple_mode2(**inputs(metric_byte38=byte))
        else:
            result = ref.project_simple_mode2(**inputs(metric_byte38=byte))
            assert result["tailGate"]["charsetAlternate"] is False


@pytest.mark.parametrize("p0,c0,c8,subtract,add", [
    (0, 70, 80, False, True), (1 << 15, 70, 70, False, False),
    (1 << 16, 50, 50, True, False), (3 << 15, 50, 50, True, False),
])
def test_unaligned_halfword_gate_shares_p0_bits_with_subtraction(p0, c0, c8, subtract, add):
    arguments = inputs(tail_p0=p0)
    before = copy.deepcopy(arguments)
    result = ref.project_simple_mode2(**arguments)
    assert result["tailArithmetic"] == dict(subtractVc=subtract, addV10=add)
    assert result["preScaleFields"] == dict(c0=c0, c8=c8, c4=30, cc=20, d0=0)
    assert result["h2Result"]["h2"] == 144
    assert result["scaledFields"] == result["preScaleFields"]
    assert result["updatedMetricWords"] == [c8, 30, c8 + 30]
    assert [row["offset"] for row in result["conversions"]] == list(ref.VERTICAL)
    assert arguments == before


@pytest.mark.parametrize("p0,byte,enabled", [
    (0, 1, False), (1 << 15, 1, False), (1 << 16, 0, False),
    (1 << 16, 2, False), (1 << 16, 1, True), (1 << 16, 255, True),
    (3 << 15, 1, False),
])
def test_later_correction_word_is_an_independent_read_point(p0, byte, enabled):
    result = ref.project_simple_mode2(**inputs(correction_p0=p0, metric_byte37=byte))
    assert result["tailArithmetic"] == dict(subtractVc=False, addV10=True)
    assert result["c8Correction"] is enabled
    assert result["c8IncrementI32"] == (2 if enabled else 0)
    assert result["updatedMetricWords"] == ([82, 30, 112] if enabled else [80, 30, 110])


@pytest.mark.parametrize("scale,increment", [(35, 0), (36, 1), (-35, 0), (-36, -1), (-80, -2)])
def test_correction_truncates_signed_scale_towards_zero(scale, increment):
    result = ref.project_simple_mode2(**inputs(scale_y=scale, correction_p0=1 << 16, metric_byte37=1))
    assert result["c8IncrementI32"] == increment
    assert result["afterCorrectionFields"]["c8"] == result["scaledFields"]["c8"] + increment


def test_wrap_is_applied_to_tail_difference_and_addition():
    maximum, minimum = ref.MAX_I32, ref.MIN_I32
    added = ref.project_simple_mode2(**inputs(t_prefix_words=[100, maximum, 30, 20, 1, 5, 5]))
    assert added["preScaleFields"]["c8"] == minimum
    subtracted = ref.project_simple_mode2(**inputs(
        t_prefix_words=[100, minimum, 30, 1, 10, 5, 5], tail_p0=1 << 16))
    assert subtracted["preScaleFields"]["c0"] == maximum
    assert subtracted["preScaleFields"]["c8"] == maximum


def test_h2_uses_wrapped_t_difference_and_unsigned_clamp():
    result = ref.project_simple_mode2(**inputs(t_prefix_words=[ref.MIN_I32, 70, 30, 1, 10, 5, 5],
                                              input_size=0))
    assert result["h2Result"]["rawDifferenceI32"] == ref.MAX_I32
    assert result["h2Result"]["h2"] == 1
    result = ref.project_simple_mode2(**inputs(t_prefix_words=[0, 70, 30, 80, 10, 5, 5],
                                              input_size=1))
    assert result["h2Result"]["roundedI32"] == -1
    assert result["h2Result"]["h2"] == 3276


def test_scaler_product_and_denominator_wrap_before_muldiv():
    result = ref.project_simple_mode2(**inputs(t_prefix_words=[100, ref.MAX_I32, 30, 20, 0, 5, 5],
                                              input_size=2, scale_y=1))
    first = result["conversions"][0]
    assert first["productI32"] == -2
    assert first["denominatorI32"] == 11520
    assert first["resultI32"] == 0
    result = ref.project_simple_mode2(**inputs(dy=1 << 28))
    assert result["conversions"][0]["denominatorI32"] == 0
    assert set(result["scaledFields"].values()) == {ref.MAX_I32}


def test_zero_denominator_correction_and_sum_wrap_without_full_m_claim():
    result = ref.project_simple_mode2(**inputs(dy=0, scale_y=36,
                                              correction_p0=1 << 16, metric_byte37=1))
    assert result["h2Result"]["h2"] == 3276
    assert result["scaledFields"]["c8"] == ref.MAX_I32
    assert result["updatedMetricWords"] == [ref.MIN_I32, ref.MAX_I32, -1]
    assert "updatedMWords" not in result and "initialMWords" not in result
    assert set(result["preScaleFields"]) == set(ref.VERTICAL)


def test_provider_projection_connects_to_updated_vertical_words_not_initial_m():
    metrics = {name: 0 for name in record.UNSIGNED + record.SIGNED + ["hasTypographicMetrics"]}
    metrics.update(designUnitsPerEm=2048, ascent=1825, descent=443, lineGap=87)
    source = record.project_face1(initialized=metrics, requested=None, lf_height=-2048,
                                  xavg_width=821, width_scale=1, escapement=0)
    result = ref.project_simple_mode2(**inputs(t_prefix_words=source["tPrefixWords"],
                                              input_size=24, dy=2048, scale_y=294912))
    assert source["initialMWords"] == [1825, 443, 2268, 87, 821, 0]
    assert result["preScaleFields"] == dict(c0=1825, c8=1912, c4=443, cc=220, d0=0)
    assert result["h2Result"]["h2"] == 24
    assert result["updatedMetricWords"] == [45888, 10632, 56520]
    assert result["updatedMetricWords"] != source["initialMWords"][:3]


@pytest.mark.parametrize("name", ["tail_p0", "tail_p13", "metric_byte38", "input_size", "dy",
                                  "scale_y", "correction_p0", "metric_byte37"])
@pytest.mark.parametrize("bad", [True, None, 1.0, "1"])
def test_all_scalar_inputs_are_strict_integers(name, bad):
    with pytest.raises(ValueError):
        ref.project_simple_mode2(**inputs(**{name: bad}))


@pytest.mark.parametrize("name,bad", [
    ("tail_p0", -1), ("correction_p0", 1 << 32), ("tail_p13", 256),
    ("metric_byte38", -1), ("metric_byte37", 256), ("input_size", -1),
    ("input_size", 65536), ("dy", ref.MIN_I32 - 1), ("scale_y", ref.MAX_I32 + 1),
    ("t_prefix_words", None), ("t_prefix_words", [0] * 6), ("t_prefix_words", [0] * 8),
    ("t_prefix_words", [True] * 7), ("t_prefix_words", [ref.MAX_I32 + 1] * 7),
])
def test_input_ranges_and_prefix_shape_are_not_silently_coerced(name, bad):
    with pytest.raises(ValueError):
        ref.project_simple_mode2(**inputs(**{name: bad}))
