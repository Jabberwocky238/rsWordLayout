import copy
from fractions import Fraction
import itertools
from pathlib import Path
import random
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wordmeasure import font_adjustment_reference as ref


def inputs(**changes):
    values = {offset: 10 for offset in ref.FIELDS}
    values.update(c0=100, c8=120, c4=30, cc=20, d0=-5, **{"184": 77})
    result = dict(pre_scale=values, scale_x=300, scale_y=600, h2=24,
                  dx=1000, dy=2000, initial_m=[11, 22, 33, 44, 55, -66], c8_correction=False)
    result.update(changes)
    return result


@pytest.mark.parametrize("a,b,c,expected", [
    (1, 1, 2, 1), (-1, 1, 2, -1), (1, 1, -2, -1), (-1, 1, -2, 1),
    (7, 1, 3, 2), (-7, 1, 3, -2), (0, 0, 0, ref.MAX_I32),
    (0, 5, 3, 0), (-123, -7, -7, -123),
    (ref.MAX_I32, ref.MAX_I32, 1, ref.MAX_I32),
    (ref.MIN_I32, ref.MAX_I32, 1, ref.MIN_I32),
    (ref.MIN_I32, 1, -1, ref.MIN_I32),
    (1073741824, -2, -1, ref.MIN_I32),
])
def test_n_helper_halfway_shortcuts_saturation_and_fast_overflow(a, b, c, expected):
    assert ref.native_muldiv(a, b, c) == expected


def test_n_helper_matches_independent_rational_reference_on_signed_edges():
    # Independent magnitude/remainder rounding, not the native biased numerator.
    edges = [ref.MIN_I32, ref.MIN_I32 + 1, -144, -3, -1, 0, 1, 2, 144, ref.MAX_I32]
    cases = list(itertools.product(edges, repeat=3))
    rng = random.Random(20260927)
    cases += [tuple(rng.randint(ref.MIN_I32, ref.MAX_I32) for _ in range(3)) for _ in range(1000)]
    for a, b, c in cases:
        if c == 0:
            expected = ref.MAX_I32
        else:
            exact = Fraction(a * b, c)
            q, r = divmod(abs(exact.numerator), exact.denominator)
            q += 2 * r >= exact.denominator
            expected = -q if exact < 0 else q
            if a * b == ref.MIN_I32 and c == -1:
                expected = ref.MIN_I32
            expected = min(ref.MAX_I32, max(ref.MIN_I32, expected))
        assert ref.native_muldiv(a, b, c) == expected, (a, b, c)


def test_axes_round_separately_then_default_m_overwrites_only_four_words():
    arguments = inputs()
    before = copy.deepcopy(arguments)
    result = ref.project_mode2(**arguments)
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert [row["offset"] for row in result["conversions"]] == list(ref.VERTICAL + ref.HORIZONTAL)
    assert result["scaledFields"]["c8"] == 6  # 600 * (120*24) / (2000*144).
    assert result["scaledFields"]["c4"] == 2  # 1.5 rounds away from zero.
    assert result["scaledFields"]["180"] == 1  # Separate horizontal half-way.
    assert result["scaledFields"]["184"] == 77
    assert result["updatedMWords"] == [6, 2, 8, 44, 77, -66]
    assert result["copiedContextFields"]["188"] == 300
    assert result["copiedContextFields"]["18c"] == 600
    assert "28c" not in result["copiedContextFields"]
    assert arguments == before


@pytest.mark.parametrize("scale,increment", [(600, 16), (-600, -16), (35, 0), (-35, 0), (36, 1), (-36, -1)])
def test_explicit_c8_correction_truncates_signed_scale_over_36(scale, increment):
    result = ref.project_mode2(**inputs(scale_y=scale, c8_correction=True))
    assert result["c8IncrementI32"] == increment
    assert result["afterCorrectionFields"]["c8"] == result["scaledFields"]["c8"] + increment
    assert result["updatedMWords"][0] == result["afterCorrectionFields"]["c8"]
    assert result["updatedMWords"][1] == result["scaledFields"]["c4"]


