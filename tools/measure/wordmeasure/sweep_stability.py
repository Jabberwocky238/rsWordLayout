"""Fail closed unless two complete Word sweeps preserve every source reading."""

from __future__ import annotations

from itertools import zip_longest

from . import OK, UNDECIDABLE
from .source_text import InvalidSourceOffset, SourceText


def parse_receipt(raw: str) -> dict:
    """Keep valid rows for diagnosis; malformed receipts never become usable."""
    head, separator, body = raw.partition("\n---\n")
    result = {"endOfContent": None, "positions": [], "parseErrors": []}
    if not separator:
        result["parseErrors"].append("SWEEP_SEPARATOR_MISSING")
        return result
    try:
        result["endOfContent"] = int(head.strip())
    except ValueError:
        result["parseErrors"].append("END_OF_CONTENT_INVALID")
    for number, line in enumerate(body.splitlines(), 1):
        if not line.strip():
            continue
        try:
            offset, ordinal, page = (int(value) for value in line.split(","))
        except ValueError:
            result["parseErrors"].append("POSITION_ROW_INVALID: row %d" % number)
            continue
        result["positions"].append({"offset": offset, "line": ordinal, "page": page})
    return result


def _validate(scan: dict, source: SourceText | None) -> dict:
    reasons = list(scan["parseErrors"])
    if scan.get("readError"):
        reasons.append("SWEEP_READ_FAILED: %s" % scan["readError"])
    eoc, positions = scan["endOfContent"], scan["positions"]
    if eoc is None or eoc < 0:
        reasons.append("END_OF_CONTENT_UNAVAILABLE_OR_NEGATIVE")
    elif source is not None and eoc != source.length:
        reasons.append("CONTENT_UTF16_LENGTH_MISMATCH: endOfContent=%d, text=%d" % (eoc, source.length))
    complete = (eoc is not None and eoc >= 0 and len(positions) == eoc
                and all(record["offset"] == index for index, record in enumerate(positions)))
    if not complete:
        reasons.append("INCOMPLETE_UTF16_COVERAGE: require every unit exactly once in source order")
    if any(record["page"] <= 0 or record["line"] <= 0 for record in positions):
        reasons.append("NONPOSITIVE_PAGE_OR_LINE_ORDINAL")
    if source is not None and complete and eoc == source.length:
        previous = None
        try:
            for record in positions:
                key = (record["page"], record["line"])
                if key != previous:
                    source.index(record["offset"])
                previous = key
        except InvalidSourceOffset as exc:
            reasons.append("LINE_SPLITS_UNICODE_SCALAR: %s" % exc)
    return {"state": UNDECIDABLE if reasons else OK, "reasons": reasons,
            "endOfContent": eoc, "observedPositions": len(positions),
            "completeUtf16Coverage": complete}


def verify(first: dict, repeat: dict, content_text: str) -> dict:
    """Equality is necessary evidence of stability, not proof of line accuracy."""
    reasons = []
    try:
        source = SourceText(content_text)
    except InvalidSourceOffset as exc:
        source = None
        reasons.append("INVALID_CAPTURED_SOURCE_TEXT: %s" % exc)
    scans = {"first": _validate(first, source), "repeat": _validate(repeat, source)}
    for label, scan in scans.items():
        reasons.extend("%s_SWEEP_INVALID: %s" % (label.upper(), reason) for reason in scan["reasons"])
    eoc_equal = first["endOfContent"] == repeat["endOfContent"]
    if not eoc_equal:
        reasons.append("END_OF_CONTENT_CHANGED: %s -> %s" % (first["endOfContent"], repeat["endOfContent"]))
    count = 0
    first_difference = None
    for before, after in zip_longest(first["positions"], repeat["positions"]):
        if before != after:
            count += 1
            if first_difference is None:
                first_difference = {"first": before, "repeat": after}
    if count:
        reasons.append("POSITION_READINGS_CHANGED: %d row(s)" % count)
    raw_equal = first["rawReceipt"]["sha256"] == repeat["rawReceipt"]["sha256"]
    if not raw_equal:
        reasons.append("RAW_RECEIPT_CHANGED")
    return {
        "schema": "rsword-layout-sweep-stability/1",
        "state": UNDECIDABLE if reasons else OK,
        "reasons": reasons,
        "method": "Two consecutive scans of the same open document after one PDF export; no retry or replacement",
        "sourceUtf16Length": source.length if source is not None else None,
        "scans": {label: {**scans[label], "rawReceipt": scan["rawReceipt"]}
                  for label, scan in (("first", first), ("repeat", repeat))},
        "endOfContentEqual": eoc_equal,
        "rawReceiptsEqual": raw_equal,
        "positionDifferences": {"count": count, "first": first_difference},
        "note": "Agreement does not independently establish line identity, PDF alignment, or Word layout quiescence.",
    }
