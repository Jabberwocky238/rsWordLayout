#!/usr/bin/env python3
"""Create deterministic, unmeasured exact-line holdout probes (R04).

Each case is a sequence of single-label probe paragraphs with their own exact
line values, followed by the canonical R001/R002 exact480 references. The
package profile, property order and verifier are those of
make_exact_vertical_fixture.py; only the paragraph sequence differs.
No expected y, line box or page count is written here: candidate predictions
are frozen separately, before any capture.
"""

from __future__ import annotations

import argparse
import io
import json
from pathlib import Path
import zipfile

import make_exact_vertical_fixture as canonical
import make_paragraph_mark_fixture as mark_package
from make_paragraph_mark_fixture import TNR, W, XML, font, sha256


DEFAULT_OUT = Path("fixtures/exact-vertical-holdout-2026-09-28")


def variants() -> list[dict]:
    base = {"topTwips": 720, "bodySize": 24, "markSize": 24}
    selected = [
        ("holdout-neg-d-256x3", {"lines": [256, 256, 256]},
         "Three exact256 paragraphs from a canonical top."),
        ("holdout-pos-d-248x3", {"lines": [248, 248, 248]},
         "Three exact248 paragraphs from a canonical top."),
        ("holdout-large-1000-1016", {"lines": [1000, 1016]},
         "Two large exact values."),
        ("holdout-phase-mixed", {"topTwips": 725, "lines": [217, 223, 229, 233, 239, 241]},
         "Six different exact values accumulated from a fractional top."),
        ("holdout-top723-mixed", {"topTwips": 723, "lines": [248, 256, 272, 280]},
         "Four exact values from another fractional top."),
        ("holdout-clip-166-body24", {"lines": [166, 166], "bodySize": 48, "markSize": 48},
         "Two exact166 paragraphs with 24 pt body and mark."),
    ]
    return [{**base, **changes, "name": name, "purpose": purpose}
            for name, changes, purpose in selected]


def build(case: dict) -> tuple[dict[str, str], dict]:
    body_font = font(TNR, case["bodySize"])
    mark_font = font(TNR, case["markSize"])
    inputs = [([f"P{index:03d}"], body_font, mark_font, line, "probe")
              for index, line in enumerate(case["lines"])]
    reference = font(TNR, 24)
    inputs += [([label], reference, reference, 480, "reference") for label in ("R001", "R002")]
    paragraphs, records, source = [], [], ""
    for index, (group, body, mark, line, role) in enumerate(inputs):
        xml, record, text = canonical.paragraph(group, body, mark, line, index,
                                                canonical.utf16_length(source), role)
        paragraphs.append(xml)
        records.append(record)
        source += text
    section = ('<w:sectPr><w:type w:val="nextPage"/>'
               '<w:pgSz w:w="11906" w:h="16838"/>'
               f'<w:pgMar w:top="{case["topTwips"]}" w:right="720" w:bottom="720" w:left="720" '
               'w:header="360" w:footer="360" w:gutter="0"/>'
               '<w:cols w:num="1" w:space="720" w:equalWidth="1"/></w:sectPr>')
    parts, _ = mark_package.build({"name": case["name"], "bodySize": 24, "markSize": 24})
    parts["word/document.xml"] = (XML + f'<w:document xmlns:w="{W}"><w:body>'
                                  + "".join(paragraphs) + section + '</w:body></w:document>')
    record = {"name": case["name"], "file": case["name"] + ".docx",
              "captureStatus": "UNMEASURED", "purpose": case["purpose"],
              "probeLines": case["lines"], "bodyFont": body_font, "paragraphMarkFont": mark_font,
              "topMarginTwips": case["topTwips"], "paragraphCount": len(records),
              "sourceText": source, "sourceLengthUtf16": canonical.utf16_length(source),
              "sourceTextUtf16LeSha256": sha256(source.encode("utf-16-le")),
              "paragraphs": records, "anchors": [a for p in records for a in p["anchors"]]}
    canonical.verify(parts, record)
    return parts, record


README = """# Exact Vertical Holdout Probes (R04)

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
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    choice = parser.add_mutually_exclusive_group()
    choice.add_argument("--out", type=Path, help="New output directory; existing directories are rejected")
    choice.add_argument("--check", type=Path, help="Verify an existing output without changing it")
    args = parser.parse_args()
    generated = []
    for case in variants():
        parts, record = build(case)
        data = canonical.package_bytes(parts)
        assert data == canonical.package_bytes(parts)
        record["sha256"] = sha256(data)
        record["packagePartSha256"] = {name: sha256(value.encode("utf-8")) for name, value in parts.items()}
        generated.append((record, data))
    manifest = {
        "schema": "rsword-exact-vertical-source/1", "captureStatus": "UNMEASURED",
        "generator": "tools/measure/make_exact_holdout_fixture.py",
        "generatorSha256": sha256(Path(__file__).read_bytes()),
        "profileGenerator": "tools/measure/make_exact_vertical_fixture.py",
        "profileGeneratorSha256": sha256(Path(canonical.__file__).read_bytes()),
        "pPrPropertyOrder": canonical.PPR_ORDER, "rPrPropertyOrder": canonical.RPR_ORDER,
        "sectPrPropertyOrder": canonical.SECT_ORDER,
        "referenceParagraphs": {"labels": ["R001", "R002"], "font": font(TNR, 24),
                                "lineRule": "exact", "lineTwips": 480},
        "paragraphBeforeTwips": 0, "paragraphAfterTwips": 0, "compatibilityMode": 15,
        "pageTwips": [11906, 16838], "marginsTwips": {"top": "per-case", "left": 720, "right": 720, "bottom": 720},
        "sourceUnit": "UTF-16 code units; half-open ranges, CR paragraph marks.",
        "cases": [record for record, _ in generated],
    }
    manifest_text = json.dumps(manifest, indent=2) + "\n"
    if args.check:
        assert (args.check / "manifest.json").read_text(encoding="utf-8") == manifest_text
        assert (args.check / "README.md").read_text(encoding="utf-8") == README
        assert {p.name for p in args.check.iterdir()} == {r["file"] for r, _ in generated} | {"manifest.json", "README.md"}
        for record, expected in generated:
            actual = (args.check / record["file"]).read_bytes()
            assert actual == expected
            with zipfile.ZipFile(io.BytesIO(actual)) as archive:
                canonical.verify({n: archive.read(n).decode("utf-8") for n in archive.namelist()}, record)
        print(json.dumps({"directory": str(args.check), "cases": len(generated), "state": "CHECKED_UNMEASURED",
                          "manifestSha256": sha256(manifest_text.encode("utf-8"))}))
        return
    output = args.out or DEFAULT_OUT
    output.mkdir(parents=True, exist_ok=False)
    for record, data in generated:
        with (output / record["file"]).open("xb") as file:
            file.write(data)
    (output / "manifest.json").write_text(manifest_text, encoding="utf-8")
    (output / "README.md").write_text(README, encoding="utf-8")
    print(json.dumps({"directory": str(output), "cases": len(generated),
                      "manifestSha256": sha256(manifest_text.encode("utf-8"))}))


if __name__ == "__main__":
    main()
