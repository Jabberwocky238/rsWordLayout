"""New captures retain both observations and refuse unstable or incomplete scans."""

import hashlib
import json
from pathlib import Path
import zipfile

import pytest

from wordmeasure import OK, UNDECIDABLE, capture, wordmodel
from wordmeasure.applescript import AppleScriptError


TEXT = "A\U0001f642\rB\r"
POSITIONS = [(offset, 1 if offset < 4 else 2, 1) for offset in range(6)]


def receipt(rows=POSITIONS, eoc=6):
    return str(eoc) + "\n---\n" + "\n".join(
        "%d,%d,%d" % row for row in rows
    ) + "\n"


def run_capture(tmp_path, monkeypatch, first, repeat, *, content=TEXT):
    docx = tmp_path / "source.docx"
    with zipfile.ZipFile(docx, "w") as archive:
        archive.writestr("word/document.xml", (
            '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
            '<w:body><w:p><w:r><w:t>A\U0001f642</w:t></w:r></w:p>'
            '<w:p><w:r><w:t>B</w:t></w:r></w:p></w:body></w:document>'
        ))
    original = docx.read_bytes()
    bundle = tmp_path / "new-capture"
    events = []
    replies = iter([first, repeat])

    def open_document(path):
        assert Path(path) == docx
        events.append("open")
        return "capture-owned"

    def tell_word(script, **kwargs):
        assert 'document "capture-owned"' in script
        if script.startswith("return content"):
            events.append("content")
            return content
        if "set n to count of paragraphs" in script:
            events.append("paragraphs")
            return "0,4\n4,6\n"
        assert "set eoc to end of content" in script
        events.append("sweep")
        result = next(replies)
        if isinstance(result, Exception):
            raise result
        return result

    def export_pdf(doc, path):
        assert doc == 'document "capture-owned"'
        events.append("export")
        Path(path).write_bytes(b"mock PDF geometry")

    def read_pdf(path):
        return {"sourceSha256": hashlib.sha256(Path(path).read_bytes()).hexdigest(),
                "pages": [{"width": 612, "height": 792, "glyphs": []}]}

    monkeypatch.setattr(capture, "open_document", open_document)
    monkeypatch.setattr(capture, "tell_word", tell_word)
    monkeypatch.setattr(capture, "export_pdf", export_pdf)
    monkeypatch.setattr(capture, "close_document", lambda _: events.append("close"))
    monkeypatch.setattr(capture.preflight, "preflight", lambda _: {"result": "PASS"})
    monkeypatch.setattr(capture.preflight, "font_substitution_check", lambda *a, **kw: {"result": "PASS"})
    monkeypatch.setattr(capture.fingerprint, "capture_environment", lambda **kw: {"platform": "mac"})
    monkeypatch.setattr(capture.pdfglyphs, "read_pdf", read_pdf)
    meta = capture.capture(docx, bundle, required_families=["Test"], label="explicit capture label",
                           slot=None, work_pdf=None)
    assert meta["label"] == "explicit capture label"
    assert docx.read_bytes() == original
    assert (bundle / "case.docx").read_bytes() == original
    assert json.loads((bundle / "META.json").read_text()) == meta
    return meta, bundle, events


def assert_refused(meta, bundle, reason):
    assert meta["usability"] == UNDECIDABLE
    assert meta["sweepStability"]["state"] == UNDECIDABLE
    assert any(reason in item for item in meta["usabilityReason"])
    model = wordmodel.build(capture.load_bundle(bundle))
    assert model["state"] == UNDECIDABLE
    assert model["reason"].startswith("BUNDLE_NOT_USABLE:")
    assert model["pages"] == []


def test_identical_complete_utf16_sweeps_preserve_both_receipts(tmp_path, monkeypatch):
    raw = receipt()
    meta, bundle, events = run_capture(tmp_path, monkeypatch, raw, raw)
    assert events == ["open", "content", "paragraphs", "export", "sweep", "sweep", "close"]
    stability = meta["sweepStability"]
    assert stability["state"] == OK
    assert meta.get("usability") != UNDECIDABLE
    assert stability["sourceUtf16Length"] == 6  # Five Unicode scalars, six Word units.
    assert stability["positionDifferences"] == {"count": 0, "first": None}
    first = json.loads((bundle / "sweep.json").read_text())
    repeat = json.loads((bundle / "sweep-repeat.json").read_text())
    assert first["positions"] == repeat["positions"]
    assert first["endOfContent"] == repeat["endOfContent"] == 6
    assert "contentText" not in repeat  # This text was not read again.
    assert first["contentText"] == TEXT
    for label in ["first", "repeat"]:
        info = stability["scans"][label]
        data = (bundle / info["rawReceipt"]["file"]).read_bytes()
        assert data == raw.encode("utf-8")
        assert hashlib.sha256(data).hexdigest() == info["rawReceipt"]["sha256"]
        assert info["completeUtf16Coverage"] is True
        assert info["observedPositions"] == 6


