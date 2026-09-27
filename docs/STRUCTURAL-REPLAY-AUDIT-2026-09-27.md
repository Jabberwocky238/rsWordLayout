# Archived structural differences: source identity first

The Mac replay after `a7d099d` has three captures with structural differences:
`page-start`, `kinsoku`, and `kinsoku2`. Their causes are different. The first
has real page-placement differences; the latter two contain contradictory
line ordinals. None of these findings is a new Word capture or an acceptance
PASS. Historical capture files and frozen replay reports remain unchanged.

## Kinsoku: reject contradictory source line readings

Both engine traces have four pages with two lines per page. The paragraph-local
UTF-16 partitions are 38/25 units for `kinsoku` and 36/27 for `kinsoku2`, matching
the already established source boundaries. The archived scans instead give
short prefixes of pages 2-4 line number 3 or 7, then decrease to 1 within the
same recorded paragraph and page. Folding each ordinal change into a line
creates a spurious third line. The six exact transitions and their independent
diagnostic limits are preserved in the
[earlier ordinal audit](CAPTURE-LINE-ORDINAL-AUDIT-2026-09-22.md).

`wordmodel.build` now checks for this contradiction before assigning PDF glyphs
to lines. It only applies when both adjacent UTF-16 units have unambiguous
ownership by the same valid recorded paragraph. Affected pages retain their
actual PDF page identity and become `UNDECIDABLE`, with an explicit
`SOURCE_LINE_ORDINAL_DECREASE` reason and no inferred line partition. Other
pages continue through the existing checks.

The check allows page-local resets, globally increasing line numbers,
nonconsecutive increasing numbers, and resets across recorded paragraph or
section boundaries. Missing or ambiguous paragraph ownership does not prove
this particular contradiction. Old bundles are not rejected merely for lacking
repeat-scan metadata. Existing invalid UTF-16 coverage and scalar-boundary
errors retain their original reason. No PDF y clustering, corrected ordinal,
later scan replacement, or relaxed geometry tolerance is introduced.

## Page-start: a real font-height and pagination gap

The unchanged fixture has 322 paragraphs and 1,288 UTF-16 units. Every paragraph
is one line. All 322 engine/reference line source intervals agree, and all 966
visible label characters agree with the stored PDF extraction. Nevertheless,
35 paragraphs, or 140 UTF-16 units including paragraph marks, move to an earlier
page in the engine. Both outputs have 36 pages.

| Font | Sizes, pt | Extra paragraphs on each first page |
| --- | --- | --- |
| Brush Script MT | 29 / 33 / 35 | 4 / 4 / 4 |
| AppleMyungjo | 29 / 33 / 35 | 5 / 4 / 4 |
| Arial Unicode MS | 29 / 33 / 35 | 4 / 3 / 3 |

These nine groups produce 18 pages with different source membership. The other
nine groups, Khmer Sangam MN, Lao Sangam MN and Silom at the same sizes, have
matching page membership. For example, the first Brush Script group occupies
Word source ranges `[0,60)` / `[60,84)` but engine ranges `[0,76)` / `[76,84)`.
This is not a source-pairing error or a regression from the mixed-run fix:
the complete before/after engine traces are byte-identical.

The source declares auto/240 line spacing, zero before/after spacing and
`widowControl=false`. Run and paragraph-mark font/size declarations agree in
all 322 paragraphs, as do the trace's effective mark properties. Each line has
one run and one font. The package has no styles, settings or theme part, and
neither `docGrid` nor `snapToGrid` is declared. The replay's `--vertical-grid mac`
is the engine's coordinate-quantization policy, not a DOCX grid declaration.

The [page-start audit](../artifacts/page-start-height-audit-2026-09-27/README.md)
retains the full per-page CP membership, OOXML properties, current font tables,
fixture/capture/trace identities, and a reproducible offline audit script.
Capture metadata does not pin the historical font-file bytes; current font
hashes do not remove that limitation.

## A concrete arithmetic branch, still conditional

The earlier N7 fit correlated about 1.3 times the hhea height with East Asian
code-page declarations. It was explicitly a backtest and had counterexamples;
it did not establish a production multiplier. The old engine-sweep wording
was stronger than that evidence and is now qualified.

Offline host code supplies a more specific next step. In `0x102e4dcac`, the
post-rewrite metric charset in `{0x80, 0x81, 0x86, 0x88}` selects:

```text
extra = native_muldiv(V.i32[0], 15, 100)
F.c4 = wrap32(F.c4 + extra)
F.c8 = wrap32(F.c8 + extra)
```

The function then applies component floors; its preceding redistribution and
gate also matter. This is two integer additions with the native rounding rule,
not a floating-point multiplication of an inferred height by 1.3. The charset
set is SHIFTJIS, HANGEUL, GB2312 and CHINESEBIG5; JOHAB is not in it. The bound
provider writes the request's `lfCharSet` to the metric byte, and a default
charset can be rewritten before this gate. Neither step proves that OS/2 code
page bits alone select the branch for the three page-start fonts.

The [bounded alternate-tail evidence](../artifacts/font-vertical-alternate-tail-2026-09-27/README.md)
records the complete functions, exact field/stack destinations, table values,
native integer semantics and remaining input gaps. The next algorithm slice
can model these explicit inputs. Applying it to DOCX still needs the actual
font request, metric values, flags, units and active branch; no new production
font multiplier is enabled here.

## Validation

The new ordinal tests fail in six cases against the old implementation, with
16 controls passing; all 22 pass after the change. The complete Python suite
passes 823 tests. Independent review found no blocking issue.

The [offline replay](../artifacts/sweep-ordinal-validation-2026-09-27/README.md)
uses the same 25 frozen engine traces and exactly reproduces all 25 old CLI
reports before applying the new check. Only `kinsoku` and `kinsoku2` change,
from FAIL with three `LINE_COUNT_MISMATCH` failures each to UNDECIDABLE with
three `PAGE_UNDECIDABLE` reasons each. The other 23 reports are equal in full;
the five packages without admitted traces keep their existing result. No
comparison becomes OK. All 211 replay input files retain their hashes.

No Rust implementation changed or new engine trace was generated in this
slice. The complete Rust and Android checks from the preceding mixed-run fix
remain the last engine validation, rather than being counted again here.

Reviewed evidence manifests:

| Bundle | Members | SHA-256 of SHA256SUMS |
| --- | ---: | --- |
| page-start-height-audit-2026-09-27 | 5 | `c52813b7a2d69b850b714662d7c6356401485685ced77b2e4cadf5ae4c5f58b3` |
| font-vertical-alternate-tail-2026-09-27 | 8 | `d9d550272d707658b793f4f5710dcda6f07256324625b50ac5a06f9676e89741` |
| sweep-ordinal-validation-2026-09-27 | 73 | `8f489257b71b30c0037fe4081823461d8c9d0b8d0e2d0cddf277c63c1d6718c1` |
