# Capture line-ordinal audit, 2026-09-22

Status: offline audit and two subsequent local diagnostics complete. The new
kinsoku2 run directly exhibits unstable line-number readings. The six archived
line-ordinal reversals are reproducible in their stored data, but their native
cause is unresolved.
This audit does not change capture bytes, measurement criteria, line assignment,
or engine behavior. Its diagnostic matrix was designed after reading the data.

Repository baseline: `1fddc3a` on `codex/oracle-layout-improvements`.
Reference repository: `/Users/lilleap/code/docx-layout`, commit
`8e81e76bbfb307c186d9ec77a3bce6e54e586c6d`. Only that reference checkout was read.
The initial audit launched, selected, queried, or closed no Word application.
The later native session was run by the task controller; its evidence was
reviewed separately without further Word calls.

## Archived observations

Source offsets are UTF-16 units, ranges are half-open, and page numbers here are
the captured one-based values. Both fixtures contain four 63-unit paragraphs
with ranges `[0,63)`, `[63,126)`, `[126,189)`, and `[189,252)`. Their characters
are BMP characters, so no surrogate boundary is involved. Paragraphs 2-4 have
`w:pageBreakBefore`; this property adds no source character. The fixtures differ
in `w:overflowPunct=false`, not paragraph content or source ranges.

| Capture | Page | Initial range and captured Ln | Following range and captured Ln | Final range and captured Ln |
| --- | ---: | --- | --- | --- |
| kinsoku | 2 | `[63,66)` -> 3 | `[66,101)` -> 1 | `[101,126)` -> 2 |
| kinsoku | 3 | `[126,129)` -> 3 | `[129,164)` -> 1 | `[164,189)` -> 2 |
| kinsoku | 4 | `[189,194)` -> **7** | `[194,227)` -> 1 | `[227,252)` -> 2 |
| kinsoku2 | 2 | `[63,66)` -> 3 | `[66,99)` -> 1 | `[99,126)` -> 2 |
| kinsoku2 | 3 | `[126,128)` -> 3 | `[128,162)` -> 1 | `[162,189)` -> 2 |
| kinsoku2 | 4 | `[189,192)` -> 3 | `[192,225)` -> 1 | `[225,252)` -> 2 |

Thus the initial anomalous prefix has lengths 3/3/5 and 3/2/3, respectively.
It is not always three characters, and the anomalous ordinal is not always 3.
Page 1 has the ordinary two contiguous segments: kinsoku `[0,38)` / `[38,63)`;
kinsoku2 `[0,36)` / `[36,63)`, with Ln 1 / 2.

The stored PDF glyphs have two consecutive groups of identical glyph-origin y
on every page, at approximately 87.12pt and 108.96pt. Group sizes are 38/25 for
kinsoku and 36/27 for kinsoku2. This is a descriptive observation on these plain
CJK fixtures, not a new y-clustering line-assignment rule. In particular, there
is no third PDF group containing only the anomalous 2-5 initial characters.
The remaining captured wrap positions agree with those two PDF groups after
subtracting the paragraph start. The historical PDF bytes were not re-extracted;
the later diagnostic PDF was separately read as described below.

A scan of all **25** stored `captures/*/sweep.json` files, containing **18,046**
position records, found same-page line-number decreases within a single stored
paragraph in exactly these two packages: 6 transitions total. This inventory
includes every stored sweep, without treating a package's existence as usable
capture admission. It does not establish that the other packages are correct.

Both capture META files identify Word version `16.112.3`, build
`16.112.26083020`, PID **71656**, executable
`/Applications/Microsoft Word.app/Contents/MacOS/Microsoft Word`, and start time
`2026-09-18 14:09:17` in the captured local timezone. Capture timestamps are
`2026-09-18T06:24:16Z` and `2026-09-18T06:27:21Z`, 185 seconds apart. These are
two different fixture captures in the same process, not same-input repetitions.

## Byte identities

All SHA256 values below were read or recomputed during this audit. The PDFs are
identified by META and the extractor's `sourceSha256`; their original PDF bytes
are not present in these tracked package directories.

