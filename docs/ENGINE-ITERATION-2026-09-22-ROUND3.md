# Measurement-driven engine iteration, round 3

Starting engine: `1fddc3a`. This round removes representational rounding in font
size and run rise, makes pagination reservations agree with the fine cursor,
and adds a new-capture stability gate after observing changing native Word line
ordinals. The numerical changes are not a newly established Word layout rule.

## Exact size and rise through the engine

`FontSpec` can retain an effective size in hundredths of a point. Under the
existing sub/superscript model, 10/12/18pt now becomes 6.60/7.92/11.88pt, rather
than 6.5/8/12pt. Metrics, direct shaping, fallback shaping, positioned glyphs,
oracle JSON, SVG, GPU atlas keys and raster hint caches all read the precise
size. Equivalent 8pt representations share one raster key; 7.92pt has its own.
The trace adds `sizeCentipoints`; the engine preregistration reader prefers it
and still accepts old `sizeHalfPoints` traces. Its text slices now use UTF-16.

Run rise has an optional fine value in the existing 1/7200-inch unit (0.01pt).
The 12pt example retains 408 units of superscript rise and 96 of subscript drop,
instead of losing them to 405 and 95 through integer twips. The same value is
preserved on ordinary text, wraps, forced fits, empty lines and terminator
fragments. The ordinary baseline is still quantized before subtracting rise;
this round does not change that unproven ordering or the existing 0.66/0.34/0.08
ratios. Their Word evidence remains one Liberation Serif 12pt backtest.

SVG reads the existing precise fragment origin even without a shaper. GPU
placement uses the precise glyph origin when converting to device pixels.
The GPU's existing raster-size/DPI limitation is separate and remains open;
the new DPI test establishes placement conversion, not DPI-correct bitmaps.
Skrifa's own 26.6 raster scale and 16.16 advance resolution also remain.

The approximate `SimpleMetrics` fine outputs now avoid reading back already
truncated coarse metrics, including at ordinary half-point sizes. Thus its
historical geometry is not wholly unchanged: at 8.5pt, `A ` advances 6.375pt
instead of 6.35pt, and natural height is 978 fine units instead of 975.
Its legacy `measure` integer outputs remain the coarse projection.

## Pagination consistency

A boundary test exposed an existing disagreement between coarse fit checks and
the fine page cursor. With an introductory 195.6-twip line followed by a
three-line `keepLines` paragraph in a 781-twip area, the coarse reservation
accepted the paragraph, then split it when the fine cursor ran out of space.
The result was `[3,1]` lines per page instead of `[1,3]`.

Paragraph reservation, `keepNext` lookahead and individual line fit now use
`height_fine`, the same height subsequently added to the cursor. Five new tests
failed before this change and pass after it, including exact capacity and one
twip less. Existing Exact/Auto/AtLeast tests also pass. These are internal
consistency checks, not new Word observations at page bottoms. Wrap-region
intersection probes still use coarse geometry.

## Native capture diagnosis and new admission checks

The separate [line-ordinal audit](CAPTURE-LINE-ORDINAL-AUDIT-2026-09-22.md)
records the historical six reversals and the local native experiments. The
reference implementation in `~/code/docx-layout` supplied protocol and
independent-observation guidance; that checkout was not modified.

Two source-identical fixture copies were opened in a newly started local Word
16.112.3 session. The controller first observed no open documents. Each fixture
had six 252-position observations and one PDF export. Only the two diagnostic
documents were closed, without saving; the remote Mac's existing Word process
was not operated. The audit distinguishes controller-reported observations
from independently hash-verified files and does not claim process containment.

For `kinsoku`, all six scans agree. For `kinsoku2`, the first scan follows PDF
export in the same AppleScript call and returns line ordinals 1 through 8 across
four pages. The next scan returns 1 and 2 on every page: 189 of 252 ordinals
change. Page ordinals and source line boundaries do not change in this new
example. Later scans agree, and the change precedes explicit repagination.
Both new PDFs' complete glyph arrays equal their respective old captures on
all four pages. The historical short-prefix reversal mechanism is still
unresolved; this does not prove background pagination caused it.

New production captures retain the original export/first-scan call sequence
and add one immediate repeated scan. Both stdout receipts, hashes and parsed
readings are saved. The first scan remains `sweep.json`. Changed readings,
parse/read errors, incomplete UTF-16 coverage, split surrogate boundaries or
invalid ordinals produce `META.usability=UNDECIDABLE`; the model rejects the
bundle before pairing, and `wm capture` returns exit status 2. There is no
retry-until-pass or replacement with the second reading. Even matching scans
do not establish correct line identity or PDF alignment.

The new gate detects the native `kinsoku2` pair's 189 differences. Eighteen
mocked capture tests cover storage, failure propagation and observation order.
No archived capture gains a fabricated stability result, and no old FAIL/VOID
is promoted using the new diagnostic readings.

## Replay and validation

Authoritative runs are `artifacts/engine-sweep-2026-09-22-round3/before-final`
and `after`. The earlier `before` directory predates the new capture code and
is retained only as an intermediate run. The final runs have identical
measurement-module bytes, input/capture hashes, font entries and source-binding
provenance. Tolerance stays zero and exclusions stay empty.

| Archived-capture measure | Before | After |
| --- | --- | --- |
| Admitted / rejected VOID captures | 24 / 5 | 24 / 5 |
| Structurally sound | 21/24 | 21/24 |
| Page counts equal | 24/24 | 24/24 |
| Full comparison | 24 FAIL | 24 FAIL |
| Selfcheck | 24 UNDECIDABLE | 24 UNDECIDABLE |

Every admitted capture retains identical glyph origins and advances, page
structure and comparison result. Those fixtures do not contain `w:vertAlign`;
the existing fixture that does, `probe-metrics`, remains VOID and excluded.
Consequently this sweep establishes no regression on the admitted archive,
not an independently measured sub/superscript improvement. Font-unit and
synthetic boundary tests establish the arithmetic and complete propagation.
The exact binary/module hashes and per-capture results are in
[the structured report](ENGINE-ITERATION-2026-09-22-ROUND3.json).

Validation: 189 Rust workspace tests, 126 Python tests, strict workspace Clippy,
workspace wasm target checking and `git diff --check` pass. Independent review
caught the pagination reservation issue and a capture-label variable shadow;
both have regression coverage. `Fragment::Text` remains inline, with a focused
large-enum lint exception to avoid adding a heap allocation per text fragment.

API changes: the unpublished Rust structs `FontSpec`, `Run`, `TextFragment`,
`PositionedGlyph` and `GlyphRecord` gain precision fields. External exhaustive
struct literals need updating. Existing half-point constructors and old trace
reading remain available; `GlyphKey` stores canonical centipoints. Setting an
exact override takes precedence over later direct writes to its legacy field.
GPOS offsets remain in twips; resolved styles, initial-baseline rules, general
cross-run kinsoku reflow and the remaining archive geometry errors remain open.
