"""Audit archived Android PGIDX logs without inferring a print channel from width.

The legacy hook has no document, thread, or channel tags on PGIDX records. Even
a hash-bound sidecar cannot retrofit those tags, so strict comparison remains
undecidable. An explicit assumption can enable a conditional count comparison.
"""

from __future__ import annotations

from collections import Counter
import hashlib
import re

from . import FAIL, OK, UNDECIDABLE

_PAGE = re.compile(r"PGIDX n=(\d+) count=([0-9a-fA-F]+) index=([0-9a-fA-F]+)\s*")
_ENTER = re.compile(r"ENTER n=(\d+) tid=(\d+) w2=([0-9a-fA-F]+) w3=([0-9a-fA-F]+)(?:\s.*)?")
_SHA = re.compile(r"[0-9a-f]{64}")
_TRACE_MODE_SUFFIX = re.compile(
    r"；平台 (?:mac|android)，视图 (print|mobile)；行末标点 (?:按 w:overflowPunct 挂出|不挂出)$")
# word_analyse/tools/layout-probe/lineprobe.c records PGIDX only while n < 80.
_LEGACY_PGIDX_LIMIT = 80


def _widths(readings: list[dict]) -> list[dict]:
    return [{"widthTwips": width, "readings": count}
            for width, count in sorted(Counter(r["widthTwips"] for r in readings).items())]


def parse_page_log(raw: bytes) -> dict:
    """Keep every page-count/width observation, sequence, and terminal context.

Unrelated probe lines are retained in the hashed source, not interpreted here.
Neither the final count nor nearby ENTER widths prove a settled print layout.
    """
    issues = []
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        text = raw.decode("utf-8", errors="replace")
        issues.append({"reason": "INVALID_LOG_ENCODING"})
    if raw and not raw.endswith(b"\n"):
        issues.append({"reason": "TRUNCATED_OR_UNTERMINATED_LOG"})
    pages, enters, runs = [], [], []
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("PGIDX"):
            match = _PAGE.fullmatch(line)
            if not match:
                issues.append({"reason": "MALFORMED_PGIDX", "line": number})
                continue
            ordinal, count, index = int(match[1]), int(match[2], 16), int(match[3], 16)
            if (count == 0 and index != 0) or (count > 0 and index >= count):
                issues.append({"reason": "INVALID_PAGE_INDEX", "line": number})
            if pages and ordinal != pages[-1]["ordinal"] + 1:
                issues.append({"reason": "PGIDX_SEQUENCE_DISCONTINUITY", "line": number})
            elif not pages and ordinal != 0:
                issues.append({"reason": "PGIDX_PREFIX_MISSING", "line": number})
            record = {"line": number, "ordinal": ordinal, "count": count, "index": index}
            pages.append(record)
            if runs and runs[-1]["count"] == count:
                runs[-1]["readings"] += 1
                runs[-1]["lastLine"] = number
            else:
                runs.append({"count": count, "readings": 1,
                             "firstLine": number, "lastLine": number})
        elif line.startswith("ENTER"):
            match = _ENTER.fullmatch(line)
            if not match:
                issues.append({"reason": "MALFORMED_ENTER", "line": number})
                continue
            enters.append({"line": number, "ordinal": int(match[1]),
                           "tid": int(match[2]), "cpFirst": int(match[3], 16),
                           "widthTwips": int(match[4], 16)})
    limit_reached = bool(pages and pages[-1]["ordinal"] >= _LEGACY_PGIDX_LIMIT - 1)
    if limit_reached:
        issues.append({"reason": "PGIDX_SAMPLE_LIMIT_REACHED", "line": pages[-1]["line"]})
    last = pages[-1]["line"] if pages else None
    before = [r for r in enters if last is not None and r["line"] < last]
    after = [r for r in enters if last is not None and r["line"] > last]
    return {
        "schema": "rsword-android-page-log/1",
        "logSha256": hashlib.sha256(raw).hexdigest(),
        "pgidx": pages, "countRuns": runs, "finalCount": pages[-1]["count"] if pages else None,
        "enterReadings": enters, "widths": _widths(enters),
        "sampling": {"pgidxLimit": _LEGACY_PGIDX_LIMIT, "limitReached": limit_reached,
                     "source": "word_analyse/tools/layout-probe/lineprobe.c: PGIDX n < 80"},
        "lastPgidxContext": {"line": last, "precedingEnter": before[-1] if before else None,
                             "followingEnters": after, "followingWidths": _widths(after)},
        "issues": issues,
        "identity": {"document": "unrecorded", "pgidxThread": "unrecorded",
                     "pgidxChannel": "unrecorded", "completion": "unrecorded"},
    }


