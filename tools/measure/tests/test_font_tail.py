import copy
from fractions import Fraction
from pathlib import Path
import random
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wordmeasure import font_tail_reference as ref


def inputs(**changes):
    result = dict(v_prefix_words=[1000, 800, 200, 77, 50, 23, 17],
                  tail_p0=0, tail_p8=100, tail_p13=8, metric_byte38=128,
                  mode=2, incoming_scale=300, dy=2048)
    result.update(changes)
    return result


def nearest(value):
    # Independent magnitude/remainder reference for bounded rational goldens.
    quotient, remainder = divmod(abs(value.numerator), value.denominator)
    magnitude = quotient + (2 * remainder >= value.denominator)
    return -magnitude if value < 0 else magnitude


def test_special_positive_split_is_preserved_before_two_separate_increments():
    result = ref.project_alternate_tail(**inputs())
    assert result["status"] == "ARITHMETIC_REFERENCE"
    assert result["redistribution"]["fields"] == dict(c0=800, c4=200, c8=800, d0=0)
    assert result["padding"]["mulDivInputs"] == [1000, 15, 100]
    assert not result["padding"]["addV10"]
    assert result["finalFields"] == dict(c0=800, c4=350, c8=950, d0=0)
    assert result["stackResult"] == 0
    assert "cc" not in result["finalFields"]
    tiny = ref.project_alternate_tail(**inputs(v_prefix_words=[10, 8, 2, 77, 999, 0, 0]))
    assert tiny["finalFields"] == dict(c0=8, c4=4, c8=10, d0=0)
    # Two rounded 1.5 increments give 14, not a single rounded 1.3 * 10.
    assert tiny["finalFields"]["c4"] + tiny["finalFields"]["c8"] == 14


@pytest.mark.parametrize("p13,selector,redistributed,final,stack", [
    (0, 0, (1, 99, 1, -79), (1, 114, 16, -79), 79),
    (4, 1, (50, 50, 50, -30), (50, 65, 65, -30), 30),
    (8, 2, (80, 20, 80, 0), (80, 35, 95, 0), 0),
    (12, 3, (95, 5, 95, 15), (95, 20, 110, 15), -15),
    (16, 2, (80, 20, 80, 0), (80, 35, 95, 0), 0),
    (20, 5, (80, 20, 80, 0), (80, 35, 95, 0), 0),
    (24, 6, (80, 20, 80, 0), (80, 35, 95, 0), 0),
    (28, 7, (80, 20, 80, 0), (80, 35, 95, 0), 0),
])
def test_all_initial_selector_goldens(p13, selector, redistributed, final, stack):
    result = ref.project_alternate_tail(**inputs(
        tail_p13=p13, v_prefix_words=[100, 80, 20, 0, 7, 0, 0]))
    assert result["gate"]["selector"] == selector
    assert tuple(result["redistribution"]["fields"][k] for k in ("c0", "c4", "c8", "d0")) == redistributed
    assert tuple(result["finalFields"][k] for k in ("c0", "c4", "c8", "d0")) == final
    assert result["stackResult"] == stack


@pytest.mark.parametrize("p13,charset,selector", [
    (0x10, 128, 2), (0x11, 128, 2), (0x90, 128, 2), (0x91, 128, 1),
    (0x91, 1, 1), (0x9d, 1, 7),
])
def test_selector_four_is_reassigned_before_optional_charset_gate(p13, charset, selector):
    result = ref.project_alternate_tail(**inputs(tail_p13=p13, metric_byte38=charset))
    assert result["gate"]["selector"] == selector


@pytest.mark.parametrize("p13", [0x08, 0x10, 0x11, 0x90])
def test_simple_gate_is_rejected_instead_of_fabricating_alternate_metrics(p13):
    with pytest.raises(ValueError, match="simple tail 1003477e8"):
        ref.project_alternate_tail(**inputs(tail_p13=p13, metric_byte38=1))


