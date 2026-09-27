#!/usr/bin/env python3
"""Create deterministic, unmeasured paragraph-mark font probes."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr
import zipfile


W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
TNR = "Times New Roman"
DEFAULT_OUT = Path("fixtures/paragraph-mark-canonical-2026-09-27")
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def font(family: str, size: int, hidden: bool = False) -> dict:
    return {"family": family, "sizeHalfPoints": size, "hidden": hidden}


def rpr(style: dict) -> str:
    family = quoteattr(style["family"])
    size = style["sizeHalfPoints"]
    return (f'<w:rPr><w:rFonts w:ascii={family} w:hAnsi={family} '
            f'w:eastAsia={family} w:cs={family}/>'
            f'<w:vanish w:val="{int(style["hidden"])}"/>'
            f'<w:sz w:val="{size}"/><w:szCs w:val="{size}"/></w:rPr>')


def variants() -> list[dict]:
    return [
        {"name": "body12-mark12", "bodySize": 24, "markSize": 24},
        {"name": "body12-mark24", "bodySize": 24, "markSize": 48},
        {"name": "body12-mark8", "bodySize": 24, "markSize": 16},
        {"name": "body24-mark12", "bodySize": 48, "markSize": 24},
        {"name": "body12Arial-mark12TNR", "bodyFamily": "Arial",
         "bodySize": 24, "markSize": 24},
        {"name": "empty-mark24", "bodySize": None, "markSize": 48,
         "empty": True},
        {"name": "hiddenBody12-mark24", "bodySize": 24, "markSize": 48,
         "hidden": True},
        {"name": "body12-softReturn-emptyTail-mark24", "bodySize": 24,
         "markSize": 48, "softReturn": True},
    ]


def paragraph(label: str, body_font: dict | None, mark_font: dict,
              soft_return: bool, index: int, source_start: int) -> tuple[str, dict, str]:
    content = ""
    runs = []
    controls = []
    cursor = source_start
    if label:
        assert body_font is not None
        text_end = cursor + len(label.encode("utf-16-le")) // 2
        content = f'<w:r>{rpr(body_font)}<w:t>{escape(label)}</w:t></w:r>'
        runs.append({"kind": "text", "label": label, "sourceStart": cursor,
                     "sourceEnd": text_end, "font": body_font})
        cursor = text_end
    if soft_return:
        assert body_font is not None
        content += f'<w:r>{rpr(body_font)}<w:br w:type="textWrapping"/></w:r>'
        runs.append({"kind": "soft-return", "sourceStart": cursor,
                     "sourceEnd": cursor + 1, "font": body_font})
        controls.append({"kind": "SOFT_RETURN", "sourceStart": cursor,
                         "sourceEnd": cursor + 1, "inputXml": '<w:br w:type="textWrapping"/>'})
        cursor += 1
    mark_offset = cursor
    controls.append({"kind": "PARAGRAPH_MARK", "sourceStart": cursor,
                     "sourceEnd": cursor + 1, "inputXml": "implicit end of w:p"})
    properties = ('<w:widowControl w:val="0"/>'
                  '<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'
                  '<w:jc w:val="left"/>' + rpr(mark_font))
    xml = f'<w:p><w:pPr>{properties}</w:pPr>{content}</w:p>'
    record = {
        "index": index, "role": "probe" if index == 0 else "reference",
        "sourceStart": source_start, "sourceEnd": cursor + 1,
        "paragraphMarkOffset": mark_offset,
        "softBreakOffsets": [c["sourceStart"] for c in controls if c["kind"] == "SOFT_RETURN"],
        "bodyFont": body_font, "paragraphMarkFont": mark_font,
        "runs": runs, "controls": controls,
    }
    return xml, record, label + ("\v" if soft_return else "") + "\r"


def build(case: dict) -> tuple[dict[str, str], dict]:
    body_font = None if case.get("empty") else font(
        case.get("bodyFamily", TNR), case["bodySize"], case.get("hidden", False))
    mark_font = font(TNR, case["markSize"])
    ref_font = font(TNR, 24)
    inputs = [("" if case.get("empty") else "M000", body_font, mark_font,
               case.get("softReturn", False)),
              ("R001", ref_font, ref_font, False),
              ("R002", ref_font, ref_font, False)]
    paragraphs, records, source = [], [], ""
    for index, (label, body, mark, soft) in enumerate(inputs):
        xml, record, text = paragraph(label, body, mark, soft, index,
                                      len(source.encode("utf-16-le")) // 2)
        paragraphs.append(xml)
        records.append(record)
        source += text
    section = ('<w:sectPr><w:type w:val="nextPage"/>'
               '<w:pgSz w:w="11906" w:h="16838"/>'
               '<w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
               'w:header="360" w:footer="360" w:gutter="0"/>'
               '<w:cols w:num="1" w:space="720" w:equalWidth="1"/></w:sectPr>')
    parts = {
        "[Content_Types].xml": XML + (
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            '<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>'
            '</Types>'),
        "_rels/.rels": XML + (
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
            '</Relationships>'),
        "word/_rels/document.xml.rels": XML + (
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>'
            '</Relationships>'),
        "word/document.xml": XML + f'<w:document xmlns:w="{W}"><w:body>'
            + "".join(paragraphs) + section + '</w:body></w:document>',
        "word/settings.xml": XML + (
            f'<w:settings xmlns:w="{W}"><w:compat>'
            '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/>'
            '</w:compat></w:settings>'),
    }
    record = {
        "name": case["name"], "file": case["name"] + ".docx",
        "captureStatus": "UNMEASURED", "paragraphCount": len(records),
        "sourceText": source, "sourceLengthUtf16": len(source.encode("utf-16-le")) // 2,
        "sourceTextUtf16LeSha256": sha256(source.encode("utf-16-le")),
        "paragraphs": records,
    }
    return parts, record


README = """# Paragraph Mark Input Probes

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
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT,
                        help="New output directory; existing directories are rejected")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    cases = []
    for case in variants():
        parts, record = build(case)
        path = args.out / record["file"]
        with zipfile.ZipFile(path, "x", zipfile.ZIP_DEFLATED) as archive:
            for name, value in parts.items():
                info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = 0o644 << 16
                archive.writestr(info, value.encode("utf-8"))
        record["sha256"] = sha256(path.read_bytes())
        record["packagePartSha256"] = {
            name: sha256(value.encode("utf-8")) for name, value in parts.items()
        }
        cases.append(record)
    manifest = {
        "schema": "rsword-paragraph-mark-source/1",
        "generator": "tools/measure/make_paragraph_mark_fixture.py",
        "generatorSha256": sha256(Path(__file__).read_bytes()),
        "captureStatus": "UNMEASURED",
        "purpose": "Observe paragraph-mark font contributions and subsequent paragraph advancement.",
        "pPrPropertyOrder": ["widowControl", "spacing", "jc", "rPr"],
        "rPrPropertyOrder": ["rFonts", "vanish", "sz", "szCs"],
        "sectPrPropertyOrder": ["type", "pgSz", "pgMar", "cols"],
        "fontApplication": "Explicit ascii, hAnsi, eastAsia, cs, sz, szCs and vanish on every run and paragraph mark.",
        "fontIdentityNote": "Input family declarations only; actual font files must be bound during capture.",
        "lineRule": "auto", "lineValue": 240, "lineValueUnit": "240ths-of-line",
        "paragraphBeforeTwips": 0, "paragraphAfterTwips": 0,
        "widowControl": False, "alignment": "left", "compatibilityMode": 15,
        "pageTwips": [11906, 16838], "marginTwips": 720, "columns": 1,
        "docGrid": False, "stylesPart": False, "docDefaults": False,
        "sourceUnit": "UTF-16 code units; all ranges are half-open, paragraphs include CR and soft returns use VT.",
        "sourceStatus": "Derived from input OOXML, including hidden text; native source must independently verify offsets and endOfContent.",
        "cases": cases,
    }
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (args.out / "README.md").write_text(README, encoding="utf-8")
    print(json.dumps({"directory": str(args.out), "cases": len(cases),
                      "manifestSha256": sha256((args.out / "manifest.json").read_bytes())}))


if __name__ == "__main__":
    main()
