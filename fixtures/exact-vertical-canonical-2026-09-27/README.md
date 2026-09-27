# Exact Vertical Input Probes

These 12 canonical inputs are UNMEASURED. They contain no expected glyph y,
baseline, line height, advance, page count, or control glyph count. They prepare
new Word observations; historical noncanonical inputs are not an oracle here.

The exact480 baseline contains E000 followed by reference paragraphs R001 and
R002. Four cases independently vary 12/24 pt body and mark sizes. A fifth varies
only the body family to Arial. The short exact218 pair varies body and mark
sizes together (12 versus 24 pt), so it does not isolate their individual roles.

The structure pair has the same E000/E001/E002 probe labels, either as three
paragraphs or one paragraph with two textWrapping soft returns. Both then have
R001 and R002 reference paragraphs. The exact481 phase probe changes only the
three probe paragraphs' exact line spacing relative to exact480-three-paragraphs.
References always remain Times New Roman 12 pt, exact480. The empty-first case
has no body run in its first paragraph and an explicit 12 pt mark. The top721
case changes only pgMar.top from 720 to 721 twips relative to the baseline.

All paragraphs explicitly set keepNext, keepLines and widowControl false,
before/after spacing zero, exact line spacing, and left alignment. All text,
soft-return runs and paragraph marks explicitly set four font slots, sz, szCs
and vanish false. Compatibility mode is 15, with one column and no docGrid,
styles or docDefaults. pPr/rPr/sectPr property order is listed in the manifest.

The manifest records each input-derived UTF-16 range and CR/VT control.
Paragraph boundaries are CR, soft returns are VT; control identities must be
verified against native Word source scans before any PDF glyph pairing. Font
declarations are not actual font-file bindings. Capture native source, repeated
scans, font identity and vector output before deriving layout expectations.

Run python3 tools/measure/make_exact_vertical_fixture.py --out NEW_DIRECTORY to
reproduce the package, manifest and README. Existing directories are rejected.
Use --check EXISTING_DIRECTORY for a read-only deterministic/source audit.
Generation and checking invoke neither Word nor the layout engine. The XML
profile checks are not a complete OOXML XSD validation or proof of Word behavior.
