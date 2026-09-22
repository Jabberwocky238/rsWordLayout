#!/usr/bin/env python3
"""Build a deterministic, source-indexed, same-line vertical alignment probe."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from make_probe_fixture import R, W, para, run, sect_pr

FAMILIES = ["Times New Roman", "Arial", "Courier New"]
SIZES_HALF_POINTS = [17, 20, 21, 24, 26, 36, 43]


def build_body() -> tuple[str, list[dict]]:
    paragraphs, cases = [], []
    cursor = 0
    for font_index, family in enumerate(FAMILIES):
        for size in SIZES_HALF_POINTS:
            case_id = f"F{font_index}S{size}"
            segments = [
                ("tag", case_id + ":", ""),
                ("anchorA", "A", ""),
                ("normal", "HHHHHH", ""),
                ("anchorB", "B", ""),
                ("superscript", "HHHHHH", "superscript"),
                ("anchorC", "C", ""),
                ("subscript", "HHHHHH", "subscript"),
                ("anchorD", "D", ""),
            ]
            spans, runs, text = {}, [], ""
            start = cursor
            for name, value, alignment in segments:
                spans[name] = [cursor, cursor + len(value)]
                cursor += len(value)
                text += value
                extra = ('<w:kern w:val="3276"/><w:spacing w:val="0"/>'
                         '<w:w w:val="100"/>')
                extra += f'<w:vertAlign w:val="{alignment or "baseline"}"/>'
                runs.append(run(value, size, family, extra))
            cursor += 1
            paragraphs.append(para("".join(runs), size, family,
                                   line=600, line_rule="exact"))
            cases.append({"id": case_id, "family": family, "sizeHalfPoints": size,
                          "sourceStart": start, "sourceEnd": cursor,
                          "text": text + "\r", "spans": spans})
    return "<w:body>" + "".join(paragraphs) + sect_pr("continuous") + "</w:body>", cases


def write_fixture(docx: Path, manifest: Path, font_inputs: Path) -> dict:
    body, cases = build_body()
    declaration = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    parts = {
        "[Content_Types].xml": declaration + (
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            '</Types>'),
        "_rels/.rels": declaration + (
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
            '</Relationships>'),
        "word/document.xml": declaration + f'<w:document xmlns:w="{W}" xmlns:r="{R}">{body}</w:document>',
    }
    docx.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(docx, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, data.encode("utf-8"))
    result = {
        "schema": "rsword-layout-vertical-probes/1",
        "fixtureSha256": hashlib.sha256(docx.read_bytes()).hexdigest(),
        "fontInputsSha256": hashlib.sha256(font_inputs.read_bytes()).hexdigest(),
        "expectedPages": 1, "expectedLines": len(cases), "cases": cases,
    }
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docx", type=Path, default=Path("fixtures/vertical-precision.docx"))
    parser.add_argument("--manifest", type=Path, default=Path("fixtures/vertical-precision.probes.json"))
    parser.add_argument("--font-inputs", type=Path, default=Path("fixtures/vertical-precision.font-inputs.json"))
    args = parser.parse_args()
    result = write_fixture(args.docx, args.manifest, args.font_inputs)
    print(json.dumps({key: value for key, value in result.items() if key != "cases"}, indent=2))


if __name__ == "__main__":
    main()
