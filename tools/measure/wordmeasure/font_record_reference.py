"""Numeric Face1/non-CFF projection from the pinned Word host instructions.

This models U's defined fields, T[0..0x18], and construction-time M only.
Inputs describe the successful initialization path, not a DOCX font mapping.
Arithmetic assumes IEEE round-to-nearest, ties-to-even floating operations.
"""

import math
import struct

MIN_I32, MAX_I32 = -(1 << 31), (1 << 31) - 1
UNSIGNED = ("designUnitsPerEm ascent descent capHeight xHeight underlineThickness "
            "strikethroughThickness").split()
SIGNED = ("lineGap underlinePosition strikethroughPosition glyphBoxLeft glyphBoxTop "
          "glyphBoxRight glyphBoxBottom subscriptPositionX subscriptPositionY subscriptSizeX "
          "subscriptSizeY superscriptPositionX superscriptPositionY superscriptSizeX superscriptSizeY").split()
SCALED_EXTENDED = SIGNED[7:]
SCALED_BASE = ("ascent descent lineGap capHeight xHeight underlinePosition "
               "underlineThickness strikethroughPosition strikethroughThickness").split()


def integer(value, low, high, name):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")
    return value


def f32(value):
    try:
        return struct.unpack("<f", struct.pack("<f", value))[0]
    except OverflowError:
        return math.copysign(math.inf, value)


def i16(value):
    return ((value + 0x8000) & 0xffff) - 0x8000


def i32(value):
    return ((value + 0x80000000) & 0xffffffff) - 0x80000000


def round_leaf(value):
    """0x100346a94 with w0=0, including the double-ULP push from zero."""
    exponent = (struct.unpack("<Q", struct.pack("<d", value))[0] >> 52) & 0x7ff
    epsilon = math.ldexp(1.0, exponent - 1075)
    negative = value < 0.0
    pushed = value + (-epsilon if negative else epsilon)
    shifted = pushed + (-0.5 if negative else 0.5)
    if not math.isfinite(shifted):
        return shifted
    return math.ceil(shifted) if shifted < 0.0 else math.floor(shifted)


def convert_scalar(value):
    """0x1003469f4 for the observed metric callers' m=d=1 arguments."""
    result = round_leaf(float(f32(value)))
    return int(result) if math.isfinite(result) and MIN_I32 <= result <= MAX_I32 else MAX_I32


def metric_record(record):
    names = set(UNSIGNED + SIGNED + ["hasTypographicMetrics"])
    if not isinstance(record, dict) or set(record) != names:
        raise ValueError("expected exactly the 23 METRICS1 fields")
    for name in UNSIGNED:
        integer(record[name], 0, 65535, name)
    for name in SIGNED:
        integer(record[name], -32768, 32767, name)
    integer(record["hasTypographicMetrics"], MIN_I32, MAX_I32, "hasTypographicMetrics")
    if record["designUnitsPerEm"] == 0:
        raise ValueError("designUnitsPerEm must be nonzero")
    return dict(record)


def project_face1(*, initialized, requested, lf_height, xavg_width, width_scale, escapement):
    """Project a successful Face1/non-CFF path, with explicit read-point inputs.

    initialized is Q's GDI-compatible record at designUnitsPerEm. requested
    is GDI-compatible output at float32(-lf_height), or None on the copy path.
    xavg_width is the signed OS/2 +2 halfword from a valid >=78-byte table.
    Q's flags/width scale/angle must hold at their respective observed reads;
    this does not assert that unexpanded callees cannot mutate host state.
    """
    q = metric_record(initialized)
    integer(lf_height, MIN_I32 + 1, -1, "lf_height")
    integer(xavg_width, -32768, 32767, "xavg_width")
    integer(escapement, MIN_I32, MAX_I32, "escapement")
    if type(width_scale) not in (int, float):
        raise ValueError("width_scale must be finite binary32")
    try:
        scale = f32(float(width_scale))
    except OverflowError:
        raise ValueError("width_scale must be finite binary32") from None
    if not math.isfinite(scale):
        raise ValueError("width_scale must be finite binary32")
    size, units = f32(-lf_height), f32(q["designUnitsPerEm"])
    ratio = f32(size / units)
    copied = size == units
    conversions = []

    def store_half(name, value, signed):
        product = f32(ratio * f32(value))
        converted = convert_scalar(product)
        stored = i16(converted) if signed else converted & 0xffff
        conversions.append(dict(field=name, productF32=product, convertedI32=converted, stored=stored))
        return stored

    if copied:
        if requested is not None:
            raise ValueError("the copy path must not supply a requested metric record")
        u, extra = dict(q), xavg_width
    else:
        u = metric_record(requested)
        if u["designUnitsPerEm"] != q["designUnitsPerEm"]:
            raise ValueError("records must describe the same designUnitsPerEm")
        for name in SCALED_EXTENDED:
            u[name] = store_half(name, u[name], True)
        extra = store_half("hostExtraI16", xavg_width, True)
        for name in SCALED_BASE:
            u[name] = store_half(name, u[name], name in SIGNED)

    # The gate is read from Q, not from the temporary record returned at size.
    typographic = q["hasTypographicMetrics"] != 0
    a = i32(u["ascent"] + (u["lineGap"] if typographic else 0))
    b, g = u["descent"], 0 if typographic else u["lineGap"]
    height = i32(a + b)
    candidate = max(convert_scalar(f32(scale * f32(extra))), 1)
    t = [height, a, b, i32(lf_height + height), g, candidate, candidate]
    e = 0
    if escapement in (900, 2700):
        e = convert_scalar(f32(f32(f32(u["designUnitsPerEm"]) * ratio) * f32(1 / 64)))
        for index, increment in enumerate((i32(e << 1), e, e, i32(e << 1))):
            t[index] = i32(t[index] + increment)
    return dict(branch="copy50" if copied else "scaledFace1", emSizeF32=size,
                ratioF32=ratio, widthScaleF32=scale, temporaryMetrics=u,
                temporaryExtraI16=extra, halfwordConversions=conversions,
                typographicGate=typographic, angleIncrementI32=e, tPrefixWords=t,
                initialMWords=[t[1], t[2], t[0], t[4], t[5], 0])
