# Exact spacing: vertical placement evidence

The engine still places exact-spacing glyphs using the maximum visible run
ascent. Its exact advance already comes from `w:line`, but those are different
decisions. Existing Word glyph origins contradict the ascent-based placement;
the replacement baseline rule remains unresolved.

## Input-bound observations

`artifacts/exact-baseline-investigation-2026-09-27/old-input-evidence.json`
binds 113 first lines from seven non-VOID historical bundles to native DOCX,
META before/after hashes, XML, source scans and retained glyph records. Its
SHA-256 is `6705ce17ae951de817b6d4704a3f5763bae408071ecb5c0f664359cf4cd22950`.
Those old bundles retain extracted PDF records; this audit does not claim to
have re-extracted missing original PDFs. EVALUATED with no falsifiers does not
turn a bundle-level UNDECIDABLE result into a complete geometry pass.

| Input | Word first glyph y | Current engine y |
| --- | --- | --- |
| font-free, exact222, six font families, 13 pt | 80.88 pt in all six | 96.48 / 86.88 / 89.04 / 84.96 / 83.28 / 82.32 pt |
| size-free, exact218, Luminari 8 / 13 / 20 / 30 pt | 80.64 pt in all four | 79.92 / 84.96 / 91.68 / 101.28 pt |
| vmisc2, exact320, body 10/20/10 or 8/30/8 pt, mark 13 pt | 84.96 pt | 91.68 / 101.28 pt |

The first two fixtures change the paragraph mark along with the body. They
exclude the current visible-ascent rule on these inputs, but cannot separately
identify mark participation. Their paragraphs also have the historical
noncanonical property order. Each repeated row is a separate paragraph, not
one paragraph with ten wrapped lines. Page top is 72 pt. Do not generalize
these observations to hidden runs, clipping, ink bounds, late pages or a new
compatibility mode.

The canonical `terminal40-noBalance0` column capture provides a second input
profile: top 720 twips, exact480, no paragraph gaps, explicit TNR 12 pt body
and mark, compatibility mode 15. C000 and C032, at the tops of the two columns,
have first glyph y = 55.2 pt, or 19.2 pt below the body top. Its actual PDF
hash agrees with META and glyph extraction, and the two source scans agree.
This is a glyph-origin constraint, not a measured line box. See
[column evidence](COLUMN-BALANCE-MAC-2026-09-27.md).

## Rejected shortcuts

- `exact-spacing` X1: restarting accumulation from the already quantized first
  glyph gives only 300/342. The unknown continuous initial phase matters.
- `a-form` Z1: no single relative baseline proportion explains all inputs.
- `fixed-distance` D1: no single distance above the line bottom explains them.
- A general affine `T + k*L` was also infeasible on the old combined 32-group
  exploratory backtest. This is a backtest, not an independent new experiment.
- The canonical exact480 offset happens to equal 0.8 times the advance. That
  observation cannot rescue the rejected universal proportion.

`shift-floor` is compatible with a continuous phase followed by truncation on
its measured inputs. It does not supply that phase as an algorithm. No fitted
baseline coefficient or universal fixed descent has been added to the engine.

## Correction to the historical atLeast summary

The original vmisc narrative claimed a constant 0.72 pt difference for all
three exact/atLeast pairs. The raw first glyph readings disagree:

| line (twips) | atLeast y (pt) | exact y (pt) | exact minus atLeast (pt) |
| ---: | ---: | ---: | ---: |
| 400 | 87.36 | 88.08 | +0.72 |
| 520 | 93.36 | 92.88 | -0.48 |
| 640 | 99.36 | 97.68 | -1.68 |

These correspond to zero-based PDF pages 9/10, 11/12 and 13/14. The original
P3 result, 0/18, remains a failure; the constant-offset explanation is wrong
within vmisc itself. vmisc3's differences +0.72 / 0 / -1.20 pt remain another
counterexample. The preregistered predictions and raw verdicts stay intact.

## Engine diagnostic channel

`LinePlacement` records actual engine decisions in integer 1/7200-inch units:
page top, cursor advance, required extent from that top, pre-quantization
baseline offset, and the absolute quantized baseline before run/mark shifts.
The page-wide line index carries these through `Page`, `PaintPage` and
`LineRecord` to `engineVerticalDiagnostic`. Missing metadata serializes as null.
Manual pages must provide their own metadata; glyph coordinates cannot supply
it. The existing coarse diagnostic box fields are not reconstructed from it.

The new field is diagnostic only and is excluded from Word acceptance. It
does not change spacing, pagination, glyph coordinates or the baseline rule.
In particular, `required` is the engine's fitting reservation, not glyph ink
height. Independent mark paint still does not contribute new line metrics.

Tests cover fractional origins and quantization before run shifts, keepLines
moving a trial to a new page, balanced columns followed by a continuous group,
sparse/missing page-wide IDs and a source-only empty section line. A private
formatter test preserves distinct advance/required values while confirming
that changing required extent does not move glyphs.
