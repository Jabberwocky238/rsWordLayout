"""Bind offline DOCX annotations to an unchanged, independently captured source."""

from __future__ import annotations

import copy
import hashlib
import json
import tempfile
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

from . import OK, docxtext
from .source_text import SourceText, utf16_length


def _json_hash(value: dict) -> str:
    # In-memory callers may use integer mark offsets; JSON object keys are text.
    value = json.loads(json.dumps(value, ensure_ascii=True, allow_nan=False))
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"),
                         ensure_ascii=True, allow_nan=False).encode("ascii")
    return hashlib.sha256(encoded).hexdigest()


def bind_source(bundle: dict, source_docx: Path) -> dict:
    """Return an annotated copy; never edit a capture or infer marks from LF.

    The requested DOCX must match both capture hashes, all captured paragraph
    ranges, and the complete captured source. Existing marks must also agree.
    Invalid or insufficient evidence raises ValueError, which callers must
    expose as UNDECIDABLE rather than falling back to unverified annotation.
    """
    try:
        return _bind_source(bundle, Path(source_docx))
    except (OSError, zipfile.BadZipFile, ET.ParseError, KeyError, TypeError, AttributeError) as exc:
        raise ValueError("SOURCE_BINDING_INVALID_INPUT: %s" % exc) from exc


def _bind_source(bundle: dict, source_docx: Path) -> dict:
    meta, sweep = bundle["META"], bundle["sweep"]
    fixture = meta.get("fixture") or {}
    before = (fixture.get("before") or {}).get("sha256")
    after = (fixture.get("after") or {}).get("sha256")
    if fixture.get("unchanged") is not True or not before or before != after:
        raise ValueError("FIXTURE_IDENTITY_UNVERIFIED: META must confirm unchanged before/after SHA256")
    snapshot = source_docx.read_bytes()
    digest = hashlib.sha256(snapshot).hexdigest()
    if digest != before:
        raise ValueError("SOURCE_DOCX_HASH_MISMATCH: input SHA256 %s != captured %s" % (digest, before))

    # Hash and derive from the same bytes even if the original path later changes.
    with tempfile.TemporaryDirectory(prefix="wordmeasure-source-") as directory:
        snapshot_path = Path(directory) / "source.docx"
        snapshot_path.write_bytes(snapshot)
        derived = docxtext.content_text(snapshot_path)

    verification = docxtext.verify(
        derived, end_of_content=sweep.get("endOfContent"),
        paragraphs=sweep.get("paragraphs"),
    )
    if verification["state"] != OK:
        raise ValueError("SOURCE_RANGE_VERIFICATION_FAILED: %s" % "; ".join(verification["reasons"]))

    captured = SourceText(sweep["contentText"])
    source = SourceText(derived["text"])
    if captured.length != source.length or captured.length != sweep["endOfContent"]:
        raise ValueError("CONTENT_TEXT_LENGTH_MISMATCH: captured text and verified UTF-16 extent differ")
    if len(source.text) != len(captured.text):
        raise ValueError("CONTENT_TEXT_MISMATCH: Unicode scalar counts differ")
    is_mac = sweep.get("platform", meta.get("environment", {}).get("platform")) == "mac"
    normalized_marks = 0
    offset = 0
    for expected, actual in zip(source.text, captured.text):
        if expected != actual:
            if (is_mac and expected == "\r" and actual == "\n"
                    and derived["marks"].get(offset) == docxtext.MARK_PARAGRAPH):
                normalized_marks += 1
            else:
                raise ValueError("CONTENT_TEXT_MISMATCH: UTF-16 offset %d: source %r, capture %r"
                                 % (offset, expected, actual))
        offset += utf16_length(expected)

    existing = sweep.get("marks")
    if existing is None:
        existing = {}
    if not isinstance(existing, dict):
        raise ValueError("SOURCE_MARKS_INVALID: existing marks must be a mapping")
    checked = {}
    for key, mark in existing.items():
        if isinstance(key, bool) or not isinstance(key, (int, str)):
            raise ValueError("SOURCE_MARK_OFFSET_INVALID: %r" % (key,))
        try:
            at = int(key)
        except ValueError:
            raise ValueError("SOURCE_MARK_OFFSET_INVALID: %r" % (key,)) from None
        if str(at) != str(key):
            raise ValueError("SOURCE_MARK_OFFSET_INVALID: %r" % (key,))
        source.index(at)
        if at not in derived["marks"] or derived["marks"][at] != mark:
            raise ValueError("SOURCE_MARK_CONFLICT: UTF-16 offset %d: existing %r, derived %r"
                             % (at, mark, derived["marks"].get(at)))
        checked[at] = mark

    out = copy.deepcopy(bundle)
    marks = copy.deepcopy(existing)
    for at, mark in derived["marks"].items():
        if at not in checked:
            marks[str(at)] = mark
    out["sweep"]["marks"] = marks
    out["sourceAnnotation"] = {
        "schema": "rsword-layout-source-annotation/1",
        "state": OK,
        "method": "VERIFIED_DOCX_SOURCE_MARKS",
        "derived": True,
        "backtest": True,
        "sourceDocx": {"path": str(source_docx.resolve()), "sha256": digest, "bytes": len(snapshot)},
        "captureInputs": {
            "bundle": bundle.get("path"),
            "metaSha256": _json_hash(meta),
            "sweepSha256": _json_hash(sweep),
            "hashEncoding": "SHA256 of canonical ASCII JSON, sorted keys, compact separators",
        },
        "verification": {
            "fixture": {"state": OK, "beforeSha256": before, "afterSha256": after, "unchanged": True},
            "sourceRanges": verification,
            "contentText": {"state": OK, "utf16Length": captured.length,
                            "paragraphCrToMacLf": normalized_marks,
                            "rule": "Exact text, except source-annotated paragraph CR may be captured as Mac LF"},
            "existingMarks": {"state": OK, "preserved": len(checked),
                              "added": len(derived["marks"]) - len(checked)},
        },
        "note": "Offline source annotations, not a new Word observation or independent validation; capture bytes unchanged.",
    }
    return out