| Item | kinsoku | kinsoku2 |
| --- | --- | --- |
| Fixture, META before/after | `37abbd1186bc9711679816ae0addec1e4c45d23c835d5c621b188f171583fead` | `57e52d52a70650529b7166f6a5fd379fc5dd91a1ca5d362814f0fc8ef106f8ec` |
| PDF, recorded source hash | `aaad7bffe1447a086320c1ece16f888256f220035be79500b48f3ae93a3a902d` | `b7e21d52607526a309ce4f4cf5ea1f9cad58013f5eee259cbfd5cde5422a247d` |
| sweep.json | `6af37b55fda458d69262d2d5d695a701ea04c0d7f3a4a05f60d5f30a71d5d4d3` | `5ccb11d5fe13da91e5b4137ab633bb2665bc736f59265fecbc22098c04ae4147` |
| glyphs.json | `b29df9f126c2ddb75695bf1844db8c09d5e994e4d0594ac0040d24a42f4fe128` | `90730de8095abbb2ccb306467e9f1cb3c0a30cc5c30f63be53b0d08782be8d38` |
| META.json | `a271021d03e4b71de54a7075e63814bacd532a0b383da0b5d02db857e782fb44` | `15065c0380000dea9f8273f1f455ae79c6d67757a29c2e8265bf9e3593abf422` |

Sources: [kinsoku package](../captures/kinsoku-2026-09-18/),
[kinsoku2 package](../captures/kinsoku2-2026-09-18/),
[kinsoku generator](../tools/measure/make_kinsoku_fixture.py),
[kinsoku2 generator](../tools/measure/make_kinsoku2_fixture.py), and
[`para()` pageBreakBefore construction](../tools/measure/make_probe_fixture.py).

## Capture-path audit

[`capture.py`](../tools/measure/wordmeasure/capture.py), SHA256
`4c3bc5c93bdbf841af2974384252339af2958879be91fba196fc71dca2420fea`, does:

1. Open the fixture copy in a reusable slot; read content and paragraph ranges.
2. Export the PDF using AppleScript `save as ... format PDF`.
3. For each `i` from 0 to end-1, create a collapsed range `[i,i]`, read
   `first character line number`, then `active end page number`.
4. Close the named document without saving; serialize the scan afterward.

There is no explicit `repaginate`, view selection/readback, pagination-option
readback, per-query timestamp, range start/end/text readback, repeated scan, or
query-order comparison. A fresh range is created for every offset; the script
does not intentionally move the selection. Therefore a selection-induced view
change is not evidenced by this script, even though it is a known diagnostic
hazard in another channel. Page and line are separate calls, not an atomic pair.

The collapse matters: both queries are intended to concern the point at `i`.
Using a full range instead would make active-end page identity a different
quantity at boundaries. This audit does not assume that changing `end i` to
`end (i+1)` is a transparent fix.

The local installed `Word.sdef` declares `create range` with document/start/end,
`repaginate` on a document, `pagination` as a Boolean, and `view type` including
`print view`. This proves dictionary availability, not successful execution or
that repagination repairs these readings. The initial audit ran no such command.

The varying short prefix immediately after pageBreakBefore is consistent with
lazy layout or first-query state effects. It does **not** identify their cause.
Neither an incomplete pagination explanation, a collapsed-range boundary bug,
nor a stale range-information cache can be distinguished from these one-pass
records. An offset/range-delivery error is also not excluded by native readbacks,
because those readbacks were not captured.

## Independent reference evidence

These are bounded observations from the reference checkout, not replacement
truth for this Mac build or the two CJK fixtures.

