#!/usr/bin/env python3
"""Create deterministic keep-chain inputs and unmeasured competing page cuts."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
from xml.etree import ElementTree as ET
import zipfile


DEFAULT_OUT = Path("fixtures/keep-chain-canonical-2026-09-27")
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W}
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
FAMILY = "Times New Roman"
PPR_ORDER = ["keepNext", "keepLines", "pageBreakBefore", "widowControl", "spacing", "jc", "rPr"]
RPR_ORDER = ["rFonts", "vanish", "sz", "szCs"]
SECT_ORDER = ["type", "pgSz", "pgMar", "cols"]
SPACING = {"before": "0", "after": "0", "line": "480", "lineRule": "exact"}
FONT = {"family": FAMILY, "sizeHalfPoints": 24, "hidden": False}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def variants() -> list[dict]:
    definitions = [
        ("f1-a1-b3", [("F", 1), ("A", 1), ("B", 3)], "A", None, None,
         "Whole A+B fits a fresh nominal page; contrast whole-terminal reservation with a legal terminal prefix.",
         [("whole-terminal", [1, 4]), ("terminal-prefix", [4, 1])]),
        ("f1-a2-b3", [("F", 1), ("A", 2), ("B", 3)], "A", None, None,
         "A+B exceeds a fresh nominal page; distinguish prefix reservation from moving an oversized group first.",
         [("terminal-prefix", [4, 2]), ("move-before-relaxing", [1, 4, 1])]),
        ("a1-b3-c3", [("A", 1), ("B", 3), ("C", 3)], "AB", None, None,
         "An oversized chain starts at page top; inspect whether B's final line remains with C's first line.",
         [("tail-link-preserved", [3, 4]), ("middle-link-relaxed", [4, 3]), ("whole-middle-reserved", [1, 4, 2])]),
        ("f1-a1-b3-c3", [("F", 1), ("A", 1), ("B", 3), ("C", 3)], "AB", None, None,
         "A filler reduces initial capacity; compare partial middle-paragraph continuation and group movement.",
         [("prefix-and-tail-link", [4, 4]), ("whole-middle-reserved", [2, 4, 2]), ("move-chain-first", [1, 3, 4])]),
        ("a1-b1-c2-d3", [("A", 1), ("B", 1), ("C", 2), ("D", 3)], "ABC", None, None,
         "Three keep links test whether continuation keeps the final linked paragraph adjacent to D.",
         [("tail-link-preserved", [3, 4]), ("last-link-relaxed", [4, 3]), ("whole-middle-reserved", [2, 4, 1])]),
        ("a1-b3-keepLines-c3", [("A", 1), ("B", 3), ("C", 3)], "AB", "B", None,
         "B has keepLines; distinguish moving intact B plus a C prefix from relaxing either B constraint.",
         [("middle-kept-with-prefix", [1, 4, 2]), ("middle-link-relaxed", [4, 3]), ("keepLines-relaxed", [3, 4])]),
        ("a1-b3-widow-c3", [("A", 1), ("B", 3), ("C", 3)], "AB", None, "B",
         "B has widowControl; inspect the interaction between its legal split and the following keep link.",
         [("middle-kept-with-prefix", [1, 4, 2]), ("middle-link-relaxed", [4, 3]), ("widow-relaxed", [3, 4])]),
        ("f1-a1-b4-c3", [("F", 1), ("A", 1), ("B", 4), ("C", 3)], "AB", None, None,
         "Four-line B tests continuation when the remaining B and C prefix fit on a later page.",
         [("prefix-and-tail-link", [4, 4, 1]), ("whole-middle-reserved", [2, 3, 4]), ("move-chain-first", [1, 4, 4])]),
    ]
    return [{"name": name, "blocks": [
                {"role": role, "lineCount": count, "keepNext": role in keep,
                 "keepLines": role == together, "widowControl": role == widow}
                for role, count in blocks], "question": question, "candidateCuts": candidates}
            for name, blocks, keep, together, widow, question, candidates in definitions]


def rpr() -> str:
    return (f'<w:rPr><w:rFonts w:ascii="{FAMILY}" w:hAnsi="{FAMILY}" '
            f'w:eastAsia="{FAMILY}" w:cs="{FAMILY}"/><w:vanish w:val="0"/>'
            '<w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr>')


def package_parts(body: str) -> dict[str, str]:
    return {
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
        "word/document.xml": XML + f'<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>',
        "word/settings.xml": XML + (
            f'<w:settings xmlns:w="{W}"><w:compat>'
            '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/>'
            '</w:compat></w:settings>'),
    }


def build(case: dict) -> tuple[dict[str, str], dict]:
    paragraphs, records, anchors, source = [], [], [], ""
    for index, block in enumerate(case["blocks"]):
        start = len(source)
        labels = [f'{block["role"]}{i:03}' for i in range(block["lineCount"])]
        runs, controls, xml_runs = [], [], []
        for line_index, label in enumerate(labels):
            cp = len(source)
            last = line_index == len(labels) - 1
            anchors.append({"label": label, "paragraphIndex": index, "lineInParagraph": line_index,
                            "sourceStart": cp, "textEnd": cp + 4, "sourceEnd": cp + 5,
                            "terminator": "PARAGRAPH_MARK" if last else "SOFT_RETURN"})
            runs.append({"kind": "text", "label": label, "sourceStart": cp,
                         "sourceEnd": cp + 4, "font": FONT})
            xml_runs.append(f'<w:r>{rpr()}<w:t>{label}</w:t></w:r>')
            source += label
            controls.append({"kind": "PARAGRAPH_MARK" if last else "SOFT_RETURN",
                             "sourceStart": cp + 4, "sourceEnd": cp + 5})
            if not last:
                xml_runs.append(f'<w:r>{rpr()}<w:br w:type="textWrapping"/></w:r>')
                runs.append({"kind": "soft-return", "sourceStart": cp + 4,
                             "sourceEnd": cp + 5, "font": FONT})
            source += "\r" if last else "\v"
        props = (f'<w:keepNext w:val="{int(block["keepNext"])}"/>'
                 f'<w:keepLines w:val="{int(block["keepLines"])}"/><w:pageBreakBefore w:val="0"/>'
                 f'<w:widowControl w:val="{int(block["widowControl"])}"/>'
                 '<w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/>'
                 '<w:jc w:val="left"/>' + rpr())
        paragraphs.append(f'<w:p><w:pPr>{props}</w:pPr>{"".join(xml_runs)}</w:p>')
        records.append({"index": index, **block, "sourceStart": start, "sourceEnd": len(source),
                        "paragraphMarkOffset": len(source) - 1,
                        "softBreakOffsets": [c["sourceStart"] for c in controls if c["kind"] == "SOFT_RETURN"],
                        "bodyFont": FONT, "paragraphMarkFont": FONT,
                        "runs": runs, "controls": controls, "pageBreakBefore": False,
                        "spacing": SPACING, "visibleLabels": labels})
    section = ('<w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="11906" w:h="3360"/>'
               '<w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" '
               'w:header="360" w:footer="360" w:gutter="0"/>'
               '<w:cols w:num="1" w:space="720" w:equalWidth="1"/></w:sectPr>')
    parts = package_parts("".join(paragraphs) + section)
    labels = [a["label"] for a in anchors]
    candidates = []
    for name, counts in case["candidateCuts"]:
        require(sum(counts) == len(labels) and all(0 < n <= 4 for n in counts), "invalid candidate partition")
        cursor, pages = 0, []
        for count in counts:
            pages.append(labels[cursor:cursor + count])
            cursor += count
        candidates.append({"id": name, "status": "UNMEASURED", "hypotheticalLabelPages": pages})
    record = {"name": case["name"], "file": case["name"] + ".docx", "captureStatus": "UNMEASURED",
              "question": case["question"], "paragraphCount": len(records), "inputLineCount": len(anchors),
              "sourceText": source, "visibleText": "\n".join(labels), "sourceLengthUtf16": len(source),
              "sourceTextUtf16LeSha256": sha256(source.encode("utf-16-le")), "anchors": anchors,
              "paragraphs": records, "competingCandidates": candidates,
              "candidateScope": "Nonexhaustive hypothetical page cuts under a nominal four-line budget; not Word observations, engine output, or expectedLayout."}
    verify(parts, record)
    return parts, record


def names(element: ET.Element | None) -> list[str]:
    return [] if element is None else [c.tag.removeprefix(f"{{{W}}}") for c in element]


def attributes(element: ET.Element, tag: str, expected: dict[str, str]) -> None:
    child = element.find(f"w:{tag}", NS)
    require(child is not None and child.attrib == {f"{{{W}}}{k}": v for k, v in expected.items()},
            f"unexpected {tag} attributes")


def verify_font(element: ET.Element | None) -> None:
    require(names(element) == RPR_ORDER, "unexpected rPr order")
    attributes(element, "rFonts", {slot: FAMILY for slot in ("ascii", "hAnsi", "eastAsia", "cs")})
    for tag, value in (("vanish", "0"), ("sz", "24"), ("szCs", "24")):
        attributes(element, tag, {"val": value})


def verify(parts: dict[str, str], record: dict) -> None:
    require(set(parts) == set(package_parts("")), "extra or missing package part")
    for text in parts.values():
        ET.fromstring(text)
    body = ET.fromstring(parts["word/document.xml"]).find("w:body", NS)
    require(names(body) == ["p"] * record["paragraphCount"] + ["sectPr"], "unexpected body")
    source, anchors = "", []
    for index, (node, expected) in enumerate(zip(body.findall("w:p", NS), record["paragraphs"])):
        props = node.find("w:pPr", NS)
        require(names(props) == PPR_ORDER, "unexpected pPr order")
        for prop in ("keepNext", "keepLines", "pageBreakBefore", "widowControl"):
            attributes(props, prop, {"val": str(int(expected[prop]))})
        attributes(props, "spacing", SPACING)
        attributes(props, "jc", {"val": "left"})
        verify_font(props.find("w:rPr", NS))
        start, controls, labels = len(source), [], []
        for run in node.findall("w:r", NS):
            verify_font(run.find("w:rPr", NS))
            if names(run) == ["rPr", "t"]:
                label = run.find("w:t", NS).text
                require(label == f'{expected["role"]}{len(labels):03}', "unexpected label")
                cp = len(source)
                labels.append(label)
                source += label
                anchors.append({"label": label, "paragraphIndex": index, "lineInParagraph": len(labels) - 1,
                                "sourceStart": cp, "textEnd": cp + 4, "sourceEnd": cp + 5,
                                "terminator": "SOFT_RETURN"})
            else:
                require(names(run) == ["rPr", "br"], "unexpected source run")
                attributes(run, "br", {"type": "textWrapping"})
                controls.append({"kind": "SOFT_RETURN", "sourceStart": len(source), "sourceEnd": len(source) + 1})
                source += "\v"
        controls.append({"kind": "PARAGRAPH_MARK", "sourceStart": len(source), "sourceEnd": len(source) + 1})
        anchors[-1]["terminator"] = "PARAGRAPH_MARK"
        source += "\r"
        require(labels == expected["visibleLabels"] and len(labels) == expected["lineCount"], "line labels differ")
        require(controls == expected["controls"], "control CP mismatch")
        require(expected["sourceStart"] == start and expected["sourceEnd"] == len(source), "paragraph CP mismatch")
        require(expected["paragraphMarkOffset"] == len(source) - 1, "mark CP mismatch")
        require(expected["softBreakOffsets"] == [c["sourceStart"] for c in controls[:-1]], "soft CP mismatch")
    require(anchors == record["anchors"] and len({a["label"] for a in anchors}) == len(anchors), "anchor mismatch")
    require(source == record["sourceText"] and len(source.encode("utf-16-le")) // 2 == record["sourceLengthUtf16"], "source mismatch")
    require(sha256(source.encode("utf-16-le")) == record["sourceTextUtf16LeSha256"], "source hash mismatch")
    section = body.find("w:sectPr", NS)
    require(names(section) == SECT_ORDER, "unexpected section or docGrid")
    attributes(section, "type", {"val": "nextPage"})
    attributes(section, "pgSz", {"w": "11906", "h": "3360"})
    attributes(section, "pgMar", {"top": "720", "right": "720", "bottom": "720", "left": "720",
                                   "header": "360", "footer": "360", "gutter": "0"})
    attributes(section, "cols", {"num": "1", "space": "720", "equalWidth": "1"})
    settings = ET.fromstring(parts["word/settings.xml"])
    require(names(settings) == ["compat"] and names(settings[0]) == ["compatSetting"], "unexpected settings")
    attributes(settings[0], "compatSetting", {"name": "compatibilityMode", "uri": "http://schemas.microsoft.com/office/word", "val": "15"})


def package_bytes(parts: dict[str, str]) -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, text in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type, info.create_system, info.external_attr = zipfile.ZIP_DEFLATED, 3, 0o644 << 16
            archive.writestr(info, text.encode("utf-8"))
    return output.getvalue()


README = """# Keep-Chain Canonical Inputs

