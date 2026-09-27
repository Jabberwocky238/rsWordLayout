"""The pinned PTLS SimpleW fit decision, with explicit native i32 inputs.

Evidence: artifacts/docgrid-pts-region-source-2026-09-27/simple-line.txt
0x1920c4..0x192138 and simple-results.txt 0x18ada0..0x18adb4 / 0x190208.
That bundle's SHA256SUMS is
5a720498d3c8639e842f00be73907e6569bbf17249b0fe4b9776fe3d175d46ca.

U/V are the values after any ChangeSplat adjustment; S is the successful,
validated callback value, or the explicit zero from the no-query path.
No units, R.f8 value, host flags, callback result or font metric is inferred.
This does not simulate callback failure, construction failure, prior host
mutations, or the outer Story state machine. KEEP/CLIP mean only that this
local fit decision permits proceeding to line construction.
"""

from dataclasses import dataclass, fields, replace

MIN_I32, MAX_I32 = -(1 << 31), (1 << 31) - 1
MIN_S, MAX_S = -0x3fffffff, 0x3fffffff
STATUS = "ARITHMETIC_REFERENCE"


@dataclass(frozen=True)
class Inputs:
    u: int
    v: int
    s: int
    limit: int


@dataclass(frozen=True)
class Decision:
    decision: str
    inputs: Inputs
    sum_i32: int
    # A native comparison operand, not measured ink or physical occupied height.
    required_i32: int
    # The wrapped SUBS result passed to the overhang callback, not its flags.
    excess_i32: int
    final_u: int | None = None
    final_v: int | None = None
    advance_i32: int | None = None
    allow_overhang: bool | None = None


def _integer(value, low, high, name):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")


def _i32(value):
    return ((value + (1 << 31)) & 0xffffffff) - (1 << 31)


def classify(u, v, s, limit):
    """Return REJECT, KEEP, or NEEDS_OVERHANG without choosing host policy.

    0x18ada4..b4 checks unsigned(s - 0x40000000) >= 0x80000001 after
    callback success, equivalent to the inclusive MIN_S..MAX_S interval.
    Other inputs are only restricted to their read-point signed i32 type.
    """
    for name, value in (("u", u), ("v", v), ("limit", limit)):
        _integer(value, MIN_I32, MAX_I32, name)
    _integer(s, MIN_S, MAX_S, "s")
    total = _i32(u + v)
    required = _i32(total - s)
    excess = _i32(total - limit)

    # SUBS sets N/Z/V for signed total <= limit, even when excess wraps.
    # CCMP compares required only under LE, otherwise NZCV=0 makes LE false.
    # The following CMP independently rejects required > limit.
    if required > limit:
        kind = "REJECT"
    elif total <= limit:
        kind = "KEEP"
    else:
        kind = "NEEDS_OVERHANG"
    kept = kind == "KEEP"
    return Decision(kind, Inputs(u, v, s, limit), total, required, excess,
                    u if kept else None, v if kept else None,
                    total if kept else None)


def resolve(decision, allow):
    """Resolve only an unmodified NEEDS_OVERHANG decision with explicit bool.

    The caller supplies the successful callback's boolean interpretation.
    No callback is executed. The native denial path reloads U at 0x192130;
    this projection assumes no intervening callback changes to the inputs.
    Denial preserves U and replaces V with
    wrap32(limit-U); this can be negative. The resulting wrapped advance is
    limit, conditional on later line construction/retention succeeding.
    """
    if type(allow) is not bool:
        raise ValueError("allow must be an explicit bool")
    if type(decision) is not Decision or type(decision.inputs) is not Inputs:
        raise ValueError("decision must be returned by classify")
    inputs = decision.inputs
    expected = classify(inputs.u, inputs.v, inputs.s, inputs.limit)
    for field in fields(Decision):
        actual_value, expected_value = getattr(decision, field.name), getattr(expected, field.name)
        if type(actual_value) is not type(expected_value) or actual_value != expected_value:
            raise ValueError("decision does not match classify for its inputs")
    if expected.decision != "NEEDS_OVERHANG":
        raise ValueError("only NEEDS_OVERHANG decisions require resolution")
    final_v = inputs.v if allow else _i32(inputs.limit - inputs.u)
    return replace(expected, decision="KEEP" if allow else "CLIP",
                   final_u=inputs.u, final_v=final_v,
                   advance_i32=_i32(inputs.u + final_v), allow_overhang=allow)