| Reference path under `/Users/lilleap/code/docx-layout` | What it establishes; limitation |
| --- | --- |
| `docs/implementation/information10-desk-round125.md` | The archived WdInformation definition relates code 10 to status-bar Ln, and discusses collapsed ranges. The report explicitly declines to certify a page-local visual-line interpretation. |
| `docs/implementation/information10-round126.md` | Windows native code 10/3 sweeps used explicit Repaginate, range-identity checks, and recorded pagination treatments. Pass A/B endpoint values agreed in three fixtures; code 10 varied with position. This does not prove stability on Mac or on kinsoku. |
| `artifacts/information10-round126-20260916/harness/information_worker.py` | Executable diagnostic pattern: read Start/End/Text/StoryType, sample collapse delivery, record individual Information reads, save bounded partial coverage, and clean up only owned processes. The archived evidence and worker are present locally. |
| `docs/implementation/mac-word-probe-round129.md` | Mac AppleScript code 10/3 was successfully read in two fixtures. The report explicitly does not certify it as a visual-line bridge. Its named raw `artifacts/mac-word-probe-20260917/` directory is absent in this checkout, so this audit checked the report rather than those Mac raw bytes. |
| `docs/implementation/a4-pagination-round29.md` | On Windows, setting Pagination before opening/print-view setup did not preserve the requested distinction. Post-view native readback established False/True treatments. One effective pair produced equal geometry; it did not rank reliability. Three equal page counts were explicitly not proof that background work finished. |
| `docs/implementation/line-selection-round36.md` | Selecting a separator-story range changed view and pane count; restoring only the selection did not restore view. This cautions against introducing selection as an unrecorded repair. It does not explain our non-selection Mac sweep. |
| `tools/oracle/CAPTURE-CHECK.md` | Mac ownership B-3 is not implemented by that checker; a new PID relative to a baseline is explicitly insufficient proof of ownership. |

Reference document SHA256 values, in table order excluding the desk/worker rows:

```text
information10-round126.md cb77e654bc114dac1374a54c48cb83d78a0c4eedbfe84425799d43eb4dbf6288
mac-word-probe-round129.md 89b68e59690d23f4ce512c0599f765a95e33723b0c54ee64e73714b3f6c44958
a4-pagination-round29.md 0b9f955a7d7e9a1297db1797fd94d29a5a8c083f6ff8dda7426d37cb30a46d5f
line-selection-round36.md 69c294f6cb3c4496bde975cff49d362cf53f0d57418c6c455e04060e0c9bd0d3
CAPTURE-CHECK.md 1353666317671bf705f15d5b47ad54f06b021714cdc07669e908760593666d8a
```

## Host and ownership observations

Read-only checks around `2026-09-22T06:29:44Z`:

| Host | Observation | Availability conclusion |
| --- | --- | --- |
| Local Mac, `100.67.7.59` | `/Applications/Microsoft Word.app` exists; plist version/build 16.112.3 / 16.112.26083020. `pgrep -x 'Microsoft Word'` found no process. Songti.ttc exists. | Candidate for a newly controlled diagnostic session; not yet admitted. Automation grant, license/UI readiness, and Word-process font visibility were not established. |
| `usermacbook-pro`, `100.98.255.4` | Online. SSH OS process listing finds PID 71656, owner `user`, start Fri Sep 18 14:09:17 2026; installed Word has the same version/build. | This is the historical capture process and is not owned by this task. It was not touched. |
| `USER809E`, `100.97.5.89` | Tailscale reports online Windows host. | SSH account and Word ownership unknown; no guessed account or remote Word call attempted. Windows would not replace Mac evidence. |
| `DESKTOP-0U9GEGH`, `100.120.16.30` | Historical reference Windows oracle host is offline. | Not available at check time. |
| `DESKTOP-M287D3V`, `100.99.5.60` | Offline. | Not available at check time. |

No current Mac lease/owner protocol was found in this repository's capture
implementation or documentation. The local fixed slot
`~/Documents/rsword-captures/_word-slot.docx` is absent. The remote slot exists
and is owned by `user`; its directory enumeration did not succeed, so no absence
of leases or open documents is inferred. No remote slot was overwritten.
A read-only query of the local user TCC database failed to open it; no automation
permission conclusion follows, and no permission setting was changed.

Useful read-only rechecks, which do not launch Word:

```sh
/usr/bin/pgrep -x 'Microsoft Word'
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' '/Applications/Microsoft Word.app/Contents/Info.plist'
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' '/Applications/Microsoft Word.app/Contents/Info.plist'
ssh -o BatchMode=yes -o ConnectTimeout=6 user@100.98.255.4 'ps -axo pid,ppid,user,lstart,comm | grep "[/]Microsoft Word$"'
```