All eight inputs are UNMEASURED. They have not been opened in Word or run through
an engine. The competing label-page partitions in manifest.json are explicitly
nonexhaustive hypotheses, not an oracle or acceptance criteria.

The page is 11906 by 3360 twips with 720-twip margins: body height 1920 equals
four declared exact480 advances. This is a nominal four-line budget; actual
Word page capacity and required extents still need native/PDF observations.
Every paragraph has zero before/after spacing, explicit keepNext, keepLines,
pageBreakBefore=false and widowControl. All text, soft returns and paragraph
marks explicitly use four Times New Roman font slots, sz=szCs=24 and vanish=0.
There is one column, compatibility mode 15, and no docGrid, styles, docDefaults,
hard breaks, tables, header/footer content, or trailing empty paragraph.

| Case | Paragraph line counts | keepNext | Other true flag |
| --- | --- | --- | --- |
| f1-a1-b3 | F1 A1 B3 | A | none |
| f1-a2-b3 | F1 A2 B3 | A | none |
| a1-b3-c3 | A1 B3 C3 | A B | none |
| f1-a1-b3-c3 | F1 A1 B3 C3 | A B | none |
| a1-b1-c2-d3 | A1 B1 C2 D3 | A B C | none |
| a1-b3-keepLines-c3 | A1 B3 C3 | A B | B.keepLines |
| a1-b3-widow-c3 | A1 B3 C3 | A B | B.widowControl |
| f1-a1-b4-c3 | F1 A1 B4 C3 | A B | none |

