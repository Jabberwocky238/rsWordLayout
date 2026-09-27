#!/usr/bin/env python3
"""Create deterministic contextual-spacing inputs and separate unmeasured hypotheses."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
from xml.etree import ElementTree as ET
import zipfile


DEFAULT_OUT = Path("fixtures/contextual-spacing-canonical-2026-09-27")
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/package/2006/relationships"
CT = "http://schemas.openxmlformats.org/package/2006/content-types"
RT = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
NS = {"w": W}
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
FAMILY = "Times New Roman"
FONT = {"family": FAMILY, "sizeHalfPoints": 24, "hidden": False}
PPR_ORDER = ["pStyle", "keepNext", "keepLines", "pageBreakBefore", "widowControl",
             "spacing", "contextualSpacing", "jc", "rPr"]
RPR_ORDER = ["rFonts", "vanish", "sz", "szCs"]
SECT_ORDER = ["type", "pgSz", "pgMar", "cols"]
MARGINS = {"top": "720", "right": "720", "bottom": "720", "left": "720",
           "header": "360", "footer": "360", "gutter": "0"}
FIXED_FLAGS = {"keepNext": False, "keepLines": False, "pageBreakBefore": False,
               "widowControl": False}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def spacing(line: int = 480, before: int = 0, after: int = 0) -> dict[str, str]:
    return {"before": str(before), "after": str(after), "line": str(line), "lineRule": "exact"}


def style(style_id: str, values: dict[str, str], contextual: bool) -> dict:
    return {"id": style_id, "spacing": values, "contextualSpacing": contextual}


def variants() -> list[dict]:
    common = spacing()
    controls = [False, False, True, False]
    same = {"name": "same-style-200-240", "styles": [style("ProbeA", common, False)],
            "styleIds": ["ProbeA"] * 4, "directSpacing": spacing(before=240, after=200),
            "directContextual": controls,
            "question": "Distinguish subtract-after-max, previous-paragraph suppression, and zero-before-max."}
    return [
        {"name": "style-only-line-control", "styles": [style("ProbeA", spacing(360), False),
                                                        style("ProbeB", spacing(720), False)],
         "styleIds": ["ProbeA", "ProbeA", "ProbeB", "ProbeB"], "directSpacing": None,
         "directContextual": [None] * 4,
         "question": "Verify style-only line inheritance using same-style adjacent pairs; do not interpret the A/B transition."},
        same,
        {**same, "name": "different-style-200-240",
         "styles": [style("ProbeA", common, False), style("ProbeB", common, False)],
         "styleIds": ["ProbeA", "ProbeB", "ProbeA", "ProbeB"],
         "question": "Different explicit style IDs have identical property definitions; test style identity rather than property equality."},
        {"name": "style-only-contextual-override",
         "styles": [style("ProbeA", spacing(before=240, after=200), True)],
         "styleIds": ["ProbeA"] * 3, "directSpacing": None,
         "directContextual": [None, False, None],
         "question": "Verify style-only spacing/contextual inheritance and a direct false override."},
        {"name": "before-phase", "styles": [style("ProbeA", common, False)],
         "styleIds": ["ProbeA"] * 4, "directSpacing": spacing(before=600),
         "directContextual": [True, False, True, False],
         "question": "Observe the alternating gap phase directly; the old 22-line capacity count did not distinguish ownership."},
        {"name": "both-phase", "styles": [style("ProbeA", common, False)],
         "styleIds": ["ProbeA"] * 4, "directSpacing": spacing(before=300, after=100),
         "directContextual": [True, False, True, False],
         "question": "Distinguish own-side subtraction from the previous-paragraph-drops-both report interpretation."},
    ]


def rpr() -> str:
    return (f'<w:rPr><w:rFonts w:ascii="{FAMILY}" w:hAnsi="{FAMILY}" '
            f'w:eastAsia="{FAMILY}" w:cs="{FAMILY}"/><w:vanish w:val="0"/>'
            '<w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr>')


def spacing_xml(values: dict[str, str]) -> str:
    return '<w:spacing ' + " ".join(f'w:{key}="{value}"' for key, value in values.items()) + '/>'


def package_parts(body: str, styles: list[dict]) -> dict[str, str]:
    declarations = "".join(
        f'<w:style w:type="paragraph" w:styleId="{s["id"]}"><w:name w:val="{s["id"]}"/>'
        f'<w:pPr>{spacing_xml(s["spacing"])}'
        f'<w:contextualSpacing w:val="{int(s["contextualSpacing"])}"/></w:pPr></w:style>'
        for s in styles)
    return {
        "[Content_Types].xml": XML + (
            f'<Types xmlns="{CT}">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
            '<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>'
            '</Types>'),
        "_rels/.rels": XML + (
            f'<Relationships xmlns="{R}"><Relationship Id="rId1" Type="{RT}officeDocument" '
            'Target="word/document.xml"/></Relationships>'),
        "word/_rels/document.xml.rels": XML + (
            f'<Relationships xmlns="{R}"><Relationship Id="rId1" Type="{RT}styles" Target="styles.xml"/>'
            f'<Relationship Id="rId2" Type="{RT}settings" Target="settings.xml"/></Relationships>'),
        "word/document.xml": XML + f'<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>',
        "word/styles.xml": XML + f'<w:styles xmlns:w="{W}">{declarations}</w:styles>',
        "word/settings.xml": XML + (
            f'<w:settings xmlns:w="{W}"><w:compat><w:compatSetting w:name="compatibilityMode" '
            'w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>'),
    }


def build(case: dict, case_index: int) -> tuple[dict[str, str], dict]:
    paragraphs, records, anchors, source = [], [], [], ""
    styles = {s["id"]: s for s in case["styles"]}
    for index, (style_id, direct_context) in enumerate(zip(case["styleIds"], case["directContextual"])):
        label = f'C{case_index}{index:02}'
        require(len(label) == 4 and label.isascii(), "label must be four ASCII characters")
        cp = len(source)
        props = f'<w:pStyle w:val="{style_id}"/>'
        props += "".join(f'<w:{key} w:val="0"/>' for key in FIXED_FLAGS)
        if case["directSpacing"] is not None:
            props += spacing_xml(case["directSpacing"])
        if direct_context is not None:
            props += f'<w:contextualSpacing w:val="{int(direct_context)}"/>'
        props += '<w:jc w:val="left"/>' + rpr()
        paragraphs.append(f'<w:p><w:pPr>{props}</w:pPr><w:r>{rpr()}<w:t>{label}</w:t></w:r></w:p>')
        source += label + "\r"
        anchors.append({"label": label, "paragraphIndex": index, "lineInParagraph": 0,
                        "sourceStart": cp, "textEnd": cp + 4, "sourceEnd": cp + 5,
                        "terminator": "PARAGRAPH_MARK"})
        records.append({"index": index, "label": label, "styleId": style_id,
                        "directSpacing": case["directSpacing"], "directContextualSpacing": direct_context,
                        "inputCascade": {"spacing": case["directSpacing"] or styles[style_id]["spacing"],
                                         "contextualSpacing": styles[style_id]["contextualSpacing"] if direct_context is None else direct_context},
                        "sourceStart": cp, "sourceEnd": cp + 5, "paragraphMarkOffset": cp + 4,
                        "bodyFont": FONT, "paragraphMarkFont": FONT, **FIXED_FLAGS,
                        "runs": [{"kind": "text", "label": label, "sourceStart": cp, "sourceEnd": cp + 4, "font": FONT}],
                        "controls": [{"kind": "PARAGRAPH_MARK", "sourceStart": cp + 4, "sourceEnd": cp + 5}]})
    section = ('<w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="11906" w:h="16838"/>'
               '<w:pgMar ' + " ".join(f'w:{key}="{value}"' for key, value in MARGINS.items()) + '/>'
               '<w:cols w:num="1" w:space="720" w:equalWidth="1"/></w:sectPr>')
    parts = package_parts("".join(paragraphs) + section, case["styles"])
    record = {"name": case["name"], "file": case["name"] + ".docx", "captureStatus": "UNMEASURED",
              "question": case["question"], "styles": case["styles"],
              "paragraphCount": len(records), "sourceText": source, "sourceLengthUtf16": len(source),
              "sourceTextUtf16LeSha256": sha256(source.encode("utf-16-le")),
              "anchors": anchors, "paragraphs": records}
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
    expected_parts = package_parts("", record["styles"])
    require(set(parts) == set(expected_parts), "unexpected package parts")
    for text in parts.values():
        ET.fromstring(text)
    for name in ("[Content_Types].xml", "_rels/.rels", "word/_rels/document.xml.rels", "word/settings.xml"):
        require(parts[name] == expected_parts[name], f"unexpected package contract: {name}")
    styles_root = ET.fromstring(parts["word/styles.xml"])
    require(names(styles_root) == ["style"] * len(record["styles"]), "unexpected styles/defaults")
    style_map = {}
    for node, expected in zip(styles_root, record["styles"]):
        require(node.attrib == {f"{{{W}}}type": "paragraph", f"{{{W}}}styleId": expected["id"]}, "unexpected style identity/default flag")
        require(names(node) == ["name", "pPr"], "unexpected style inheritance or properties")
        attributes(node, "name", {"val": expected["id"]})
        props = node.find("w:pPr", NS)
        require(names(props) == ["spacing", "contextualSpacing"], "unexpected style pPr order")
        attributes(props, "spacing", expected["spacing"])
        attributes(props, "contextualSpacing", {"val": str(int(expected["contextualSpacing"]))})
        require(expected["id"] not in style_map, "duplicate style ID")
        style_map[expected["id"]] = expected
    body = ET.fromstring(parts["word/document.xml"]).find("w:body", NS)
    require(names(body) == ["p"] * record["paragraphCount"] + ["sectPr"], "unexpected body")
    source, anchors = "", []
    for index, (node, expected) in enumerate(zip(body.findall("w:p", NS), record["paragraphs"])):
        require(names(node) == ["pPr", "r"], "unexpected paragraph children")
        props = node.find("w:pPr", NS)
        order = [name for name in PPR_ORDER if not (
            name == "spacing" and expected["directSpacing"] is None or
            name == "contextualSpacing" and expected["directContextualSpacing"] is None)]
        require(names(props) == order, "unexpected pPr order/direct property presence")
        attributes(props, "pStyle", {"val": expected["styleId"]})
        for name in FIXED_FLAGS:
            attributes(props, name, {"val": "0"})
            require(expected[name] is False, "fixed flag differs")
        if expected["directSpacing"] is not None:
            attributes(props, "spacing", expected["directSpacing"])
        if expected["directContextualSpacing"] is not None:
            attributes(props, "contextualSpacing", {"val": str(int(expected["directContextualSpacing"]))})
        attributes(props, "jc", {"val": "left"})
        verify_font(props.find("w:rPr", NS))
        inherited = style_map[expected["styleId"]]
        require(expected["inputCascade"] == {
            "spacing": expected["directSpacing"] or inherited["spacing"],
            "contextualSpacing": inherited["contextualSpacing"] if expected["directContextualSpacing"] is None else expected["directContextualSpacing"]}, "input cascade differs")
        run = node.find("w:r", NS)
        require(names(run) == ["rPr", "t"], "unexpected run/control")
        verify_font(run.find("w:rPr", NS))
        label = run.find("w:t", NS).text
        require(label == expected["label"] and len(label) == 4 and label.isascii(), "unexpected label")
        cp = len(source)
        source += label + "\r"
        require(expected["sourceStart"] == cp and expected["sourceEnd"] == cp + 5 and expected["paragraphMarkOffset"] == cp + 4, "paragraph CP mismatch")
        require(expected["runs"] == [{"kind": "text", "label": label, "sourceStart": cp, "sourceEnd": cp + 4, "font": FONT}], "run contract mismatch")
        require(expected["controls"] == [{"kind": "PARAGRAPH_MARK", "sourceStart": cp + 4, "sourceEnd": cp + 5}], "mark contract mismatch")
        anchors.append({"label": label, "paragraphIndex": index, "lineInParagraph": 0,
                        "sourceStart": cp, "textEnd": cp + 4, "sourceEnd": cp + 5, "terminator": "PARAGRAPH_MARK"})
    require(anchors == record["anchors"] and len({a["label"] for a in anchors}) == len(anchors), "anchor mismatch")
    require(source == record["sourceText"] and len(source.encode("utf-16-le")) // 2 == record["sourceLengthUtf16"], "source mismatch")
    require(sha256(source.encode("utf-16-le")) == record["sourceTextUtf16LeSha256"], "source hash mismatch")
    section = body.find("w:sectPr", NS)
    require(names(section) == SECT_ORDER, "unexpected section/docGrid")
    attributes(section, "type", {"val": "nextPage"})
    attributes(section, "pgSz", {"w": "11906", "h": "16838"})
    attributes(section, "pgMar", MARGINS)
    attributes(section, "cols", {"num": "1", "space": "720", "equalWidth": "1"})


def package_bytes(parts: dict[str, str]) -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, text in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type, info.create_system, info.external_attr = zipfile.ZIP_DEFLATED, 3, 0o644 << 16
            archive.writestr(info, text.encode("utf-8"))
    return output.getvalue()


def hypotheses(cases: list[dict]) -> dict:
    # Literal hypotheses are intentionally separate from the source contract and
    # do not run an engine or infer an observed Word spacing rule.
    rows = [
        ("same-style-200-240", [240, 0, 40], [240, 240, 0], [240, 200, 240]),
        ("different-style-200-240", [240, 240, 240], [240, 240, 0], [240, 240, 240]),
        ("style-only-contextual-override", [40, 0], [0, 240], [240, 200]),
        ("before-phase", [600, 0, 600], [0, 600, 0], [600, 0, 600]),
        ("both-phase", [200, 0, 200], [0, 300, 0], [300, 100, 300]),
    ]
    by_name = {case["name"]: case for case in cases}
    result = [{"case": "style-only-line-control", "status": "UNMEASURED",
               "conditionalSameStyleOriginStepTwips": [
                   {"from": "C000", "to": "C001", "stepTwips": 360},
                   {"from": "C002", "to": "C003", "stepTwips": 720}],
               "excludedTransition": ["C001", "C002"]}]
    for name, spec, previous, zero_first in rows:
        labels = [anchor["label"] for anchor in by_name[name]["anchors"]]
        require(len(spec) == len(previous) == len(zero_first) == len(labels) - 1, "candidate length differs")
        result.append({"case": name, "status": "UNMEASURED", "adjacentLabelPairs": list(zip(labels, labels[1:])),
                       "candidates": [{"id": key, "status": "UNMEASURED", "hypotheticalGapTwips": gaps,
                                       "conditionalOriginStepTwips": [480 + gap for gap in gaps]}
                                      for key, gaps in (("same-style-subtract-after-max", spec),
                                                        ("old-report-previous-paragraph-drops-both", previous),
                                                        ("same-style-zero-own-side-before-max", zero_first))]})
    return {"schema": "rsword-contextual-spacing-unmeasured-hypotheses/1", "status": "UNMEASURED",
            "conditions": ["Declared paragraph styles and direct false overrides must load as specified; actual style identity needs readback.",
                           "Each unique label must remain on one line in one flow region, with the declared fixed line spacing and identical run/mark fonts.",
                           "Origin-step arithmetic assumes stable per-paragraph baseline offset for each compared pair; the mixed-line A/B transition is excluded.",
                           "Raw observed origins may be quantized; preserve raw values and establish a bounded observation tolerance before verdicts.",
                           "A gap or glyph-origin step is not a required extent, page-fit result, or Word acceptance criterion."],
            "basis": "Competing models from the local ECMA 17.3.1.9 text/example and the audited old contextual-spacing report; no new Word observations.",
            "cases": result}


README = """# Contextual Spacing Canonical Inputs

