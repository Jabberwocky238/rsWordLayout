# Measurement-driven engine iteration, 2026-09-22

Starting point: `origin/main` fast-forwarded from `5b4f5a6` to `19a290e`.
Reference implementation: the existing `../docx-layout` checkout, used read-only;
see [the evidence survey](ORACLE-REFERENCE-2026-09-22.md).

This iteration replays archived Mac Word captures. It starts no Word process,
changes no capture, and does not infer Windows behavior from Mac results.
The changes are backtested against existing readings, not independent new
Word experiments. Zero tolerance and the existing comparison rules remain
unchanged.

## Changes

- S-1: disable optional Latin `liga`, `clig`, `dlig`, and `hlig` while preserving
  required script shaping. Zapfino has AAT `morx`, not GSUB: its default rare
  ligature survives `liga=0` but is disabled by `dlig=0`. Synthetic AAT tests
  establish both that the test font actually forms a ligature and that optional
  and required selectors receive different treatment.
- Preserve UTF-16 shaper cluster spans through fallback runs and paint. A
  ligature, non-BMP character, combining sequence, or reordered cluster no longer
  gives an unrelated trailing space the entire fragment's source interval.
- S-2: paragraph endings and inline breaks are distinct. A page/soft break
  terminates the line where it occurs. All 33 source intervals and physical page
  assignments in `breaks-sections` match the archived sweep exactly, including
  nonpainting section marks. A trailing page break retains the paragraph mark
  on its breaking line; a trailing soft return creates the final empty line.
- S-3, partial: restore missing visible terminal spaces, retain empty lines and
  source-only controls, and preserve the control run's font/color/rise. Leading
  objects and objects after a page break retain their source positions. The
  unresolved default paragraph-mark font and lone-page-break count remain below.
- S-4: resolve actual full/PostScript/compatible names from font tables, without
  guessed suffix removal or using fallback to satisfy `--require`. Ordinary
  family requests continue to select by bold/italic. TTC face identity now
  includes its index through metrics, shaping, paint and rasterization.
- Preserve the fine natural line-height accumulator when `fit()` emits a
  wrapped prefix. Previously that path used the rounded content-height fallback;
  a multi-line regression with a fractional-twip step now detects the drift.
- Add an offline sweep runner: bind fixtures by their captured SHA, verify
  required font names, retain VOID/missing/undecidable states, and record input,
  font, executable and measurement-module hashes. Each run snapshots the engine
  executable so a concurrent rebuild cannot mix engine revisions between cases.

## Validation and replay

The checked-in compact result is
[`ENGINE-ITERATION-2026-09-22.json`](ENGINE-ITERATION-2026-09-22.json).
Full local outputs are under `artifacts/engine-sweep-2026-09-22/` (gitignored).
`before` uses the clean `19a290e` checkout; `verified` uses the final implementation.
Other directories are intermediate runs and are not the final result.

| Measurement | Baseline | Final |
| --- | --- | --- |
| Capture directories considered | 29 | 29 |
| VOID captures excluded | 5 | 5 |
| Engine traces admitted to comparison | 22 | 24 |
| Structurally sound, same 22-case scope | 14/22 | 15/22 |
| Structurally sound, all admitted cases | 14/22 | 17/24 |
| Page counts equal, all admitted cases | 22/22 | 24/24 |
| Selfcheck FAIL | 1 | 0 |
| Comparison state | 21 FAIL, 1 UNDECIDABLE | 21 FAIL, 3 UNDECIDABLE |

There are no structural regressions among the original 22 compared cases.
`font-free` changes from 717 to 720 glyphs, matching Word's 720; its three
glyph-count mismatches are gone. The two newly admitted cases are `first-line`
and `cursor-unit`, both structurally sound, contributing 216 and 3,048 paired
glyphs. `breaks-sections` changes from 75 to 87 glyphs (Word: 90), and `vmisc2`
from 83 to 87 (Word: 88). Their formerly definite glyph-count mismatches vanish
on the pages the comparator can admit, but undecidable pages remain.

