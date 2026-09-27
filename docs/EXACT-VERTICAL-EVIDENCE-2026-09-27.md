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

## Canonical discriminating inputs

[`fixtures/exact-vertical-canonical-2026-09-27/`](../fixtures/exact-vertical-canonical-2026-09-27/README.md)
contains 12 deterministic inputs, initially UNMEASURED and subsequently captured
in the independent batch recorded below. Its manifest SHA-256 is
`d538d7798687c9b20fd633083ee0d5a2159430a145b3a1ca5fd96ecef74b22eb`.
`tools/measure/make_exact_vertical_fixture.py --check DIRECTORY` independently
walks package XML and source offsets, and compares the deterministic bytes.

- An exact480 two-by-two matrix varies body and mark sizes independently:
  12/12, 24/12, 12/24 and 24/24 pt. Another case changes only the body to Arial.
- The exact218 pair changes both body and mark together, 12/12 to 24/24 pt.
  It tests a short line and does not isolate the two contributions.
- Three separate paragraphs and one paragraph with two soft returns use the
  same probe labels; their UTF-16 source differs only at the two CR/VT controls.
- exact481 changes the three probe paragraphs' advances; top721 changes only
  the top margin of the baseline case. An empty first paragraph isolates the
  absence of a body run.

Every case ends with two TNR 12 pt/exact480 reference paragraphs. All inputs
explicitly disable keepNext, keepLines and widowControl, have zero paragraph
gaps, compatibility mode 15, one column, and no grid/styles/docDefaults.
Body, soft-return runs and marks have four explicit font slots, size/szCs and
vanish=false in the canonical property order. These source/profile checks are
not complete XSD validation, font-file binding or proof of Word output.

No y, line-box size, page count or control glyph count is prefilled as expected
Word output. The next native capture must bind the source bytes, repeated CP
scans, resolved fonts and vector output before testing a candidate algorithm.

The real CLI parsed and painted all 12 inputs with explicitly bound TNR/Arial
font files, Mac grid and print view. Input checks cover 40 independent mark
declarations/effective properties, 164 body glyphs, 42 source controls and 42
engine lines. All 206 glyphs retain the expected source/style identities;
source warnings, notdef and unassigned counts are zero. Exact values
218/480/481 produce integer diagnostic advances 1090/2400/2405. These are
engine-input checks, not observed Word expectations.

`artifacts/exact-vertical-probes-2026-09-27/run-02/manifest.json` has SHA-256
`da0b25672571fc0534ac4309b30f04012174bafe4785c07fbdc2c236dc98843f`.
The first validator incorrectly expected `LINE_BREAK` instead of the existing
JSON contract `SOFT_RETURN`; its failed result and script snapshot are retained.
After fixing that check, all 12 cases pass and all 12 trace hashes are identical
between the two runs. No fixture, engine behavior or capture was changed to
obtain the passing validator result.

## Native host arithmetic candidate

The installed Word 16.112.3 / 16.112.26083020 arm64 host contains an actual
two-stage integer calculation. This supplies a new source-derived candidate;
the public property mapping and runtime branch selection are still unknown.
The whole host binary SHA-256 is
`b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c`,
and its arm64 slice SHA-256 is
`ee57641615f6889e854e058e665e3932f23db42c0f947c6ec8cc52d4280bd94d`.

The host wrapper at `0x100379ef4` forwards four integer components to
`PTLS7::LsModifyLineHeight` and conditionally `LsModifyDisplayLineHeight`.
Its caller at `0x1003789a8` contains this bounded branch, using neutral field
names rather than assuming that recovered offsets are OOXML properties:

```text
H = signed16(thread_context + 0x38)
S = signed32(*global_slot_0x10472c620 + 0x18c)
T = helper(abs(H), S, 1440)
... branch conditions ...
C = helper(T, 4, 5)
signed32(line_record + 0xc0) = C
```

