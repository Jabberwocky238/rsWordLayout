# Keep-Chain Canonical Inputs

All eight inputs are UNMEASURED. They have not been opened in Word or run through
an engine. The competing label-page partitions in manifest.json are explicitly
nonexhaustive hypotheses, not an oracle or acceptance criteria.

The page is 11906 by 3360 twips with 720-twip margins: body height 1920 equals
four declared exact480 advances. This is a nominal four-line budget; actual
Word page capacity and required extents still need native/PDF observations.
Every paragraph has zero before/after spacing, explicit keepNext, keepLines,
pageBreakBefore=false and widowControl. All text, soft returns and paragraph
marks explicitly use four Times New Roman font slots, sz=szCs=24 and vanish=0.
There is one column, compatibility mode 15, and no docGrid, styles, docDefaults,
hard breaks, tables, header/footer content, or trailing empty paragraph.

| Case | Paragraph line counts | keepNext | Other true flag |
| --- | --- | --- | --- |
| f1-a1-b3 | F1 A1 B3 | A | none |
| f1-a2-b3 | F1 A2 B3 | A | none |
| a1-b3-c3 | A1 B3 C3 | A B | none |
| f1-a1-b3-c3 | F1 A1 B3 C3 | A B | none |
| a1-b1-c2-d3 | A1 B1 C2 D3 | A B C | none |
| a1-b3-keepLines-c3 | A1 B3 C3 | A B | B.keepLines |
| a1-b3-widow-c3 | A1 B3 C3 | A B | B.widowControl |
| f1-a1-b4-c3 | F1 A1 B4 C3 | A B | none |

Each input line has a unique four-character ASCII label such as A000 or B002.
Lines inside one paragraph are separated by textWrapping w:br (source U+000B);
each paragraph ends with source U+000D. The manifest records every visible run,
soft return, paragraph mark, UTF-16 half-open interval and package/source hash.
These are derived source contracts; capture must independently verify native
content, paragraph ranges and both complete CP scans, including all pages.
Actual font-file hashes, PDF family binding and print-view state remain capture
requirements. A label or glyph origin alone does not establish line-box height.

Use `python3 tools/measure/make_keep_chain_fixture.py --out NEW_DIRECTORY` to
generate a byte-reproducible copy. Existing paths are rejected. Use `--check
EXISTING_DIRECTORY` for read-only byte reconstruction and narrow XML/source
validation. ZIP timestamps and permissions are fixed. This verifier is not a
complete OOXML XSD validator. Generation/checking does not invoke Word, UI,
AppleScript, the layout engine, native APIs, Cargo or the network.
