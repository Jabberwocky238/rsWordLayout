# Contextual Spacing Canonical Inputs

All six inputs and all predictions are UNMEASURED. Generation/checking invokes
no Word, UI, native API, engine, Cargo, or network operation. manifest.json is
the source contract; hypotheses.json separately records conditional competing
gap/origin-step models. Neither file contains measured Word layout or an
expectedLayout oracle. Gap/origin-step values are not required extents.

| Case | Style IDs | Direct spacing | Contextual declarations |
| --- | --- | --- | --- |
| style-only-line-control | A,A,B,B | absent; styles exact360/exact720, zero gaps | absent; styles false |
| same-style-200-240 | A,A,A,A | exact480, before240, after200 | false,false,true,false |
| different-style-200-240 | A,B,A,B | exact480, before240, after200 | false,false,true,false |
| style-only-contextual-override | A,A,A | absent; style exact480, before240, after200 | inherit true, direct false, inherit true |
| before-phase | A,A,A,A | exact480, before600, after0 | true,false,true,false |
| both-phase | A,A,A,A | exact480, before300, after100 | true,false,true,false |

A/B mean explicit paragraph styles ProbeA/ProbeB. Different-style's definitions
have identical paragraph properties; only identity/name differ. There is no
basedOn, default style flag, docDefaults, linked style, or hidden inheritance.
The styles relationship belongs to word/document.xml and targets styles.xml;
settings.xml also has its own document relationship and content-type entry.
The package-root relationship targets only word/document.xml. Style-only
inputs contain no direct w:spacing, including no direct line setting.

All 23 paragraphs have one globally unique four-character ASCII label C000
through the case-specific C5xx labels, one run, and one CR paragraph mark.
Each occupies five UTF-16 code units. The manifest records exact labels,
run/mark ranges, styles, direct properties, and the declared input cascade.
That cascade is an input calculation, not a Word effective-property readback.

Every run and paragraph mark explicitly sets all four Times New Roman slots,
sz=szCs=24 and vanish=0. keepNext, keepLines, pageBreakBefore, widowControl are
explicitly false on every paragraph. Alignment is left. There is one column,
compatibility mode 15, no docGrid, no hard/soft breaks, no tables, no trailing
empty paragraph, and no header/footer content. Page size is 11906 by 16838
twips; all four margins are 720, gutter is zero, and body height is 15398.
These short inputs investigate adjacent origins, not page capacity.

Capture must bind actual font files, loaded style IDs/definitions, compatibility
mode and page setup, source text and paragraph ranges, complete dual CP scans,
and every PDF page/label. In the line-control input only compare A/A and B/B
pairs; its A/B boundary changes exact line spacing and is excluded from the
simple origin-step prediction. Other step hypotheses assume a stable baseline
offset for the matched same-font exact480 paragraphs, and still require a
declared quantization tolerance from raw observations. Do not turn a single
page count or glyph origin into a required-height rule.

Use `python3 tools/measure/make_contextual_spacing_fixture.py --out NEW_DIR`
for a byte-reproducible copy, or `--check EXISTING_DIR` for read-only exact-byte
and narrow XML/source validation. Existing output directories are rejected;
ZIP timestamps, permissions and part order are fixed. The checker validates
the explicit package/style/input profile, not the entire OOXML XSD. It rejects
unexpected files, symlinks, changed manifest/hypotheses, and changed ZIP bytes.