All six inputs and all predictions are UNMEASURED. Generation/checking invokes
no Word, UI, native API, engine, Cargo, or network operation. manifest.json is
the source contract; hypotheses.json separately records conditional competing
gap/origin-step models. Neither file contains measured Word layout or an
expectedLayout oracle. Gap/origin-step values are not required extents.

| Case | Style IDs | Direct spacing | Contextual declarations |
| --- | --- | --- | --- |
| style-only-line-control | A,A,B,B | absent; styles exact360/exact720, zero gaps | absent; styles false |
| same-style-200-240 | A,A,A,A | exact480, before240, after200 | false,false,true,false |
| different-style-200-240 | A,B,A,B | exact480, before240, after200 | false,false,true,false |
| style-only-contextual-override | A,A,A | absent; style exact480, before240, after200 | inherit true, direct false, inherit true |
| before-phase | A,A,A,A | exact480, before600, after0 | true,false,true,false |
| both-phase | A,A,A,A | exact480, before300, after100 | true,false,true,false |

A/B mean explicit paragraph styles ProbeA/ProbeB. Different-style's definitions
have identical paragraph properties; only identity/name differ. There is no
basedOn, default style flag, docDefaults, linked style, or hidden inheritance.
The styles relationship belongs to word/document.xml and targets styles.xml;
settings.xml also has its own document relationship and content-type entry.
The package-root relationship targets only word/document.xml. Style-only
inputs contain no direct w:spacing, including no direct line setting.

