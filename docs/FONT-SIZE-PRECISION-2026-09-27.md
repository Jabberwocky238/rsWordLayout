# Exact font size in ideograph/numeric spacing

`FontSpec.size_centipoints`, when present, overrides the rounded legacy
`size_half_points` field. Shaping, ordinary character widths, fine advances
and vertical metrics already consume that exact size. The integer-width
ideograph/numeric spacing path in both `SimpleMetrics` and `RealMetrics`
still consumed the legacy field, so line fitting could use a different
size from shaping.

For example, the existing superscript projection of 12 pt is 7.92 pt,
with a legacy size of 8 pt. Under the engine's existing quarter-em spacing
rule, each integer-width gap should use `floor(7.92 * 20 / 4) = 39` twips.
The old call used 40. For an ideograph-digit-ideograph sequence, the stub's
character widths total 396 twips: the correct integer width under the
existing rule is 474, while the old result was 476. At a 474-twip content
width that changed a one-line input into two lines.

Both metric providers now pass `effective_size_centipoints()` to the shared
integer spacing helper. Each gap retains the previous truncation to integer
twips; this change does not choose a new rounding rule. Multiplication uses
a wide unsigned intermediate and clamps the helper's result at the signed
twip limit. The fine advance path, quarter-em ratio, boundary classification
and behavior across separate runs are unchanged.

This fixes an internal input inconsistency. It does not establish that the
existing spacing ratio or quantization matches Word for superscript text,
and it does not map DOCX sizes to Word's private font-selection records.
Those are separate evidence questions.

## Verification

All four new integration regressions failed before the production change.
After it, those four and the three spacing unit tests pass. The integration
checks exercise exact-size precedence, ordinary half-point behavior and
actual Engine line ranges at adjacent widths. With the bundled Liberation
Serif and Droid Sans Fallback fonts, independent font-table advances total
395 twips; two gaps add 78, so the line fits at 473 and wraps at 472. The
stub independently fits at 474 and wraps at 473. Both preserve UTF-16 source
ranges and the paragraph terminator.

Commands, exit codes, source hashes and the original failing output are
retained in `artifacts/exact-size-autospace-2026-09-27/`.

`cargo test --offline --workspace --features fontenv` passes 622 tests,
with 12 existing ignored tests. Workspace/all-targets Clippy with fontenv
and `-D warnings` passes. The rebuilt CLI preserves all 186 ordered line
ranges across the 11 archived Android captures under the explicitly
accepted legacy narrow-view assumptions. This is a conditional boundary
regression, not a new Word capture or a geometry/page alignment claim.

The validation bundle's 77-member `SHA256SUMS` has SHA-256
`bf124dff017608bbd01ab035add78bef317d9ee383e280a72b8bd83b796090d7`.