def test_multiplications_wrap_before_n_instead_of_using_unbounded_ratio():
    arguments = inputs(scale_y=1, h2=2, dy=1)
    arguments["pre_scale"]["c8"] = ref.MAX_I32
    result = ref.project_mode2(**arguments)
    row = next(row for row in result["conversions"] if row["offset"] == "c8")
    assert row["productI32"] == -2 and row["resultI32"] == 0
    arguments.update(h2=1, dy=1 << 28)
    result = ref.project_mode2(**arguments)
    assert result["conversions"][0]["denominatorI32"] == 0
    assert result["scaledFields"]["c8"] == ref.MAX_I32


def test_correction_and_final_component_sum_wrap_independently():
    arguments = inputs(dy=0, scale_y=36, c8_correction=True)
    result = ref.project_mode2(**arguments)
    assert result["scaledFields"]["c8"] == ref.MAX_I32
    assert result["afterCorrectionFields"]["c8"] == ref.MIN_I32
    assert result["updatedMWords"][:3] == [ref.MIN_I32, ref.MAX_I32, -1]
    uncorrected = ref.project_mode2(**inputs(dy=0))
    assert uncorrected["updatedMWords"][2] == -2


@pytest.mark.parametrize("raw0,raw_c,size,denom,diff,rounded,h2", [
    (2200, 200, 24, 2000, 2000, 24, 24),
    (5, 0, 1, 2, 5, 3, 3),
    (0, 0, 24, 1000, 0, 0, 1),
    (1, 0, 65535, 1, 1, 65535, 3276),
    (-1, 0, 24, 1, -1, -24, 3276),
    (1, 0, 24, 0, 1, ref.MAX_I32, 3276),
    (ref.MIN_I32, 1, 1, ref.MAX_I32, ref.MAX_I32, 1, 1),
])
def test_h2_producer_has_independent_difference_rounding_and_unsigned_clamp(raw0, raw_c, size, denom, diff, rounded, h2):
    result = ref.produce_mode2_h2(raw0=raw0, raw_c=raw_c, input_size=size, denominator=denom)
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["rawDifferenceI32"] == diff
    assert result["roundedI32"] == rounded
    assert result["roundedU32"] == rounded & 0xffffffff
    assert result["h2"] == h2


@pytest.mark.parametrize("change", [
    {"scale_x": True}, {"scale_y": 1.0}, {"dx": None}, {"dy": ref.MAX_I32 + 1},
    {"dy": ref.MIN_I32 - 1}, {"h2": False}, {"h2": -1}, {"h2": 65536},
    {"h2": float("nan")}, {"c8_correction": 1}, {"c8_correction": None},
    {"initial_m": [1] * 5}, {"initial_m": [0, 1, 2, True, 4, 5]},
    {"initial_m": [0, 1, 2, 3, float("inf"), 5]}, {"pre_scale": None},
    {"pre_scale": {offset: 1 for offset in ref.FIELDS if offset != "184"}},
    {"pre_scale": dict.fromkeys(ref.FIELDS + ("196",), 1)},
    {"pre_scale": dict.fromkeys(ref.FIELDS, True)},
])
def test_projection_rejects_ambiguous_types_and_incomplete_inputs(change):
    with pytest.raises(ValueError):
        ref.project_mode2(**inputs(**change))


@pytest.mark.parametrize("change", [{"raw0": True}, {"raw_c": 2.0}, {"input_size": -1},
                                    {"input_size": 65536}, {"denominator": 1 << 31}])
def test_h2_producer_requires_explicit_typed_inputs(change):
    args = dict(raw0=2048, raw_c=0, input_size=24, denominator=2048)
    args.update(change)
    with pytest.raises(ValueError):
        ref.produce_mode2_h2(**args)


@pytest.mark.parametrize("values", [(True, 1, 2), (1, 2.0, 3), (1, 2, float("nan")),
                                    (1 << 31, 1, 1), (1, 1, -(1 << 31) - 1)])
def test_n_helper_validates_public_inputs(values):
    with pytest.raises(ValueError):
        ref.native_muldiv(*values)
