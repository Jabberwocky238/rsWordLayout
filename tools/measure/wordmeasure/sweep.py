"""Reproducible offline capture sweep; comparison rules stay in the existing CLI."""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys

from . import FAIL, OK, UNDECIDABLE
from .fontcover import _face_offsets, _table_offsets

REPO = Path(__file__).resolve().parents[3]
FONT_EXTENSIONS = {".ttf", ".otf", ".ttc", ".otc"}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n")


def font_names(path: Path) -> set[str]:
    """Read declared family, full and PostScript names from every SFNT/TTC face.

    Matching is exact except for case; filenames and substring aliases are never
    evidence. Unsupported encodings stay unread rather than being guessed.
    """
    data = path.read_bytes()
    if data[:4] not in (b"\x00\x01\x00\x00", b"OTTO", b"true", b"ttcf"):
        raise ValueError("UNSUPPORTED_FONT_CONTAINER")
    result = set()
    for face in _face_offsets(data):
        base = _table_offsets(data, face).get("name")
        if base is None:
            continue
        count, storage = struct.unpack_from(">HH", data, base + 2)
        for i in range(count):
            platform, encoding, _, name_id, length, offset = struct.unpack_from(
                ">HHHHHH", data, base + 6 + i * 12
            )
            if name_id not in (1, 4, 6, 16, 18, 21):
                continue
            start = base + storage + offset
            if start + length > len(data):
                raise ValueError("TRUNCATED_FONT_NAME")
            codec = "utf-16-be" if platform in (0, 3) else (
                "mac_roman" if platform == 1 and encoding == 0 else None
            )
            if codec:
                result.add(data[start : start + length].decode(codec))
    return result


def font_inventory(roots: list[Path]) -> tuple[list[dict], list[dict]]:
    records, errors = [], []
    paths = sorted({p.resolve() for root in roots for p in root.rglob("*")
                    if p.is_file() and p.suffix.lower() in FONT_EXTENSIONS})
    for path in paths:
        try:
            records.append({"path": str(path), "sha256": digest(path),
                            "names": sorted(font_names(path))})
        except (OSError, ValueError, struct.error) as exc:
            errors.append({"path": str(path), "reason": str(exc)})
    return records, errors


def fixture_index(root: Path) -> dict[str, list[Path]]:
    found: dict[str, list[Path]] = {}
    for path in sorted(root.rglob("*.docx")):
        found.setdefault(digest(path), []).append(path.resolve())
    return found


def bind_inputs(meta: dict, fixtures: dict[str, list[Path]], fonts: list[dict]) -> dict:
    fixture = meta.get("fixture", {})
    before, after = fixture.get("before", {}), fixture.get("after", {})
    sha = before.get("sha256")
    if not sha or fixture.get("unchanged") is not True or after.get("sha256") != sha:
        return {"state": UNDECIDABLE, "reason": "FIXTURE_IDENTITY_UNVERIFIED"}
    candidates = fixtures.get(sha, [])
    if not candidates:
        return {"state": UNDECIDABLE, "reason": "FIXTURE_HASH_NOT_FOUND", "sha256": sha}
    required = meta.get("preflight", {}).get("requiredFamilies")
    if not isinstance(required, dict) or not required or any(v is not True for v in required.values()):
        return {"state": UNDECIDABLE, "reason": "FONT_PREFLIGHT_UNVERIFIED"}
    if meta.get("fontSubstitution", {}).get("result") != "PASS":
        return {"state": UNDECIDABLE, "reason": "CAPTURE_FONT_SUBSTITUTION_UNVERIFIED"}
    if meta.get("environment", {}).get("platform") != "mac":
        return {"state": UNDECIDABLE, "reason": "CAPTURE_PLATFORM_NOT_MAC"}
    matches = {name: [r for r in fonts if name.casefold() in {n.casefold() for n in r["names"]}]
               for name in sorted(required)}
    missing = [name for name, values in matches.items() if not values]
    if missing:
        return {"state": UNDECIDABLE, "reason": "REQUIRED_FONT_MISSING", "missingFamilies": missing}
    optional = meta.get("fontSubstitution", {}).get("optionalFamilies", [])
    wanted = {name.casefold() for name in list(required) + optional}
    selected = [r for r in fonts if wanted.intersection(n.casefold() for n in r["names"])]
    return {"state": OK, "fixture": str(candidates[0]), "fixtureSha256": sha,
            "equivalentFixturePaths": [str(p) for p in candidates],
            "requiredFamilies": sorted(required), "optionalFamilies": optional,
            "fonts": selected,
            "fontIdentityLimit": "Declared names and current file hashes are checked; capture META does not pin individual font bytes."}