def test_changed_ordinals_with_unchanged_boundaries_do_not_replace_first_scan(tmp_path, monkeypatch):
    changed = [(offset, line + 2, page) for offset, line, page in POSITIONS]
    first, repeat = receipt(), receipt(changed)
    meta, bundle, events = run_capture(tmp_path, monkeypatch, first, repeat)
    assert_refused(meta, bundle, "POSITION_READINGS_CHANGED")
    assert events.count("sweep") == 2
    assert meta["sweepStability"]["positionDifferences"]["count"] == 6
    assert json.loads((bundle / "sweep.json").read_text())["positions"][0]["line"] == 1
    assert json.loads((bundle / "sweep-repeat.json").read_text())["positions"][0]["line"] == 3
    assert (bundle / "sweep-first.raw.txt").read_bytes() == first.encode()
    assert (bundle / "sweep-repeat.raw.txt").read_bytes() == repeat.encode()


def test_changed_end_of_content_is_not_admitted(tmp_path, monkeypatch):
    meta, bundle, _ = run_capture(tmp_path, monkeypatch, receipt(), receipt(POSITIONS + [(6, 2, 1)], 7))
    assert_refused(meta, bundle, "END_OF_CONTENT_CHANGED")
    assert json.loads((bundle / "sweep.json").read_text())["endOfContent"] == 6


@pytest.mark.parametrize("rows,eoc,reason", [
    (POSITIONS[:-1], 6, "INCOMPLETE_UTF16_COVERAGE"),
    ([POSITIONS[0], POSITIONS[0], *POSITIONS[2:]], 6, "INCOMPLETE_UTF16_COVERAGE"),
    ([*POSITIONS[:-1], (6, 2, 1)], 6, "INCOMPLETE_UTF16_COVERAGE"),
    (list(reversed(POSITIONS)), 6, "INCOMPLETE_UTF16_COVERAGE"),
    (POSITIONS[:-1], 5, "CONTENT_UTF16_LENGTH_MISMATCH"),
    ([(offset, 1 if offset < 2 else 2, page) for offset, _, page in POSITIONS], 6, "LINE_SPLITS_UNICODE_SCALAR"),
    ([(offset, 0, page) for offset, _, page in POSITIONS], 6, "NONPOSITIVE_PAGE_OR_LINE"),
    ([(offset, line, 0) for offset, line, _ in POSITIONS], 6, "NONPOSITIVE_PAGE_OR_LINE"),
])
def test_agreeing_but_invalid_scans_still_fail_closed(tmp_path, monkeypatch, rows, eoc, reason):
    raw = receipt(rows, eoc)
    meta, bundle, _ = run_capture(tmp_path, monkeypatch, raw, raw)
    assert_refused(meta, bundle, reason)
    assert meta["sweepStability"]["rawReceiptsEqual"] is True


@pytest.mark.parametrize("raw,reason", [
    ("", "SWEEP_SEPARATOR_MISSING"),
    ("invalid\n---\n0,1,1\n", "END_OF_CONTENT_INVALID"),
    ("6\n---\n0,1,1\n1,1\n", "POSITION_ROW_INVALID"),
    ("-1\n---\n", "END_OF_CONTENT_UNAVAILABLE_OR_NEGATIVE"),
])
def test_malformed_first_receipt_is_saved_and_cannot_be_replaced_by_a_valid_repeat(tmp_path, monkeypatch, raw, reason):
    meta, bundle, events = run_capture(tmp_path, monkeypatch, raw, receipt())
    assert_refused(meta, bundle, reason)
    assert events.count("sweep") == 2
    assert (bundle / "sweep-first.raw.txt").read_bytes() == raw.encode()


def test_even_raw_only_changes_remain_visible_and_are_not_silently_normalized(tmp_path, monkeypatch):
    meta, bundle, _ = run_capture(tmp_path, monkeypatch, receipt(), receipt() + "\n")
    assert_refused(meta, bundle, "RAW_RECEIPT_CHANGED")
    assert meta["sweepStability"]["positionDifferences"]["count"] == 0


@pytest.mark.parametrize("failed_pass", ["first", "repeat"])
def test_automation_failure_records_partial_reply_and_never_retries_to_pass(tmp_path, monkeypatch, failed_pass):
    error = AppleScriptError("APPLESCRIPT_TIMEOUT_WORD_UNCONFIRMED", "partial response", "timeout", -1)
    first, repeat = (error, receipt()) if failed_pass == "first" else (receipt(), error)
    meta, bundle, events = run_capture(tmp_path, monkeypatch, first, repeat)
    assert_refused(meta, bundle, "SWEEP_READ_FAILED")
    assert events.count("sweep") == (1 if failed_pass == "first" else 2)
    assert events[-1] == "close"
    assert (bundle / ("sweep-%s.raw.txt" % failed_pass)).read_bytes() == b"partial response"
