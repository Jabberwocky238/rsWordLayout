from dataclasses import FrozenInstanceError, asdict, replace
import itertools
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wordmeasure import simple_fit_reference as ref


@pytest.mark.parametrize("u,v,s,limit,kind", [
    (70, 30, 0, 100, "KEEP"),
    (70, 30, 0, 99, "REJECT"),
    (70, 30, 20, 80, "NEEDS_OVERHANG"),
    (70, 30, 20, 79, "REJECT"),
    (70, 30, 20, 100, "KEEP"),
    (70, 30, -20, 110, "REJECT"),
    (70, 30, -20, 120, "KEEP"),
    (0, 0, 0, 0, "KEEP"),
    (-10, -20, 0, -30, "KEEP"),
    (-10, -20, 0, -31, "REJECT"),
])
def test_local_fit_boundaries_include_negative_s_and_signed_inputs(u, v, s, limit, kind):
    result = ref.classify(u, v, s, limit)
    assert result.decision == kind
    assert result.allow_overhang is None
    assert asdict(result.inputs) == dict(u=u, v=v, s=s, limit=limit)
    if kind == "KEEP":
        assert (result.final_u, result.final_v, result.advance_i32) == (u, v, u + v)
    else:
        assert (result.final_u, result.final_v, result.advance_i32) == (None, None, None)


def test_overhang_remains_unknown_until_resolved_and_does_not_mutate_input():
    pending = ref.classify(70, 30, 20, 80)
    before = asdict(pending)
    kept = ref.resolve(pending, True)
    clipped = ref.resolve(pending, False)
    assert (kept.decision, kept.final_u, kept.final_v, kept.advance_i32) == ("KEEP", 70, 30, 100)
    assert (clipped.decision, clipped.final_u, clipped.final_v, clipped.advance_i32) == ("CLIP", 70, 10, 80)
    assert kept.allow_overhang is True and clipped.allow_overhang is False
    assert kept.required_i32 == clipped.required_i32 == 80
    assert kept.excess_i32 == clipped.excess_i32 == 20
    assert asdict(pending) == before
    with pytest.raises(FrozenInstanceError):
        pending.inputs.u = 1
    with pytest.raises(FrozenInstanceError):
        pending.decision = "KEEP"


def test_denied_overhang_can_make_v_negative_without_clamping_u():
    result = ref.resolve(ref.classify(100, 20, 40, 90), False)
    assert result.final_u == 100
    assert result.final_v == -10
    assert result.advance_i32 == 90
    assert result.required_i32 == 80


def test_wrapped_excess_sign_is_not_the_signed_comparison_condition():
    kept = ref.classify(ref.MIN_I32, 0, 0, ref.MAX_I32)
    assert kept.excess_i32 == 1
    assert kept.decision == "KEEP"
    pending = ref.classify(ref.MAX_I32, 0, -1, -1)
    assert pending.sum_i32 == ref.MAX_I32
    assert pending.required_i32 == pending.excess_i32 == ref.MIN_I32
    assert pending.decision == "NEEDS_OVERHANG"
    clipped = ref.resolve(pending, False)
    assert (clipped.final_u, clipped.final_v, clipped.advance_i32) == (ref.MAX_I32, ref.MIN_I32, -1)


def test_sum_and_required_operands_wrap_before_their_comparisons():
    result = ref.classify(ref.MAX_I32, 1, 0, 0)
    assert result.sum_i32 == result.required_i32 == ref.MIN_I32
    assert result.decision == "KEEP" and result.advance_i32 == ref.MIN_I32
    result = ref.classify(ref.MIN_I32, 0, 1, 0)
    assert result.required_i32 == ref.MAX_I32
    assert result.decision == "REJECT"


def _sub_flags(a, b):
    # Independent ARM32-bit subtraction flags; LE means Z || N != V.
    left, right = a & 0xffffffff, b & 0xffffffff
    result = (left - right) & 0xffffffff
    n = bool(result & 0x80000000)
    z = result == 0
    overflow = bool((left ^ right) & (left ^ result) & 0x80000000)
    return n, z, overflow


def _le(flags):
    n, z, overflow = flags
    return z or n != overflow


def test_classification_matches_subs_ccmp_cmp_flags_at_signed_edges():
    edges = [ref.MIN_I32, ref.MIN_I32 + 1, -1, 0, 1, ref.MAX_I32 - 1, ref.MAX_I32]
    for u, v, s, limit in itertools.product(edges, edges, [ref.MIN_S, -1, 0, 1, ref.MAX_S], edges):
        total = (u + v) & 0xffffffff
        required = (total - s) & 0xffffffff
        first = _sub_flags(total, limit)
        conditional = _sub_flags(required, limit) if _le(first) else (False, False, False)
        if _le(conditional):
            expected = "KEEP"
        elif _le(_sub_flags(required, limit)):
            expected = "NEEDS_OVERHANG"
        else:
            expected = "REJECT"
        assert ref.classify(u, v, s, limit).decision == expected, (u, v, s, limit)


@pytest.mark.parametrize("s", [ref.MIN_S, -1, 0, 1, ref.MAX_S])
def test_s_wrapper_inclusive_range_accepts_signed_values(s):
    assert ((s - 0x40000000) & 0xffffffff) >= 0x80000001
    assert ref.classify(0, 0, s, 0).inputs.s == s


@pytest.mark.parametrize("s", [ref.MIN_I32, ref.MIN_S - 1, ref.MAX_S + 1, ref.MAX_I32])
def test_s_wrapper_rejects_outside_range_instead_of_clamping(s):
    assert ((s - 0x40000000) & 0xffffffff) < 0x80000001
    with pytest.raises(ValueError, match="s must be an integer"):
        ref.classify(0, 0, s, 0)


@pytest.mark.parametrize("name", ["u", "v", "s", "limit"])
@pytest.mark.parametrize("bad", [True, False, None, 1.0, float("nan"), "1", ref.MIN_I32 - 1, ref.MAX_I32 + 1])
def test_classify_requires_explicit_native_integers(name, bad):
    inputs = dict(u=70, v=30, s=20, limit=80)
    inputs[name] = bad
    with pytest.raises(ValueError):
        ref.classify(**inputs)


@pytest.mark.parametrize("allow", [None, 0, 1, 1.0, "false", [], {}])
def test_resolve_requires_boolean_policy(allow):
    with pytest.raises(ValueError, match="explicit bool"):
        ref.resolve(ref.classify(70, 30, 20, 80), allow)


@pytest.mark.parametrize("value", [None, {}, "NEEDS_OVERHANG"])
def test_resolve_does_not_accept_untyped_decisions(value):
    with pytest.raises(ValueError):
        ref.resolve(value, False)


def test_resolve_rejects_final_decisions_and_modified_derived_fields():
    pending = ref.classify(70, 30, 20, 80)
    invalid = [ref.classify(70, 30, 0, 100), ref.classify(70, 30, 0, 99),
               ref.resolve(pending, False), ref.resolve(pending, True),
               replace(pending, sum_i32=99), replace(pending, required_i32=79),
               replace(pending, excess_i32=0), replace(pending, final_v=10),
               replace(pending, advance_i32=80), replace(pending, allow_overhang=False),
               replace(pending, inputs=asdict(pending.inputs)),
               replace(pending, inputs=replace(pending.inputs, u=True))]
    for decision in invalid:
        with pytest.raises(ValueError):
            ref.resolve(decision, False)
    one = ref.classify(1, 0, 1, 0)
    with pytest.raises(ValueError):
        ref.resolve(replace(one, sum_i32=True), False)