def invoke(command: list[str], output: Path, *, timeout: float) -> dict:
    try:
        run = subprocess.run(command, cwd=REPO / "tools/measure", capture_output=True,
                             text=True, timeout=timeout)
        output.with_suffix(".stdout.txt").write_text(run.stdout)
        output.with_suffix(".stderr.txt").write_text(run.stderr)
        return {"command": command, "exitCode": run.returncode}
    except subprocess.TimeoutExpired as exc:
        output.with_suffix(".stdout.txt").write_bytes(exc.stdout or b"")
        output.with_suffix(".stderr.txt").write_bytes(exc.stderr or b"")
        return {"command": command, "exitCode": None, "reason": "TIMEOUT"}


def run_bundle(bundle: Path, out: Path, *, binary: Path, fixtures: dict,
               fonts: list[dict], timeout: float) -> dict:
    out.mkdir()
    record = {"bundle": str(bundle), "state": UNDECIDABLE, "output": str(out)}
    try:
        meta = json.loads((bundle / "META.json").read_text())
        record["metaSha256"] = digest(bundle / "META.json")
        verdict = bundle / "VERDICT.json"
        if verdict.is_file() and json.loads(verdict.read_text()).get("verdict") == "VOID":
            return {**record, "reason": "CAPTURE_VOID"}
        missing = [name for name in ("sweep.json", "glyphs.json") if not (bundle / name).is_file()]
        if missing:
            return {**record, "reason": "CAPTURE_FILES_MISSING", "missingFiles": missing}
        record["captureHashes"] = {name: digest(bundle / name) for name in ("sweep.json", "glyphs.json")}
        binding = bind_inputs(meta, fixtures, fonts)
        record["binding"] = binding
        if binding["state"] != OK:
            return {**record, "reason": binding["reason"]}
        # Recheck at use time so a stale inventory cannot silently bind changed bytes.
        if digest(Path(binding["fixture"])) != binding["fixtureSha256"]:
            return {**record, "reason": "FIXTURE_CHANGED_DURING_SWEEP"}
        for font in binding["fonts"]:
            if digest(Path(font["path"])) != font["sha256"]:
                return {**record, "reason": "FONT_CHANGED_DURING_SWEEP", "font": font["path"]}
        trace = out / "trace.json"
        command = [str(binary), "--metrics", "real", "--vertical-grid", "mac"]
        for font in binding["fonts"]:
            command.extend(["--font", font["path"]])
        for family in binding["requiredFamilies"]:
            command.extend(["--require", family])
        command.extend([binding["fixture"], str(trace)])
        record["traceRun"] = invoke(command, out / "trace", timeout=timeout)
        if record["traceRun"]["exitCode"] != 0 or not trace.is_file():
            return {**record, "reason": "TRACE_FAILED"}
        record["traceSha256"] = digest(trace)
        for stage, args in (("selfcheck", [str(trace)]),
                            ("compare", [str(bundle), str(trace), "--source-docx", binding["fixture"]])):
            name = "comparison" if stage == "compare" else stage
            result_path = out / (name + ".json")
            command = [sys.executable, "-m", "wordmeasure.cli", stage, *args, "--output", str(result_path)]
            record[name + "Run"] = invoke(command, out / name, timeout=timeout)
            if not result_path.is_file() or record[name + "Run"]["exitCode"] not in (0, 1, 2):
                return {**record, "reason": name.upper() + "_FAILED"}
            result = json.loads(result_path.read_text())
            if stage == "compare":
                annotation = result.get("reference", {}).get("sourceAnnotation", {})
                record["sourceAnnotation"] = annotation
                if annotation.get("state") != OK:
                    return {**record, "reason": "SOURCE_ANNOTATION_UNVERIFIED"}
            record[name + "State"] = result["state"]
            if stage == "compare":
                comparison = result["comparison"]
                record["structure"] = comparison["structure"]
                record["structurallySound"] = comparison["structurallySound"]
                record["failureCodes"] = dict(Counter(f["code"] for f in comparison["failures"]))
                record["falsifierHits"] = result["falsifierHits"]
                record["pairs"] = comparison["pairs"]
                record["maxAbs"] = comparison["maxAbs"]
        states = (record["selfcheckState"], record["comparisonState"])
        record["state"] = FAIL if FAIL in states else (UNDECIDABLE if UNDECIDABLE in states else OK)
        if record["selfcheckState"] != OK:
            record["reason"] = "TRACE_SELFCHECK_" + record["selfcheckState"]
        return record
    except (OSError, ValueError, KeyError, TypeError) as exc:
        return {**record, "state": UNDECIDABLE, "reason": "INPUT_OR_EXECUTION_ERROR", "detail": str(exc)}