The first helper call is at `0x100378f1c`, the second at `0x100379800`, and
the store at `0x10037988c`. Reaching the second call depends on a negative H,
intervening object/compatibility gates, a virtual-method result and selector
`thread_context+0x6c == 0`. Neighboring branches instead use one half, one fifth
or a difference from another component. Later code can modify the result.
Consequently, the existence of the 4/5 operation does not make it a universal
baseline proportion or establish `C` as a PDF glyph offset.

For normal in-range arguments the helper at `0x100064708` calculates `a*b/c`
with nearest rounding, ties away from zero. A separate instruction
transliteration agrees with independent rational arithmetic on 59,628
synthetic checks; it did not execute the native binary. The saved analysis
also distinguishes division by zero and the ARM64 fast-path signed-division
overflow exception, so the helper is not described as globally saturating.

The two scale slots are `0x10472c620` and `0x10472c628`. Their encoded Mach-O
fixups point at an initial structure in `__DATA.__common`, a `S_ZEROFILL`
section. Offline zero-initialized words at +0x188/+0x18c are not file-backed
runtime scale values. Initialization/selection can replace these values; no
unconditional scale of 300 was found. The frozen native README initially used
the inaccurate phrase "file-backed words"; the companion zero-fill correction
records the section evidence while preserving the original artifact hashes.

Before forwarding to PTLS, `0x100379b98` also converts multiple components
between the two scale contexts and reconciles their sum with a separately
rounded total, distributing integer remainders. That is a concrete next
dataflow target. It has not been established as the cause of the residuals.

Static files are in `artifacts/exact-baseline-native-2026-09-27/`; its
11-file `SHA256SUMS` has SHA-256
`743dde772e90b77ae1cdc71fffc3765c940a836bf56005210014ad3418c75677`.
The additive correction is
`artifacts/exact-baseline-native-2026-09-27-zero-fill-correction.md`.
Its SHA-256 is
`7ee26b530709425518cec2d305cd63037c1db5fdc6291161889e0914e1d6f696`.
All disassembly was offline, with no process launch, attachment or Word event.

## Fixed retrospective candidate result

The candidate evaluation fixed S=300, both rounding operations, and this
component-to-PDF mapping before reading the bound old-input table:

```text
predicted_y = (topTwips * 300 / 1440
               + round(round(lineTwips * 300 / 1440) * 4 / 5)) * 72 / 300
```

S=300 was an explicit hypothesis motivated by the 0.24 pt PDF lattice,
not measured host state. No scale search, coefficient fitting, added bias or
font-specific adjustment was performed. All arithmetic uses rational values.
Archived decimal origins are also retained; lattice normalization only removes
serialization residuals below the predeclared 1e-8 pt bound.

Of 113 bound page starts, 110 satisfy the exact/zero-before/unraised-origin
scope: 98 match and 12 fail. Three atLeast entries remain OUT_OF_SCOPE. All
12 failures have observed y greater than predicted y by exactly 0.24 pt:

| exact line (twips) | Failing observations | Predicted y (pt) | Word y (pt) |
| ---: | ---: | ---: | ---: |
| 256 | 6 font families | 82.08 | 82.32 |
| 280 | 1 | 83.04 | 83.28 |
| 304 | 1 | 84.00 | 84.24 |
| 328 | 1 | 84.96 | 85.20 |
| 400 | 1 | 87.84 | 88.08 |
| 520 | 1 | 92.64 | 92.88 |
| 640 | 1 | 97.44 | 97.68 |

The previously observed canonical exact480/top720 column point matches and is
reported separately. Eight local vmisc2 matches do not upgrade that bundle's
UNDECIDABLE status. The table above is retrospective counterevidence to the
complete hypothesized mapping, not an independent validation or disproof of
the native branch itself. The engine baseline rule remains unchanged.