## Minimum native diagnostic matrix

Do not run the existing production `capture()` as an ownership probe: it binds
Word by application/document name and rewrites the fixed slot. First establish
an explicitly controlled local session, retain process identity and launch
provenance, verify which instance automation reaches, and verify no competing
document/session uses the diagnostic paths. Absence of an existing PID alone
does not complete that proof. Use copied fixtures with verified hashes and a new
diagnostic output directory; preserve failed/partial runs and original captures.

The first diagnostic inputs should be the two unchanged 252-unit fixtures.
Record build, process identity, font preflight, source hash, view and pagination
readbacks, actual range Start/End/Text/StoryType where accessible, per-call order,
timestamps, and raw failures. Do not install fonts, mutate source paragraphs,
change global pagination options, or move the selection in the first matrix.

| Arm | One changed factor | Reads and purpose |
| --- | --- | --- |
| A | Production call sequence | After the same PDF export, ascending `[i,i]` sweep, Ln then Page. Repeat the complete sweep immediately, then read in descending offset order. Tests first-pass/order dependence without calling it a repair. |
| B | Query order | From a freshly opened fixture copy, Page then Ln, otherwise A's first pass. Re-read Ln at the same range after the pair. Tests whether the page query changes the subsequent line answer. |
| C | Explicit repagination | From a freshly opened copy, retain the production export, then explicitly `repaginate` that document and record completion before A's sweep. Tests this call sequence; do not infer that it proves global background quiescence. |
| D | Range shape | From a freshly opened copy, use `[i,i+1]` for first-character Ln and retain separately labelled collapsed `[i,i]` page reads. Re-read both range identities. Tests point-range versus first-character behavior without silently conflating active ends. |

Priority offsets are 0-8 as a first-page control, 63-70, 126-133, and 189-197,
plus 35-40, 98-104, 161-167, and 224-230 around the normal wrap boundaries.
The complete 252-position sweeps are small and retain the denominator.
Treat every arm and repeat as a separate labelled observation. Fresh-open
comparisons must retain their order because the process itself can remain warm.
An explicit print-view intervention is a later separate arm if view readbacks
show it is needed; it should not be bundled into C and attributed to repaginate.

Stop with a recorded undecidable result on unavailable range identity,
automation failure, competing ownership, or inconsistent source/hash identity.
Agreement after one intervention is evidence about that intervention on these
fixtures, not certification of every line ordinal. In particular, do not merge
the first 2-5 characters into Ln 1 or overwrite old sweeps merely because the
PDF has two visible groups. A new native reading must remain a new capture.

## Subsequent local diagnostic: kinsoku

The controller then performed one local session, recorded under
[`artifacts/line-ordinal-diagnostic-2026-09-22/`](../artifacts/line-ordinal-diagnostic-2026-09-22/).
This section is an independent read-only audit of those saved files, not a second
native execution. It supplements, rather than retroactively completes, the
planned matrix above.

`prelaunch-pids.txt` is empty. `launch-start.txt` records
`2026-09-22T06:31:11Z`; `launched-process.txt` records PID **39057**, start
`Tue Sep 22 14:31:11 2026`, and the expected local Word executable. The controller
reported that the first AppleEvent returned version 16.112.3 and documents=0;
that initial response was later preserved as an explicitly labelled controller
transcription, not an original raw-response receipt (see final receipts below).
The saved `document-identity.txt` independently records exactly one document,
`kinsoku-diagnostic.docx`, `saved=true`, with full HFS path corresponding to the
unique artifact copy. The copied DOCX has the same SHA256 as the historical
fixture, `37abbd1186bc9711679816ae0addec1e4c45d23c835d5c621b188f171583fead`.

`probe-kinsoku.py` has SHA256
`07b04bc1128382b97306b5dd22a0885a5e7cb3bc92de64e095fed21de43fbb72`.
It checks that `pgrep -x 'Microsoft Word'` is exactly PID 39057 before and after
each AppleEvent. This is evidence for a controlled session; it is not the
reference Windows retained-handle ownership guarantee. The six stored scripts
and their raw text results agree with the following actual sequence:

| Scan | Actual treatment | UTC start | UTC end |
| --- | --- | --- | --- |
| 01-collapsed-forward | `[i,i]`, ascending, Ln then Page | 06:34:16.732977 | 06:34:23.621999 |
| 02-collapsed-repeat | Same | 06:34:23.622841 | 06:34:30.412381 |
| 03-collapsed-reverse | `[i,i]`, descending, Ln then Page | 06:34:30.412997 | 06:34:37.186101 |
| 04-character-forward | `[i,i+1]`, ascending; both Ln and Page use that range | 06:34:37.187630 | 06:34:44.385747 |
| 05-repaginated-forward | Explicit document repaginate, then original scan | 06:34:44.586683 | 06:34:51.606752 |
| 06-repaginated-repeat | Original scan again | 06:34:51.607489 | 06:34:58.648493 |

All dates in this table are 2026-09-22. Each raw scan contains exactly one record
for every offset 0-251 and reports endOfContent=252. Independent parsing of every
`.txt` reproduces its `.json` positions exactly. Scan 03 really visited offsets
251 down to 0. After sorting by source offset, **1,512/1,512 records across the
six scans have identical (offset, Ln, Page) values**. Every scan yields exactly
these eight contiguous source-line records:

```text
page 1: Ln 1 [0,38)     Ln 2 [38,63)
page 2: Ln 1 [63,101)   Ln 2 [101,126)
page 3: Ln 1 [126,164)  Ln 2 [164,189)
page 4: Ln 1 [189,227)  Ln 2 [227,252)
```

The three historical kinsoku reversals were absent even in scan 01, before this
probe's explicit repaginate call. Thus this run does not establish that
repaginate corrected the historical anomaly.

The new PDF SHA256 is
`b432338e88f87785b94ebb199c1e3b8ba82d9c06f3d148dd09e2e7629d6ea11f`.
Reading it in memory with the repository's `wordmeasure.pdfglyphs` yields four
pages of 63 glyphs each, using `AAAAAC+STSongti-SC-Regular`. Its **complete glyph
array on each of the four pages equals the corresponding historical
`kinsoku-2026-09-18/glyphs.json` array, field for field (4/4 pages)**. PDF file
hashes differ; identical extracted glyph arrays do not claim identical PDF
bytes or certify every PDF operator.

SHA256 identities of the six structured scans:

```text
01-collapsed-forward.json ee00352dd4b619610618c2b0d70e4d74ff87c42b7ff95b7fe67be8b4a2ab1bd6
02-collapsed-repeat.json 062732f471cb712ab6667c2765d9d8067edf43a879710c8f98ce8ba071c43226
03-collapsed-reverse.json 48ddc182b6eee681b2eb16a5535931c4721f2a53a760756865e72cdb4597f5e9
04-character-forward.json 6c89386e5c2cfb6c1637e2a879fb1b523304d43fd2a1712433c8cfb323df215b
05-repaginated-forward.json 79e6f3bc6e069061d6a80775456a4c884f50f794089d4b5bb2e4788c2abfb69b
06-repaginated-repeat.json 605f7ddb7a8bf42d99c192b386555f6cea5d8a2ea70f5982741dacdba229571f
```

The saved `hashes.json` lists 23 files. At review, all 22 still-existing files
matched their listed hash. The remaining entry is the Word temporary lock file
`~$nsoku-diagnostic.docx`, which was no longer present; it is not treated as
verified retained evidence. The six JSON hashes above were recomputed separately
because this manifest covered the raw text/scripts, not the JSON scans.

Limits retained with this result:

- The first scan was delayed after PDF export. The PDF filesystem modification
  timestamp is `06:32:44.305619Z`, 92.427358 seconds before scan 01 began. This
  timestamp is not a native export-return receipt; an exact export-to-read
  latency was not captured. The result therefore does not reproduce production's
  immediate post-export timing.
- All six scans used the same already-open document and process. These are
  within-session treatments, not six independent cold opens. Page-before-Ln was
  not tested. The noncollapsed arm changed the range used by **both** Information
  queries, unlike the separately collapsed page read in planned arm D.
