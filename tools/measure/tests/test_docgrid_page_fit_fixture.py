"""Source-contract checks only; none of these assertions predict Word pagination."""

import json
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile

import pytest

import make_docgrid_page_fit_fixture as fixture
from wordmeasure import docxtext


def snapshot(directory):
    return {path.name: (path.read_bytes(), path.stat().st_mtime_ns)
            for path in directory.iterdir()}


def test_two_generations_and_cli_check_preserve_all_bytes_and_mtimes(tmp_path):
    first, second = tmp_path / "first", tmp_path / "second"
    fixture.write_fixture(first)
    fixture.write_fixture(second)
    expected = snapshot(first)
    assert {name: data for name, (data, _) in expected.items()} == {
        name: data for name, (data, _) in snapshot(second).items()}
    result = subprocess.run([sys.executable, str(Path(fixture.__file__).resolve()),
                             "--check", str(first)], capture_output=True, text=True, check=True)
    report = json.loads(result.stdout)
    assert report["state"] == "CHECKED_UNMEASURED"
    assert report["cases"] == 11
    assert report["manifestSha256"] == fixture.sha256((first / "manifest.json").read_bytes())
    assert snapshot(first) == expected


def test_packaged_xml_and_independent_source_reader_match_the_declared_contract(tmp_path):
    output = tmp_path / "inputs"
    fixture.write_fixture(output)
    manifest = json.loads((output / "manifest.json").read_text())
    cases = manifest["cases"]
    assert [case["bodyHeightTwips"] for case in cases[:7]] == [4200, 4240, 4260, 4280, 4300, 4320, 4360]
    assert [case["bodyHeightTwips"] for case in cases[7:]] == [3300, 3310, 3320, 3340]
    assert all(case["docGrid"]["present"] for case in cases[:7])
    assert not any(case["docGrid"]["present"] for case in cases[7:])
    w = "{" + fixture.W + "}"
    for case in cases:
        path = output / case["file"]
        source = docxtext.content_text(path)
        assert source["text"] == "".join(f"G{index:03}\r" for index in range(12)) == case["sourceText"]
        assert [(p["start"], p["end"]) for p in source["paragraphs"]] == [
            (index * 5, index * 5 + 5) for index in range(12)]
        assert case["sourceLengthUtf16"] == 60
        for index, anchor in enumerate(case["anchors"]):
            assert anchor["sourceStart"] == index * 5
            assert anchor["textEnd"] == anchor["paragraphMarkOffset"] == index * 5 + 4
            assert anchor["sourceEnd"] == index * 5 + 5
        with zipfile.ZipFile(path) as archive:
            root = ET.fromstring(archive.read("word/document.xml"))
            section = root.find(f"{w}body/{w}sectPr")
            height = int(section.find(w + "pgSz").get(w + "h"))
            margins = section.find(w + "pgMar")
            assert height - int(margins.get(w + "top")) - int(margins.get(w + "bottom")) == case["bodyHeightTwips"]
            for paragraph in root.findall(f"{w}body/{w}p"):
                properties = paragraph.find(w + "pPr")
                assert [child.tag for child in properties] == [w + name for name in
                    ("keepNext", "keepLines", "widowControl", "snapToGrid", "spacing", "jc", "rPr")]
                for tag in ("keepNext", "keepLines", "widowControl"):
                    assert properties.find(w + tag).get(w + "val") == "0"
                assert properties.find(w + "snapToGrid").get(w + "val") == "1"
                assert ET.tostring(properties.find(w + "rPr")) == ET.tostring(paragraph.find(f"{w}r/{w}rPr"))
            assert set(archive.namelist()) == set(case["packagePartSha256"])
            for name in archive.namelist():
                assert fixture.sha256(archive.read(name)) == case["packagePartSha256"][name]
    assert "expectedLayout" not in (output / "manifest.json").read_text()


def test_existing_output_is_refused_without_changing_it(tmp_path):
    output = tmp_path / "existing"
    output.mkdir()
    (output / "sentinel").write_bytes(b"preserve")
    before = snapshot(output)
    with pytest.raises(FileExistsError):
        fixture.write_fixture(output)
    assert snapshot(output) == before


@pytest.mark.parametrize("mutation", ["manifest", "docx", "missing", "extra"])
def test_check_rejects_modified_packages_or_directory_without_repairing(tmp_path, mutation):
    output = tmp_path / "inputs"
    fixture.write_fixture(output)
    first = output / (fixture.variants()[0]["name"] + ".docx")
    if mutation == "manifest":
        with (output / "manifest.json").open("ab") as file:
            file.write(b" ")
    elif mutation == "docx":
        with first.open("ab") as file:
            file.write(b"changed")
    elif mutation == "missing":
        first.unlink()
    else:
        (output / "extra").write_bytes(b"unexpected")
    before = snapshot(output)
    with pytest.raises(ValueError):
        fixture.check_fixture(output)
    assert snapshot(output) == before


@pytest.mark.parametrize(("before", "after"), [
    ('<w:keepNext w:val="0"/>', '<w:keepNext w:val="1"/>'),
    ('w:h="5640"', 'w:h="5641"'),
    ('w:linePitch="360"', 'w:linePitch="361"'),
    ('<w:szCs w:val="24"/>', '<w:szCs w:val="25"/>'),
    ('G011', 'G010'),
])
def test_xml_contract_checker_rejects_wrong_properties_geometry_or_source(before, after):
    parts, record = fixture.build(fixture.variants()[0])
    assert before in parts["word/document.xml"]
    parts["word/document.xml"] = parts["word/document.xml"].replace(before, after, 1)
    with pytest.raises(ValueError):
        fixture.verify(parts, record)