def summarize(rows: list[dict]) -> dict:
    states = Counter(r["state"] for r in rows)
    comparisons = [r for r in rows if "comparisonState" in r]
    return {"bundles": len(rows), "states": {s: states[s] for s in (OK, FAIL, UNDECIDABLE)},
            "compared": len(comparisons), "notCompared": len(rows) - len(comparisons),
            "selfcheckStates": dict(Counter(r.get("selfcheckState", "NOT_RUN") for r in rows)),
            "comparisonStates": dict(Counter(r.get("comparisonState", "NOT_RUN") for r in rows)),
            "structurallySound": sum(r.get("structurallySound") is True for r in comparisons),
            "pageCountsEqual": sum(r["structure"]["referencePages"] == r["structure"]["candidatePages"]
                                   for r in comparisons)}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace-bin", required=True, type=Path)
    parser.add_argument("--font-dir", action="append", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New output directory; never overwrites a run")
    parser.add_argument("--bundle", action="append", type=Path, help="Capture directory; repeat to select a subset")
    parser.add_argument("--timeout", type=float, default=120, help="Per command timeout in seconds")
    args = parser.parse_args(argv)
    binary = args.trace_bin.expanduser().resolve()
    roots = [p.expanduser().resolve() for p in args.font_dir]
    if not binary.is_file() or any(not p.is_dir() for p in roots):
        parser.error("--trace-bin must be a file and every --font-dir must be a directory")
    bundles = sorted({p.expanduser().resolve() for p in args.bundle}) if args.bundle else sorted(
        p.parent for p in (REPO / "captures").glob("*/META.json")
    )
    if not bundles:
        parser.error("No capture bundles selected")
    if len({p.name for p in bundles}) != len(bundles):
        parser.error("Selected bundles must have distinct directory names")
    out = args.output.expanduser().resolve()
    out.mkdir(parents=True, exist_ok=False)
    # Cargo may replace the source executable while a sweep is running. All
    # bundles in this run must use the same bytes, including their provenance.
    snapshot = out / ("layout-trace.exe" if binary.suffix == ".exe" else "layout-trace")
    shutil.copy2(binary, snapshot)
    fonts, font_errors = font_inventory(roots)
    fixtures = fixture_index(REPO / "fixtures")
    provenance = {"schema": "rsword-layout-offline-sweep/1", "traceBinary": str(binary),
                  "traceBinarySnapshot": str(snapshot),
                  "traceBinarySha256": digest(snapshot), "fontRoots": [str(p) for p in roots],
                  "fontReadErrors": font_errors, "python": sys.version,
                  "measurementHashes": {str(p.relative_to(REPO)): digest(p)
                                        for p in sorted((REPO / "tools/measure/wordmeasure").glob("*.py"))},
                  "criteria": {"tolerancePt": 0, "excludedRules": [], "metrics": "real", "verticalGrid": "mac",
                               "sourceAnnotations": "SHA-bound DOCX, verified UTF-16 ranges and captured text; offline backtest"}}
    dump(out / "provenance.json", provenance)
    rows = []
    for bundle in bundles:
        record = run_bundle(bundle, out / bundle.name, binary=snapshot, fixtures=fixtures,
                            fonts=fonts, timeout=args.timeout)
        dump(out / bundle.name / "result.json", record)
        rows.append(record)
        print("%s: %s%s" % (bundle.name, record["state"], " (" + record["reason"] + ")" if record.get("reason") else ""), flush=True)
    summary = {"schema": provenance["schema"], "coverage": summarize(rows), "results": rows}
    dump(out / "summary.json", summary)
    lines = ["# Offline engine sweep", "", "Comparison tolerance: 0 pt; no rule exclusions.", "",
             "Coverage: `" + json.dumps(summary["coverage"], ensure_ascii=False) + "`", "",
             "| Bundle | State | Selfcheck | Compare | Reason |", "| --- | --- | --- | --- | --- |"]
    for row in rows:
        lines.append("| %s | %s | %s | %s | %s |" % (
            row["bundle"].rsplit("/", 1)[-1], row["state"], row.get("selfcheckState", "NOT_RUN"),
            row.get("comparisonState", "NOT_RUN"), row.get("reason", "")))
    (out / "summary.md").write_text("\n".join(lines) + "\n")
    print(json.dumps(summary["coverage"], ensure_ascii=False))
    return 1 if any(r["state"] == FAIL for r in rows) else (2 if any(r["state"] == UNDECIDABLE for r in rows) else 0)