- Per-position range Start/End/Text/StoryType and view/pagination readbacks were
  not recorded. PID stability and one document's full-name identity do not
  substitute for those missing native readings.
- This was not a full preflight-qualified capture. It has no complete capture
  META/font-environment/preflight qualification, and matching PDF output does
  not retroactively supply that qualification. The local host and process also
  differ from the historical remote host and PID 71656.
- The observation narrows the issue to a condition not reproduced by this
  particular run; it neither proves a general API defect nor identifies a
  repair. Existing capture FAIL/VOID decisions and recorded readings are
  unchanged. The separate kinsoku2 audit follows.

## Subsequent local diagnostic: immediate-export kinsoku2

Evidence is under
[`artifacts/line-ordinal-diagnostic-2026-09-22/kinsoku2/`](../artifacts/line-ordinal-diagnostic-2026-09-22/kinsoku2/).
The preserved `probe-kinsoku2.py` (originally root `probe.py`) has SHA256
`3e423ab904236c84bf35cfc3b10614c83611abe48969bdb808a0717649d84f52`.
It verifies the preceding document's path, closes that named diagnostic document
without saving, requires document count zero, opens the unique kinsoku2 copy,
and checks the returned full path. Every AppleEvent retains the same PID 39057
before/after checks. The final saved identity names only that copy, count=1,
saved=true. This remains the same controlled Word session, not a fresh process.

The copied fixture SHA256 is
`57e52d52a70650529b7166f6a5fd379fc5dd91a1ca5d362814f0fc8ef106f8ec`, identical to
the historical fixture. Unlike the first kinsoku experiment, scan 01's stored
AppleScript performs the PDF export and then enters the collapsed ascending
scan in **the same AppleScript execution**, with no inserted wait. Its script
SHA256 is `04a0f0942c78118738685757296109191db00bc44195b0e7050591545c0a21e0`.
There is still no timestamp at the native export-return boundary.

| Scan | UTC start | UTC end | Line ordinals by page |
| --- | --- | --- | --- |
| 01, export then collapsed forward | 06:36:34.511254 | 06:36:45.336586 | page 1: 1/2; page 2: 3/4; page 3: 5/6; page 4: 7/8 |
| 02, collapsed repeat | 06:36:45.338322 | 06:36:51.900877 | 1/2 on every page |
| 03, collapsed reverse | 06:36:51.901843 | 06:36:58.697945 | 1/2 on every page |
| 04, character forward | 06:36:58.699602 | 06:37:05.216245 | 1/2 on every page |
| 05, repaginate then collapsed forward | 06:37:05.382562 | 06:37:11.865306 | 1/2 on every page |
| 06, collapsed repeat | 06:37:11.866137 | 06:37:18.347285 | 1/2 on every page |

All timestamps are on 2026-09-22. Each scan has 252 unique, complete positions,
and independent parsing of its raw text reproduces its structured JSON.
Between scans 01 and 02, **189/252** positions change Ln: every offset 63-251.
No position changes its reported page. The source partition is identical in
all six scans despite the changed ordinals:

```text
page 1: [0,36)     [36,63)
page 2: [63,99)    [99,126)
page 3: [126,162)  [162,189)
page 4: [189,225)  [225,252)
```

Scans 02-06 have identical (offset, Ln, Page) values at all **1,260/1,260**
positions after source-order normalization. The first repeat already differs
from the original reading, before the explicit repaginate operation. It would
therefore be incorrect to credit repaginate, reverse scanning, or expanded
ranges with fixing the first reading.

The new PDF SHA256 is
`58b75225d5ce1f3a977b385998169048b113c9624444c909ec2446b8de20c189`.
Independent in-memory extraction produces four pages of 63 glyphs using
`AAAAAC+STSongti-SC-Regular`; their complete glyph arrays are field-for-field
equal to the historical kinsoku2 arrays on **4/4 pages**. This does not prove
that layout state remained constant during every later query, because there
was no PDF export between those scans.