def test_every_charset_uses_exact_four_member_gate_and_table_membership():
    expected_special = {128, 129, 134, 136}
    for charset in range(256):
        args = inputs(metric_byte38=charset)
        if charset in expected_special:
            assert ref.project_alternate_tail(**args)["gate"]["specialCharset"]
        else:
            with pytest.raises(ValueError, match="simple tail"):
                ref.project_alternate_tail(**args)
        # Selector 3 reaches alternate for every charset, including JOHAB=130.
        result = ref.project_alternate_tail(**inputs(
            metric_byte38=charset, tail_p13=12,
            v_prefix_words=[100, 80, 20, 0, 7, 0, 0]))
        special = charset in expected_special
        assert result["redistribution"]["tableCoefficient"] == (750 if special else 497)
        assert result["redistribution"]["fields"] == (
            dict(c0=95, c4=5, c8=95, d0=15) if special else dict(c0=90, c4=10, c8=90, d0=10))
        assert result["padding"]["incrementI32"] == (15 if special else 0)
        assert result["padding"]["addV10"] is (not special)


@pytest.mark.parametrize("upper,lower,expected", [(12, -1, (10, 1)), (13, -1, (10, 2)), (800, 0, (715, 85)), (-2, -1, (-3, 0))])
def test_default_fallback_uses_wrapped_sum_and_strict_greater_than_eleven(upper, lower, expected):
    # Only a special charset with nonpositive V8 reaches this fallback.
    result = ref.project_alternate_tail(**inputs(
        v_prefix_words=[0, upper, lower, 0, 0, 0, 0], tail_p13=20, metric_byte38=128))
    fields = result["redistribution"]["fields"]
    assert (fields["c0"], fields["c4"]) == expected


@pytest.mark.parametrize("selector", [5, 6, 7])
@pytest.mark.parametrize("charset", [0, 1, 130])
@pytest.mark.parametrize("lower", [200, 0, -200])
def test_default_non_special_keeps_original_split_even_with_nonpositive_v8(selector, charset, lower):
    result = ref.project_alternate_tail(**inputs(
        tail_p13=selector << 2, metric_byte38=charset,
        v_prefix_words=[1000, 800, lower, 0, 50, 0, 0]))
    assert result["redistribution"]["branch"] == "preserve-original-split"
    assert result["redistribution"]["fields"] == dict(c0=800, c4=lower, c8=800, d0=0)
    assert result["finalFields"] == dict(c0=800, c4=max(1, lower), c8=850, d0=0)
    assert result["stackResult"] == 0


@pytest.mark.parametrize("lower", [0, -1])
def test_special_default_branch_requires_strictly_positive_v8(lower):
    result = ref.project_alternate_tail(**inputs(v_prefix_words=[0, 800, lower, 0, 0, 0, 0]))
    assert result["redistribution"]["branch"] == "105-per-thousand"


def test_negative_odd_half_uses_arithmetic_shift_and_floor_keeps_stack_result():
    result = ref.project_alternate_tail(**inputs(
        tail_p13=4, v_prefix_words=[0, -2, -1, 0, 0, 0, 0]))
    assert result["redistribution"]["fields"] == dict(c0=-2, c4=-1, c8=-2, d0=0)
    assert result["finalFields"] == dict(c0=-2, c4=1, c8=-2, d0=0)
    assert result["stackResult"] == 0


@pytest.mark.parametrize("mode,p0,p13,add", [
    (0, 0, 0, False), (0, 0, 1, True), (1, 0, 0, True),
    (1, 0x8000, 0, False), (1, 0x8000, 1, True),
    (2, 0, 0, True), (2, 0x8000, 0, False), (2, 0x8000, 1, True),
    (3, 0, 0, False), (3, 0x8000, 1, True),
    (2, 0x10000, 0, True),  # P0.bit16 is irrelevant here; bit15 is the gate.
])
def test_non_special_v10_addition_gate(mode, p0, p13, add):
    result = ref.project_alternate_tail(**inputs(
        mode=mode, tail_p0=p0, tail_p13=p13, metric_byte38=1))
    assert result["padding"]["addV10"] is add
    assert result["finalFields"] == dict(c0=1, c4=999, c8=51 if add else 1, d0=-799)
    assert result["stackResult"] == 799


