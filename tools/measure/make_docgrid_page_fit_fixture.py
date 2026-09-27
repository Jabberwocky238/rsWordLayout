#!/usr/bin/env python3
"""Create deterministic, unmeasured docGrid page-capacity inputs; never overwrite."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
from xml.etree import ElementTree as ET
import zipfile


DEFAULT_OUT = Path("artifacts/docgrid-page-fit-source-2026-09-27")
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
NS = {"w": W}
FAMILY = "Times New Roman"
SIZE_HALF_POINTS = 24
LABEL_COUNT = 12
PAGE_WIDTH = 11906
MARGIN = 720
GRID_HEIGHTS = (4200, 4240, 4260, 4280, 4300, 4320, 4360)
CONTROL_HEIGHTS = (3300, 3310, 3320, 3340)
PPR_ORDER = ["keepNext", "keepLines", "widowControl", "snapToGrid", "spacing", "jc", "rPr"]
RPR_ORDER = ["rFonts", "sz", "szCs"]
SECT_ORDER = ["type", "pgSz", "pgMar", "cols", "docGrid"]
SPACING_XML = '<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def utf16_length(text: str) -> int:
    return len(text.encode("utf-16-le")) // 2


def variants() -> list[dict]:
    return [{"name": f"auto240-{group}-snapTrue-body{height}",
             "bodyHeightTwips": height, "linePitchTwips": pitch}
            for group, pitch, heights in (("pitch360", 360, GRID_HEIGHTS),
                                         ("no-grid", None, CONTROL_HEIGHTS))
            for height in heights]


def rpr() -> str:
    return (f'<w:rPr><w:rFonts w:ascii="{FAMILY}" w:hAnsi="{FAMILY}" '
            f'w:eastAsia="{FAMILY}" w:cs="{FAMILY}"/>'
            f'<w:sz w:val="{SIZE_HALF_POINTS}"/><w:szCs w:val="{SIZE_HALF_POINTS}"/></w:rPr>')


def grid_xml(pitch: int | None) -> str:
    return "" if pitch is None else f'<w:docGrid w:type="lines" w:linePitch="{pitch}"/>'


def build(case: dict) -> tuple[dict[str, str], dict]:
    height, pitch = case["bodyHeightTwips"], case["linePitchTwips"]
    if type(height) is not int or height <= 0 or pitch not in (None, 360):
        raise ValueError("positive integer body height and absent/pitch360 grid required")
    paragraphs, anchors, ranges, source = [], [], [], ""
    for index in range(LABEL_COUNT):
        label = f"G{index:03}"
        start = utf16_length(source)
        text_end = start + utf16_length(label)
        properties = ('<w:keepNext w:val="0"/><w:keepLines w:val="0"/>'
                      '<w:widowControl w:val="0"/><w:snapToGrid w:val="1"/>'
                      + SPACING_XML + '<w:jc w:val="left"/>' + rpr())
        paragraphs.append(f'<w:p><w:pPr>{properties}</w:pPr><w:r>{rpr()}'
                          f'<w:t xml:space="preserve">{label}</w:t></w:r></w:p>')
        anchors.append({"label": label, "paragraphIndex": index,
                        "sourceStart": start, "textEnd": text_end,
                        "sourceEnd": text_end + 1, "paragraphMarkOffset": text_end,
                        "terminator": "PARAGRAPH_MARK"})
        ranges.append({"index": index, "sourceStart": start, "sourceEnd": text_end + 1,
                       "paragraphMarkOffset": text_end, "snapToGrid": True})
        source += label + "\r"
    section = ('<w:sectPr><w:type w:val="nextPage"/>'
               f'<w:pgSz w:w="{PAGE_WIDTH}" w:h="{height + 2 * MARGIN}"/>'
               f'<w:pgMar w:top="{MARGIN}" w:right="{MARGIN}" w:bottom="{MARGIN}" '
               f'w:left="{MARGIN}" w:header="360" w:footer="360" w:gutter="0"/>'
               '<w:cols w:num="1" w:space="720" w:equalWidth="1"/>'
               + grid_xml(pitch) + '</w:sectPr>')
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
        "word/document.xml": XML + f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>'
                             + "".join(paragraphs) + section + '</w:body></w:document>',
        "word/settings.xml": XML + (
            f'<w:settings xmlns:w="{W}"><w:compat>'
            '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/>'
            '</w:compat></w:settings>'),
    }
    record = {
        "name": case["name"], "file": case["name"] + ".docx", "captureStatus": "UNMEASURED",
        "bodyHeightTwips": height, "pageTwips": [PAGE_WIDTH, height + 2 * MARGIN],
        "contentWidthTwips": PAGE_WIDTH - 2 * MARGIN,
        "lineRule": "auto", "lineValue": 240, "lineValueUnit": "240ths-of-line",
        "spacingXml": SPACING_XML, "snapToGrid": True,
        "docGrid": {"present": pitch is not None, "kind": "lines" if pitch is not None else None,
                    "linePitchTwips": pitch, "xml": grid_xml(pitch) or None},
        "paragraphCount": LABEL_COUNT, "structure": "paragraphs",
        "sourceText": source, "sourceLengthUtf16": utf16_length(source),
        "sourceTextUtf16LeSha256": sha256(source.encode("utf-16-le")),
        "anchors": anchors, "paragraphs": ranges,
    }
    verify(parts, record)
    return parts, record


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def names(element: ET.Element | None) -> list[str]:
    return [] if element is None else [child.tag.removeprefix(f"{{{W}}}") for child in element]


def attributes(element: ET.Element, tag: str, expected: dict[str, str]) -> None:
    child = element.find(f"w:{tag}", NS)
    require(child is not None and child.attrib == {f"{{{W}}}{k}": v for k, v in expected.items()},
            f"unexpected {tag} attributes")


def verify_font(element: ET.Element | None) -> None:
    require(element is not None and names(element) == RPR_ORDER, "unexpected rPr property order")
    attributes(element, "rFonts", {slot: FAMILY for slot in ("ascii", "hAnsi", "eastAsia", "cs")})
    for tag in ("sz", "szCs"):
        attributes(element, tag, {"val": str(SIZE_HALF_POINTS)})


def verify(parts: dict[str, str], record: dict) -> None:
    """Walk XML to validate the source contract, independently of formatting."""
    require(len(parts) == 5 and "word/styles.xml" not in parts, "unexpected package parts")
    for value in parts.values():
        ET.fromstring(value)
    document = ET.fromstring(parts["word/document.xml"])
    body = document.find("w:body", NS)
    require(body is not None and names(body) == ["p"] * LABEL_COUNT + ["sectPr"],
            "expected exactly 12 paragraphs and a terminal sectPr")
    source, anchors, ranges = "", [], []
    for index, paragraph in enumerate(body.findall("w:p", NS)):
        require(names(paragraph) == ["pPr", "r"], "unexpected paragraph children")
        properties = paragraph.find("w:pPr", NS)
        require(names(properties) == PPR_ORDER, "unexpected pPr property order")
        for tag in ("keepNext", "keepLines", "widowControl"):
            attributes(properties, tag, {"val": "0"})
        attributes(properties, "snapToGrid", {"val": "1"})
        attributes(properties, "spacing", {"before": "0", "after": "0", "line": "240", "lineRule": "auto"})
        attributes(properties, "jc", {"val": "left"})
        verify_font(properties.find("w:rPr", NS))
        run = paragraph.find("w:r", NS)
        require(names(run) == ["rPr", "t"], "unexpected run children or source control")
        verify_font(run.find("w:rPr", NS))
        label = run.find("w:t", NS).text
        require(label == f"G{index:03}", "unexpected or repeated source label")
        start, text_end = utf16_length(source), utf16_length(source + label)
        anchors.append({"label": label, "paragraphIndex": index, "sourceStart": start,
                        "textEnd": text_end, "sourceEnd": text_end + 1,
                        "paragraphMarkOffset": text_end, "terminator": "PARAGRAPH_MARK"})
        ranges.append({"index": index, "sourceStart": start, "sourceEnd": text_end + 1,
                       "paragraphMarkOffset": text_end, "snapToGrid": True})
        source += label + "\r"
    require(anchors == record["anchors"] and ranges == record["paragraphs"], "source anchors differ")
    require(record["paragraphCount"] == LABEL_COUNT and source == record["sourceText"], "source text differs")
    require(utf16_length(source) == record["sourceLengthUtf16"] == 60, "source length differs")
    require(sha256(source.encode("utf-16-le")) == record["sourceTextUtf16LeSha256"], "source hash differs")
    section = body.find("w:sectPr", NS)
    pitch = record["docGrid"]["linePitchTwips"]
    require(names(section) == (SECT_ORDER if pitch is not None else SECT_ORDER[:-1]),
            "unexpected sectPr property order")
    attributes(section, "type", {"val": "nextPage"})
    attributes(section, "pgSz", {"w": str(PAGE_WIDTH), "h": str(record["bodyHeightTwips"] + 2 * MARGIN)})
    attributes(section, "pgMar", {"top": "720", "right": "720", "bottom": "720", "left": "720",
                                   "header": "360", "footer": "360", "gutter": "0"})
    attributes(section, "cols", {"num": "1", "space": "720", "equalWidth": "1"})
    if pitch is not None:
        attributes(section, "docGrid", {"type": "lines", "linePitch": str(pitch)})
    settings = ET.fromstring(parts["word/settings.xml"])
    require(names(settings) == ["compat"] and names(settings[0]) == ["compatSetting"],
            "unexpected settings")
    attributes(settings[0], "compatSetting", {"name": "compatibilityMode",
                                              "uri": "http://schemas.microsoft.com/office/word", "val": "15"})


def package_bytes(parts: dict[str, str]) -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, value in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o644 << 16
            archive.writestr(info, value.encode("utf-8"))
    return output.getvalue()


README = """# DocGrid Page-Capacity Source Probes