`artifacts/exact-baseline-candidate-2026-09-27/results.json` has SHA-256
`d4695ce4f04bc1688b937ec950271a803f8c9665dfcc897d6a45a4408f9f1cb1`.
Its four-file `SHA256SUMS` has SHA-256
`dbba25d60c4b5eecadebc74fc05bb32e78c57be62df8843e60fb03f6ebb892fd`.
Next work is to identify scale units, selector/property mapping and subsequent
component-to-glyph conversion, then test them against the new canonical inputs.

## Scale selection and component reconciliation

A subsequent bounded offline slice identifies actual scale initialization
paths, without measuring the active mode. The initializer at `0x10006f468`
uses `input & 3`: mode 2 supplies 294912 on both axes, mode 3 supplies 1440,
and modes 0/1 obtain their values from other records. The parent at
`0x10006e9c8` contains the diagnostic string `Unexpected SetFlm call during
font loading`. Its request's low three bits select slot 620, and bits 3..5
select slot 628; these selectors are distinct from the initializer's modes.
Some paths alias the two pointers. Others use a two-stage nearest-rounded
72/100 calculation; cancelling the factors would discard an intermediate
rounding step. No physical DPI or PDF export mode name is established.

The complete reconciliation function `[0x100379b98,0x100379ef4)` has now been
extracted into a neutral model. Let `Fxx` denote the record's signed 32-bit
field at hexadecimal offset xx, and `S0/S1` the two scale words. The branch
tests **pointer identity**, not equality of the scale values. When pointers
are equal it copies six fields and returns these differences:

```text
B = Fb8 - (Fc0 + Fc8)
C = Fc0 - Fd8
```

For distinct pointers, the function separately rounds the total, four
partition components and another extent using `R(v,S1,S0)`, then distributes
their discrepancy. Positive discrepancy adds first to B, then C, then B.
Negative discrepancy first removes from the other components; when those
cannot supply a unit, a helper compares reverse-converted differences to
choose B or C. Its comparison is not simply which component is larger.
This path also clamps one extent; the equal-pointer path does not.

The caller forwards `(Fc4, B, C, Fd4)` as four PTLS height arguments. Thus
the earlier `4/5` result stored at Fc0 is split between differences rather
than passed unchanged as one height. Other conditional writes can change
Fc0 before reconciliation. A subsequent conditional hook can independently
increase Fc4 and Fd4 before forwarding. The hook's increments remain unknown.

The model agrees with a saved-instruction interpreter on 10,385 complete
function inputs, visiting 253/253 instruction addresses. Another 12,744
checks cover the rounding helper's signed and overflow boundaries. These
are static-model checks, not execution of native Word. Automatic conversion
conserves a nonnegative partition under the documented finite positive-scale
conditions and a nonnull adjustment context. Conservation is not universal:
four synthetic explicit-total-override counterexamples are retained. The
first overly broad conservation assertion and its failure log are preserved.

The PTLS display entry supplies a further boundary. Under bit 1 of the
display-subline record's +0x58 flags, `LsDisplayLineNew` rounds the incoming
point using +0x64/+0x5c on x and +0x68/+0x60 on y before calling
`LserrDisplaySublineCore`. Later code uses inverse ratios. This establishes
another conditional conversion, not the incoming point or the PDF origin.

The bounded artifacts and their hash-list SHA-256 values are:

| Directory under `artifacts/` | Hash list | SHA-256 |
| --- | --- | --- |
| `exact-scale-native-2026-09-27/` | `SHA256SUMS`, 5 files | `d2371ee4fb8bbc6929a76fecab28c803d9bf42e93627140367aee20c521d3912` |
| `exact-height-reconcile-2026-09-27/` | `SHA256SUMS`, 11 files | `bab4723bcbedc9cde66b3b1a41af7493c74937848baae977469d1a63327c2827` |
| `exact-display-origin-native-2026-09-27/` | `SHA256SUMS`, 3 files | `7fa1526e25c99c0d393d6f4ad702c8c36d89a7e41ed274a907df66f5f2ece576` |