All 23 paragraphs have one globally unique four-character ASCII label C000
through the case-specific C5xx labels, one run, and one CR paragraph mark.
Each occupies five UTF-16 code units. The manifest records exact labels,
run/mark ranges, styles, direct properties, and the declared input cascade.
That cascade is an input calculation, not a Word effective-property readback.

Every run and paragraph mark explicitly sets all four Times New Roman slots,
sz=szCs=24 and vanish=0. keepNext, keepLines, pageBreakBefore, widowControl are
explicitly false on every paragraph. Alignment is left. There is one column,
compatibility mode 15, no docGrid, no hard/soft breaks, no tables, no trailing
empty paragraph, and no header/footer content. Page size is 11906 by 16838
twips; all four margins are 720, gutter is zero, and body height is 15398.
These short inputs investigate adjacent origins, not page capacity.

Capture must bind actual font files, loaded style IDs/definitions, compatibility
mode and page setup, source text and paragraph ranges, complete dual CP scans,
and every PDF page/label. In the line-control input only compare A/A and B/B
pairs; its A/B boundary changes exact line spacing and is excluded from the
simple origin-step prediction. Other step hypotheses assume a stable baseline
offset for the matched same-font exact480 paragraphs, and still require a
declared quantization tolerance from raw observations. Do not turn a single
page count or glyph origin into a required-height rule.

