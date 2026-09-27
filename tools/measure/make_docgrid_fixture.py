#!/usr/bin/env python3
"""Create deterministic source-indexed docGrid probes without layout expectations."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from make_probe_fixture import R, W, rpr, run

FAMILY = "Times New Roman"
SIZE_HALF_POINTS = 24
LABEL_COUNT = 12
PITCHES = (240, 300, 360, 480)


def grid_xml(pitch: int | None) -> str:
    if pitch is None:
        return ""
    return f'<w:docGrid w:type="lines" w:linePitch="{pitch}"/>'


def snap_xml(snap: bool | None) -> str:
    return "" if snap is None else f'<w:snapToGrid w:val="{int(snap)}"/>'


def spacing_xml(line: int, line_rule: str) -> str:
    return (f'<w:spacing w:before="0" w:after="0" '
            f'w:line="{line}" w:lineRule="{line_rule}"/>')


def paragraph(content: str, line: int, line_rule: str, snap: bool | None,
              size_half_points: int) -> str:
    properties = ('<w:widowControl w:val="0"/>' + snap_xml(snap)
                  + spacing_xml(line, line_rule) + '<w:jc w:val="left"/>'
                  + rpr(size_half_points, FAMILY))
    return f'<w:p><w:pPr>{properties}</w:pPr>{content}</w:p>'


def section(pitch: int | None) -> str:
    return ('<w:sectPr><w:type w:val="nextPage"/>'
            '<w:pgSz w:w="11906" w:h="16838"/>'
            '<w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
            'w:header="360" w:footer="360" w:gutter="0"/>'
            '<w:cols w:num="1" w:space="720" w:equalWidth="1"/>'
            f'{grid_xml(pitch)}</w:sectPr>')


def build(line: int, line_rule: str, pitch: int | None,
          snap: bool | None, structure: str = "paragraphs", size_half_points: int = SIZE_HALF_POINTS
          ) -> tuple[dict[str, str], list[dict], str, list[dict]]:
    if line_rule not in {"auto", "exact", "atLeast"} or line <= 0:
        raise ValueError("line must be positive and line_rule must be auto, exact or atLeast")
    if pitch is not None and pitch <= 0:
        raise ValueError("linePitch must be positive when docGrid is present")
    if structure not in {"paragraphs", "soft-lines", "snap-phase"}:
        raise ValueError("unknown probe structure")
    if size_half_points <= 0:
        raise ValueError("font size must be positive")
    paragraphs, paragraph_ranges, anchors, source, runs = [], [], [], [], []
    cursor = 0
    for index in range(LABEL_COUNT):
        label = f"G{index:03}"
        para_snap = index >= 3 if structure == "snap-phase" else snap
        runs.append(run(label, size_half_points, FAMILY))
        has_soft_break = structure == "soft-lines" and index < LABEL_COUNT - 1
        text_end = cursor + len(label.encode("utf-16-le")) // 2
        anchors.append({"label": label,
                        "paragraphIndex": 0 if structure == "soft-lines" else index,
                        "sourceStart": cursor, "textEnd": text_end,
                        "sourceEnd": text_end + 1,
                        "paragraphMarkOffset": None if has_soft_break else text_end,
                        "softBreakOffset": text_end if has_soft_break else None,
                        "terminator": "SOFT_RETURN" if has_soft_break else "PARAGRAPH_MARK"})
        source.append(label + ("\v" if has_soft_break else "\r"))
        if has_soft_break:
            runs.append(f'<w:r>{rpr(size_half_points, FAMILY)}<w:br/></w:r>')
        else:
            paragraphs.append(paragraph("".join(runs), line, line_rule, para_snap, size_half_points))
            paragraph_ranges.append({
                "index": len(paragraph_ranges),
                "sourceStart": 0 if structure == "soft-lines" else cursor,
                "sourceEnd": text_end + 1, "paragraphMarkOffset": text_end,
                "snapToGrid": para_snap, "snapToGridXml": snap_xml(para_snap) or None,
            })
            runs = []
        cursor = text_end + 1
    body = "<w:body>" + "".join(paragraphs) + section(pitch) + "</w:body>"
    declaration = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    parts = {
        "[Content_Types].xml": declaration + (
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            '<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>'
            '</Types>'),
        "_rels/.rels": declaration + (
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
            '</Relationships>'),
        "word/_rels/document.xml.rels": declaration + (
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>'
            '</Relationships>'),
        "word/document.xml": declaration + f'<w:document xmlns:w="{W}" xmlns:r="{R}">{body}</w:document>',
        "word/settings.xml": declaration + (
            f'<w:settings xmlns:w="{W}"><w:compat>'
            '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/>'
            '</w:compat></w:settings>'),
    }
    return parts, anchors, "".join(source), paragraph_ranges


def variants() -> list[tuple[str, int, str, int | None, bool | None, str]]:
    cases = [("auto240-no-grid-snapAbsent", 240, "auto", None, None, "paragraphs")]
    for pitch in PITCHES:
        for snap, suffix in ((None, "Absent"), (True, "True"), (False, "False")):
            cases.append((f"auto240-pitch{pitch}-snap{suffix}", 240, "auto", pitch, snap, "paragraphs"))
    cases.extend([
        ("exact480-pitch360-snapTrue", 480, "exact", 360, True, "paragraphs"),
        ("atLeast240-pitch360-snapTrue", 240, "atLeast", 360, True, "paragraphs"),
        ("auto240-pitch360-snapTrue-softLines", 240, "auto", 360, True, "soft-lines"),
        ("auto240-pitch360-snapFalse-softLines", 240, "auto", 360, False, "soft-lines"),
        ("auto240-pitch360-snapPhase3False9True", 240, "auto", 360, None, "snap-phase"),
    ])
    return cases


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path,
                        help="New output directory; existing captures are never overwritten")
    parser.add_argument("--discriminators", action="store_true",
                        help="Additional pitch270/12pt and no-grid versus pitch300/18pt controls")
    args = parser.parse_args()
    if args.out is None:
        args.out = Path("fixtures/docgrid-canonical-2026-09-27")
        if args.discriminators:
            args.out /= "append-discriminators"
    args.out.mkdir(parents=True, exist_ok=False)
    selected = [(*variant, SIZE_HALF_POINTS) for variant in variants()]
    purposes = {}
    if args.discriminators:
        selected = [
            ("auto240-pitch270-snapTrue-size12", 240, "auto", 270, True, "paragraphs", 24),
            ("auto240-no-grid-snapAbsent-size18", 240, "auto", None, None, "paragraphs", 36),
            ("auto240-pitch300-snapTrue-size18", 240, "auto", 300, True, "paragraphs", 36),
        ]
        purposes = {
            selected[0][0]: "Place the pitch between the recorded 12pt font content and natural line extents to distinguish which extent controls grid rounding.",
            selected[1][0]: "Measure an 18pt no-grid control for the paired 18pt pitch300 probe.",
            selected[2][0]: "Distinguish larger-font behavior from the existing 12pt pitch300 probe using the paired 18pt no-grid control.",
        }
    cases = []
    for name, line, line_rule, pitch, snap, structure, size_half_points in selected:
        parts, anchors, source, paragraph_ranges = build(line, line_rule, pitch, snap, structure, size_half_points)
        path = args.out / f"{name}.docx"
        with zipfile.ZipFile(path, "x", zipfile.ZIP_DEFLATED) as archive:
            for part, value in parts.items():
                info = zipfile.ZipInfo(part, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = 0o644 << 16
                archive.writestr(info, value.encode("utf-8"))
        cases.append({
            "name": name, "file": path.name,
            **({"fontSizeHalfPoints": size_half_points, "purpose": purposes[name]}
               if args.discriminators else {}),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "documentXmlSha256": hashlib.sha256(parts["word/document.xml"].encode("utf-8")).hexdigest(),
            "settingsXmlSha256": hashlib.sha256(parts["word/settings.xml"].encode("utf-8")).hexdigest(),
            "lineRule": line_rule, "lineValue": line,
            "lineValueUnit": "240ths-of-line" if line_rule == "auto" else "twips",
            "spacingXml": spacing_xml(line, line_rule),
            "structure": structure, "paragraphCount": len(paragraph_ranges),
            "snapToGridMode": "per-paragraph" if structure == "snap-phase" else "uniform",
            **({} if structure == "snap-phase" else {
                "snapToGrid": snap, "snapToGridXml": snap_xml(snap) or None,
            }),
            "docGrid": {"present": pitch is not None, "kind": "lines" if pitch is not None else None,
                        "linePitchTwips": pitch, "xml": grid_xml(pitch) or None},
            "sourceText": source, "sourceLengthUtf16": len(source.encode("utf-16-le")) // 2,
            "sourceTextUtf16LeSha256": hashlib.sha256(source.encode("utf-16-le")).hexdigest(),
            "anchors": anchors, "paragraphs": paragraph_ranges,
        })
    manifest = {
        "schema": "rsword-docgrid-source/1",
        "generator": "tools/measure/make_docgrid_fixture.py",
        "purpose": "Collect line-grid, paragraph snap and line-spacing observations without assumed layout results.",
        "pPrPropertyOrder": ["widowControl", "snapToGrid", "spacing", "jc", "rPr"],
        "sectPrPropertyOrder": ["type", "pgSz", "pgMar", "cols", "docGrid"],
        "optionalPropertyNote": "snapToGrid and docGrid are omitted only in the cases that declare them absent.",
        "fontFamily": FAMILY, "fontSizeHalfPoints": None if args.discriminators else SIZE_HALF_POINTS,
        **({"fontSizeNote": "Mixed sizes: each case records its explicit fontSizeHalfPoints.",
            "batch": "append-discriminators",
            "baseManifest": "../manifest.json"} if args.discriminators else {}),
        "fontApplication": "Explicit ascii, hAnsi, eastAsia, cs, sz and szCs on every text run and paragraph mark.",
        "fontIdentityNote": "OOXML family declaration only; the font files used for capture must be recorded separately.",
        "labelCount": LABEL_COUNT, "paragraphBeforeTwips": 0, "paragraphAfterTwips": 0,
        "widowControl": False, "alignment": "left", "stylesPart": False,
        "docDefaults": False, "compatibilityMode": 15,
        "pageTwips": [11906, 16838], "marginTwips": 720, "columns": 1,
        "sourceUnit": "UTF-16 code units; half-open label ranges include a CR paragraph mark or VT soft return; paragraph ranges include the final CR",
        "sourceStatus": "Derived from input OOXML; capture paragraph ranges and endOfContent must independently verify these anchors.",
        "cases": cases,
    }
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"directory": str(args.out), "cases": len(cases)}))


if __name__ == "__main__":
    main()