These 11 canonical inputs are UNMEASURED. Seven sweep body heights 4200, 4240,
4260, 4280, 4300, 4320, 4360 twips with lines/pitch360. Four no-grid controls
sweep 3300, 3310, 3320, 3340 twips. Every case explicitly sets snapToGrid true.

Each input has 12 independent paragraphs labelled G000 through G011, Times New
Roman 12 pt, auto240, zero before/after spacing, and left alignment. All four
font slots, sz and szCs match on text runs and paragraph marks. keepNext,
keepLines and widowControl are explicitly false in every paragraph. There
are no soft/hard breaks, tables, styles, docDefaults, or inheritance inputs.
Compatibility is 15, one column, width 11906 twips, and all four margins 720.
Page height is body height plus 1440; headers and footers have no content.

This is a new source contract, not a byte-equivalent repeat of the earlier
large-page docgrid batch: page heights now vary; keepNext and keepLines are
explicitly disabled; the no-grid controls also explicitly enable snapToGrid.
Old fixtures are neither changed nor treated as measurements of these inputs.

The manifest contains input-derived UTF-16 intervals: four label characters
and a CR paragraph mark per paragraph, 60 units total. Native Word source scans
must independently verify those anchors and endOfContent before comparison.
Record page count and actual per-page label/source ownership for every page.
Those observations constrain page-fit behavior; PDF glyph origins alone do
not identify a required extent, consumed line box, or page-fitting formula.
No expected page count, baseline, advance, or required extent is included.
Font family declarations do not bind the actual font files used by capture.

