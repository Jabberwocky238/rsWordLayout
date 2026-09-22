# Round 4: independent vertical alignment evidence

The new native Word experiment admits all 21 lines and all 42 marked runs. It
rejects the existing proportional model as a general description of the measured
quantities. No replacement layout algorithm is claimed by this round: every
preregistered candidate fails at least one condition, and the new promising paint
candidate was derived after observation.

## Frozen protocol and evidence

Before opening this fixture in Word, commit
`0e785f863946e90c740e21fddf022c0f1aa22eca` fixed the
[protocol](PREREG-2026-09-22-vertical-precision.md), executable evaluator,
deterministic DOCX, exact UTF-16 spans, font byte inputs, and fault tests.
The separate freeze receipt is timestamped `2026-09-22T07:07:42.315875+00:00`.
It pins 28 files, including the controller and production measurement modules.

The [versioned capture](../captures/vertical-precision-2026-09-22/) retains the
DOCX, PDF, raw scans, structured glyphs, font checks, and controller receipts.
[Machine-readable results](ENGINE-ITERATION-2026-09-22-ROUND4.json) retain the
full candidate denominators, paired-candidate discrimination counts, and all
six-H observations per condition. Complete run placement is measured to the next
ordinary anchor; no source role is inferred from observed size or y position.

The local Word process was PID 39057, UID 501, started at 14:31:11 local time,
version 16.112.3. Before and after this capture it had zero documents. The one
opened document's POSIX full path and saved state were verified. The process
identity remained unchanged across all ten recorded controller calls. Only that
document was closed, without saving; the project coordination lock was released.
This is a controlled local session, not Windows owned-job containment.

The first and immediate repeat native scans are identical over all 609 source
positions. Source hashes, font preflight, actual PDF font names, all 588 visible
ASCII identities, 21 paragraph-mark slots, and source line ranges pass. One page,
21 lines, 609 PDF glyphs; all four quantity groups are readable for 42/42 runs.
No convergence retry or repagination was used. The old `probe-metrics` evidence
remains VOID under its original criteria.

An independent audit re-extracted the PDF and reproduced the entire glyph JSON,
re-ran the evaluator with identical results, and verified the frozen code/input
hashes and all original AppleScript receipt hashes.

## Preregistered results

All comparisons below use the frozen 1e-6 pt threshold. Six Hs form one marked-run
condition, not six independent samples. A prediction failure does not remove a
condition from the denominator.

| Quantity / candidate | Matches | Failures |
| --- | ---: | ---: |
| Painted X/Y size: 0.66 times nominal size | 6/42 | 36/42 |
| Painted X/Y size: nearest 0.24 pt of that value | 18/42 | 24/42 |
| Painted X/Y size: floor 0.24 pt of that value | 24/42 | 18/42 |
| Painted X/Y size: half-point nearest of 0.66 times size | 0/42 | 42/42 |
| Painted X/Y size: 2/3 times size | 0/42 | 42/42 |
| Painted X/Y size: direct OS/2 recommendations | 0/42 | 42/42 |
| Placement extent: every preregistered size-ratio candidate | 0/42 | 42/42 |
| Rise/drop: unquantized 0.34 / 0.08 times size | 7/42 | 35/42 |
| Rise/drop: nearest 0.24 pt of those magnitudes | 11/42 | 31/42 |
| Rise/drop: direct OS/2 YOffset | 0/42 | 42/42 |

All three fonts have the same painted sizes below, for both superscript and
subscript. Ordinary and marked X/Y dimensions agree within the frozen threshold.

| Source size | Ordinary paint | Marked paint | Current engine marked size |
| ---: | ---: | ---: | ---: |
| 8.5 | 8.40 | 5.52 | 5.61 |
| 10 | 10.08 | 6.48 | 6.60 |
| 10.5 | 10.56 | 6.96 | 6.93 |
| 12 | 12.00 | 7.92 | 7.92 |
| 13 | 12.96 | 8.40 | 8.58 |
| 18 | 18.00 | 11.52 | 11.88 |
| 21.5 | 21.60 | 13.92 | 14.19 |

Offsets also vary with font. At 12 pt, Times New Roman and Arial superscripts
rise 4.08 pt, whereas Courier New rises 3.60 pt. At 8.5 pt, Times New Roman and
Courier New rise 2.40 pt, whereas Arial rises 3.12 pt. These are differences from
ordinary anchors on the same line, so paragraph pitch and first-line placement
do not explain them away.

