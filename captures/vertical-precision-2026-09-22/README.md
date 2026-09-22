# Vertical precision: native Word capture

This capture was made once after preregistration commit
`0e785f863946e90c740e21fddf022c0f1aa22eca`. It passes the source, font, repeated
line-scan, count, and visible-character identity gates of
[`prereg_vertical_precision.py`](../../tools/measure/prereg_vertical_precision.py).
All 42 marked-run conditions are readable; every preregistered candidate has
counterexamples. Admission is not a claim that the engine matches Word.

See the [Round 4 report](../../docs/ENGINE-ITERATION-2026-09-22-ROUND4.md) and
[frozen protocol](../../docs/PREREG-2026-09-22-vertical-precision.md).

`receipt-hashes.json` binds the original capture and controller files copied here
without modification. `provenance/freeze-receipt.json` identifies the original
repository-relative paths and pre-capture hashes; those paths intentionally refer
to their original locations, not this archival copy. `META.json` likewise retains
the original capture paths. The original `capture-controller.py` is an archived
execution record, not a command to rerun from this directory.

`exploration/` contains subsequent independent audits, local font candidates,
and offset-model backtests with their own hashes. They were produced after the
Word observations and do not alter the preregistered criteria or capture records.

To reproduce the frozen evaluation without Word:

```sh
tools/measure/.venv/bin/python tools/measure/prereg_vertical_precision.py \
  captures/vertical-precision-2026-09-22 \
  --probes fixtures/vertical-precision.probes.json \
  --source-docx fixtures/vertical-precision.docx \
  --font-inputs fixtures/vertical-precision.font-inputs.json \
  -o /tmp/vertical-precision-evaluation.json
```

Source annotations remain explicitly derived and hash-verified. Native Mac line
ordinals and count-derived PDF grouping do not supply independent line boxes.