The reconciliation passing report has SHA-256
`29963e16369aa40cdf1454ea9c2433abbf17f30f6c9636c3ecba2f8a764fd711`.
Host and PTLS hashes match the preceding evidence. No production formula or
old frozen artifact changed. Runtime selectors/scales, the complete public
property mapping and the final glyph-origin calculation remain unresolved;
these static findings do not rescue the rejected fixed-300 candidate.

A separate selector slice proves a representation conversion, while leaving
its public names unresolved. `0x100338fb0` converts H at context+0x38 and
F at +0x3a into three halfwords: negative H becomes `[1,-H,0]`; nonnegative
H with F=0 becomes `[0,H,0]`; otherwise it becomes `[2,0,H]`. Another path
preserves selector +0x6c values 0..4 and reports an error before mapping
larger values to 4 in a destination object's byte. The height consumer reads
the raw context selector, so that separate validation is not its proven
precondition. A discovered 696-byte context write is a paired save/restore,
not an original property setter. No OOXML enum or default is inferred.

`artifacts/exact-selector-map-2026-09-27/SHA256SUMS` binds six files and has
SHA-256 `1aac3f2449fe05a65bb2bc38e6615b8551d1494cdc27ffd7355e327f4b2980ea`.
Its README records the bounded lookup and the next concrete context-population
entry, `0x100353de4`, rather than assigning names from historical offsets.

Following that entry in one further bounded slice finds parameter forwarding,
property-block lookup/validation and cache copies. It still supplies no named
public property, enum or default. The observed 696-byte copy from context+0x28
to +0x1078 saves the current block; it is not an original property setter.
Unrelated accesses at the same numeric object offset cannot establish type
identity. This query stops without expanding more unnamed calls.
`artifacts/exact-property-map-native-2026-09-27/SHA256SUMS` binds five files,
SHA-256 `73cf11777906fdf745c0fc1d36a1f06e2b8a2acb00c64d62959c2a9307d5dea6`.

## Two higher-scale scalar hypotheses also fail

The explicit native initialization values allow two fixed follow-up
hypotheses, S=1440 and S=294912. Their formulas were recorded before the new
evaluator ran, with no scale search or per-input adjustment:

```text
L = R(lineTwips * S / 1440)
C = R(L * 4 / 5)
O = R(topTwips * S / 1440)
predicted_y = R((O + C) * 300 / S) * 72 / 300
```

R is nearest integer, ties away from zero. The final 300-unit lattice is
still a hypothesis from PDF observations, not a measured runtime scale.
The component-to-origin mapping, reconciliation and hook effects remain
unproven. Historical observations had already been read; this evaluation is
retrospective. It does not consume any new canonical exact capture.

Both candidates match 87 of the same 110 eligible historical origins and
fail 23; the other three rows remain out of scope. Their predictions coincide
on this input set. Both match the previously known canonical column point.

| Exact line (twips) | Count | Predicted y (pt) | Word y (pt) |
| ---: | ---: | ---: | ---: |
| 248 | 4 | 81.84 | 82.08 |
| 272 | 1 | 82.80 | 83.04 |
| 296 | 1 | 83.76 | 84.00 |
| 320 | 17 | 84.72 | 84.96 |

Delaying scale conversion resolves the preceding fixed-300 candidate's 12
failures but introduces these 23. The complete scalar explanation remains
rejected. Choosing a different candidate per row or adding a one-cell bias
after seeing the results would be fitting, not recovered behavior.

`artifacts/exact-native-scale-candidates-2026-09-27/` retains the fixed
assumptions, exclusive-output evaluator and all row results. Its four-file
`SHA256SUMS` has SHA-256
`7def15e7de2be3682835616a269307ba6893f58c3c15b03b4ef5070deda79b1b`;
`results.json` has SHA-256
`60d8c4db7624cf21c93741c046de8f513d815d5afe30cc17a6bded8a192953e5`.
The first reader invocation stopped before output on a misspelled JSON key;
fixing it did not alter either declared formula. Source and prior frozen
capture bindings were rechecked. No production rule changed.

