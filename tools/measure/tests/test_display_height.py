from fractions import Fraction
from pathlib import Path
import random
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wordmeasure.display_height_reference import (
    MAX_I32, MIN_I32, adjust_line_height, convert_height_at_origin, project,
)


def convert(origin, height=1, source=2, target=1):
    return convert_height_at_origin(origin=origin, height_b8=height,
                                    source_scale=source, target_scale=target)


def test_endpoint_rounding_preserves_origin_phase():
    assert [convert(z)["requestedI32"] for z in range(6)] == [1, 0, 1, 0, 1, 0]
    assert sum(convert(z)["requestedI32"] for z in range(6)) == 3
    assert 6 * convert(0)["requestedI32"] == 6


def test_endpoint_difference_against_exact_rational_coordinates():
    def nearest(value):
        magnitude = abs(value) + Fraction(1, 2)
        rounded = magnitude.numerator // magnitude.denominator
        return rounded if value >= 0 else -rounded

    rng = random.Random(1034)
    for _ in range(400):
        origin, height = rng.randrange(-1000, 1001), rng.randrange(-100, 101)
        source, target = rng.randrange(1, 100), rng.randrange(-100, 101)
        expected = (nearest(Fraction((origin + height) * target, source))
                    - nearest(Fraction(origin * target, source)))
        assert convert(origin, height, source, target)["requestedI32"] == expected


@pytest.mark.parametrize("origin,height,source,target,end,converted_end,converted_origin,requested", [
    (MAX_I32, 1, 1, 1, MIN_I32, MIN_I32, MAX_I32, 1),
    (0, MIN_I32, -1, 1, MIN_I32, MIN_I32, 0, MIN_I32),
    (-MAX_I32, MAX_I32, 1, 2, 0, 0, MIN_I32, MIN_I32),
    (1, 1, 0, 1, 2, MAX_I32, MAX_I32, 0),
])
def test_endpoint_and_difference_wrap_and_native_helper_exceptions(
        origin, height, source, target, end, converted_end, converted_origin, requested):
    assert convert(origin, height, source, target) == dict(
        endpointI32=end, convertedEndpointI32=converted_end,
        convertedOriginI32=converted_origin, requestedI32=requested)


@pytest.mark.parametrize("requested,bc,target,forwarded", [
    (120, 60, 120, [10, 50, 40, 20]),
    (90, 30, 90, [10, 50, 20, 10]),
    (75, 20, 75, [5, 50, 20, 0]),
    (60, 20, 60, [0, 40, 20, 0]),
    (-5, 20, 0, [0, -20, 20, 0]),
    (100, 40, 100, None),
])
def test_height_redistribution_keeps_component_order(requested, bc, target, forwarded):
    result = adjust_line_height(requested=requested, b4=100, bc=40, c4=10, d4=20, has_line=True)
    assert result["directStores"] == [
        {"offset": "bc", "value": bc, "phase": "beforeForward"},
        {"offset": "b4", "value": target, "phase": "afterForward"},
        {"offset": "1c0", "value": 1, "phase": "afterForward"},
    ]
    assert result["forwardedHeights"] == forwarded


def test_wrapped_subs_sign_selects_reduction_even_when_target_is_greater():
    # SUBS MAX-(-1) sets N=1,V=1: B.PL is false although signed GE is true.
    result = adjust_line_height(requested=MAX_I32, b4=-1, bc=0, c4=10, d4=20, has_line=True)
    assert result["targetDeltaI32"] == MIN_I32
    assert result["directStores"][0]["value"] == -20
    assert result["forwardedHeights"] == [10, -2147483639, 2147483608, -2147483628]


def test_negative_request_changes_bc_before_clamp_and_equal_target_skips_forwarding():
    result = adjust_line_height(requested=-5, b4=0, bc=20, c4=3, d4=50, has_line=True)
    assert result["directStores"][0]["value"] == 15
    assert result["targetI32"] == 0
    assert result["forwardedHeights"] is None


def test_missing_line_still_changes_direct_fields():
    result = adjust_line_height(requested=20, b4=10, bc=3, c4=4, d4=5, has_line=False)
    assert [item["value"] for item in result["directStores"]] == [13, 20, 1]
    assert result["forwardedHeights"] is None


def test_minimum_d4_negation_wraps_before_signed_comparison():
    result = adjust_line_height(requested=1, b4=0, bc=0, c4=0, d4=MIN_I32, has_line=True)
    assert result["bcChangeI32"] == 1
    assert result["forwardedHeights"] == [0, 0, -MAX_I32, MIN_I32]


@pytest.mark.parametrize("requested,b4,bc,c4,d4,expected_bc,forwarded", [
    (0, 1, 0, 0, MIN_I32, -1, [MAX_I32, -2147483646, -1, 0]),
    (MIN_I32, MAX_I32, 0, 10, 20, 1, [0, -1, 1, 0]),
    (1, 0, MAX_I32, 0, 1, MIN_I32, [0, -2147483647, MAX_I32, 1]),
])
def test_independent_instruction_boundary_vectors(requested, b4, bc, c4, d4, expected_bc, forwarded):
    result = adjust_line_height(requested=requested, b4=b4, bc=bc, c4=c4, d4=d4, has_line=True)
    assert result["directStores"][0]["value"] == expected_bc
    assert result["forwardedHeights"] == forwarded


def test_forwarded_components_sum_to_target_modulo_32_bits():
    rng = random.Random(34941)
    for _ in range(400):
        values = [rng.randint(MIN_I32, MAX_I32) for _ in range(5)]
        result = adjust_line_height(**dict(zip(("requested", "b4", "bc", "c4", "d4"), values)),
                                    has_line=True)
        if result["forwardedHeights"] is not None:
            assert sum(result["forwardedHeights"]) & 0xffffffff == result["targetI32"]


def test_composition_keeps_source_b8_separate_from_destination_b4():
    conversion = dict(origin=1, height_b8=1, source_scale=2, target_scale=1)
    record = dict(b4=7, bc=4, c4=2, d4=3)
    result = project(conversion=conversion, record=record, has_line=True)
    assert result["conversion"]["requestedI32"] == 0
    assert result["adjustment"]["initialDeltaI32"] == -7
    assert result["adjustment"]["forwardedHeights"] == [0, -1, 1, 0]
    assert record == dict(b4=7, bc=4, c4=2, d4=3)
    assert conversion["height_b8"] == 1


@pytest.mark.parametrize("field", ["origin", "height_b8", "source_scale", "target_scale"])
@pytest.mark.parametrize("invalid", [True, 1.0, None, MIN_I32 - 1, MAX_I32 + 1])
def test_conversion_rejects_non_i32(field, invalid):
    values = dict(origin=1, height_b8=1, source_scale=2, target_scale=1)
    values[field] = invalid
    with pytest.raises(ValueError):
        convert_height_at_origin(**values)


@pytest.mark.parametrize("field", ["requested", "b4", "bc", "c4", "d4"])
@pytest.mark.parametrize("invalid", [False, 0.5, "0", MIN_I32 - 1, MAX_I32 + 1])
def test_adjustment_rejects_non_i32(field, invalid):
    values = dict(requested=1, b4=1, bc=0, c4=0, d4=0, has_line=True)
    values[field] = invalid
    with pytest.raises(ValueError):
        adjust_line_height(**values)


@pytest.mark.parametrize("invalid", [0, 1, None, "true"])
def test_line_presence_requires_explicit_bool(invalid):
    with pytest.raises(ValueError):
        adjust_line_height(requested=1, b4=1, bc=0, c4=0, d4=0, has_line=invalid)
