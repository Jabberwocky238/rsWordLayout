# Canonical docGrid observations

The original 18 inputs and three discriminators now have Mac Word 16.112.3
captures: one recovered no-grid baseline plus a separate completed 20-case
batch. This closes the missing-input-observation gap, not the grid algorithm.
The engine still retains docGrid without applying it. Glyph origins are
observations; required extent and pagination capacity remain unmeasured.

## Bound inputs and observations

The unchanged source manifests are in `fixtures/docgrid-canonical-2026-09-27/`
and its `append-discriminators/` directory. Each case has twelve unique labels,
60 UTF-16 source units, explicit TNR body/mark fonts, zero paragraph gaps,
widowControl disabled, one column and compatibility mode 15. Body top and left
are 36pt. All observed PDFs have one page. Two cases use eleven soft returns
within one paragraph; the phase case changes snap after three paragraphs.

Every case retains the actual PDF, source DOCX and XML bindings, native source
and paragraph receipts, two complete identical CP scans, font preflight and
font/size/rise checks. The additional batch has 1200 glyphs, 240 native lines,
218 paragraph marks and 22 soft returns. Its per-case and final document/window
inventories are 0/0 after closing only the owned document without saving.

The following y values are first PDF glyph origins in points. Alternating
differences are listed in their observed order, not averaged into a cursor.

| Input | First y | Successive y differences |
| --- | ---: | --- |
| 12pt, no grid | 47.04 | 13.68 / 13.92 alternating |
| 12pt, auto240, pitch240, snap=true | 52.32 | 24.00 |
| 12pt, auto240, pitch270, snap=true | 54.00 | 27.12 / 26.88 alternating |
| 12pt, auto240, pitch300, snap=true | 48.00 | 15.12 / 14.88 alternating |
| 12pt, auto240, pitch360, snap=true | 49.44 | 18.00 |
| 12pt, auto240, pitch480, snap=true | 52.32 | 24.00 |
| 12pt, atLeast240, pitch360, snap=true | 49.44 | 18.00 |
| 12pt, exact480, pitch360, snap=true | 55.20 | 24.00 |
| 18pt, no grid | 52.80 | 20.64 / 20.88, full sequence retained |
| 18pt, auto240, pitch300, snap=true | 57.60 | 30.00 |

At pitches 240/300/360/480, absent snap has the same twelve label page/xy values
as explicit true. Explicit false at all four pitches has exactly the same
raw label values as the old no-grid baseline. The true/false soft-return cases
match their respective twelve-paragraph counterparts. Auto240 and atLeast240
match at pitch360/true. These eleven full-vector equalities were checked on
retained raw floats, not selected rounded values. They establish the measured
scope, not all absent defaults, line rules or paragraph structures.

With three snap=false paragraphs followed by nine true ones at pitch360,
the y sequence is:

```text
47.04, 60.72, 74.64, 90.72, 108.72, 126.72,
144.72, 162.72, 180.72, 198.72, 216.72, 234.72
```

G003, CP15, is the first true paragraph. Its y is 90.72, whereas G003 in the
pure-true case is 103.44. A policy that discards the preceding unsnapped flow
and blindly reuses the pure-true row number is contradicted. The reading does
not itself identify a universal page/column grid anchor or continuous cursor.

## Step hypotheses and origin counterexamples

For the bound TNR table, upem2048/ascent1825/descent443/gap87, 12pt content
height is 265.78125 twips and natural height is 275.9765625 twips. Thus pitch270
distinguishes two proposed step rules: `ceil(content/pitch)*pitch` gives270,
whereas `ceil(natural/pitch)*pitch` gives540twips. The observed approximately
27pt differences reject the one-pitch content-height candidate on this input.
They are compatible with the two-pitch natural-height candidate, but do not
prove a general rule or identify the host's internal source fields.

The existing bounded host transcript at `0x100379014..0x10037902c` contains
integer division, remainder, conditional increment and multiplication: in
its positive domain this is `ceil(w28/w24)*w24`. Subsequent code takes a maximum
with another candidate and splits a discrepancy. The runtime meaning of w20,
x27 and w28 has not been proved to be a grid switch, pitch and font extent.
The arithmetic motivates experiments; it does not provide that missing mapping.

A separately recorded exploratory origin formula used rounded natural height
minus rounded descent, then half of the difference between rounded grid step
and rounded content height. It was written after the initial 12pt summaries
but before the three appended results were received. Its declared content-based
step and first-origin predictions were retained unchanged:

| Holdout | Predicted first y | Observed first y | Result |
| --- | ---: | ---: | --- |
| 12pt/pitch270/true | 47.04 | 54.00 | FAIL; step13.5pt also fails |
| 18pt/no grid | 52.80 | 52.80 | Local match |
| 18pt/pitch300/true | 57.84 | 57.60 | FAIL despite the matching30pt step |