@pytest.mark.parametrize("mode,p8,scale,minimum", [
    (0, 100, 72, 3), (1, 100, 300, 6), (2, 100, 2048, 44),
    (3, 100, 1440, 31), (3, 75, 1080, 23), (0, 65535, 47185, 1016),
])
def test_four_scale_sources_and_minimum_goldens(mode, p8, scale, minimum):
    result = ref.project_alternate_tail(**inputs(
        mode=mode, tail_p8=p8, tail_p13=0x48,
        v_prefix_words=[0, 8, 1, 0, 0, 0, 0]))
    assert result["floor"]["scaleI32"] == scale
    assert result["floor"]["minimumI32"] == minimum
    assert result["finalFields"] == dict(c0=8, c4=minimum, c8=8, d0=0)


@pytest.mark.parametrize("p8,signed,rounded", [
    (0, 0, 0), (50, 50, 2), (32767, 32767, 983),
    (32768, -32768, -983), (65486, -50, -2), (65535, -1, 0),
])
def test_minimum_reads_p8_as_signed16_and_rounds_half_away(p8, signed, rounded):
    result = ref.project_alternate_tail(**inputs(tail_p8=p8, tail_p13=0x48, dy=0))
    assert result["floor"]["signedP8"] == signed
    assert result["floor"]["p8MinimumI32"] == rounded
    assert result["floor"]["minimumI32"] == max(1, rounded)


def test_p8_bounded_helper_agrees_with_independent_rational_across_signed_domain():
    for unsigned in range(65536):
        result = ref.project_alternate_tail(**inputs(tail_p8=unsigned, tail_p13=0x48, dy=0))
        signed = unsigned if unsigned < 32768 else unsigned - 65536
        assert result["floor"]["p8MinimumI32"] == nearest(Fraction(3 * signed, 100))


@pytest.mark.parametrize("charset,target,minimum,final", [
    (128, "c8-minus-c0", 31, (8, 1, 39, 0)),
    (129, "c8-minus-c0", 40, (8, 1, 48, 0)),
    (134, "c4", 40, (8, 40, 8, 0)),
    (136, "c4", 40, (8, 40, 8, 0)),
])
def test_floor_target_depends_on_charset_and_both_p13_bits(charset, target, minimum, final):
    result = ref.project_alternate_tail(**inputs(
        metric_byte38=charset, tail_p13=0x49, dy=1440,
        v_prefix_words=[0, 8, 1, 0, 0, 0, 0]))
    assert result["floor"]["target"] == target
    assert result["floor"]["minimumI32"] == minimum
    assert tuple(result["finalFields"][k] for k in ("c0", "c4", "c8", "d0")) == final
    assert result["stackResult"] == 0


def test_w_register_add_sub_and_simd_padding_wrap_instead_of_saturating():
    half = ref.project_alternate_tail(**inputs(
        tail_p13=4, v_prefix_words=[0, ref.MAX_I32, 1, 0, 0, 0, 0]))
    assert half["redistribution"]["sumI32"] == ref.MIN_I32
    assert half["redistribution"]["fields"] == dict(
        c0=-1073741824, c4=-1073741824, c8=-1073741824, d0=1073741825)
    assert half["stackResult"] == -1073741825
    padded = ref.project_alternate_tail(**inputs(
        v_prefix_words=[ref.MAX_I32, ref.MAX_I32, 1, 0, 0, 0, 0]))
    assert padded["padding"]["incrementI32"] == 322122547
    assert padded["finalFields"] == dict(c0=ref.MAX_I32, c4=322122548, c8=-1825361102, d0=0)
    lower_wrap = ref.project_alternate_tail(**inputs(
        v_prefix_words=[ref.MAX_I32, 1, ref.MAX_I32, 0, 0, 0, 0]))
    assert lower_wrap["padding"]["fields"]["c4"] == -1825361102
    assert lower_wrap["finalFields"]["c4"] == 1
    assert lower_wrap["finalFields"]["d0"] == 0


def test_v10_addition_and_upper_floor_addition_wrap():
    gap = ref.project_alternate_tail(**inputs(
        tail_p13=0, metric_byte38=1, v_prefix_words=[0, 1, 1, 0, ref.MAX_I32, 0, 0]))
    assert gap["finalFields"] == dict(c0=1, c4=1, c8=ref.MIN_I32, d0=0)
    floor = ref.project_alternate_tail(**inputs(
        tail_p13=0x49, dy=1440, v_prefix_words=[0, ref.MAX_I32, 1, 0, 0, 0, 0]))
    assert floor["finalFields"]["c8"] == -2147483618
    assert floor["stackResult"] == 0


