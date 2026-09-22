# Offset exploration after the frozen capture

Backtest only. This uses the already observed `evaluation-01.json` from the
protocol frozen in commit `0e785f8`. No Word calls, capture changes, evaluator
changes, font installation, or engine changes were made. The full input hashes,
font-table fields, native probe source, source/binary hashes, observations, and
all 48 candidate combinations are retained in `offset-exploration.json`.

There is no complete replacement formula. The observed family differences are
real admitted counterexamples to a universal `.34*S` rise / `.08*S` drop in this
fixture. A fixed em coefficient with only the same size-dependent quantization
cannot explain different fonts at the same source size.

## Font-metric difference candidate

A testable mechanism is alignment of ordinary and reduced font metrics, rather
than applying a single em shift. Let `Q_g` be nearest rounding to grid g, with
positive half ties upward. Using the independently suggested size candidate
`K = Q_0.5(.65*S)`, define:

```
D(s) = Q_0.5(abs(hhea.descender) / upem * s)
drop = Q_0.24(D(S) - D(K))
```

This explains 20/21 subscript observations, including the larger Courier drops,
but is already falsified by Arial at 8.5 pt: predicted .96 pt, observed .48 pt.
There is no exception added for that font. The `.65` size rule is itself a
post-observation candidate, not an independently validated input here.

For superscript, four corresponding metric definitions were checked: ascent,
ascent plus lineGap, natural height minus separately rounded descent, and an
ascent-plus-descent control without lineGap. Nearest/floor/ceil and .5/1/.24 pt
metric grids were all retained. Raw .65/.66/(2/3) script-size controls and typo
ascent controls were also retained. No per-family constants were fitted. These
alternatives were chosen after seeing the data, so their pass counts cannot be
reported as independent confirmation. The best ascent-side count is only 11/21.

The natural-height-minus-descent candidate at a .5 pt metric grid produces
4.08 pt for Times New Roman / Arial at 12 pt and 3.60 pt for Courier New at
12 pt, the requested same-size distinction. But it produces 2.40 pt for both
Times New Roman and Arial at 8.5 pt; Arial actually rises 3.12 pt. It also misses
other sizes (10/21 superscript matches overall). Thus this is a possible source
of font dependence, not an explanation of Word's complete algorithm.

The strongest repeated subscript pattern is consistent with intermediate
font-metric rounding, but its counterexample prevents using this formula as an
engine rule. Further work should first distinguish metric source and rounding
stage in a new frozen fixture, rather than add family exceptions to this batch.

## Local native controls

CoreText `kCTSuperscriptAttributeName` values -1, 0, +1 were passed to
`CTLineCreateWithAttributedString` for six Hs at each font and size. All 63 runs
retained the requested font size, identity run text matrix, and origin [0,0].
The local SDK header explicitly conditions the attribute on font support.
This API path does not reproduce Word's synthetic scaling/shift here; it does
not establish that Word does or does not use CoreText elsewhere.

AppKit TextKit `NSSuperscriptAttributeName` was then tested with ordinary anchors,
first using the source font size and then explicitly supplying the candidate
reduced font size to the H run. Word was never invoked. After applying the same
.24 pt diagnostic output quantization, the source-size path matches 0/42 Word
offsets; the manually reduced-font path matches 6/42. The latter predicts
4.08 pt for superscript in all three families at 12 pt, so it fails Courier's
3.60 pt. Neither native control provides a replacement.

CoreText probe source SHA256:
`e865c4aae9bb7c2e63a8448ea9558e4d0b639e9cdea1833807a9c2b7dd274d3c`.
AppKit probe source SHA256:
`c6d57280ed34a83b13239794ebaa13ba4df6d3ec253e5da89105a58f6fb94edd`.
Frozen font-input bytes SHA256:
`1a03114d854bbbc3b2df36f51f67d6155c9809cc3a5fce667389a5c0a1b71081`.

All original font hashes were rechecked before reading the extra OS/2 fields.
The exploration JSON embeds the probe programs and native observations, so the
native comparisons do not depend on retaining `/tmp` files. These reference
systems are controls, not Word oracles.
