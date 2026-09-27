#!/usr/bin/env python3
"""Create deterministic, unmeasured exact-line first-y and advance probes."""

from __future__ import annotations

import argparse
import io
import json
from pathlib import Path
from xml.etree import ElementTree as ET
from xml.sax.saxutils import escape
import zipfile

import make_paragraph_mark_fixture as mark_package
from make_paragraph_mark_fixture import TNR, W, XML, font, rpr, sha256


DEFAULT_OUT = Path("fixtures/exact-vertical-canonical-2026-09-27")
PPR_ORDER = ["keepNext", "keepLines", "widowControl", "spacing", "jc", "rPr"]
RPR_ORDER = ["rFonts", "vanish", "sz", "szCs"]
SECT_ORDER = ["type", "pgSz", "pgMar", "cols"]
NS = {"w": W}


def utf16_length(text: str) -> int:
    return len(text.encode("utf-16-le")) // 2


def variants() -> list[dict]:
    base = {"lineTwips": 480, "bodyFamily": TNR, "bodySize": 24,
            "markSize": 24, "structure": "one-probe", "topTwips": 720}
    selected = [
        ("exact480-body12-mark12", {}, "Baseline for independent body and mark changes."),
        ("exact480-body24-mark12", {"bodySize": 48}, "Change only the probe body size."),
        ("exact480-body12-mark24", {"markSize": 48}, "Change only the probe mark size."),
        ("exact480-body24-mark24", {"bodySize": 48, "markSize": 48},
         "Complete the two-by-two body/mark size comparison."),
        ("exact480-body12Arial-mark12TNR", {"bodyFamily": "Arial"},
         "Change only the probe body family."),
        ("exact218-body12-mark12", {"lineTwips": 218}, "Short exact line with 12 pt body and mark."),
        ("exact218-body24-mark24", {"lineTwips": 218, "bodySize": 48, "markSize": 48},
         "Paired short exact line; body and mark sizes change together."),
        ("exact480-three-paragraphs", {"structure": "three-paragraphs"},
         "Three separate probe paragraphs followed by two reference paragraphs."),
        ("exact480-two-soft-returns", {"structure": "two-soft-returns"},
         "Same three probe labels in one paragraph, separated by two soft returns."),
        ("exact481-three-paragraphs", {"structure": "three-paragraphs", "lineTwips": 481},
         "Change only probe exact spacing relative to exact480-three-paragraphs."),
        ("exact480-empty-first-mark12", {"structure": "empty-first"},
         "An empty first probe paragraph with an explicit 12 pt mark."),
        ("exact480-body12-mark12-top721", {"topTwips": 721},
         "Change only the top margin by one twip relative to the baseline."),
    ]
    return [{**base, **changes, "name": name, "purpose": purpose}
            for name, changes, purpose in selected]


def paragraph(labels: list[str], body_font: dict | None, mark_font: dict,
              line: int, index: int, start: int, role: str) -> tuple[str, dict, str]:
    content, runs, controls, anchors = [], [], [], []
    source, cursor = "", start
    for label_index, label in enumerate(labels):
        label_start = cursor
        if label:
            assert body_font is not None
            content.append(f'<w:r>{rpr(body_font)}<w:t xml:space="preserve">'
                           f'{escape(label)}</w:t></w:r>')
            cursor += utf16_length(label)
            source += label
            runs.append({"kind": "text", "text": label, "sourceStart": label_start,
                         "sourceEnd": cursor, "font": body_font})
        text_end = cursor
        soft = label_index + 1 < len(labels)
        if soft:
            assert body_font is not None
            control_xml = '<w:br w:type="textWrapping"/>'
            content.append(f'<w:r>{rpr(body_font)}{control_xml}</w:r>')
            runs.append({"kind": "soft-return", "sourceStart": cursor,
                         "sourceEnd": cursor + 1, "font": body_font})
        else:
            control_xml = "implicit end of w:p"
        controls.append({"kind": "SOFT_RETURN" if soft else "PARAGRAPH_MARK",
                         "character": "\v" if soft else "\r",
                         "codePoint": "U+000B" if soft else "U+000D",
                         "sourceStart": cursor, "sourceEnd": cursor + 1,
                         "inputXml": control_xml})
        cursor += 1
        source += "\v" if soft else "\r"
        anchors.append({"label": label, "paragraphIndex": index, "role": role,
                        "sourceStart": label_start, "textEnd": text_end,
                        "sourceEnd": cursor, "terminator": controls[-1]["kind"]})
    properties = ('<w:keepNext w:val="0"/><w:keepLines w:val="0"/>'
                  '<w:widowControl w:val="0"/>'
                  f'<w:spacing w:before="0" w:after="0" w:line="{line}" w:lineRule="exact"/>'
                  '<w:jc w:val="left"/>' + rpr(mark_font))
    xml = f'<w:p><w:pPr>{properties}</w:pPr>{"".join(content)}</w:p>'
    record = {"index": index, "role": role, "sourceStart": start, "sourceEnd": cursor,
              "paragraphMarkOffset": cursor - 1, "lineRule": "exact", "lineTwips": line,
              "bodyFont": body_font, "paragraphMarkFont": mark_font,
              "runs": runs, "controls": controls, "anchors": anchors}
    return xml, record, source