Generate with tools/measure/make_docgrid_page_fit_fixture.py --out NEW_DIRECTORY.
Existing directories are rejected. --check EXISTING_DIRECTORY only reads;
it verifies exact deterministic files and walks the XML/source contract.
Checks cover this narrow profile, not a complete OOXML XSD validation.
Generation and verification invoke neither Word nor the layout engine.
"""


def expected_files() -> dict[str, bytes]:
    files, cases = {}, []
    for case in variants():
        parts, record = build(case)
        data = package_bytes(parts)
        record.update({"sha256": sha256(data),
                       "documentXmlSha256": sha256(parts["word/document.xml"].encode("utf-8")),
                       "settingsXmlSha256": sha256(parts["word/settings.xml"].encode("utf-8")),
                       "packagePartSha256": {name: sha256(value.encode("utf-8"))
                                             for name, value in parts.items()}})
        files[record["file"]] = data
        cases.append(record)
    manifest = {
        "schema": "rsword-docgrid-page-fit-source/1", "captureStatus": "UNMEASURED",
        "generator": "tools/measure/make_docgrid_page_fit_fixture.py",
        "generatorSha256": sha256(Path(__file__).read_bytes()),
        "purpose": "Observe page count and per-page source ownership across body-height sweeps; no assumed page-fit or glyph-origin formula.",
        "pPrPropertyOrder": PPR_ORDER, "rPrPropertyOrder": RPR_ORDER, "sectPrPropertyOrder": SECT_ORDER,
        "optionalPropertyNote": "Only docGrid is absent in the no-grid controls; snapToGrid is explicitly true in all cases.",
        "fontFamily": FAMILY, "fontSizeHalfPoints": SIZE_HALF_POINTS,
        "fontApplication": "Explicit ascii, hAnsi, eastAsia, cs, sz and szCs on every text run and paragraph mark.",
        "fontIdentityNote": "Family declarations only; capture must bind actual font files separately.",
        "labelCount": LABEL_COUNT, "structure": "paragraphs", "lineRule": "auto", "lineValue": 240,
        "lineValueUnit": "240ths-of-line", "paragraphBeforeTwips": 0, "paragraphAfterTwips": 0,
        "keepNext": False, "keepLines": False, "widowControl": False, "snapToGrid": True,
        "alignment": "left", "stylesPart": False, "docDefaults": False,
        "hardBreaks": False, "softBreaks": False, "headerFooterContent": False,
        "compatibilityMode": 15, "columns": 1, "pageWidthTwips": PAGE_WIDTH,
        "pageHeightRule": "bodyHeightTwips + top margin 720 + bottom margin 720",
        "marginsTwips": {"top": MARGIN, "right": MARGIN, "bottom": MARGIN, "left": MARGIN,
                         "header": 360, "footer": 360, "gutter": 0},
        "gridBodyHeightsTwips": list(GRID_HEIGHTS), "noGridBodyHeightsTwips": list(CONTROL_HEIGHTS),
        "differencesFromEarlierLargePageInputs": ["Page height now varies with the body-height sweep.",
                                                  "keepNext and keepLines are explicitly false.",
                                                  "No-grid controls explicitly set snapToGrid true."],
        "zipProfile": {"compression": "deflate", "timestamp": [2026, 1, 1, 0, 0, 0],
                       "createSystem": 3, "permissionsOctal": "644"},
        "sourceUnit": "UTF-16 code units; half-open ranges including U+000D paragraph marks.",
        "sourceStatus": "Input-derived; native source scans must verify labels, paragraph ranges and endOfContent.",
        "observationBoundary": "Page count and per-page source ownership constrain fit; PDF glyph origins are not required extents.",
        "cases": cases,
    }
    files["manifest.json"] = (json.dumps(manifest, indent=2) + "\n").encode("utf-8")
    files["README.md"] = README.encode("utf-8")
    return files


def write_fixture(output: Path) -> dict:
    files = expected_files()
    output.mkdir(parents=True, exist_ok=False)
    for name, data in files.items():
        with (output / name).open("xb") as file:
            file.write(data)
    return summary(output, files, "GENERATED_UNMEASURED")


def check_fixture(directory: Path) -> dict:
    files = expected_files()
    require({path.name for path in directory.iterdir()} == set(files), "fixture directory contents differ")
    for name, expected in files.items():
        require((directory / name).read_bytes() == expected, f"deterministic bytes differ: {name}")
    manifest = json.loads(files["manifest.json"])
    for record in manifest["cases"]:
        with zipfile.ZipFile(directory / record["file"]) as archive:
            require(len(archive.namelist()) == len(set(archive.namelist())) == 5, "duplicate or extra ZIP part")
            verify({name: archive.read(name).decode("utf-8") for name in archive.namelist()}, record)
    return summary(directory, files, "CHECKED_UNMEASURED")


def summary(directory: Path, files: dict[str, bytes], state: str) -> dict:
    return {"directory": str(directory), "cases": len(variants()), "state": state,
            "manifestSha256": sha256(files["manifest.json"])}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    choice = parser.add_mutually_exclusive_group()
    choice.add_argument("--out", type=Path, help="New output directory; existing directories are rejected")
    choice.add_argument("--check", type=Path, help="Read-only deterministic package/XML/source verification")
    args = parser.parse_args()
    try:
        result = check_fixture(args.check) if args.check is not None else write_fixture(args.out or DEFAULT_OUT)
    except (OSError, ValueError, ET.ParseError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