Each input line has a unique four-character ASCII label such as A000 or B002.
Lines inside one paragraph are separated by textWrapping w:br (source U+000B);
each paragraph ends with source U+000D. The manifest records every visible run,
soft return, paragraph mark, UTF-16 half-open interval and package/source hash.
These are derived source contracts; capture must independently verify native
content, paragraph ranges and both complete CP scans, including all pages.
Actual font-file hashes, PDF family binding and print-view state remain capture
requirements. A label or glyph origin alone does not establish line-box height.

Use `python3 tools/measure/make_keep_chain_fixture.py --out NEW_DIRECTORY` to
generate a byte-reproducible copy. Existing paths are rejected. Use `--check
EXISTING_DIRECTORY` for read-only byte reconstruction and narrow XML/source
validation. ZIP timestamps and permissions are fixed. This verifier is not a
complete OOXML XSD validator. Generation/checking does not invoke Word, UI,
AppleScript, the layout engine, native APIs, Cargo or the network.
"""


def expected_files() -> dict[str, bytes]:
    files, cases = {}, []
    for case in variants():
        parts, record = build(case)
        data = package_bytes(parts)
        record.update({"sha256": sha256(data), "documentXmlSha256": sha256(parts["word/document.xml"].encode()),
                       "settingsXmlSha256": sha256(parts["word/settings.xml"].encode()),
                       "packagePartSha256": {name: sha256(text.encode()) for name, text in parts.items()}})
        files[record["file"]] = data
        cases.append(record)
    manifest = {
        "schema": "rsword-keep-chain-canonical-source/1", "captureStatus": "UNMEASURED",
        "generator": "tools/measure/make_keep_chain_fixture.py", "generatorSha256": sha256(Path(__file__).read_bytes()),
        "fontFamily": FAMILY, "fontSizeHalfPoints": 24,
        "fontIdentityNote": "Declared four-slot font family only; bind actual font bytes during capture.",
        "pPrPropertyOrder": PPR_ORDER, "rPrPropertyOrder": RPR_ORDER, "sectPrPropertyOrder": SECT_ORDER,
        "pageTwips": [11906, 3360], "bodyHeightTwips": 1920, "nominalLineCapacity": 4,
        "capacityStatus": "UNMEASURED; 1920 / declared exact480, not a Word page-fit observation",
        "marginsTwips": {"top": 720, "right": 720, "bottom": 720, "left": 720, "header": 360, "footer": 360, "gutter": 0},
        "lineRule": "exact", "lineValue": 480, "lineValueUnit": "twips", "paragraphBeforeTwips": 0,
        "paragraphAfterTwips": 0, "compatibilityMode": 15, "columns": 1, "docGrid": False,
        "stylesPart": False, "docDefaults": False, "hardBreaks": False, "headerFooterContent": False,
        "sourceUnit": "UTF-16 code units; half-open ranges include CR paragraph marks and VT soft returns.",
        "sourceStatus": "Input-derived; native Word content/paragraph ranges/dual CP scans must verify identity.",
        "hypothesisStatus": "UNMEASURED; competing cuts are nonexhaustive, not expected Word results or engine predictions.",
        "zipProfile": {"timestamp": [2026, 1, 1, 0, 0, 0], "compression": "deflate", "createSystem": 3, "permissionsOctal": "644"},
        "cases": cases,
    }
    files["manifest.json"] = (json.dumps(manifest, indent=2) + "\n").encode()
    files["README.md"] = README.encode()
    return files


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    choice = parser.add_mutually_exclusive_group()
    choice.add_argument("--out", type=Path, help="New directory; never overwrite")
    choice.add_argument("--check", type=Path, help="Read-only deterministic XML/source/package check")
    args = parser.parse_args()
    try:
        files = expected_files()
        directory = args.check if args.check is not None else args.out or DEFAULT_OUT
        if args.check is None:
            directory.mkdir(parents=True, exist_ok=False)
            for name, data in files.items():
                with (directory / name).open("xb") as handle:
                    handle.write(data)
        else:
            require({p.name for p in directory.iterdir()} == set(files), "unexpected directory contents")
            for name, data in files.items():
                require(not (directory / name).is_symlink() and (directory / name).read_bytes() == data,
                        f"deterministic bytes differ: {name}")
            for case in json.loads(files["manifest.json"])["cases"]:
                with zipfile.ZipFile(directory / case["file"]) as archive:
                    require(len(archive.namelist()) == len(set(archive.namelist())) == 5, "unexpected ZIP parts")
                    verify({name: archive.read(name).decode() for name in archive.namelist()}, case)
        print(json.dumps({"directory": str(directory), "cases": 8,
                          "state": "CHECKED_UNMEASURED" if args.check else "GENERATED_UNMEASURED",
                          "manifestSha256": sha256(files["manifest.json"])}))
    except (OSError, ValueError, ET.ParseError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