def build(case: dict) -> tuple[dict[str, str], dict]:
    body_font = font(case["bodyFamily"], case["bodySize"])
    mark_font = font(TNR, case["markSize"])
    structure = case["structure"]
    labels = ["E000", "E001", "E002"]
    groups = ([[label] for label in labels] if structure == "three-paragraphs" else
              [labels] if structure == "two-soft-returns" else
              [[""]] if structure == "empty-first" else [["E000"]])
    inputs = [(group, None if structure == "empty-first" else body_font,
               mark_font, case["lineTwips"], "probe") for group in groups]
    reference = font(TNR, 24)
    inputs += [([label], reference, reference, 480, "reference") for label in ("R001", "R002")]
    paragraphs, records, source = [], [], ""
    for index, (group, body, mark, line, role) in enumerate(inputs):
        xml, record, text = paragraph(group, body, mark, line, index, utf16_length(source), role)
        paragraphs.append(xml)
        records.append(record)
        source += text
    section = ('<w:sectPr><w:type w:val="nextPage"/>'
               '<w:pgSz w:w="11906" w:h="16838"/>'
               f'<w:pgMar w:top="{case["topTwips"]}" w:right="720" w:bottom="720" w:left="720" '
               'w:header="360" w:footer="360" w:gutter="0"/>'
               '<w:cols w:num="1" w:space="720" w:equalWidth="1"/></w:sectPr>')
    # Reuse the existing OPC/settings template; only the document body differs.
    parts, _ = mark_package.build({"name": case["name"], "bodySize": 24, "markSize": 24})
    parts["word/document.xml"] = (XML + f'<w:document xmlns:w="{W}"><w:body>'
                                  + "".join(paragraphs) + section + '</w:body></w:document>')
    record = {"name": case["name"], "file": case["name"] + ".docx",
              "captureStatus": "UNMEASURED", "purpose": case["purpose"],
              "probe": {"lineRule": "exact", "lineTwips": case["lineTwips"],
                        "structure": structure, "bodyFont": None if structure == "empty-first" else body_font,
                        "paragraphMarkFont": mark_font},
              "topMarginTwips": case["topTwips"], "paragraphCount": len(records),
              "sourceText": source, "sourceLengthUtf16": utf16_length(source),
              "sourceTextUtf16LeSha256": sha256(source.encode("utf-16-le")),
              "paragraphs": records, "anchors": [a for p in records for a in p["anchors"]]}
    verify(parts, record)
    return parts, record


def local_names(element: ET.Element) -> list[str]:
    return [child.tag.split("}")[-1] for child in element]