## Canonical Word capture: independent body and mark interventions

All 12 new canonical inputs were captured on Mac Word 16.112.3 between
2026-09-27 05:05:55 and 05:11:24 UTC. The original source manifest is unchanged.
Each case has its actual PDF, unchanged DOCX, exact native content/paragraph
ranges, two complete CP scans, font preflight, per-CP PDF font/size/rise audit,
and source/PDF hashes. All 206 source units, including 40 paragraph marks and
two soft returns, are accounted for. All cases are one page; there are 42
native lines in total. Each owned document was closed without saving and
followed by an empty native document/window inventory.

These are raw PDF glyph origins, presented to two decimal places. The extractor
still reports `UNCALIBRATED` and no measured `lineBaseline`; do not reinterpret
the readings as line boxes, required extent or ink clipping. All labels have
x=36 pt, and all audited glyph rise values are zero.

| Controlled input | Probe glyph y (pt) | R001 / R002 y (pt) |
| --- | --- | --- |
| exact480, all four body12/24 x mark12/24 combinations | E000 55.20 | 79.20 / 103.20 |
| exact480, Arial12 body / TNR12 mark | E000 55.20 | 79.20 / 103.20 |
| exact218, body/mark both12 or both24 | E000 44.64 | 66.00 / 90.00 |
| exact480, three paragraphs | E000/E001/E002 55.20 / 79.20 / 103.20 | 127.20 / 151.20 |
| exact480, two soft returns within one paragraph | E000/E001/E002 55.20 / 79.20 / 103.20 | 127.20 / 151.20 |
| exact481, three probe paragraphs | E000/E001/E002 55.20 / 79.20 / 103.20 | 127.44 / 151.44 |
| exact480, empty first paragraph | CP0 mark 55.20; no E000 label | 79.20 / 103.20 |
| exact480, top margin721 instead of720 twips | E000 55.20 | 79.20 / 103.20 |

The independent 2x2 size inputs resolve the earlier confounding: actual body
and mark sizes each change in the PDF, but neither intervention changes these
origins. Arial changes the actual body font while the mark remains TNR; the
origins still agree. These controlled cases contradict the current maximum-run
ascent placement and do not support attributing the offset to mark size alone.
They do not establish a universal font-independent formula or a replacement
scalar rule. The earlier scalar counterexamples remain failures.

The soft-return and paragraph cases have different source controls and native
paragraph counts; their equal label origins are a measured comparison, not an
assumption that the source structures are equivalent. The 481 case affects only
the three probe paragraphs; both references retain exact480. Its delayed
0.24 pt difference is not interpreted as a measured unquantized advance.
The empty paragraph's control is bound only after checking the entire ASCII
PDF stream against the exact source, one whitespace per declared CR/VT, with
independent unique-label/CP/native-scan checks. No glyph ordinal is treated as
source identity without those checks.

The final batch is `artifacts/exact-vertical-word-continuation2-2026-09-27/`.
Its 631-file `durable-hashes.json` has SHA-256
`2e4b62f80da009d26cfe16466102959863f70c468b914ecdee92c8fc04e9572b`;
`offline-audit.json` has SHA-256
`fa2acca8c821dba3102cc8381df3c19e704dc0022c543de5860c723a3692dbed`.
All prior frozen capture files were rechecked unchanged. The two earlier
pre-event stops and the first open's conservative UNCONFIRMED status remain
recorded. After an exact-file access grant, the same already-open document
was identified and resumed without repeating open. Each subsequent grant was
limited to the matching owned file, with AX/path receipts. All 133 Word events
are terminal with process return code zero; final inventory is 0/0.

Each case has one PDF export, so two CP scans establish source-scan stability,
not repeated-export stability. This single-page ASCII batch does not validate
pagination, mixed scripts, docGrid or a generalized baseline algorithm.
