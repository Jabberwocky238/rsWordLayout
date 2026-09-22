"""Offline annotations require fixture identity and source identity, not a guess."""

import copy
import hashlib
import json
import zipfile
from pathlib import Path

import pytest

from wordmeasure import OK, UNDECIDABLE, capture, cli, source_binding, wordmodel

ROOT = Path(__file__).resolve().parents[3]
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def fixture(tmp_path, body=None, text="A\f\nB\n", ranges=((0, 3), (3, 5))):
    if body is None:
        body = ('<w:p><w:r><w:t>A</w:t><w:br w:type="page"/></w:r></w:p>'
                '<w:p><w:r><w:t>B</w:t></w:r></w:p>')
    path = tmp_path / "case.docx"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("word/document.xml", '<w:document xmlns:w="%s"><w:body>%s</w:body></w:document>' % (W, body))
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return path, {
        "path": str(tmp_path / "capture"),
        "META": {"fixture": {"unchanged": True, "before": {"sha256": digest}, "after": {"sha256": digest}}},
        "sweep": {"platform": "mac", "contentText": text, "endOfContent": ranges[-1][1],
                  "paragraphs": [{"start": start, "end": end} for start, end in ranges]},
        "glyphs": {"pages": []},
    }


def test_missing_marks_are_bound_without_mutating_the_capture(tmp_path):
    path, bundle = fixture(tmp_path)
    original = copy.deepcopy(bundle)
    bound = source_binding.bind_source(bundle, path)
    assert bundle == original
    assert bound["sweep"]["contentText"] == "A\f\nB\n"
    assert bound["sweep"]["marks"] == {"1": "PAGE_BREAK", "2": "PARAGRAPH_MARK", "4": "PARAGRAPH_MARK"}
    record = bound["sourceAnnotation"]
    assert record["state"] == OK
    assert record["derived"] is record["backtest"] is True
    assert record["sourceDocx"]["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
    assert record["verification"]["contentText"]["paragraphCrToMacLf"] == 2
    assert record["verification"]["sourceRanges"]["state"] == OK
    assert record["verification"]["existingMarks"] == {"state": OK, "preserved": 0, "added": 3}


@pytest.mark.parametrize("which", ["unchanged", "before", "after"])
def test_capture_must_confirm_identical_before_and_after_bytes(tmp_path, which):
    path, bundle = fixture(tmp_path)
    if which == "unchanged":
        bundle["META"]["fixture"][which] = False
    else:
        bundle["META"]["fixture"][which]["sha256"] = "0" * 64
    with pytest.raises(ValueError, match="FIXTURE_IDENTITY_UNVERIFIED"):
        source_binding.bind_source(bundle, path)


def test_same_document_text_with_different_archive_bytes_is_rejected(tmp_path):
    path, bundle = fixture(tmp_path)
    with zipfile.ZipFile(path, "a") as archive:
        archive.writestr("unrelated.txt", "different bytes")
    with pytest.raises(ValueError, match="SOURCE_DOCX_HASH_MISMATCH"):
        source_binding.bind_source(bundle, path)


def test_same_length_but_different_captured_text_is_rejected(tmp_path):
    path, bundle = fixture(tmp_path)
    bundle["sweep"]["contentText"] = "A\f\nC\n"
    with pytest.raises(ValueError, match="CONTENT_TEXT_MISMATCH"):
        source_binding.bind_source(bundle, path)


@pytest.mark.parametrize("change", ["end", "paragraph", "missing_paragraphs"])
def test_all_captured_ranges_must_be_verified(tmp_path, change):
    path, bundle = fixture(tmp_path)
    if change == "end":
        bundle["sweep"]["endOfContent"] += 1
    elif change == "paragraph":
        bundle["sweep"]["paragraphs"][0]["end"] -= 1
    else:
        del bundle["sweep"]["paragraphs"]
    with pytest.raises(ValueError, match="SOURCE_RANGE_VERIFICATION_FAILED"):
        source_binding.bind_source(bundle, path)


def test_partial_annotations_are_preserved_and_additions_are_reported(tmp_path):
    path, bundle = fixture(tmp_path)
    bundle["sweep"]["marks"] = {1: "PAGE_BREAK", "2": "PARAGRAPH_MARK"}
    bound = source_binding.bind_source(bundle, path)
    assert bound["sweep"]["marks"][1] == "PAGE_BREAK"
    assert "1" not in bound["sweep"]["marks"]
    assert bound["sourceAnnotation"]["verification"]["existingMarks"]["preserved"] == 2
    assert bound["sourceAnnotation"]["verification"]["existingMarks"]["added"] == 1
    assert bundle["sweep"]["marks"] == {1: "PAGE_BREAK", "2": "PARAGRAPH_MARK"}


@pytest.mark.parametrize("marks", [{"1": "SECTION_BREAK"}, {"0": "PARAGRAPH_MARK"}, {"0": None}])
def test_conflicting_annotations_are_not_replaced(tmp_path, marks):
    path, bundle = fixture(tmp_path)
    bundle["sweep"]["marks"] = marks
    with pytest.raises(ValueError, match="SOURCE_MARK_CONFLICT"):
        source_binding.bind_source(bundle, path)
    assert bundle["sweep"]["marks"] == marks


def test_mac_lf_exception_applies_only_to_source_annotated_paragraph_marks(tmp_path):
    path, bundle = fixture(tmp_path,
        '<w:p><w:r><w:t>A&#13;B</w:t></w:r></w:p>', "A\nB\n", ((0, 4),))
    with pytest.raises(ValueError, match="CONTENT_TEXT_MISMATCH"):
        source_binding.bind_source(bundle, path)


def test_section_break_is_not_relabelled_from_captured_lf(tmp_path):
    path, bundle = fixture(tmp_path,
        '<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>A</w:t></w:r></w:p>', "A\n", ((0, 2),))
    with pytest.raises(ValueError, match="CONTENT_TEXT_MISMATCH"):
        source_binding.bind_source(bundle, path)


def test_paragraph_lf_substitution_requires_a_mac_capture(tmp_path):
    path, bundle = fixture(tmp_path)
    bundle["sweep"]["platform"] = "windows"
    with pytest.raises(ValueError, match="CONTENT_TEXT_MISMATCH"):
        source_binding.bind_source(bundle, path)


@pytest.mark.parametrize("field", ["endOfContent", "contentText"])
def test_missing_source_evidence_is_a_clear_verification_error(tmp_path, field):
    path, bundle = fixture(tmp_path)
    del bundle["sweep"][field]
    with pytest.raises(ValueError):
        source_binding.bind_source(bundle, path)


def test_utf16_annotations_after_non_bmp_text_are_preserved(tmp_path):
    path, bundle = fixture(tmp_path,
        '<w:p><w:r><w:t>&#x1f600;</w:t><w:br w:type="page"/></w:r></w:p>', "\U0001f600\f\n", ((0, 4),))
    bound = source_binding.bind_source(bundle, path)
    assert bound["sweep"]["marks"] == {"2": "PAGE_BREAK", "3": "PARAGRAPH_MARK"}


@pytest.mark.parametrize("command", ["model", "compare"])
def test_cli_reports_unverified_input_as_undecidable_without_building_or_comparing(tmp_path, monkeypatch, capsys, command):
    path, bundle = fixture(tmp_path)
    bundle["sweep"]["contentText"] = "A\f\nC\n"
    monkeypatch.setattr(cli.capture_mod, "load_bundle", lambda _: bundle)
    def must_not_run(*args, **kwargs):
        pytest.fail("Unverified source must not reach the model or comparator")
    monkeypatch.setattr(cli.wordmodel, "build", must_not_run)
    monkeypatch.setattr(cli.compare_mod, "compare", must_not_run)
    args = [command, "bundle"]
    if command == "compare":
        trace = tmp_path / "trace.json"
        trace.write_text(json.dumps({"schema": "rsword-layout-trace/1", "unit": "pt", "pages": []}))
        args.append(str(trace))
    assert cli.main(args + ["--source-docx", str(path), "--json"]) == 2
    output = json.loads(capsys.readouterr().out)
    assert output["state"] == UNDECIDABLE
    reference = output["reference"] if command == "compare" else output
    assert reference["sourceAnnotation"]["state"] == UNDECIDABLE
    assert "CONTENT_TEXT_MISMATCH" in reference["sourceAnnotation"]["reason"]


def test_vmisc2_backtest_uses_verified_marks_without_changing_original_capture():
    bundle = capture.load_bundle(ROOT / "captures/vmisc2-2026-09-17")
    original = copy.deepcopy(bundle)
    before = wordmodel.build(bundle)
    assert sum(page["state"] == UNDECIDABLE for page in before["pages"]) == 3
    bound = source_binding.bind_source(bundle, ROOT / "fixtures/vmisc2.docx")
    model = wordmodel.build(bound)
    assert model["state"] == OK
    expected_lines = len(wordmodel.lines_from_sweep(bundle["sweep"]))
    lines = [line for page in model["pages"] for line in page["lines"]]
    assert len(lines) == expected_lines
    assert all(line["state"] == OK for line in lines)
    assert sum(len(line["glyphs"]) for line in lines) == 88
    assert sum(line["identityMismatched"] for line in lines) == 0
    assert bound["sourceAnnotation"]["backtest"] is True
    assert bundle == original


def test_model_cli_emits_annotation_provenance(tmp_path):
    output = tmp_path / "model.json"
    assert cli.main(["model", str(ROOT / "captures/vmisc2-2026-09-17"),
                     "--source-docx", str(ROOT / "fixtures/vmisc2.docx"), "--output", str(output)]) == 0
    model = json.loads(output.read_text())
    assert model["sourceAnnotation"]["state"] == OK
    assert model["sourceAnnotation"]["verification"]["existingMarks"]["added"] > 0


def test_compare_cli_emits_reference_annotation_provenance(tmp_path):
    bundle_path = ROOT / "captures/vmisc2-2026-09-17"
    source = ROOT / "fixtures/vmisc2.docx"
    model = wordmodel.build(source_binding.bind_source(capture.load_bundle(bundle_path), source))
    trace = tmp_path / "trace.json"
    trace.write_text(json.dumps({**model, "schema": "rsword-layout-trace/1", "unit": "pt"}))
    output = tmp_path / "comparison.json"
    assert cli.main(["compare", str(bundle_path), str(trace), "--source-docx", str(source), "--output", str(output)]) == 0
    result = json.loads(output.read_text())
    assert result["reference"]["sourceAnnotation"]["state"] == OK
    assert result["reference"]["sourceAnnotation"]["derived"] is True