**No case is a complete zero-error acceptance.** All 24 admitted traces still
have a selfcheck UNDECIDABLE result, often because that capture has no
standalone one-character terminator line for that particular check. This is
neither a hard failure nor a pass. The remaining geometry errors are retained.
Both runs used identical fixture and font-file bytes for every common binding;
all comparison-module hashes are identical between baseline and final runs.

```sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings
cargo check --workspace --all-features --target wasm32-unknown-unknown --locked --offline
PYTHONPATH=tools/measure tools/measure/.venv/bin/python -m pytest tools/measure/tests -q

cargo build -p rsword-layout-core --features fontenv --bin layout-trace --locked
tools/measure/.venv/bin/python tools/measure/sweep.py \
  --trace-bin target/debug/layout-trace \
  --font-dir /System/Library/Fonts --font-dir fixtures/fonts \
  --font-dir "$HOME/Library/Fonts" --font-dir /Library/Fonts \
  --output artifacts/engine-sweep-next
```

The runner intentionally returns nonzero for remaining failures or undecidable
cases. A completed replay is not a zero-error Word acceptance claim.
Font names and current font-file hashes are recorded, but old META files do not
pin each captured font's bytes; a matching name alone does not prove version
identity. Full-name requests combined with contradictory bold/italic properties
also remain uncalibrated.

Validation: 141 Rust tests and 62 Python measurement tests pass; strict Clippy,
the workspace wasm target check and `git diff --check` pass. Runtime fixture
tests use bundled fonts. System Mac fonts are used only in the archived-capture
replay. The Python replay environment uses Python 3.12.13 and the same pinned
`pdfminer.six==20231228`; replay performs no new PDF extraction.

## Remaining work

1. S-3: `breaks-sections` still draws 87 glyphs versus Word's 90. The three
   residual glyphs occur in paragraphs containing only a page break. Its Word
   count model still reports those pages undecidable. `vmisc2` section 5.1 also
   documents a Times New Roman paragraph-mark glyph in a control-only paragraph.
   The adapter currently has no separate resolved paragraph-mark style. Do not
   hide these issues by weakening counts, removing pages, or assuming every
   terminal space inherits the preceding run.
2. S-5: integer half-point sizes lose the fractional `vertAlign` scaling, and
   integer twips lose rise precision. A precise-size branch must reach shaping,
   metrics, glyph records, SVG, GPU cache keys and rasterization together.
   `probe-metrics` is VOID: its pre-registration's P1-P5 are not independently
   verified Word rules. The existing scale/rise factors have a narrower backtest
   basis and must not be promoted by an engine-only check.
3. S-8: the pinned `rsword a8d24ea` parser does not expose `overflowPunct` in
   native JSON, even when `kinsoku2.docx` explicitly disables it. Fix the parser
   projection before adding a default-enabled layout flag; otherwise the on/off
   fixtures become indistinguishable.
4. S-6/S-7/V19: the unresolved vertical metrics and initial baseline formulas
   remain. The East Asian 1.3 observation does not establish whether script or
   code-page metadata is causal. Do not fit another constant to the same captures
   and call it an independent validation.
5. `rustybuzz 0.20.1`, `src/hb/kerning.rs`: its `kern=0` path can skip restoring
   RTL buffer order after reversing it. This is reproducible with DejaVu lam-alef
   and predates this iteration. Cluster tests validate ownership under both kern
settings; they do not certify full bidi layout.
6. Investigate coarse versus fine height in pagination: `line_height_fine`
   respects exact spacing without the font-content minimum, whereas the coarse
   `line_height` path still applies that minimum. A boundary fixture is needed
before declaring pagination unaffected for oversized text on exact lines.

API note: `ShapedRun` gains an optional `source` field. External shaper struct
literals must initialize it (use `None` when provenance is unavailable). Nonzero
TTC indices now appear in opaque face identifiers as `hash:index`; callers must
not assume every face identifier is a bare content hash.

Tailscale discovery found `usermacbook-pro` and `USER809E` online; the previous
Windows oracle host `DESKTOP-0U9GEGH` was offline. No remote process was changed.
The reference survey identifies archived Windows fixtures that can support the
next paragraph/keep/section work without requiring that machine to reconnect.
