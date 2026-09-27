"""Explicit 347770 -> alternate dcac/dec4 arithmetic, without native calls.

Evidence: artifacts/font-vertical-alternate-tail-2026-09-27/README.md.
Inputs are stable values at these read points, not inferred Word properties.
The result stops before opaque 3482c0, scaling, wrapper updates, and LS.
"""

from .font_adjustment_reference import native_muldiv

MIN_I32, MAX_I32 = -(1 << 31), (1 << 31) - 1
SPECIAL = frozenset((0x80, 0x81, 0x86, 0x88))
SELECTOR_TABLE = (750, 750, 497, 497, 497, 497, 750, 497, 750)
STATUS = "ARITHMETIC_REFERENCE"


def _integer(value, low, high, name):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")


def _i32(value):
    return ((value + (1 << 31)) & 0xffffffff) - (1 << 31)


def project_alternate_tail(*, v_prefix_words, tail_p0, tail_p8, tail_p13,
                           metric_byte38, mode, incoming_scale, dy):
    """Initialize F, select the alternate tail, and return its four F fields.

    v_prefix_words is V[0,4,8,c,10,14,18], seven signed 32-bit words. V is
    the current scratch record; it is not assumed to equal provider T or M.
    tail_p0 supplies P.byte1.bit7; tail_p8 has separate u16 and s16 reads.
    dy supplies F174 only. incoming_scale supplies the wrapper's incoming w5.

    All read-point inputs remain unchanged during this model. The bound
    caller masks mode to 0..3; out-of-domain diagnostic calls are excluded.
    A gate selecting the simple tail raises ValueError. stackResult models
    the independent SP1ec output, not F.cc; no F.cc value is supplied here.
    """
    if not isinstance(v_prefix_words, (list, tuple)) or len(v_prefix_words) != 7:
        raise ValueError("v_prefix_words must contain exactly seven i32 words")
    for index, value in enumerate(v_prefix_words):
        _integer(value, MIN_I32, MAX_I32, f"v_prefix_words[{index}]")
    _integer(tail_p0, 0, 0xffffffff, "tail_p0")
    _integer(tail_p8, 0, 65535, "tail_p8")
    _integer(tail_p13, 0, 255, "tail_p13")
    _integer(metric_byte38, 0, 255, "metric_byte38")
    _integer(mode, 0, 3, "mode")
    _integer(incoming_scale, MIN_I32, MAX_I32, "incoming_scale")
    _integer(dy, MIN_I32, MAX_I32, "dy")

    v = list(v_prefix_words)
    selector = initial_selector = (tail_p13 >> 2) & 7
    if selector == 4:
        selector = 1 if tail_p13 & 0x80 else 2
        check_charset = not (tail_p13 & 1) or selector == 2
    else:
        check_charset = selector == 2
    special = metric_byte38 in SPECIAL
    if check_charset:
        if not special:
            raise ValueError("347770 selects unsupported simple tail 1003477e8")
        selector = 2
    gate = dict(initialSelector=initial_selector, selector=selector,
                checkCharset=check_charset, specialCharset=special, branch="102e4dcac")
    initialized = dict(c0=v[1], c4=v[2], c8=v[1], d0=0)

    scale_args = None
    if mode == 0:
        scale_args = [tail_p8, 72, 100]
    elif mode == 1:
        scale = incoming_scale
    elif mode == 2:
        scale = dy
    elif tail_p8 == 100:
        scale = 1440
    else:
        scale_args = [1440, tail_p8, 100]
    if scale_args is not None:
        scale = native_muldiv(*scale_args)

    signed_p8 = tail_p8 if tail_p8 < 32768 else tail_p8 - 65536
    minimum_enabled = bool(tail_p13 & 0x40)
    p8_minimum = scale_minimum = None
    minimum_args = None
    minimum = 1
    if minimum_enabled:
        # 052ffc(3,s16(P8),100): no i32 overflow or 32767 clamp is possible.
        # Only this bounded call equals N; 052ffc is not generally N.
        p8_minimum = native_muldiv(3, signed_p8, 100)
        minimum_args = [31 if metric_byte38 == 0x80 else 40, scale, 1440]
        scale_minimum = native_muldiv(*minimum_args)
        minimum = max(1, p8_minimum, scale_minimum)

    total = _i32(v[1] + v[2])
    conversion = rounded = coefficient = None
    increment = 0
    if selector == 0:
        branch, upper, lower = "one-unit-upper", 1, _i32(total - 1)
    elif selector == 1:
        branch = "arithmetic-half"
        upper = total >> 1
        lower = _i32(total - upper)
    elif selector == 3:
        branch = "charset-table"
        index = metric_byte38 ^ 0x80
        coefficient = SELECTOR_TABLE[index] if index <= 8 else 497
        conversion = [v[2], coefficient, 1000]
        rounded = native_muldiv(*conversion)
        lower = _i32(v[2] - rounded)
        upper = _i32(total - lower)
    elif not special or v[2] > 0:
        # df5c uses fallback NZCV=0 when membership is false, so B.GT is true.
        branch, upper, lower = "preserve-original-split", v[1], v[2]
    else:
        branch = "105-per-thousand"
        conversion = [total, 105, 1000]
        rounded = native_muldiv(*conversion)
        increment = int(total > 11)
        lower = _i32(rounded + increment)
        upper = _i32(total - lower)
    fields = dict(c0=upper, c4=lower, c8=upper, d0=_i32(v[2] - lower))
    stack_result = _i32(v[1] - upper)
    redistribution = dict(sumI32=total, branch=branch, mulDivInputs=conversion,
                          roundedI32=rounded, tableCoefficient=coefficient,
                          incrementI32=increment, fields=dict(fields), stackResult=stack_result)

    padding_args = [v[0], 15, 100] if special else None
    padding = native_muldiv(*padding_args) if special else 0
    add_v10 = not special and ((mode not in (0, 3) and not tail_p0 & (1 << 15))
                              or bool(tail_p13 & 1))
    if special:
        fields["c4"] = _i32(fields["c4"] + padding)
        fields["c8"] = _i32(fields["c8"] + padding)
    elif add_v10:
        fields["c8"] = _i32(fields["c8"] + v[4])
    padding_checkpoint = dict(specialCharset=special, mulDivInputs=padding_args,
                              incrementI32=padding, addV10=add_v10, fields=dict(fields))

    lower_floor = metric_byte38 in (0x86, 0x88) or (tail_p13 & 0x41) != 0x41
    compared = fields["c4"] if lower_floor else _i32(fields["c8"] - fields["c0"])
    applied = compared < minimum
    if applied:
        if lower_floor:
            fields["c4"] = minimum
        else:
            fields["c8"] = _i32(fields["c0"] + minimum)
    return dict(
        status=STATUS,
        inputs=dict(vPrefixWords=v, tailP0=tail_p0, tailP8=tail_p8, tailP13=tail_p13,
                    metricByte38=metric_byte38, mode=mode, incomingScale=incoming_scale, dy=dy),
        gate=gate, initializedFields=initialized, redistribution=redistribution,
        padding=padding_checkpoint,
        floor=dict(scaleI32=scale, scaleMulDivInputs=scale_args, signedP8=signed_p8,
                   minimumEnabled=minimum_enabled, p8MinimumI32=p8_minimum,
                   scaleMinimumI32=scale_minimum, scaleMinimumMulDivInputs=minimum_args,
                   minimumI32=minimum, target="c4" if lower_floor else "c8-minus-c0",
                   comparedI32=compared, applied=applied),
        finalFields=fields, stackResult=stack_result,
        scope=["Explicit unchanged V/P read-point inputs; no provider T or initial M substitution",
               "347770 initialization and alternate selection; dcac/dec4 arithmetic only",
               "Mode is the bound caller's masked 0..3 value; diagnostics are excluded",
               "SP1ec is independent; F.cc is neither written nor supplied by this model",
               "No opaque 3482c0, later scaling, correction, wrapper update or LS output",
               "No Word measurement, page-start branch, font identity or DOCX property mapping"],
    )