These failures separate origin and step questions. Replacing the failed
formula's height choice or adding a one-cell correction after reading them
would be another hypothesis, not a validated repair. No candidate was added
to the production engine. The existing no-grid engine mismatch and exact
body-ascent counterexamples also remain current after the compatible fine
ascent transport change.

## Artifacts and limits

The 20-case capture is `artifacts/docgrid-word-continuation-2026-09-27/`.
Its 1031-file `durable-hashes.json` SHA-256 is
`9e1b9af8a443ac316e8b334dfb7b3060b96a6c547dffb300819ae6a8084d0bcb`.
The owner audit SHA-256 is
`e44e89ecb3a7ef197c756a8f49bbbac6ff0996f790eb758083107e5bbb8533eb`;
the eleven observed-pair comparisons SHA-256 is
`8112c13ba531c61682bee3582eb790c8240f5c32433171d2f3b942d725412158`.
The separately recovered baseline and its hashes are documented in
[docGrid input evidence](DOCGRID-INPUTS-2026-09-27.md).

The fixed exploratory plan and evaluator are in
`artifacts/docgrid-baseline-candidate-2026-09-27/`; its four-file `SHA256SUMS`
SHA-256 is `eca335ac595da0dcd84083785d8f90e27844bb6fc7edf8b9036d65591aaaa259`.
The passing evaluation report, which records the formula failures, has SHA-256
`663135aac4cab656fab012e7112e8206cb496c4a1261c779afd581692bbc13b8`.

The raw extractor remains UNCALIBRATED with no lineBaseline. There is one PDF
export per case; duplicate CP scans establish scan stability, not repeated-PDF
stability. Each of the 220 Word events terminated successfully; each exact-file
access grant had a matching owned path and AX receipt. No wider permission was
granted, no open was repeated, and 1050 files across seven earlier frozen bundles
were rechecked unchanged. All Word/AX operations ended at 05:23:37 UTC.

Further algorithm work must preserve the separate advance/required/baseline
channels, establish first/last-line fit behavior and test other fonts and source
structures. Continuous-section grid switch timing and page/column anchors are
still unresolved; neither a delayed page-geometry change nor a rounded Rect.y
may silently define them.

When grid policy is implemented, choose the owning section by paragraph index
at `format_flow_paragraph`, and carry its declaration in an immutable paragraph
format context through `break_paragraph_at` and `line_vertical`. Each successor
in `keep_after_extent` needs its own lookup: a keepNext link can cross a
single-column continuous section. Widow retries in `page_line_quota` need the
same context; balanced replay can select it again from the saved paragraph
index. A mutable `Engine.current_grid` or copied `Flow` grid would risk leaking
one section's policy into another. Any required grid anchor must be an explicit
fine coordinate; the existing delayed setup/columns state is not evidence of
grid-switch timing. This is an implementation boundary, not an applied rule.

## Independent fixed-step comparison

The independent comparator verified all 21 cases and 252 source rows against
the two source manifests, both raw CP scans, unique geometric labels, actual
font names/sizes and the two frozen capture manifests. All 1086 frozen capture
files and 1124 total inputs were bound. There were no failed source-binding
cases. This result does not mean that engine geometry or a grid model passed.

Its plan declared the natural-height and content-height step candidates and
the prior exposure to early capture summaries. It deliberately leaves the
continuous origin phase unknown. For a contiguous eligible run, it reports
the range of `r_i = y_i - i*step` and half that range: the smallest symmetric
endpoint error bound that could make a constant-step sequence compatible.
It does not start the cursor at the first quantized glyph or select an
acceptance threshold after seeing the observations.

At pitch270/12pt, the natural-height candidate step 540 twips needs a minimum
bound of 0.06pt; the content-height candidate step 270 twips needs 74.31pt.
This is a concrete counterexample to the latter fixed step. At 18pt/pitch300,
both candidates give 600 twips and agree with the observed 30pt differences,
so that pair does not distinguish the two height choices. Snap and phase
comparisons independently retain the same coordinates reported above.

Four synthetic checks ensure page transitions, snap transitions and source
gaps do not become ordinary step measurements, and an unknown constant offset
does not restart accumulation from an observed quantized origin. The final
comparison is `artifacts/docgrid-canonical-comparison-results-2026-09-27/comparison-run-01.json`,
SHA-256 `fff82d0b88ac390d0fe4bfc2a562e74d8992f82a48f1ab3bfbf2577839a81fa3`.
The result directory's eight-file `SHA256SUMS` has SHA-256
`c83738ecba755020eb59485e049796acaaca212e8cc2dca74c1c2eabce09556b`.
The separate eight-file preparation manifest has SHA-256
`003de7a325bf8e48a52fdc3285b8d82808edd5bad6ac6761f7199ccc07ffbf52`.
No engine formula, old capture or input was changed by the comparison.
