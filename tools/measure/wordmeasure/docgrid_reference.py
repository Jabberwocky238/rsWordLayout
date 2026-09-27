"""Neutral model of 0x100a656bc for its valid parameter tags 0 and 2."""

MIN, MAX = -(1 << 31), (1 << 31) - 1


def i32(value):
    value &= 0xffffffff
    return value - (1 << 32) if value & (1 << 31) else value


def trunc_div(value, divisor):
    q = abs(value) // abs(divisor)
    return -q if (value < 0) != (divisor < 0) else q


def saturate(value):
    return max(MIN, min(MAX, value))


def apply_tuple(values, tag, value_u16, ratio_u16, scale_u32, period_i32, use_input_height=False):
    """Return copied 24-byte tuple and intermediate integers, no field semantics.

    Tuple offsets 0,4,8,c,10,14 are six 32-bit words. The final word is opaque.
    Diagnostic calls are recorded, assumed to return; invalid tags are rejected
    because the observed caller supplies only 0/2 on this path.
    """
    assert len(values) == 6 and tag in (0, 2)
    assert 0 <= value_u16 <= 65535 and 0 <= ratio_u16 <= 65535
    assert 0 <= scale_u32 <= 0xffffffff and MIN <= period_i32 <= MAX
    output = list(values)
    h = i32(values[0])
    converted_value = h if use_input_height else saturate((value_u16 * scale_u32 + 720) // 1440)
    period, other, multiple = period_i32, 0, h
    if period != 0 and scale_u32 != 0:
        if scale_u32 != 1440:
            period = saturate(trunc_div(period * scale_u32, 1440))
        ratio = ratio_u16 if tag == 2 else 240
        if period != 0 and ratio != 0:
            other = period if ratio == 240 else saturate(trunc_div(period * ratio, 240))
        if period >= 1:
            quotient = i32(trunc_div(h, period))
            remainder = i32(h - i32(quotient * period))
            multiple = i32(i32(quotient + int(remainder != 0)) * period)
    chosen = max(other, multiple)
    diagnostics = ['chosen_less_than_input'] if chosen < h else []
    delta = i32(chosen - h)
    low = high = 0
    if delta >= 1:
        low, high = delta >> 1, delta - (delta >> 1)
        value_c = i32(output[3] + low)
        if value_c > chosen or value_c < 0:
            diagnostics.append('offset_c_outside_chosen')
            value_c = chosen if value_c > chosen else 0
        output[3] = value_c
        output[4] = i32(output[4] + low)
    outer = h if tag == 2 else max(converted_value, h)
    # CSEL GT consumes SUBS signed-overflow flags, not the sign of wrapped result.
    extra = i32(outer - chosen) if outer > chosen else 0
    output[2] = i32(i32(output[2] + high) + extra)
    return {'tuple': output, 'convertedValue': converted_value, 'convertedPeriod': period,
            'otherCandidate': other, 'multipleCandidate': multiple, 'chosen': chosen,
            'delta': delta, 'low': low, 'high': high, 'outerCandidate': outer,
            'extra': extra, 'diagnostics': diagnostics}


def normal_consumer(result):
    """Caller's normal bit path, no intervening adjustments or zeroing."""
    h, a, c8, lo, e0, _ = result['tuple']
    return {'b8': i32(i32(h + c8) + lo), 'c0': i32(a + lo), 'c8': c8,
            'd0': h, 'd8': lo, 'e0': e0}
