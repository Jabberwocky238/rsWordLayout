"""Bound T/V -> simple tail -> mode-2 vertical arithmetic, without native calls.

Evidence: docgrid-font-raw-provider and docgrid-adjusted-font-metrics frozen
bundles, 0x100347770..1003478a8, 0x100341bac..c40, 0x100348f8c,
and 0x10034214c..194. Inputs are explicit values at those read points.
"""

from .font_adjustment_reference import native_muldiv, produce_mode2_h2

MIN_I32, MAX_I32 = -(1 << 31), (1 << 31) - 1
VERTICAL = ("c0", "c8", "c4", "cc", "d0")
STATUS = "ARITHMETIC_REFERENCE"


def _integer(value, low, high, name):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")


def _i32(value):
    return ((value + (1 << 31)) & 0xffffffff) - (1 << 31)


def project_simple_mode2(*, t_prefix_words, tail_p0, tail_p13, metric_byte38,
                         input_size, dy, scale_y, correction_p0, metric_byte37):
    """Project five vertical fields and three updated metric words only.

    T[0..0x18] is V's numeric prefix on the bound provider route, unchanged
    through the modeled read points. metric_byte38 is its post-rewrite value
    at 347770, not an inferred provider T38. The two P0 values are separate
    snapshots: tail arithmetic and the later saved correction word.

    C/F mode 2 and the completed-selection/default-wrapper update route are
    assumed, not selected from a document. dy is the same explicit F174 at
    h2 production and scaling. No intervening metric changes are simulated.
    """
    if not isinstance(t_prefix_words, (list, tuple)) or len(t_prefix_words) != 7:
        raise ValueError("t_prefix_words must contain exactly seven i32 words")
    for index, value in enumerate(t_prefix_words):
        _integer(value, MIN_I32, MAX_I32, f"t_prefix_words[{index}]")
    for name, value in (("tail_p0", tail_p0), ("correction_p0", correction_p0)):
        _integer(value, 0, 0xffffffff, name)
    for name, value in (("tail_p13", tail_p13), ("metric_byte38", metric_byte38),
                        ("metric_byte37", metric_byte37)):
        _integer(value, 0, 255, name)
    _integer(input_size, 0, 65535, "input_size")
    for name, value in (("dy", dy), ("scale_y", scale_y)):
        _integer(value, MIN_I32, MAX_I32, name)

    selector = (tail_p13 >> 2) & 7
    initial_selector = selector
    if selector == 4:
        selector = 1 if tail_p13 & 0x80 else 2
        check_charset = not (tail_p13 & 1) or selector == 2
    else:
        check_charset = selector == 2
    charset_index = metric_byte38 ^ 0x80
    charset_alternate = charset_index <= 8 and bool((0x143 >> charset_index) & 1)
    if not check_charset or charset_alternate:
        raise ValueError("347770 selects unsupported alternate tail 102e4dcac")

    t = list(t_prefix_words)
    subtract_c = bool(tail_p0 & (1 << 16))
    add_10 = not (tail_p0 & (3 << 15))
    c0 = _i32(t[1] - t[3]) if subtract_c else t[1]
    pre_scale = {"c0": c0, "c8": _i32(c0 + t[4]) if add_10 else c0,
                 "c4": t[2], "cc": t[3], "d0": 0}
    factor = produce_mode2_h2(raw0=t[0], raw_c=t[3], input_size=input_size,
                             denominator=dy)
    denominator = _i32(dy * 144)
    conversions = []
    scaled = {}
    for offset in VERTICAL:
        product = _i32(pre_scale[offset] * factor["h2"])
        value = native_muldiv(scale_y, product, denominator)
        scaled[offset] = value
        conversions.append(dict(offset=offset, inputI32=pre_scale[offset],
                                productI32=product, denominatorI32=denominator,
                                mulDivInputs=[scale_y, product, denominator], resultI32=value))

    correction = bool(correction_p0 & (1 << 16) and
                      not correction_p0 & (1 << 15) and metric_byte37 & 1)
    increment = abs(scale_y) // 36 if correction else 0
    if scale_y < 0:
        increment = -increment
    after = dict(scaled)
    after["c8"] = _i32(after["c8"] + increment)
    return dict(
        status=STATUS, mode=2,
        inputs=dict(tPrefixWords=t, tailP0=tail_p0, tailP13=tail_p13,
                    metricByte38=metric_byte38, inputSize=input_size, dy=dy,
                    scaleY=scale_y, correctionP0=correction_p0, metricByte37=metric_byte37),
        tailGate=dict(initialSelector=initial_selector, selector=selector,
                      charsetIndex=charset_index, charsetAlternate=charset_alternate,
                      branch="1003477e8"),
        tailArithmetic=dict(subtractVc=subtract_c, addV10=add_10),
        preScaleFields=pre_scale, h2Result=factor, conversions=conversions,
        scaledFields=scaled, c8Correction=correction, c8IncrementI32=increment,
        afterCorrectionFields=after,
        updatedMetricWords=[after["c8"], after["c4"], _i32(after["c8"] + after["c4"])],
        scope=["Bound RTARC/RTDWRITEFONT provider T equals V at its output call",
               "T numeric prefix is unchanged through the modeled tail and h2 read points",
               "Post-rewrite V38 and the two P0 snapshots are explicit, not inferred",
               "C/F mode 2; dy is unchanged between h2 production and scaling",
               "Simple tail, then scaling and correction with no intervening metric changes",
               "Completed F-to-C copy and default-wrapper update are conditional",
               "Only updated M0/M4/M8; no horizontal fields, F184, full M or initial-M substitution",
               "No Word measurement, DOCX mapping, alternate wrapper or LS output claim"],
    )
