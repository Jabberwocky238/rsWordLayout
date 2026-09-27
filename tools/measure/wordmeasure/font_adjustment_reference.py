"""Explicit mode-2 arithmetic from the pinned Word host, without native calls.

Evidence: artifacts/docgrid-adjusted-font-metrics-2026-09-27/README.md.
This does not infer DOCX size, font identity, runtime mode, or LS output.
Inputs are values at the documented read points, after earlier adjustments.
"""

MIN_I32, MAX_I32 = -(1 << 31), (1 << 31) - 1
VERTICAL = ("c0", "c8", "c4", "cc", "d0")
HORIZONTAL = ("294", "28c", "290", "bc", "180")
FIELDS = VERTICAL + HORIZONTAL + ("184",)
STATUS = "ARITHMETIC_REFERENCE"


def _integer(value, low, high, name):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")
    return value


def _i32(value):
    return ((value + (1 << 31)) & 0xffffffff) - (1 << 31)


def _trunc_div(numerator, denominator):
    magnitude = abs(numerator) // abs(denominator)
    return -magnitude if (numerator < 0) != (denominator < 0) else magnitude


def native_muldiv(a, b, c):
    """0x100064708, including the fast SDIV INT_MIN/-1 wrap exception."""
    for name, value in (("a", a), ("b", b), ("c", c)):
        _integer(value, MIN_I32, MAX_I32, name)
    if c == 0:
        return MAX_I32
    if a == 0 or b == c:
        return a
    half = _trunc_div(c, 2)
    numerator = a * b + (-half if _i32(a ^ b ^ c) < 0 else half)
    quotient = _trunc_div(numerator, c)
    if MIN_I32 <= numerator <= MAX_I32:
        return _i32(quotient)
    return min(MAX_I32, max(MIN_I32, quotient))


def produce_mode2_h2(*, raw0, raw_c, input_size, denominator):
    """341bac..c40's mode-2 producer, before SP+4e6 -> F+196 copy.

    raw0/raw_c are the selected V words, not inferred provider T words.
    input_size is the incoming record's unsigned halfword, not XML points.
    The two comparisons following N are unsigned, including negative N.
    """
    for name, value in (("raw0", raw0), ("raw_c", raw_c), ("denominator", denominator)):
        _integer(value, MIN_I32, MAX_I32, name)
    _integer(input_size, 0, 65535, "input_size")
    difference = _i32(raw0 - raw_c)
    rounded = native_muldiv(difference, input_size, denominator)
    unsigned = rounded & 0xffffffff
    h2 = min(3276, max(1, unsigned))
    return dict(status=STATUS, rawDifferenceI32=difference,
                mulDivInputs=[difference, input_size, denominator],
                roundedI32=rounded, roundedU32=unsigned, h2=h2,
                scope=["C mode 2 producer and its retained property-record copy",
                       "V and incoming size are explicit read-point inputs",
                       "No font identity, XML unit, or runtime execution claim"])


def project_mode2(*, pre_scale, scale_x, scale_y, h2, dx, dy, initial_m, c8_correction):
    """348f8c -> optional 342194 correction -> 341118 copy -> 34a054 M.

    pre_scale supplies exactly F's eleven modeled i32 fields, with lowercase
    hexadecimal offset keys. The scaler reads h2 as u16; its separate
    producer is not assumed to have run. initial_m supplies the six words
    present at the update, including the two preserved words.

    c8_correction explicitly selects the site whose conditions are saved
    P0.bit16, !P0.bit15, current C mode not 0/3, and V.byte37.bit0. No flags
    are inferred. The projection assumes the identified completed-selection
    copy/update path, with no intervening change to the modeled fields.
    It does not predict alternate W+38 selection or callback adjustments.
    """
    if not isinstance(pre_scale, dict) or set(pre_scale) != set(FIELDS):
        raise ValueError("pre_scale must have exactly the eleven modeled offset keys")
    for name in FIELDS:
        _integer(pre_scale[name], MIN_I32, MAX_I32, "pre_scale." + name)
    for name, value in (("scale_x", scale_x), ("scale_y", scale_y), ("dx", dx), ("dy", dy)):
        _integer(value, MIN_I32, MAX_I32, name)
    _integer(h2, 0, 65535, "h2")
    if type(c8_correction) is not bool:
        raise ValueError("c8_correction must be an explicit bool")
    if not isinstance(initial_m, (list, tuple)) or len(initial_m) != 6:
        raise ValueError("initial_m must contain exactly six i32 words")
    for index, value in enumerate(initial_m):
        _integer(value, MIN_I32, MAX_I32, f"initial_m[{index}]")

    scaled = dict(pre_scale)
    conversions = []
    for offsets, axis, scale, units in ((VERTICAL, "y", scale_y, dy),
                                       (HORIZONTAL, "x", scale_x, dx)):
        denominator = _i32(units * 144)
        for offset in offsets:
            product = _i32(pre_scale[offset] * h2)
            result = native_muldiv(scale, product, denominator)
            scaled[offset] = result
            conversions.append(dict(offset=offset, axis=axis, inputI32=pre_scale[offset],
                                    productI32=product, denominatorI32=denominator,
                                    mulDivInputs=[scale, product, denominator], resultI32=result))
    after = dict(scaled)
    increment = _trunc_div(scale_y, 36) if c8_correction else 0
    after["c8"] = _i32(after["c8"] + increment)
    # The 0x188-byte prefix copy excludes scales at C+188/18c.
    context = {offset: value for offset, value in after.items() if int(offset, 16) < 0x188}
    context.update({"168": dx, "174": dy, "188": scale_x, "18c": scale_y})
    updated = [context["c8"], context["c4"], _i32(context["c8"] + context["c4"]),
               initial_m[3], context["184"], initial_m[5]]
    return dict(status=STATUS, inputs=dict(preScaleFields=dict(pre_scale),
                scaleX=scale_x, scaleY=scale_y, h2=h2, dx=dx, dy=dy,
                initialMWords=list(initial_m), c8Correction=c8_correction),
                conversions=conversions, scaledFields=scaled,
                c8IncrementI32=increment, afterCorrectionFields=after,
                copiedContextFields=context, updatedMWords=updated,
                scope=["Explicit mode-2 read-point inputs after earlier F adjustments",
                       "Ten scaled fields; F184 is not scaled by 348f8c",
                       "Conditional correction, completed F-to-C copy, default A update",
                       "No intervening changes to modeled fields are represented",
                       "No Word measurement, font mapping, alternate wrapper, or LS output claim"])
