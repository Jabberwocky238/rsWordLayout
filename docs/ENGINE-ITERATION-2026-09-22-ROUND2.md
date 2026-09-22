# Measurement-driven engine iteration, round 2

Starting engine: `cd3894e`. This round uses the same archived Mac Word captures
and font files as round 1. It does not start Word or change capture files.
The zero tolerance, glyph exclusions and geometry comparator are unchanged.

## Separating measurement changes from engine changes

The old engine is replayed under the new measurement implementation before
testing the new engine. Full outputs are under
`artifacts/engine-sweep-2026-09-22-round2/` (gitignored). `before` snapshots the
round-1 executable, SHA256
`17efc176dfd1b172068389736ce88b92670124279ead36f0e7b89d31830f35bc`.
`after` snapshots the final engine, SHA256
`125f2f4cc6d9079e230532a922d7e1d7c5a90c8c427a638f658acca169e332ed`.
The compact results are in
[`ENGINE-ITERATION-2026-09-22-ROUND2.json`](ENGINE-ITERATION-2026-09-22-ROUND2.json).

The measurement changes are deliberate and recorded, not engine improvements:

- `source_text` maps Word UTF-16 offsets to Python scalar indices. DOCX-derived
  marks and paragraph ranges now use UTF-16 too. Incomplete/duplicate scans,
  mismatched extents, split surrogate pairs and inconsistent page-break marks
  produce UNDECIDABLE instead of silently slicing different text.
- `--source-docx` supplies missing annotations only after matching the captured
  before/after SHA256, `unchanged`, paragraph ranges, UTF-16 extent and complete
  source text. Only source-annotated paragraph CR may correspond to Mac LF.
  Existing annotations are checked and preserved. The original capture stays
  unchanged; every result records source annotation provenance and backtest status.
- A page break plus paragraph mark on one captured line is classified as
  `BEFORE_MARK`, including Mac's LF spelling. A page break alone on its captured
  line remains `OWN_LINE`. The established 1/0 glyph rules themselves do not change.

With the same old engine, these changes expose three definite missing glyphs in
`breaks-sections` and one in `vmisc2`, previously hidden behind undecidable pages.
They also make `vmisc3` structurally comparable: its 111 glyphs agree, with geometry
errors retained. Consequently structural agreement rises from 17/24 to 18/24
**before changing the engine**. This must not be credited to a layout fix.

## Engine changes

Exact line spacing now reserves its specified height during pagination and
keep checks, matching the fine baseline cursor. A synthetic 300-twip content
area with four 100-twip exact lines and 300-twip glyph metrics changes from
`[1,1,1,1]` lines per page to `[3,1]`. Six boundary tests cover empty/soft-return
lines, keep-lines, keep-next, nonpositive values and unchanged auto/at-least
behavior. These establish internal consistency, not a new Word measurement of
oversized text at a page bottom.

Control-only paragraphs ending in a page break now retain both terminal glyphs.
Their source spans remain separate UTF-16 units. This addresses the 3/1 missing
glyphs identified above without changing source-line or physical-page ownership.

The punctuation overflow path supports the four measured full-width characters
U+3002, U+FF0C, U+FF09 and U+3001. It tries a single closing punctuation glyph
outside the width before falling back to the existing line-start restrictions.
Explicit paragraph `overflowPunct=false` retains the ordinary path. Actual
advance widths are retained, not clipped to the line boundary. The original
evidence is Mac Word 16.112, Songti SC 12pt; other platforms, multiple hanging
punctuation and additional symbols are not independently validated here.

Run adjacency is passed explicitly to the overflow check. A supported punctuation
character at the start of a new run can join the preceding CJK text; a following
closing character in another run prevents single-punctuation overflow. Controls
form barriers. Runs retain their separate fonts, colors, rise and source spans.
This does not fix the pre-existing general kinsoku reflow across run boundaries.

