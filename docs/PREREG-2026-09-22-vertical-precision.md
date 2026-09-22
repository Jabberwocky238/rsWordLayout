# Same-line vertical alignment precision

Status: protocol frozen by the commit containing this file, fixture, font inputs,
and evaluator, before this fixture is opened or exported in Word. Results will be
written separately. Existing `probe-metrics` captures remain VOID; this experiment
does not revise their admission rules or reuse them as confirmation.

## Question and independent inputs

The engine currently shrinks superscript/subscript to 0.66 of the source size,
raises superscript by 0.34 em, and lowers subscript by 0.08 em. Round 3 preserved
these assumptions while eliminating half-point/twip truncation. The archive has
no admitted vertical-alignment case, so it cannot validate these formulas.

`fixtures/vertical-precision.docx` contains 21 paragraphs: Times New Roman, Arial,
Courier New, each at 8.5, 10, 10.5, 12, 13, 18, and 21.5 pt. Each paragraph has
one ordinary six-H run, one superscript six-H run, and one subscript six-H run,
separated by ordinary A/B/C/D anchors. A source tag precedes A. All content is
ASCII; each paragraph occupies 29 UTF-16 positions including its paragraph mark.
Expected total: one page, 21 lines, 609 source positions, 588 visible glyphs,
42 marked-run conditions. Six glyphs per run check consistency; they are not six
independent experiments.

Every run has explicit family and size. Paragraph marks have the same family and
size. Exact 30 pt line spacing, zero paragraph spacing, disabled widow control,
no explicit position, bold, or italic. Character spacing is explicitly zero,
horizontal scaling is 100%, and ordinary runs explicitly select baseline alignment.
The fixture builder specifies a kerning threshold above every fixture size.
The expected one-line-per-paragraph structure is an admission condition, not an
assumption used to overwrite the native line scan.

The adjacent manifest pins the exact DOCX SHA256, all source spans and text, and
the SHA256 of `vertical-precision.font-inputs.json`. Font metadata came from one
byte snapshot per installed font, with hashes and field byte offsets retained.
OS/2 values are independent candidate inputs, not evidence that Word uses them.
All three fonts happen to share XSize=1434/2048 and YSize=1331/2048; this fixture
cannot test variation of those ratios between fonts. Their YOffsets differ.

## Capture and admission

Use the existing local Microsoft Word 16.112.3 process only after verifying its
PID/start identity and zero open documents. Acquire the project coordination
lock and preserve its owner record. Open a uniquely named byte-identical copy,
verify its full path and saved state, and close only that document without saving.
Record the process identity and document inventory after capture. This establishes
a controlled local session, not the stronger Windows owned-job containment.

Use production capture with explicit required families and corresponding font
files, no shared input slot or PDF output. Preserve source bytes, export vector,
PDF, content text, paragraph ranges, first and repeat native line-scan receipts,
and hashes. Export then scan using separate Apple events, as production capture
does. No retry, sleep-to-convergence, or forced repagination is permitted to
replace a failed first scan. Capture instability is itself a result.

Require matching source hashes before/after, successful font preflight and actual
PDF font substitution checks, and identical complete first/repeat native scans.
Bind source annotations using `source_binding.bind_source`. Require the expected
page count, line count, source ranges, and visible-glyph counts. Pair glyphs in
the existing content-stream order using source line counts; independently check
every visible ASCII scalar against the source. Do not search for H strings, sort
by y, or infer run role from observed size or vertical movement.

The Mac bridge provides line ordinals, not line boxes. Content-stream order and
count-derived grouping remain explicit pairing premises. Source identity checks
strengthen this experiment but do not validate that premise for arbitrary files.

Common evidence failures produce UNDECIDABLE with all 42 conditions retained in
the denominator. Numeric prediction failures produce FAIL, never an admission
failure. An unreadable quantity does not invalidate independently readable
quantities. Require finite positive horizontal matrices for painted dimensions.
If A/B/C/D do not share a baseline within EPS, that line's shift is UNDECIDABLE.
Do not require equal inter-paragraph baseline intervals.

## Quantities

For each marked run, retain all six raw glyph records, source ranges, and matching
ordinary-run/anchor observations. The reported painted dimensions are
`sizeX = fontSize * matrix[0] * scaling` and
`sizeY = fontSize * matrix[3]`. A scalar size requires equal X/Y and scaling=1.
`effectiveSizePt` alone is insufficient to recover both dimensions.

The complete run placement extent is `x(following ordinary anchor) - x(first H)`.
Compare the marked extent L to the ordinary six-H extent L0 on the same line.
Report L/L0 and sourceSize * L/L0 as an **advance-equivalent size** only. Neither
this nor a PDF font size is a claim to recover Word's internal logical size.
The extent includes boundary Tc/TJ effects; per-glyph `advanceVector` does not.

Positive superscript rise is ordinary anchor y minus superscript H y; positive
subscript drop is subscript H y minus ordinary anchor y. Preserve the six
individual differences. Do not use PDF `rise` or `unraisedOrigin` as substitutes:
the text matrix can already contain the vertical translation.

## Frozen candidates and tolerances

EPS is 1e-6 pt for dimensional equality. All tested candidates and residuals are
reported; no closest candidate is declared correct. Qg rounds to a grid g with
positive half-up ties, separately from floor and ceil. Decimal/rational arithmetic
must make the half-grid cases independent of binary floating-point accident.

Paint and complete-run placement candidates include 0.66*S unquantized;
Q0.24(0.66*S) with nearest, floor, and ceil; 2*S/3; half-point nearest rounding;
OS/2 XSize/YSize anisotropic scaling; and isotropic OS/2 YSize scaling. Also keep
painted-ordinary-size-based 0.66 candidates separate from nominal-source-size
candidates, because the ordinary PDF transform can itself differ from S.
For X/Y each candidate predicts its own axis. Placement tests use the horizontal
candidate ratio and the residual `L - candidateSize/S * L0`, in points, not a
dimensionless ratio compared with a point tolerance.

Shift candidates are 0.34*S up / 0.08*S down, their independent nearest/floor/ceil
0.24 pt grid forms, and the font's OS/2 YOffset*S/upem. Record absolute origin
grid residuals as diagnostics. Rounding a relative displacement and rounding an
absolute baseline are different hypotheses; observed normal baselines do not
recover an unrounded internal baseline, so this experiment cannot uniquely infer
Word's internal rounding order.

The 10 pt case distinguishes 6.60/6.72 pt size, 3.40/3.36 pt rise, and .80/.72 pt
drop. The 10.5 pt subscript (.84 -> .96) and 18 pt superscript (6.12 -> 6.24)
exercise half-grid rules. Conditions where candidates coincide remain in full
denominators but do not count as distinguishing evidence.

The executable evaluator is `tools/measure/prereg_vertical_precision.py` in the
same frozen commit. Its candidate names, local metric gates, raw observations,
and M/N summaries are part of this protocol. Any discovered evaluator defect
must be recorded with the original result and affected scope; corrections after
observation are backtests, not a rewritten preregistration.

## Interpretation

One controlled Word session across three fonts and seven sizes is evidence for
these fixtures and this build. Passing a candidate does not establish a universal
Word rule. Any engine update derived from these observations requires regression
tests and a new held-out fixture before claiming independent validation.
