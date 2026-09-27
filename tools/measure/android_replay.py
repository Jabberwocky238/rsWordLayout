#!/usr/bin/env python3
"""Replay archived Android Word line ranges against a freshly run engine.

The source project is read only. Captures do not establish font identity, run
splits, geometry, or pagination; legacy narrow captures need an explicit opt-in.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys

from wordmeasure.android import compare_capture, load_capture


REPO = Path(__file__).resolve().parents[2]
SUFFIX = ".word.narrow.jsonl"
STATES = ("OK", "FAIL", "UNDECIDABLE")


def fingerprint(path: Path) -> dict:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return {"path": str(path), "sha256": digest.hexdigest()}


def checked_fingerprint(path: Path) -> dict:
    try:
        return fingerprint(path)
    except OSError as exc:
        return {"path": str(path), "error": str(exc)}


def fallback_spec(value: str) -> tuple[str, dict]:
    path, separator, face = value.rpartition("#")
    if not separator or not face.isdigit():
        path, face = value, ""
    resolved = Path(path).expanduser().resolve()
    record = checked_fingerprint(resolved)
    record["faceIndex"] = int(face) if face else None
    return str(resolved) + ("#" + face if face else ""), record


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + "\n",
                    encoding="utf-8")


def failed(reason: str) -> dict:
    return {"state": "UNDECIDABLE", "reasons": [reason], "boundary": None}


def replay_one(name: str, analysis: Path, output: Path, base_command: list[str],
               shared_inputs: list[dict], args: argparse.Namespace) -> dict:
    destination = output / name
    destination.mkdir()
    capture_path = analysis / "reports" / "diff" / (name + SUFFIX)
    fixture_path = analysis / "fixtures" / (name + ".docx")
    trace_path = destination / "engine.json"
    command = base_command + [str(fixture_path), str(trace_path)]
    inputs = {
        "capture": checked_fingerprint(capture_path),
        "fixture": checked_fingerprint(fixture_path),
    }
    result = {"fixture": name, "inputs": inputs, "command": command,
              "cwd": str(REPO), "timeoutSeconds": args.timeout,
              "assumeLegacyNarrow": args.assume_legacy_narrow}
    write_json(destination / "command.json", {"argv": command, "cwd": str(REPO),
                                             "removedEnvironment": ["RSWORD_FALLBACK_FONT"]})
    bad = [item for item in [*inputs.values(), *shared_inputs] if "error" in item]
    if bad:
        result["comparison"] = failed("Missing or unreadable input: " +
                                      "; ".join(item["path"] + ": " + item["error"] for item in bad))
    else:
        try:
            capture = load_capture(capture_path)
            if capture["meta"].get("docSha256") != inputs["fixture"]["sha256"]:
                raise ValueError("FIXTURE_HASH_MISMATCH; engine was not run")
            environment = os.environ.copy()
            environment.pop("RSWORD_FALLBACK_FONT", None)
            with (destination / "stdout.log").open("wb") as stdout, \
                    (destination / "stderr.log").open("wb") as stderr:
                process = subprocess.run(command, cwd=REPO, env=environment, stdout=stdout,
                                         stderr=stderr, timeout=args.timeout, check=False)
            result["returncode"] = process.returncode
            if process.returncode:
                result["comparison"] = failed("Engine exited with code %d; see stderr.log" %
                                              process.returncode)
            else:
                trace = json.loads(trace_path.read_text(encoding="utf-8"))
                if not isinstance(trace, dict):
                    raise ValueError("Engine trace must be a JSON object")
                result["trace"] = fingerprint(trace_path)
                result["comparison"] = compare_capture(
                    capture, trace, inputs["fixture"]["sha256"],
                    assume_legacy_narrow=args.assume_legacy_narrow)
                changed = [item["path"] for item in [*inputs.values(), *shared_inputs]
                           if checked_fingerprint(Path(item["path"])).get("sha256") != item["sha256"]]
                if changed:
                    result["comparison"] = failed("Input changed during replay: " + "; ".join(changed))
        except subprocess.TimeoutExpired:
            result["comparison"] = failed("Engine timed out after %g seconds; see command.json and logs" %
                                          args.timeout)
        except (OSError, ValueError, TypeError, KeyError) as exc:
            result["comparison"] = failed("Replay could not be evaluated: %s: %s" %
                                          (type(exc).__name__, exc))
    write_json(destination / "result.json", result)
    return result


def summarize(results: list[dict]) -> dict:
    counts = {state: 0 for state in STATES}
    tally = {"assessedFixtures": 0, "unassessedFixtures": 0, "wordLines": 0,
             "engineLines": 0, "matchedLines": 0, "denominator": 0}
    for result in results:
        comparison = result["comparison"]
        counts[comparison["state"]] += 1
        boundary = comparison.get("boundary")
        if not isinstance(boundary, dict) or boundary.get("state") == "UNDECIDABLE" or not all(
                isinstance(boundary.get(key), int)
                for key in ("wordLines", "engineLines", "matchedLines")):
            tally["unassessedFixtures"] += 1
            continue
        tally["assessedFixtures"] += 1
        for key in ("wordLines", "engineLines", "matchedLines"):
            tally[key] += boundary[key]
        tally["denominator"] += max(boundary["wordLines"], boundary["engineLines"])
    tally["fraction"] = (tally["matchedLines"] / tally["denominator"]
                         if tally["denominator"] else None)
    return {"counts": counts, "conditionalFixtures": sum(
        bool(result["comparison"].get("conditional")) for result in results), "boundary": tally}


def markdown(report: dict) -> str:
    lines = ["# Android offline line-boundary replay", "",
             "Only ordered UTF-16 source ranges are compared. Extra and missing lines count in the denominator.",
             "No run-split, geometry, font-identity, or page agreement is inferred.", "",
             "Legacy narrow assumptions: **%s**." %
             ("enabled; matches are conditional" if report["assumeLegacyNarrow"] else "disabled"), "",
             "| Fixture | State | Matching ranges / all line slots |", "| --- | --- | --- |"]
    for result in report["results"]:
        comparison = result["comparison"]
        boundary = comparison.get("boundary")
        ratio = "unassessed"
        if isinstance(boundary, dict) and boundary.get("state") != "UNDECIDABLE" and all(
                isinstance(boundary.get(key), int)
                for key in ("wordLines", "engineLines", "matchedLines")):
            ratio = "%d / %d" % (boundary["matchedLines"],
                                 max(boundary["wordLines"], boundary["engineLines"]))
        lines.append("| %s | %s | %s |" % (result["fixture"], comparison["state"], ratio))
    counts = report["summary"]["counts"]
    tally = report["summary"]["boundary"]
    lines += ["", "Fixtures: " + ", ".join("%s=%d" % (state, counts[state]) for state in STATES) + ".",
              "Conditional fixtures: %d." % report["summary"]["conditionalFixtures"],
              "Assessed ranges: %d / %d; unassessed fixtures: %d." %
              (tally["matchedLines"], tally["denominator"], tally["unassessedFixtures"]), "",
              "Each fixture directory contains the exact command, engine logs, fresh trace (when produced),",
              "input hashes, comparison details, and any failure reason. Shared binary and font hashes are in summary.json."]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--analysis-root", type=Path, default=REPO.parent / "word_analyse")
    parser.add_argument("--trace-bin", type=Path, default=REPO / "target/debug/layout-trace")
    parser.add_argument("--font", type=Path, action="append", required=True,
                        help="Explicit text font file; repeat as needed (Calibri is required)")
    parser.add_argument("--fallback-font", action="append",
                        help="Explicit fallback path[#face]; defaults to repository Droid")
    parser.add_argument("--output", type=Path, required=True, help="New directory, outside analysis root")
    parser.add_argument("--fixture", action="append", help="Fixture basename, without .docx; repeat as needed")
    parser.add_argument("--assume-legacy-narrow", action="store_true",
                        help="Conditionally assume legacy captures use main-story UTF-16 and mobile width 5329")
    parser.add_argument("--timeout", type=float, default=60, help="Per-fixture engine timeout in seconds")
    args = parser.parse_args(argv)
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be finite and positive")
    analysis = args.analysis_root.expanduser().resolve()
    output = args.output.expanduser().resolve()
    if output == analysis or analysis in output.parents:
        parser.error("--output must be outside the read-only analysis project")
    names = args.fixture or [path.name[:-len(SUFFIX)] for path in
                             sorted((analysis / "reports/diff").glob("*" + SUFFIX))]
    if not names:
        parser.error("No narrow captures found; specify --analysis-root or --fixture")
    if any(not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", name) for name in names):
        parser.error("--fixture must be a basename using letters, digits, underscore, dot, or hyphen")
    names = list(dict.fromkeys(names))
    try:
        output.mkdir(parents=True, exist_ok=False)
    except OSError as exc:
        parser.error("Cannot create new output directory: %s" % exc)
    binary = args.trace_bin.expanduser().resolve()
    fonts = [path.expanduser().resolve() for path in args.font]
    fallbacks = [fallback_spec(value) for value in
                 (args.fallback_font or [str(REPO / "fixtures/fonts/DroidSansFallbackFull.ttf")])]
    binary_record = checked_fingerprint(binary)
    font_records = [checked_fingerprint(path) for path in fonts]
    fallback_records = [record for _, record in fallbacks]
    shared_inputs = [binary_record, *font_records, *fallback_records]
    command = [str(binary), "--metrics", "real", "--vertical-grid", "none", "--platform", "android",
               "--view", "mobile", "--content-width", "5329", "--require", "Calibri"]
    for font in fonts:
        command += ["--font", str(font)]
    for spec, _ in fallbacks:
        command += ["--fallback-font", spec]
    report = {
        "schema": "rsword-layout-android-replay/1",
        "createdAt": datetime.now(timezone.utc).isoformat(),
        "analysisRoot": str(analysis), "assumeLegacyNarrow": args.assume_legacy_narrow,
        "engineBinary": binary_record, "fonts": font_records, "fallbackFonts": fallback_records,
        "scope": "Ordered UTF-16 source ranges only; no run, geometry, font-identity, or page agreement",
        "configuration": {"platform": "android", "view": "mobile", "contentWidthTwips": 5329,
                          "metrics": "real", "verticalGrid": "none", "requiredFamilies": ["Calibri"]},
        "results": [],
    }
    write_json(output / "manifest.json", {key: value for key, value in report.items() if key != "results"})
    for name in names:
        result = replay_one(name, analysis, output, command, shared_inputs, args)
        report["results"].append(result)
        print("%s: %s" % (name, result["comparison"]["state"]), flush=True)
    report["summary"] = summarize(report["results"])
    write_json(output / "summary.json", report)
    (output / "summary.md").write_text(markdown(report), encoding="utf-8")
    print("Report: %s" % (output / "summary.md"))
    if report["summary"]["counts"]["FAIL"]:
        return 1
    return 2 if report["summary"]["counts"]["UNDECIDABLE"] else 0


if __name__ == "__main__":
    sys.exit(main())