def test_upper_floor_compares_wrapped_difference_not_unbounded_subtraction():
    result = ref.project_alternate_tail(**inputs(
        tail_p13=0x49, tail_p8=0, dy=0,
        v_prefix_words=[10, ref.MAX_I32, 1, 0, 0, 0, 0]))
    assert result["floor"]["comparedI32"] == 2
    assert not result["floor"]["applied"]
    assert result["finalFields"]["c8"] == ref.MIN_I32 + 1


def test_redistribution_conserves_component_sum_modulo32_and_independent_difference():
    rng = random.Random(20260927)
    for _ in range(500):
        a, b = (rng.randint(ref.MIN_I32, ref.MAX_I32) for _ in range(2))
        result = ref.project_alternate_tail(**inputs(
            tail_p13=rng.choice([0, 4, 8, 12, 16, 20, 24, 28, 0x91]),
            metric_byte38=rng.choice([128, 129, 134, 136]),
            v_prefix_words=[rng.randint(ref.MIN_I32, ref.MAX_I32), a, b, 9, 7, 3, 1]))
        f = result["redistribution"]["fields"]
        assert (f["c0"] + f["c4"]) % (1 << 32) == (a + b) % (1 << 32)
        assert (f["d0"] + f["c4"]) % (1 << 32) == b % (1 << 32)
        assert (result["stackResult"] + f["c0"]) % (1 << 32) == a % (1 << 32)
        assert f["c0"] == f["c8"]


def test_model_does_not_mutate_input_or_alias_checkpoints():
    args = inputs()
    before = copy.deepcopy(args)
    result = ref.project_alternate_tail(**args)
    assert args == before
    assert result == ref.project_alternate_tail(**args)
    result["inputs"]["vPrefixWords"][0] = 0
    result["finalFields"]["c0"] = 0
    result["padding"]["fields"]["c4"] = 0
    assert args == before
    assert result["initializedFields"]["c0"] == 800
    assert result["redistribution"]["fields"] == dict(c0=800, c4=200, c8=800, d0=0)


@pytest.mark.parametrize("key,bad", [
    ("v_prefix_words", None), ("v_prefix_words", "0123456"),
    ("v_prefix_words", [0] * 6), ("v_prefix_words", [0] * 8),
    ("v_prefix_words", [True] + [0] * 6),
    ("v_prefix_words", [ref.MIN_I32 - 1] + [0] * 6),
    ("v_prefix_words", [ref.MAX_I32 + 1] + [0] * 6),
    ("v_prefix_words", [float("nan")] + [0] * 6),
    ("tail_p0", -1), ("tail_p0", 1 << 32), ("tail_p0", True),
    ("tail_p8", -1), ("tail_p8", 65536), ("tail_p8", False),
    ("tail_p13", -1), ("tail_p13", 256), ("tail_p13", 8.0),
    ("metric_byte38", -1), ("metric_byte38", 256), ("metric_byte38", True),
    ("mode", -1), ("mode", 4), ("mode", True), ("mode", 2.0),
    ("incoming_scale", ref.MIN_I32 - 1), ("incoming_scale", ref.MAX_I32 + 1),
    ("incoming_scale", False), ("incoming_scale", float("inf")),
    ("dy", ref.MIN_I32 - 1), ("dy", ref.MAX_I32 + 1), ("dy", True), ("dy", None),
])
def test_inputs_require_exact_integer_types_and_native_widths(key, bad):
    with pytest.raises(ValueError, match=key):
        ref.project_alternate_tail(**inputs(**{key: bad}))


def test_unused_words_and_modes_inputs_are_still_explicit_but_do_not_affect_other_reads():
    original = ref.project_alternate_tail(**inputs())
    changed = ref.project_alternate_tail(**inputs(
        incoming_scale=-123, v_prefix_words=(1000, 800, 200, ref.MIN_I32, 50, ref.MAX_I32, -99)))
    for key in ("gate", "initializedFields", "redistribution", "padding", "floor", "finalFields", "stackResult"):
        assert changed[key] == original[key]
