# docx-layout offline evidence survey

Reference checkout: `/Users/lilleap/code/docx-layout`, commit
`8e81e76`. This survey read that checkout only. No Word process was started and
no reference files were changed.

## Reusable evidence

`tools/explore/com_lineage_scan.py` qualifies source ranges and physical page
membership in `docx-layout-com-raw/2`, `/3`, and `/5` captures. The readings are
wrapped values (`readStatus == "value"`, then `value`), with text lines in
`pages[].rectangles[].lines[]`, source intervals in `range.start/end`, and
story identity in `range.storyType`. Physical page means the array position,
not a displayed page number. Table row records are excluded from text lines.

An in-memory rescan reproduced 189 usable capture instances, 21 unusable and
41 undecidable, with 5,660 text line records. These are Windows readings;
they are separate from this repository's Mac capture baselines.

The source-count report needs a correction before reuse: the reference
scanner hashes `docx[0]` once per archive (`com_lineage_scan.py:204`) and assigns
that hash to every capture in the archive. Three usable captures are therefore
bound to another capture's input:

| Archive under `artifacts/` | Capture timestamp | Actual input SHA-256 |
| --- | --- | --- |
| `a3-calibration-20260913/evidence.tgz` | `20260913T153259.158954Z` | `bd04115fec508d51a4bb4eb332e3c8ffcbbafff12128baf053a05b8341ae6fd3` |
| `a3-calibration-20260913/evidence.tgz` | `20260913T153413.412662Z` | `98e28266d5b236c2cbdd86f8c1dd1fdd780e8b18922b59dc55beb1ea43fed1ae` |
| `a3-rise-control-20260914/evidence.tgz` | `20260913T174845.277451Z` | `0e7e42c6f84c3200800fc093e4dda40567f8e33f0c2bf3999f79038452238bde` |

Hashing the `input.docx` in each capture's own directory gives 60 distinct
source hashes among usable instances, instead of the published 59. The usable
instance count and line count do not change. An importer must bind per capture;
it must not inherit the archive's first source document.

Further limits from the reference's
`docs/handoffs/2026-09-18-com-lineage-results-r2.md`: 150 of the 189 usable
instances use older schemas that do not record omitted rectangles, so complete
rectangle enumeration cannot be verified from those files alone. Coverage of
the main-story paragraph intervals is checked, but that does not independently
prove visual line identity. A parser-to-source UTF-16 mapping is still required.

## Concrete next fixtures

All paths below are relative to the reference checkout; each evidence archive
contains its own `input.docx`, `oracle.raw.json`, and capture identity records.

| Candidate | Evidence | Scope |
| --- | --- | --- |
| Widow/orphan and keep chains | `artifacts/raw-batch-round46-20260914/PRB11a-1-evidence.tgz`, `PRB11a-2-evidence.tgz` | 38 physical pages, 626 text lines; exact 24pt lines, keepNext, keepLines, widowControl, long chains, an inline image case |
| Page/section breaks and paragraph spacing | `artifacts/raw-batch-round41-20260914/PRB08a-1-evidence.tgz` through `PRB08e-1-evidence.tgz` | 9 sections, 11 pages and 43 lines per document; compatibility settings and spacing switches vary |
| Character spacing, scaling and kerning | `artifacts/raw-batch-round46-20260914/PRB12a-1-evidence.tgz` through `PRB12d-1-evidence.tgz` | raw/5, 18 text line records per capture; glyph observations can supplement source boundaries |
| Border width versus cell content width | `artifacts/mac-15e-cell-width-20260918/` | Mac only; single-column tables, single borders, sizes 2 and 48; thicker left border translates content without changing the measured width difference |

The last candidate does not establish a universal cell-width formula or a
Windows behavior. See the correction and subsequent gate note in
`docs/handoffs/2026-09-18-wbtable04-correction.md` before consuming it.

## Existing tools

The reference scanner can be replayed without Word:

```sh
python3 /Users/lilleap/code/docx-layout/tools/explore/com_lineage_scan.py \
    /Users/lilleap/code/docx-layout/artifacts --out /tmp/com-lineage.json
```

Its identity limitation above remains in that output. The pure `scan_raw(raw)`
function is useful without the archive-level identity wrapper.

`tools/oracle/pdf_state.py <case.pdf> --output <new.json>` emits
`docx-layout-pdf-raw/1` with `glyphOrigin`, `advanceVector`, actual font name,
and source PDF SHA. It requires the reference's locked Python environment.
It deliberately leaves `lineBaseline` null: a glyph baseline does not establish
the Word layout line baseline.

The reference main checkout has no `tools/compare` directory. This repository's
`tools/measure/sweep.py` instead invokes its existing selfcheck and compare
commands unchanged, binds each fixture by META SHA, and keeps Windows evidence
out of the Mac sweep.
