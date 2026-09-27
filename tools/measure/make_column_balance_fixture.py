#!/usr/bin/env python3
"""Create source-indexed column-balance probes; no expected Word layout is baked in."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from make_probe_fixture import R, W, rpr, run

FAMILY = "Times New Roman"


def paragraph(content: str, section_props: str = "", *, legacy_property_order: bool = False) -> str:
    widow = '<w:widowControl w:val="0"/>'
    spacing = '<w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/>'
    alignment = '<w:jc w:val="left"/>'
    # Legacy order reproduces the frozen first capture, including its schema defect.
    properties = spacing + alignment + widow if legacy_property_order else widow + spacing + alignment
    return f'<w:p><w:pPr>{properties}{rpr(24, FAMILY)}{section_props}</w:pPr>{content}</w:p>'


def section(columns: int, kind: str) -> str:
    return (f'<w:sectPr><w:type w:val="{kind}"/>'
            '<w:pgSz w:w="11906" w:h="16838"/>'
            '<w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
            'w:header="360" w:footer="360" w:gutter="0"/>'
            f'<w:cols w:num="{columns}" w:space="720" w:equalWidth="1"/>'
            '</w:sectPr>')


def build(continuous: bool, hard_break: bool, no_balance: bool, successor_columns: int = 1,
          legacy_property_order: bool = False) -> tuple[dict, list[dict]]:
    paragraphs, anchors = [], []
    cursor = 0
    for index in range(40):
        text = f"C{index:03}"
        has_break = hard_break and index == 9
        content = run(text, 24, FAMILY)
        if has_break:
            content += f'<w:r>{rpr(24, FAMILY)}<w:br w:type="column"/></w:r>'
        ends_section = continuous and index == 39
        paragraphs.append(paragraph(content, section(2, "nextPage") if ends_section else "",
                                    legacy_property_order=legacy_property_order))
        end = cursor + len(text) + int(has_break) + 1
        anchors.append({"label": text, "sourceStart": cursor, "sourceEnd": end,
                        "columnBreakOffset": cursor + len(text) if has_break else None,
                        "terminator": "SECTION_BREAK" if ends_section else "PARAGRAPH_MARK"})
        cursor = end
    if continuous:
        paragraphs.append(paragraph(run("END", 24, FAMILY),
                                    legacy_property_order=legacy_property_order))
        anchors.append({"label": "END", "sourceStart": cursor, "sourceEnd": cursor + 4,
                        "columnBreakOffset": None, "terminator": "PARAGRAPH_MARK"})
    body = "<w:body>" + "".join(paragraphs)
    body += section(successor_columns, "continuous") if continuous else section(2, "nextPage")
    body += "</w:body>"
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
            f'<w:settings xmlns:w="{W}"><w:compat><w:noColumnBalance w:val="{int(no_balance)}"/>'
            '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/>'
            '</w:compat></w:settings>'),
    }
    return parts, anchors


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="New output directory; defaults depend on order and column variant")
    parser.add_argument("--same-columns", action="store_true", help="Additional continuous two-column successor control")
    parser.add_argument("--legacy-property-order", action="store_true",
                        help="Reproduce the original schema/1 fixtures with their noncanonical pPr order")
    args = parser.parse_args()
    if args.out is None:
        variant = "" if args.legacy_property_order else "-canonical"
        variant += "-same-columns" if args.same_columns else ""
        args.out = Path(f"fixtures/column-balance{variant}-2026-09-27")
    args.out.mkdir(parents=True, exist_ok=False)
    cases = []
    variants = [
        ("terminal40", False, False),
        ("continuous40", True, False),
        ("continuous40-break10", True, True),
    ] if not args.same_columns else [("continuous40-sameCols", True, False)]
    for kind, continuous, hard_break in variants:
        for no_balance in ([False, True] if not args.same_columns else [False]):
            name = f"{kind}-noBalance{int(no_balance)}"
            parts, anchors = build(continuous, hard_break, no_balance, 2 if args.same_columns else 1,
                                   legacy_property_order=args.legacy_property_order)
            path = args.out / f"{name}.docx"
            with zipfile.ZipFile(path, "x", zipfile.ZIP_DEFLATED) as archive:
                for part, value in parts.items():
                    info = zipfile.ZipInfo(part, date_time=(2026, 1, 1, 0, 0, 0))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = 0o644 << 16
                    archive.writestr(info, value.encode("utf-8"))
            cases.append({"name": name, "file": path.name,
                          "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                          "documentXmlSha256": hashlib.sha256(parts["word/document.xml"].encode()).hexdigest(),
                          "settingsXmlSha256": hashlib.sha256(parts["word/settings.xml"].encode()).hexdigest(),
                          "continuousSingleColumnSuccessor": continuous and not args.same_columns,
                          **({"successorColumns": 2} if args.same_columns else {}),
                          "columnBreakAfterLabel": "C009" if hard_break else None,
                          "noColumnBalance": no_balance, "anchors": anchors})
    manifest = {"schema": f"rsword-column-balance-source/{1 if args.legacy_property_order else 2}",
                **({} if args.legacy_property_order else {
                    "pPrPropertyOrder": ["widowControl", "spacing", "jc", "rPr", "sectPr"],
                }),
                "fontFamily": FAMILY,
                "fontSizeHalfPoints": 24, "lineRule": "exact", "lineTwips": 480,
                "pageTwips": [11906, 16838], "marginTwips": 720,
                "columnGapTwips": 720, "columnWidthsTwips": [4873, 4873],
                "expectedLayout": None,
                "purpose": "Distinguish sequential fill, terminal balancing, continuous-section balancing, compatibility flag and explicit column boundaries using new Word evidence.",
                "cases": cases}
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"directory": str(args.out), "cases": len(cases)}))


if __name__ == "__main__":
    main()