## Engine baseline and limits

The baseline trace used the unchanged Round 3 executable, SHA256
`375a77f2147147f21c3c0a5c597939a1e4ee558d161ac718164a9247c02b3099`,
with real metrics, the Mac vertical grid, and the three exact font files. A locked
build verified its provenance. It has one page, 21 lines, all 609 source positions
in order, no unassigned glyphs, and matching actual font hashes. Every marked
span implements the existing 0.66 / 0.34 / 0.08 assumptions exactly.

The ordinary zero-tolerance engine comparison is structurally sound but FAIL:
609 pairs, maximum horizontal difference 3.293193 pt and maximum vertical
difference 17.53 pt. Those full-document differences include ordinary baseline
placement and must not all be attributed to superscript/subscript offsets.
The engine algorithm is unchanged; its comments now describe these ratios as
known approximations rather than a validated universal Word rule.

## Post-observation investigation

The complete [independent audit](../captures/vertical-precision-2026-09-22/exploration/independent-audit.json),
[font candidates](../captures/vertical-precision-2026-09-22/exploration/font-candidates.json),
and [offset investigation](../captures/vertical-precision-2026-09-22/exploration/offset-exploration.md)
are archived separately from the frozen capture. They are explicitly backtests.

The paint candidate `Q0.24(Q0.5(OS2_YSize * sourceSize / upem))`, using positive
nearest half-up rounding at each stage, fits painted X and Y for 42/42 conditions
in this batch. Ordinary `Q0.24(sourceSize)` fits 21/21. This is a backtest. The
three fonts share the same OS/2 YSize ratio, so this batch cannot establish that
Word selects that font field instead of a similar constant. Replacing that ratio
with an exact 0.65 in the two-stage candidate also fits all 42 painted sizes.

The corresponding placement prediction `6 * H_hmtx/upem * Q0.5(OS2_YSize*S/upem)`
still fails 42/42 at the frozen threshold; its largest residual is
0.000196875 pt, and even Times New Roman 12 pt differs by 0.0000015 pt.
Using the measured ordinary extent as a ratio baseline also fails 42/42. These
failures are retained without tolerance changes. Advance-equivalent size and
painted size therefore cannot be relabelled as recovered internal logical size.

The raw PDF also exposes a reason to keep the quantities separate. For the
Times New Roman 12 pt superscript, a 0.24 scale and 33-unit text matrix produce
7.92 pt paint. The PDF sets `Tc=0.0075`, adding 0.0594 pt between H origins, and
places the following ordinary anchor with another explicit matrix. The resulting
internal H step is about 5.77764 pt, whereas font hmtx scaled by 8 pt gives
5.77734375 pt. A nearly matching six-H extent does not establish matching internal
steps. Follow-up advance probes should vary H count (1/3/6/12) and start phase.

The next independent experiment should distinguish the competing rounding orders,
include half-point ties such as 15 and 25 pt, and use fonts whose OS/2 size ratios
differ materially. At 15 pt, the existing fonts' OS/2 YSize predicts a 9.5 pt
intermediate and 9.60 pt paint; an exact 0.65 constant predicts a 10 pt
intermediate and 10.08 pt paint. PT Serif and Menlo also have distinct OS/2 X/Y
recommendations, so those axes should remain competing candidates. Offset rules
need separate hypotheses grounded in font metric inputs.
The first batch must remain unchanged when those hypotheses are tested.

An offset investigation retained 48 combinations of metric source and rounding
stage. A difference of rounded ordinary/reduced descents matches 20/21 subscript
conditions, but fails Arial 8.5 pt (0.96 predicted, 0.48 observed). The best
ascent-side candidate matches only 11/21. Local CoreText and AppKit controls also
fail to reproduce the complete behavior. No font-specific exception was added;
these controls provide investigation leads, not a substitute Word oracle.

## Validation

163 Python tests pass, including 33 new evaluator tests, two deterministic
fixture/source tests, and two archived-native-evidence replay tests. The
evaluator's fault tests cover source and PDF hash
changes, receipt corruption, source/line/glyph identity, font substitution,
anisotropic scaling, local unreadability, exact half-grid ties, and unscaled or
unshifted counterexamples. Zero or negative target extents remain measurable
counterexamples when the ordinary extent is positive. No Rust behavior changed.
