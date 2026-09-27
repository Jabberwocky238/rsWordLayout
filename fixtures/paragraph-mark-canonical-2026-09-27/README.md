# Paragraph Mark Input Probes

These eight canonical DOCX inputs have not been measured in Word. They contain
no expected layout, page count, baseline, line advance, or control glyph count.
Each probe paragraph is followed by R001 and R002 reference paragraphs, both
with explicit Times New Roman 12 pt body and paragraph-mark formatting.

The manifest distinguishes body runs from the paragraph mark's w:pPr/w:rPr.
Every run and mark sets all four font slots, sz, szCs, and vanish explicitly.
The empty probe has no body run. The hidden probe retains its text in source
positions; the mark and reference paragraphs are explicitly not hidden. The
soft-return probe ends its body with textWrapping w:br before its paragraph mark.
All inputs use auto 240, zero paragraph spacing, widowControl false,
compatibility mode 15, one column, and no document grid, styles or docDefaults.

Source offsets are input-derived UTF-16 code units, with CR for each paragraph
mark and VT for the soft return. They must be checked against native Word
source scans before pairing with a PDF. Hidden-text display/print state and
actual font files remain capture metadata; declared families are not a font
binding. Native source, two source scans, PDF and resolved fonts are still
required. No Word or UI operation is part of generation.

Run `python3 tools/measure/make_paragraph_mark_fixture.py --out NEW_DIRECTORY`
to reproduce these inputs. Existing output directories are rejected. ZIP entry
timestamps and metadata are fixed. manifest.json records each DOCX, package
part and UTF-16 source hash; the manifest itself is not self-hashed.

The emitted property order is listed in the manifest. Offline XML checks can
verify that profile and package/source consistency; they are not a complete
OOXML XSD validation or evidence of Word layout behavior.