def check_rpr(element: ET.Element, expected: dict) -> None:
    assert local_names(element) == RPR_ORDER
    fonts = element.find("w:rFonts", NS)
    assert fonts is not None
    assert fonts.attrib == {f"{{{W}}}{slot}": expected["family"]
                            for slot in ("ascii", "hAnsi", "eastAsia", "cs")}
    for tag, value in (("vanish", "0"), ("sz", str(expected["sizeHalfPoints"])),
                       ("szCs", str(expected["sizeHalfPoints"]))):
        assert element.find(f"w:{tag}", NS).attrib == {f"{{{W}}}val": value}


def verify(parts: dict[str, str], record: dict) -> None:
    """Check the emitted profile and source independently by walking its XML."""
    assert len(parts) == 5 and "word/styles.xml" not in parts
    for xml in parts.values():
        ET.fromstring(xml)
    document = ET.fromstring(parts["word/document.xml"])
    assert document.find(".//w:docGrid", NS) is None
    paragraphs = document.findall("w:body/w:p", NS)
    assert len(paragraphs) == record["paragraphCount"] == len(record["paragraphs"])
    source, offset = "", 0
    for index, (element, expected) in enumerate(zip(paragraphs, record["paragraphs"])):
        assert expected["index"] == index and expected["sourceStart"] == offset
        ppr = element.find("w:pPr", NS)
        assert local_names(ppr) == PPR_ORDER
        for tag in ("keepNext", "keepLines", "widowControl"):
            assert ppr.find(f"w:{tag}", NS).attrib == {f"{{{W}}}val": "0"}
        assert ppr.find("w:spacing", NS).attrib == {
            f"{{{W}}}{key}": value for key, value in
            (("before", "0"), ("after", "0"), ("line", str(expected["lineTwips"])), ("lineRule", "exact"))}
        check_rpr(ppr.find("w:rPr", NS), expected["paragraphMarkFont"])
        runs = element.findall("w:r", NS)
        assert len(runs) == len(expected["runs"])
        controls = []
        for run, run_record in zip(runs, expected["runs"]):
            check_rpr(run.find("w:rPr", NS), run_record["font"])
            assert run_record["sourceStart"] == offset
            text = run.find("w:t", NS)
            if text is not None:
                assert run_record["kind"] == "text" and text.text == run_record["text"]
                content = text.text
            else:
                assert run_record["kind"] == "soft-return"
                assert run.find("w:br", NS).attrib == {f"{{{W}}}type": "textWrapping"}
                controls.append(("SOFT_RETURN", offset))
                content = "\v"
            source += content
            offset += utf16_length(content)
            assert run_record["sourceEnd"] == offset
        controls.append(("PARAGRAPH_MARK", offset))
        assert expected["paragraphMarkOffset"] == offset
        assert controls == [(control["kind"], control["sourceStart"]) for control in expected["controls"]]
        for control in expected["controls"]:
            assert control["sourceEnd"] == control["sourceStart"] + 1
            assert control["character"] == ("\v" if control["kind"] == "SOFT_RETURN" else "\r")
        source += "\r"
        offset += 1
        assert expected["sourceEnd"] == offset
    assert source == record["sourceText"] and offset == record["sourceLengthUtf16"]
    assert sha256(source.encode("utf-16-le")) == record["sourceTextUtf16LeSha256"]
    section = document.find("w:body/w:sectPr", NS)
    assert local_names(section) == SECT_ORDER
    assert section.find("w:pgMar", NS).get(f"{{{W}}}top") == str(record["topMarginTwips"])
    settings = ET.fromstring(parts["word/settings.xml"])
    compatibility = settings.find("w:compat/w:compatSetting", NS)
    assert compatibility.attrib == {f"{{{W}}}name": "compatibilityMode",
                                    f"{{{W}}}uri": "http://schemas.microsoft.com/office/word",
                                    f"{{{W}}}val": "15"}


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