A review caught and removed an extra complexity factor in the initial overflow
implementation. One fit of 400 repeated CJK groups now uses 4 measurements rather
than 802; a 160-group wrapping case reduces break-input scanning from 4,211,751
to 78,705 bytes (the ordinary path scans 77,274). Counter-based tests guard both
cases; these are synthetic operation counts, not timing or Word observations.

The parser dependency is pinned to `399e36a3c645e9b9001531fb06969f8ce5347e1f`,
published in [rsWordParser draft PR #11](https://github.com/LilLeapo/rsWordParser/pull/11).
Its six-line production schema change preserves absent versus explicitly false
`overflowPunct` through the generated native projection. The parser's default
debug/release suites each pass 1,022 tests, its compatibility suites each pass
1,141, and both variants retain 13 ignored tests. Differential checks cover 799
synthetic and 266 real documents with zero unknown differences. No parser
snapshot or expected fixture was changed. The original parser worktree is untouched.

## Final replay and checks

| Measurement, same new measurement implementation | Before | After |
| --- | --- | --- |
| Admitted captures | 24 | 24 |
| Structurally sound | 18/24 | 21/24 |
| Page counts equal | 24/24 | 24/24 |
| Hard selfcheck failures | 0 | 0 |
| Comparison state | 24 FAIL | 24 FAIL |

All common fixture bytes, font entries/hashes, capture inputs, source-annotation
provenance and measurement-module bytes are identical between the two runs.
There are no structural regressions. The changes are:

- `breaks-sections`: 87 to 90 glyphs, matching Word; all 33 lines pair.
- `vmisc2`: 87 to 88 glyphs, matching Word; all 14 lines pair.
- `cjk-plain`: 301 glyphs remain, but the line-count mismatch disappears and all
  301 glyphs pair instead of 188.
- `kinsoku`: its first page changes to the observed 38-character first line,
  removing both definite glyph-count mismatches. Three page-level line-count
  mismatches remain; 63 glyphs pair instead of zero.
- `kinsoku2` retains the explicit-off 36-character first line and unchanged
  geometry. Its three line-count mismatches remain. `page-start` retains 18.

No capture achieves zero-error acceptance. Geometry differences remain, including
the large control-only space positioning differences. All 24 selfcheck results
remain UNDECIDABLE, not passes; many lack the particular one-character terminal
line needed by that check. The complete replay's nonzero exit is expected.

Validation: 167 Rust workspace tests, 102 Python measurement tests, strict
workspace Clippy, the workspace wasm target check and `git diff --check` pass.
Two test-only Clippy findings were corrected and the affected 13 tests rerun.
Runtime tests use bundled or synthetic fonts; only capture replay uses Mac fonts.

API note: `Para` gains `overflow_punct` (default true). External struct literals
must initialize it or use `..Para::default()`. `FontMetrics` gains default overflow
methods, so existing implementations of ordinary `fit` remain usable.

## Limits retained

The control-only PDF lines paint a Times New Roman space and a Luminari space,
with a 144pt origin separation. Both source runs and paragraph properties request
Luminari; the fixtures have no styles part defining a document default font.
The PDF stream order is observed, but assigning these indistinguishable spaces
to the two source controls relies on the existing unverified content-stream-order
premise. Neither default-font ownership nor that positioning rule has been
established by this round. The Times glyph advance is 3.24pt, not 144pt.

The bridge still reads declared native JSON properties rather than the complete
resolved style chain. Font bytes in the old captures are not individually pinned.
All results remain backtests against archived evidence. Geometry, sub/superscript
precision and the unresolved initial-baseline rules still require work.

The remaining `kinsoku`/`kinsoku2` line-count failures have a capture consistency
question that must be investigated before changing engine line breaks. For
example, `kinsoku` page 2 reports source 63..66 as line 3, then 66..101 as line 1;
the corresponding first PDF glyphs share y=87.12pt and advance continuously in x.
`kinsoku2` also reverses from line 3 to line 1 within these paragraphs. This round
does not merge those records by y, rewrite the captures or turn the failures into
passes. An independent structure observation or a justified invalid-capture gate
is needed to settle the discrepancy.
