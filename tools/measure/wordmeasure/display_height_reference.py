"""Explicit host display-height arithmetic; no Word execution or unit mapping.

The pinned host uses 1034de750..784 to convert two absolute coordinates
with 100064708, then passes their wrapped difference to 1034941c0. Inputs
are the values at these read points, after earlier conditional mutations.
R.b8 (the source height) and R.b4 (the old destination height) are distinct.

The adjustment reports direct stores and arguments to 100379ef4, whose
LsModifyLineHeight / optional LsModifyDisplayLineHeight effects are outside
this model. In particular, it does not store the forwarded c4/d4 values
back to R, or claim that R.bc remains unchanged by the forwarded call.
"""

from .font_adjustment_reference import native_muldiv

MIN_I32, MAX_I32 = -(1 << 31), (1 << 31) - 1
STATUS = "ARITHMETIC_REFERENCE"


def _integer(value, name):
    if type(value) is not int or not MIN_I32 <= value <= MAX_I32:
        raise ValueError(f"{name} must be an integer in [{MIN_I32}, {MAX_I32}]")


def _i32(value):
    return ((value + (1 << 31)) & 0xffffffff) - (1 << 31)


def convert_height_at_origin(*, origin, height_b8, source_scale, target_scale):
    """N(W(origin + R.b8), target, source) - N(origin, target, source).

    The caller has already selected the scale axis. No direction, physical
    unit, display mode, or scale value is inferred from a DOCX or font.
    """
    for name, value in (("origin", origin), ("height_b8", height_b8),
                        ("source_scale", source_scale), ("target_scale", target_scale)):
        _integer(value, name)
    end = _i32(origin + height_b8)
    converted_end = native_muldiv(end, target_scale, source_scale)
    converted_origin = native_muldiv(origin, target_scale, source_scale)
    return dict(endpointI32=end, convertedEndpointI32=converted_end,
                convertedOriginI32=converted_origin,
                requestedI32=_i32(converted_end - converted_origin))


def adjust_line_height(*, requested, b4, bc, c4, d4, has_line):
    """1034941c0's direct operations with stable read-point record inputs.

    has_line is the explicit nonnull result from the normal R.ptr[1e0]
    getter. Assertion paths and native callee side effects are not run.
    The first bc store happens even when there is no line or no forwarding.
    """
    for name, value in (("requested", requested), ("b4", b4), ("bc", bc),
                        ("c4", c4), ("d4", d4)):
        _integer(value, name)
    if type(has_line) is not bool:
        raise ValueError("has_line must be an explicit bool")

    initial_delta = _i32(requested - b4)
    bc_change = max(initial_delta, _i32(-d4))
    bc_before_forward = _i32(bc + bc_change)
    target = max(requested, 0)
    delta = _i32(target - b4)
    forwarded = None
    if has_line and delta != 0:
        if delta < 0:
            # B.PL tests the wrapped SUBS sign, not signed target >= b4.
            wanted = _i32(-delta)
            from_d4 = min(d4, wanted)
            d4 = _i32(d4 - from_d4)
            remainder = _i32(wanted - from_d4)
            c4 = _i32(c4 - min(remainder, c4))
        forwarded = [c4, _i32(target - _i32(c4 + bc_before_forward)),
                     _i32(bc_before_forward - d4), d4]

    return dict(initialDeltaI32=initial_delta, bcChangeI32=bc_change,
                targetI32=target, targetDeltaI32=delta,
                directStores=[dict(offset="bc", value=bc_before_forward, phase="beforeForward"),
                              dict(offset="b4", value=target, phase="afterForward"),
                              dict(offset="1c0", value=1, phase="afterForward")],
                forwardedHeights=forwarded)


def project(*, conversion, record, has_line):
    """Compose only the captured endpoint conversion and local adjustment."""
    if not isinstance(conversion, dict) or set(conversion) != {
            "origin", "height_b8", "source_scale", "target_scale"}:
        raise ValueError("conversion must contain origin, height_b8, source_scale, target_scale")
    if not isinstance(record, dict) or set(record) != {"b4", "bc", "c4", "d4"}:
        raise ValueError("record must contain b4, bc, c4, d4")
    converted = convert_height_at_origin(**conversion)
    adjusted = adjust_line_height(requested=converted["requestedI32"], **record, has_line=has_line)
    return dict(status=STATUS, conversion=converted, adjustment=adjusted)