def trace_page_count(trace: dict, *, assume_legacy_print: bool = False) -> tuple[int | None, list[str], list[str]]:
    """Validate a supplied trace; this does not claim the engine was executed."""
    problems, assumptions = [], []
    if not isinstance(trace, dict) or trace.get("schema") != "rsword-layout-trace/1":
        return None, ["UNSUPPORTED_ENGINE_TRACE"], assumptions
    pages = trace.get("pages")
    if not isinstance(pages, list) or not pages or not all(isinstance(p, dict) for p in pages):
        return None, ["INVALID_ENGINE_PAGES"], assumptions
    count = trace.get("pageCount")
    if "pageCount" in trace and (type(count) is not int or count <= 0 or count != len(pages)):
        problems.append("ENGINE_PAGE_COUNT_MISMATCH_OR_INVALID")
    metrics = trace.get("metrics")
    metrics_mode = _TRACE_MODE_SUFFIX.search(metrics) if isinstance(metrics, str) else None
    # Older layout-trace records its mode only in this exact suffix. A known
    # mobile record contradicts the assumption even if another field says print.
    if metrics_mode and metrics_mode[1] == "mobile":
        problems.append("ENGINE_METRICS_MODE_INCOMPATIBLE")
    mode = trace.get("mode")
    if mode is None:
        if assume_legacy_print:
            assumptions.append("supplied engine trace was produced in print mode")
        else:
            problems.append("ENGINE_MODE_UNVERIFIED")
    elif mode != "print":
        problems.append("ENGINE_MODE_INCOMPATIBLE")
    return (len(pages) if not problems else None), problems, assumptions


def compare_page_count(audit: dict, engine_page_count: int | None, fixture_sha256: str,
                       binding: dict | None = None, *, assume_legacy_print: bool = False) -> dict:
    """Compare only page-container counts, subject to explicit legacy assumptions.

Optional binding: {schema: "android-page-binding/1", docSha256, logSha256,
mode: "print", modeSource: <nonempty provenance description>}.
Binding describes external provenance, not PGIDX per-record identity. Missing
legacy provenance can be assumed; a contradictory binding never can.
    """
    result = {"state": UNDECIDABLE, "scope": "final recorded page-container count only",
              "conditional": False, "assumptions": [], "fixtureSha256": fixture_sha256,
              "logSha256": audit.get("logSha256"), "modeSource": None,
              "wordPages": audit.get("finalCount"), "enginePages": engine_page_count}
    problems = [item["reason"] for item in audit.get("issues", [])]
    if audit.get("schema") != "rsword-android-page-log/1":
        problems.append("UNSUPPORTED_PAGE_AUDIT")
    if not isinstance(fixture_sha256, str) or not _SHA.fullmatch(fixture_sha256):
        problems.append("INVALID_FIXTURE_HASH")
    if binding is not None:
        if not isinstance(binding, dict) or binding.get("schema") != "android-page-binding/1":
            problems.append("INVALID_CAPTURE_BINDING")
        else:
            if binding.get("docSha256") != fixture_sha256:
                problems.append("FIXTURE_HASH_MISMATCH")
            if binding.get("logSha256") != audit.get("logSha256"):
                problems.append("LOG_HASH_MISMATCH")
            if binding.get("mode") != "print":
                problems.append("CAPTURE_MODE_INCOMPATIBLE")
            source = binding.get("modeSource")
            if not isinstance(source, str) or not source.strip():
                problems.append("CAPTURE_MODE_SOURCE_MISSING")
            else:
                result["modeSource"] = source
    if not audit.get("pgidx"):
        problems.append("NO_PGIDX_READINGS")
    count = audit.get("finalCount")
    if type(count) is not int or count <= 0:
        problems.append("NO_POSITIVE_FINAL_PAGE_COUNT")
    if type(engine_page_count) is not int or engine_page_count <= 0:
        problems.append("INVALID_OR_MISSING_ENGINE_PAGE_COUNT")
    if not assume_legacy_print:
        problems.append("PGIDX_DOCUMENT_THREAD_CHANNEL_AND_COMPLETION_UNVERIFIED")
    else:
        result["conditional"] = True
        result["modeSource"] = result["modeSource"] or "explicit --assume-legacy-print"
        result["assumptions"] = [
            "archived log belongs to the supplied fixture SHA-256 (filename/recipe binding)",
            "final PGIDX run belongs to the target document's print channel",
            "final PGIDX run is settled and capture completed without an unrecorded later relayout",
        ]
        context = audit.get("lastPgidxContext", {})
        recent = ([context["precedingEnter"]] if context.get("precedingEnter") else [])
        recent += context.get("followingEnters", [])
        # The archived device's mobile path used 5329. Treat a terminal occurrence
        # as contradictory to the legacy recipe, never as proof of any mode.
        if any(r["widthTwips"] == 5329 for r in recent):
            problems.append("TERMINAL_WIDTH_CONFLICTS_WITH_LEGACY_PRINT_RECIPE")
    if problems:
        return {**result, "reason": problems[0], "problems": list(dict.fromkeys(problems))}
    result["state"] = OK if count == engine_page_count else FAIL
    return result