Use `python3 tools/measure/make_contextual_spacing_fixture.py --out NEW_DIR`
for a byte-reproducible copy, or `--check EXISTING_DIR` for read-only exact-byte
and narrow XML/source validation. Existing output directories are rejected;
ZIP timestamps, permissions and part order are fixed. The checker validates
the explicit package/style/input profile, not the entire OOXML XSD. It rejects
unexpected files, symlinks, changed manifest/hypotheses, and changed ZIP bytes.
"""


def expected_files() -> dict[str, bytes]:
    files, cases = {}, []
    for index, case in enumerate(variants()):
        parts, record = build(case, index)
        data = package_bytes(parts)
        record.update({"sha256": sha256(data), "documentXmlSha256": sha256(parts["word/document.xml"].encode()),
                       "stylesXmlSha256": sha256(parts["word/styles.xml"].encode()),
                       "settingsXmlSha256": sha256(parts["word/settings.xml"].encode()),
                       "packagePartSha256": {name: sha256(text.encode()) for name, text in parts.items()}})
        files[record["file"]] = data
        cases.append(record)
    labels = [a["label"] for case in cases for a in case["anchors"]]
    require(len(set(labels)) == len(labels) == 23, "labels must be unique across all cases")
    manifest = {
        "schema": "rsword-contextual-spacing-canonical-source/1", "captureStatus": "UNMEASURED",
        "generator": "tools/measure/make_contextual_spacing_fixture.py", "generatorSha256": sha256(Path(__file__).read_bytes()),
        "fontFamily": FAMILY, "fontSizeHalfPoints": 24,
        "fontIdentityNote": "Declared four-slot family only; actual font bytes require capture binding.",
        "pPrPropertyOrder": PPR_ORDER, "pPrOptionalProperties": ["spacing", "contextualSpacing"],
        "stylePPrPropertyOrder": ["spacing", "contextualSpacing"], "rPrPropertyOrder": RPR_ORDER,
        "sectPrPropertyOrder": SECT_ORDER, "pageTwips": [11906, 16838], "bodyHeightTwips": 15398,
        "marginsTwips": {k: int(v) for k, v in MARGINS.items()}, "compatibilityMode": 15,
        "columns": 1, "docGrid": False, "docDefaults": False, "basedOn": False,
        "sourceUnit": "UTF-16 code units; half-open ranges include one CR paragraph mark per label.",
        "inputCascadeStatus": "Derived from the explicitly referenced local paragraph style and direct overrides, not Word readback.",
        "hypothesesFile": "hypotheses.json", "hypothesesStatus": "UNMEASURED; conditional models separate from source contract.",
        "zipProfile": {"timestamp": [2026, 1, 1, 0, 0, 0], "compression": "deflate", "createSystem": 3, "permissionsOctal": "644"},
        "cases": cases,
    }
    files["manifest.json"] = (json.dumps(manifest, indent=2) + "\n").encode()
    files["hypotheses.json"] = (json.dumps(hypotheses(cases), indent=2) + "\n").encode()
    files["README.md"] = README.encode()
    return files


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    choice = parser.add_mutually_exclusive_group()
    choice.add_argument("--out", type=Path, help="New output directory; existing paths are rejected")
    choice.add_argument("--check", type=Path, help="Read-only deterministic package/XML/source check")
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
            require(not directory.is_symlink(), "check directory must not be a symlink")
            require({p.name for p in directory.iterdir()} == set(files), "unexpected directory contents")
            for name, data in files.items():
                path = directory / name
                require(not path.is_symlink() and path.is_file() and path.read_bytes() == data,
                        f"deterministic bytes differ: {name}")
            for case in json.loads(files["manifest.json"])["cases"]:
                with zipfile.ZipFile(directory / case["file"]) as archive:
                    require(len(archive.namelist()) == len(set(archive.namelist())) == 6, "unexpected ZIP parts")
                    verify({name: archive.read(name).decode() for name in archive.namelist()}, case)
        print(json.dumps({"directory": str(directory), "cases": 6, "paragraphs": 23,
                          "state": "CHECKED_UNMEASURED" if args.check is not None else "GENERATED_UNMEASURED",
                          "manifestSha256": sha256(files["manifest.json"])}))
    except (OSError, ValueError, ET.ParseError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