```text
01-collapsed-forward.json e2fad10c0f816edb17caf297e60e8d586147259c0882b4822d8ff64d86b9edb3
02-collapsed-repeat.json c5e198a8fe002428227b8a279857d9ce31c891480e43ddb9c6defc3141a12f4c
03-collapsed-reverse.json 1d872dcf909b9455ccb3c254f6e79a5ed42fd8677d109d9e110ed4ee9bb93ff4
04-character-forward.json db0da033916943e72137698b8d0444ad7ff311796c8aec34ff82681adabaa249
05-repaginated-forward.json 513f3279310c211d0884f7fcf861a6c865fa01e7971a4afefb98f77c4010800c
06-repaginated-repeat.json f8717fdec8e067a909496b2b5acf6a11b75984ebea780f07b5ddbbb66001134c
```

At this review, all 20 entries in the kinsoku2 raw-file hash manifest existed
and matched, including the then-present temporary Word lock file. Its later
normal disappearance would not invalidate the source/PDF/scan hashes above,
but such a changing lock file is unsuitable as a required immutable artifact.

This supplies a direct, within-session counterexample to stable native Ln
readings under unchanged source content: the first immediate-export scan and
its repeat disagree. It does **not** reproduce the historical short-prefix
3/7-to-1 reversals: this new first scan instead numbers whole pages' lines
consecutively, and its source partition happens to remain correct. The
historical mixing of apparent numbering regimes within a paragraph is a
candidate connection, not an established mechanism. Timing, query side effects,
background work and native caching have not been separated by a controlled
experiment.

The same qualification limits as the kinsoku experiment apply: no complete
capture preflight, no frozen font-environment admission, no range-identity or
view/pagination readbacks, no Page-before-Ln arm, and one already-running process.
New-capture stability checking can reject disagreement while preserving both
raw scans; selecting the later scan because it looks correct would hide the
observed instability. This diagnostic grants no retrospective PASS and changes
no historical FAIL/VOID result or stored capture.

## Final controller receipts and retained artifacts

`controller-observations.json`, SHA256
`fdf6195aaf9c677b3f4169130de125b7ad325d2f327d218c8aab4412f3634ea8`, records:

- Initial Word version 16.112.3 and documents=0 as **controller-reported,
  transcribed after the experiment** from successful task-command output.
  This preserves the provenance limitation rather than presenting a newly
  written file as a contemporaneous native receipt.
- Word Info.plist SHA256
  `5fc365652b4f467ffd3f29e7cbb41a40a64ae01062a7e53e697a775d73639c29`.
- The final closed document's full path identifies only the diagnostic
  kinsoku2 copy and records saved=true. Closure time is
  `2026-09-22T06:38:51.480119Z`; remaining document count is 0.
- The post-session process record still identifies PID 39057, UID 501,
  start time 14:31:11, and the expected executable. The controller reports
  closing only its two diagnostic documents with `saving no`, without quit or
  process termination. The remote historical PID was not used.

`coordination-owner.txt`, SHA256
`50d527acac6e0ff96ae10faeb599658455137ffd8671afbb1f50438fe6738c3d`, binds task
`01a0c79b-3d51-73c2-a50b-7826a3414a2f` to PID 39057 and the local diagnostic
scope. The controller reports creating and releasing its same-project
measurement lock; the copied owner record is retained. This is coordination
evidence, not proof of an operating-system process containment boundary.

The original raw manifests remain unchanged, including their historical
temporary-lock entries. Two later `durable-hashes.json` manifests omit temporary
Word locks and incomplete stream-output files. Independent verification found
all **20/20** root entries and **17/17** kinsoku2 entries present and matching.
Their own SHA256 values are:

```text
durable-hashes.json fe2654f9ba61913337242fab19f613897305982bf0dda2fb0bc7b399102e73c3
kinsoku2/durable-hashes.json 93fde1ba6d6319c2dbb905655141a027713201bfa20fb00c29bdc2a92a994e16
```

Neither temporary `~$` lock remained at final review. The six structured-scan
hashes for each input are recorded separately above, as are both preserved
probe-source hashes. These receipts complete the bounded artifact audit; they
do not turn the diagnostic into a preflight-qualified production capture or
resolve the cause of the unstable line-number readings.
