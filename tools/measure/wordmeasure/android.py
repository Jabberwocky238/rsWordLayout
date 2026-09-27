"""Compare Android Word line ranges without upgrading missing evidence to PASS.

Legacy narrow captures have unknown view/CP space. An explicit assumption may
enable a conditional boundary replay, but never geometry, pages, or run claims.
"""

from __future__ import annotations

from itertools import zip_longest
import json
from pathlib import Path

from . import FAIL, OK, UNDECIDABLE

OFFSET_SPACE = (
    "UTF-16 code units, same stream as rsword and Word Range.Start/End; "
    "one unit per paragraph mark"
)


def load_capture(path: Path) -> dict:
    records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()
               if line.strip()]
    if (not records or not all(isinstance(r, dict) for r in records)
            or records[0].get("record") != "meta"
            or sum(r.get("record") == "meta" for r in records) != 1
            or any(r.get("record") not in {"meta", "line", "page"} for r in records)):
        raise ValueError("INVALID_CAPTURE_RECORDS")
    return {"meta": records[0], "lines": [r for r in records if r["record"] == "line"]}


def _range(start, end) -> bool:
    return type(start) is int and type(end) is int and 0 <= start < end


def compare_capture(capture: dict, trace: dict, fixture_sha256: str,
                    assume_legacy_narrow: bool = False) -> dict:
    """Compare complete main-story ranges in order, including surplus lines.

The caller must generate the trace for the hashed fixture with Android/mobile
settings and the narrow width. Font identity and execution provenance belong in
the caller's report. Unknown CP units cannot be overridden by the legacy flag.
    """
    result = {
        "state": UNDECIDABLE,
        "scope": "main-story line boundaries in capture order; pages flattened",
        "conditional": False,
        "assumptions": [],
        "boundary": None,
        "runSplits": {"state": UNDECIDABLE, "reason": "NO_INDEPENDENT_ENGINE_RUN_RECORDS"},
        "geometry": {"state": UNDECIDABLE, "reason": "ANDROID_GEOMETRY_UNITS_NOT_ALIGNED"},
        "pagination": {"state": UNDECIDABLE, "reason": "MOBILE_VIEW_HAS_NO_PAPER_PAGES"},
    }
    problems = []
    meta, word = capture.get("meta"), capture.get("lines")
    if not isinstance(meta, dict) or not isinstance(word, list):
        return {**result, "reason": "INVALID_CAPTURE"}
    if meta.get("schema") != 1 or meta.get("producer") != "word":
        problems.append("UNSUPPORTED_WORD_SCHEMA")
    if not fixture_sha256 or meta.get("docSha256") != fixture_sha256:
        problems.append("FIXTURE_HASH_MISMATCH")
    if meta.get("cpUnit") != "utf16":
        problems.append("CP_UNIT_UNVERIFIED")
    for key, expected in (("cpSpace", "document"), ("mode", "mobile-consumption")):
        value = meta.get(key)
        if value == expected:
            continue
        if assume_legacy_narrow and value == "unknown":
            result["assumptions"].append(f"legacy narrow capture: {key}={expected}")
        else:
            problems.append(f"{key.upper()}_UNVERIFIED_OR_INCOMPATIBLE")
    result["conditional"] = bool(result["assumptions"])
    if trace.get("schema") != "rsword-layout-trace/1" or trace.get("offsetSpace") != OFFSET_SPACE:
        problems.append("ENGINE_CP_CONTRACT_UNVERIFIED")
    if not word:
        problems.append("NO_WORD_LINES")
    keys = []
    previous_end = 0
    for line in word:
        if not isinstance(line, dict):
            problems.append("INVALID_WORD_LINE")
            break
        start, end = line.get("cpFirst"), line.get("cpLim")
        if line.get("story") != "main":
            problems.append("UNSUPPORTED_WORD_STORY")
        para, ordinal = line.get("para"), line.get("line")
        if type(para) is not int or type(ordinal) is not int or min(para, ordinal) < 0:
            problems.append("INVALID_WORD_LINE_IDENTITY")
        else:
            keys.append((para, ordinal))
        if not _range(start, end):
            problems.append("WORD_CP_RANGE_UNMEASURED_OR_INVALID")
        elif start != previous_end:
            problems.append("WORD_CAPTURE_INCOMPLETE_OR_OVERLAPPING")
        previous_end = end
    if keys != sorted(set(keys)):
        problems.append("WORD_LINE_ORDER_OR_IDENTITY_INVALID")
    engine = []
    pages = trace.get("pages")
    if not isinstance(pages, list):
        problems.append("INVALID_ENGINE_PAGES")
    else:
        for page in pages:
            if not isinstance(page, dict) or not isinstance(page.get("lines"), list):
                problems.append("INVALID_ENGINE_LINES")
                break
            for line in page["lines"]:
                if not isinstance(line, dict) or not _range(line.get("sourceStart"), line.get("sourceEnd")):
                    problems.append("INVALID_ENGINE_CP_RANGE")
                    break
                engine.append((line["sourceStart"], line["sourceEnd"]))
    if problems:
        return {**result, "reason": problems[0], "problems": sorted(set(problems))}
    expected = [(line["cpFirst"], line["cpLim"]) for line in word]
    mismatches = [
        {"ordinal": i, "word": list(w) if w is not None else None,
         "engine": list(e) if e is not None else None}
        for i, (w, e) in enumerate(zip_longest(expected, engine)) if w != e
    ]
    compared = max(len(expected), len(engine))
    result["boundary"] = {
        "wordLines": len(expected), "engineLines": len(engine),
        "comparedLines": compared, "matchedLines": compared - len(mismatches),
        "missingEngineLines": max(0, len(expected) - len(engine)),
        "extraEngineLines": max(0, len(engine) - len(expected)),
        "mismatches": mismatches,
    }
    result["state"] = FAIL if mismatches else OK
    return result
