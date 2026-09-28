# Exact Vertical Holdout Probes (R04)

These six inputs are UNMEASURED. They contain no expected glyph y, line box,
advance or page count. Candidate predictions are frozen separately under
artifacts/exact-candidate-r04-2026-09-28/, before any capture.

Each case is a sequence of single-label probe paragraphs (P000, P001, ...),
each with its own exact line value, followed by the canonical references R001
and R002 (Times New Roman 12 pt, exact480). Probe body and mark are Times New
Roman, 12 pt except the clip case (24 pt). Tops are 720 twips except the two
fractional-top cases (725 and 723 twips). All cases fit on one page.

The package profile is identical to the canonical exact inputs: keepNext,
keepLines and widowControl false, zero paragraph gaps, left alignment, four
explicit font slots with sz/szCs and vanish false, compatibility mode 15, one
column, no docGrid, styles or docDefaults. Paragraph marks are CR.

Run python3 tools/measure/make_exact_holdout_fixture.py --out NEW_DIRECTORY to
reproduce the packages, manifest and README; --check EXISTING_DIRECTORY audits
them read-only. Neither invokes Word nor the layout engine.
