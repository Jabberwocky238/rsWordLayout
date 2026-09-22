"""The frozen probe manifest must describe the actual packaged source bytes."""

import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET
import zipfile

from make_vertical_precision_fixture import FAMILIES, SIZES_HALF_POINTS, write_fixture
from wordmeasure import docxtext

ROOT = Path(__file__).resolve().parents[3]


def test_regeneration_preserves_frozen_source_and_manifest(tmp_path):
    source = ROOT / "fixtures/vertical-precision.docx"
    manifest = ROOT / "fixtures/vertical-precision.probes.json"
    inputs = ROOT / "fixtures/vertical-precision.font-inputs.json"
    actual = write_fixture(tmp_path / "case.docx", tmp_path / "cases.json", inputs)
    assert (tmp_path / "case.docx").read_bytes() == source.read_bytes()
    assert actual == json.loads(manifest.read_text())
    assert actual["fixtureSha256"] == hashlib.sha256(source.read_bytes()).hexdigest()
    derived = docxtext.content_text(source)
    assert len(actual["cases"]) == 21
    assert len(derived["text"]) == 609
    for case, paragraph in zip(actual["cases"], derived["paragraphs"]):
        start, end = case["sourceStart"], case["sourceEnd"]
        assert (start, end) == (paragraph["start"], paragraph["end"])
        assert derived["text"][start:end] == case["text"]
        spans = list(case["spans"].values())
        assert spans[0][0] == start and spans[-1][1] == end - 1
        assert all(a[1] == b[0] for a, b in zip(spans, spans[1:]))
        for role in ("normal", "superscript", "subscript"):
            left, right = case["spans"][role]
            assert derived["text"][left:right] == "HHHHHH"


def test_packaged_runs_have_explicit_controls_and_source_alignment():
    with zipfile.ZipFile(ROOT / "fixtures/vertical-precision.docx") as archive:
        root = ET.fromstring(archive.read("word/document.xml"))
    w = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
    paragraphs = root.findall(f"{w}body/{w}p")
    for paragraph, (family, size) in zip(paragraphs, (
            (family, size) for family in FAMILIES for size in SIZES_HALF_POINTS)):
        spacing = paragraph.find(f"{w}pPr/{w}spacing")
        assert spacing.attrib == {w + "before": "0", w + "after": "0",
                                  w + "line": "600", w + "lineRule": "exact"}
        for run in paragraph.findall(w + "r"):
            props = run.find(w + "rPr")
            assert props.find(w + "rFonts").get(w + "ascii") == family
            assert props.find(w + "sz").get(w + "val") == str(size)
            assert int(props.find(w + "kern").get(w + "val")) > max(SIZES_HALF_POINTS)
        alignments = paragraph.findall(f"{w}r/{w}rPr/{w}vertAlign")
        assert [item.get(w + "val") for item in alignments] == [
            "baseline", "baseline", "baseline", "baseline", "superscript",
            "baseline", "subscript", "baseline"]