README = """# Exact Vertical Input Probes

These 12 canonical inputs are UNMEASURED. They contain no expected glyph y,
baseline, line height, advance, page count, or control glyph count. They prepare
new Word observations; historical noncanonical inputs are not an oracle here.

The exact480 baseline contains E000 followed by reference paragraphs R001 and
R002. Four cases independently vary 12/24 pt body and mark sizes. A fifth varies
only the body family to Arial. The short exact218 pair varies body and mark
sizes together (12 versus 24 pt), so it does not isolate their individual roles.

The structure pair has the same E000/E001/E002 probe labels, either as three
paragraphs or one paragraph with two textWrapping soft returns. Both then have
R001 and R002 reference paragraphs. The exact481 phase probe changes only the
three probe paragraphs' exact line spacing relative to exact480-three-paragraphs.
References always remain Times New Roman 12 pt, exact480. The empty-first case
has no body run in its first paragraph and an explicit 12 pt mark. The top721
case changes only pgMar.top from 720 to 721 twips relative to the baseline.

All paragraphs explicitly set keepNext, keepLines and widowControl false,
before/after spacing zero, exact line spacing, and left alignment. All text,
soft-return runs and paragraph marks explicitly set four font slots, sz, szCs
and vanish false. Compatibility mode is 15, with one column and no docGrid,
styles or docDefaults. pPr/rPr/sectPr property order is listed in the manifest.

The manifest records each input-derived UTF-16 range and CR/VT control.
Paragraph boundaries are CR, soft returns are VT; control identities must be
verified against native Word source scans before any PDF glyph pairing. Font
declarations are not actual font-file bindings. Capture native source, repeated
scans, font identity and vector output before deriving layout expectations.

Run python3 tools/measure/make_exact_vertical_fixture.py --out NEW_DIRECTORY to
reproduce the package, manifest and README. Existing directories are rejected.
Use --check EXISTING_DIRECTORY for a read-only deterministic/source audit.
Generation and checking invoke neither Word nor the layout engine. The XML
profile checks are not a complete OOXML XSD validation or proof of Word behavior.
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
        data = package_bytes(parts)
        assert data == package_bytes(parts)
        record["sha256"] = sha256(data)
        record["packagePartSha256"] = {name: sha256(value.encode("utf-8")) for name, value in parts.items()}
        generated.append((record, data))
    manifest = {
        "schema": "rsword-exact-vertical-source/1", "captureStatus": "UNMEASURED",
        "generator": "tools/measure/make_exact_vertical_fixture.py",
        "generatorSha256": sha256(Path(__file__).read_bytes()),
        "packageHelper": "tools/measure/make_paragraph_mark_fixture.py",
        "packageHelperSha256": sha256(Path(mark_package.__file__).read_bytes()),
        "pPrPropertyOrder": PPR_ORDER, "rPrPropertyOrder": RPR_ORDER, "sectPrPropertyOrder": SECT_ORDER,
        "referenceParagraphs": {"labels": ["R001", "R002"], "font": font(TNR, 24),
                                "lineRule": "exact", "lineTwips": 480},
        "paragraphBeforeTwips": 0, "paragraphAfterTwips": 0, "keepNext": False,
        "keepLines": False, "widowControl": False, "compatibilityMode": 15,
        "pageTwips": [11906, 16838], "marginsTwips": {"top": "per-case", "left": 720, "right": 720, "bottom": 720},
        "columns": 1, "docGrid": False, "stylesPart": False, "docDefaults": False,
        "sourceUnit": "UTF-16 code units; half-open ranges, CR paragraph marks and VT soft returns.",
        "sourceStatus": "Input-derived; native Word source and repeated scans must independently verify it.",
        "fontIdentityNote": "Explicit family declarations; actual font files must be bound during capture.",
        "cases": [record for record, _ in generated],
    }
    manifest_text = json.dumps(manifest, indent=2) + "\n"
    if args.check:
        assert (args.check / "manifest.json").read_text(encoding="utf-8") == manifest_text
        assert (args.check / "README.md").read_text(encoding="utf-8") == README
        assert {path.name for path in args.check.iterdir()} == {record["file"] for record, _ in generated} | {"manifest.json", "README.md"}
        for record, expected in generated:
            actual = (args.check / record["file"]).read_bytes()
            assert actual == expected
            with zipfile.ZipFile(io.BytesIO(actual)) as archive:
                assert len(archive.namelist()) == len(set(archive.namelist())) == 5
                verify({name: archive.read(name).decode("utf-8") for name in archive.namelist()}, record)
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
