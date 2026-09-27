#!/usr/bin/env python3
"""Acceptance registry and panel (roadmap R01).

A case is a document plus a configuration. The registry is versioned and
committed; the panel only reads existing replay outputs and never runs Word.

- ``register`` writes the case registry from the capture bundles, the Android
  narrow captures and a declared list of word_analyse report-level constraints.
- ``run-reports`` runs the engine on the report-only cases (no Word).
- ``panel`` combines a Mac sweep, an Android replay and a report run into one
  panel: strict whole-document passes per family and configuration, plus
  capability, evidence and stability guardrails and a layered failure summary.

Strict pass requires a strict reference, a valid binding, full support, an OK
comparison and an OK self-check. Conditional and report-level results are
counted separately and never enter the strict numerator.
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import zipfile

REPO = Path(__file__).resolve().parents[2]
REGISTRY_SCHEMA = "rsword-acceptance-cases/1"
PANEL_SCHEMA = "rsword-acceptance-panel/1"
FAMILIES = ("line-breaking", "vertical-pagination", "sections-columns", "tables",
            "headers-numbering", "objects", "footnotes")
STRICT_STATES = ("PASS", "FAIL", "UNDECIDABLE", "NOT_RUN")

# Primary families from each preregistration's stated question
# (docs/PREREG-*.md, section 0). Tags may overlap.
MAC_FAMILIES = {
    "a-form": ["vertical-pagination"],
    "accumulation": ["line-breaking"],
    "baseline-order": ["vertical-pagination"],
    "breaks-sections": ["sections-columns", "vertical-pagination"],
    "cjk-plain": ["line-breaking"],
    "cursor-unit": ["line-breaking"],
    "exact-spacing": ["vertical-pagination"],
    "first-line": ["vertical-pagination"],
    "fixed-distance": ["vertical-pagination"],
    "font-free": ["vertical-pagination"],
    "hbox": ["line-breaking"],
    "hbox2": ["line-breaking"],
    "indent-align": ["line-breaking"],
    "kinsoku": ["line-breaking"],
    "kinsoku2": ["line-breaking"],
    "noise-floor": ["vertical-pagination"],
    "page-start": ["vertical-pagination"],
    "probe-metrics": ["vertical-pagination"],
    "recursive": ["vertical-pagination"],
    "rounding": ["vertical-pagination"],
    "shift-floor": ["vertical-pagination"],
    "size-free": ["vertical-pagination"],
    "vertical-precision": ["vertical-pagination"],
    "vmisc": ["vertical-pagination"],
    "vmisc2": ["vertical-pagination"],
    "vmisc3": ["vertical-pagination"],
}

ANDROID_FAMILIES = ["line-breaking"]

# word_analyse report-level constraints (Android Word print view). Their raw
# page/line logs are missing or strictly undecidable, so they are report-only.
REPORT_CASES = [
    ("table32-tail", ["tables"], {"pages": 1},
     "reports/diff/*; docs/STRUCTURED-FLOW-EVIDENCE-2026-09-27.md"),
    ("table32", ["tables"], {"pages": 2}, "reports/rsword-diff/table.md"),
    ("table32-noborder", ["tables"], {"pages": 2}, "findings/question-set.md Q9"),
    ("table32-rowh", ["tables"], {"pages": 2}, "findings/question-set.md Q9"),
    ("footnote32", ["footnotes"], {"pages": 2}, "reports/rsword-diff/footnote.md"),
    ("twocol64", ["sections-columns"], {"pages": 1}, "reports/rsword-diff/columns.md"),
    ("widow-split", ["vertical-pagination"], {"secondPageStart": 1933}, "reports/rsword-diff/widow.md"),
    ("widow-on", ["vertical-pagination"], {"secondPageStart": 1834}, "reports/rsword-diff/widow.md"),
    ("keep-next", ["vertical-pagination"], {"secondPageStart": 1054}, "reports/rsword-diff/keep-next.md"),
    ("keep-lines", ["vertical-pagination"], {"secondPageStart": 1054}, "reports/rsword-diff/keep-lines.md"),
] + [
    ("dg-decide-%d-n%d" % (pitch, n), ["vertical-pagination"], {"pages": pages},
     "reports/diff/docgrid-decided.md; strict PGIDX audit undecidable (docs/ANDROID-PAGE-EVIDENCE-2026-09-27.md)")
    for pitch, counts in ((139, (36, 37, 38)), (188, (40, 41, 42)), (220, (34, 35, 36)))
    for n, pages in zip(counts, (1, 1, 2))
]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rel(path: Path) -> str:
    path = path.resolve()
    for root, prefix in ((REPO, ""), (REPO.parent / "word_analyse", "../word_analyse/")):
        try:
            return prefix + str(path.relative_to(root))
        except ValueError:
            pass
    return str(path)


def resolve(path: str) -> Path:
    return (REPO / path).resolve()


def load(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def bundle_group(name: str) -> str:
    stem = name[:-len("-void")] if name.endswith("-void") else name
    return stem.rsplit("-", 3)[0]


# ---------------------------------------------------------------- register

def mac_cases(captures: Path, fixtures: Path, baseline: Path | None) -> list[dict]:
    index: dict[str, list[Path]] = {}
    for path in sorted(fixtures.rglob("*.docx")):
        index.setdefault(digest(path), []).append(path)
    cases = []
    for bundle in sorted(p for p in captures.iterdir() if (p / "META.json").is_file()):
        meta = load(bundle / "META.json") or {}
        group = bundle_group(bundle.name)
        sha = meta.get("fixture", {}).get("before", {}).get("sha256")
        documents = index.get(sha, [])
        word = meta.get("environment", {}).get("word", {})
        case = {
            "id": "mac-print/" + bundle.name,
            "group": group,
            "families": MAC_FAMILIES.get(group, []),
            "config": {"id": "mac-word-%s-print" % word.get("version", "unknown"),
                       "platform": "mac", "view": "print", "wordVersion": word.get("version"),
                       "wordBuild": word.get("build"),
                       "fontEnvironmentSha256": meta.get("environment", {}).get("fontEpoch", {}).get("filesSha256"),
                       "requiredFamilies": sorted(meta.get("preflight", {}).get("requiredFamilies", {}) or {}),
                       "coordinateDomain": "PDF glyph origins in points; main-story UTF-16"},
            "document": {"path": rel(documents[0]) if documents else None, "sha256": sha},
            "reference": {"kind": "mac-pdf-glyph-bundle", "path": rel(bundle),
                          "metaSha256": digest(bundle / "META.json")},
            "comparator": "wordmeasure compare (glyph origins, 0pt tolerance) + wordmeasure selfcheck",
            "evidence": "strict",
        }
        verdict = load(bundle / "VERDICT.json") or {}
        if verdict.get("verdict") == "VOID":
            case["evidence"] = "reference-invalid"
            case["evidenceReason"] = "CAPTURE_VOID"
        elif not documents:
            case["evidence"] = "binding-missing"
            case["evidenceReason"] = "FIXTURE_HASH_NOT_FOUND"
        cases.append(case)
    if baseline:
        for case in cases:
            result = load(baseline / case["id"].split("/", 1)[1] / "result.json")
            if result:
                case["baseline"] = {"source": rel(baseline), "state": result.get("state"),
                                    "reason": result.get("reason")}
    return cases


def android_cases(analysis: Path, baseline: Path | None) -> list[dict]:
    summary = load(baseline / "summary.json") if baseline else None
    previous = {r["fixture"]: r for r in (summary or {}).get("results", [])}
    cases = []
    for capture in sorted((analysis / "reports/diff").glob("*.word.narrow.jsonl")):
        name = capture.name[:-len(".word.narrow.jsonl")]
        document = analysis / "fixtures" / (name + ".docx")
        case = {
            "id": "android-mobile-legacy/" + name,
            "group": name,
            "families": ANDROID_FAMILIES,
            "config": {"id": "android-word-mobile-5329-legacy", "platform": "android", "view": "mobile",
                       "contentWidthTwips": 5329, "requiredFamilies": ["Calibri"],
                       "assumptions": ["legacy narrow capture: cpSpace=document",
                                       "legacy narrow capture: mode=mobile-consumption"],
                       "coordinateDomain": "main-story UTF-16 line boundaries only"},
            "document": {"path": rel(document), "sha256": digest(document) if document.is_file() else None},
            "reference": {"kind": "android-narrow-line-log", "path": rel(capture), "sha256": digest(capture)},
            "comparator": "tools/measure/android_replay.py (ordered source ranges)",
            "evidence": "conditional",
            "evidenceReason": "LEGACY_NARROW_ASSUMPTIONS",
        }
        if name in previous:
            comparison = previous[name]["comparison"]
            case["baseline"] = {"source": rel(baseline), "state": comparison.get("state"),
                                "matchedLines": comparison.get("boundary", {}).get("matchedLines"),
                                "wordLines": comparison.get("boundary", {}).get("wordLines")}
        cases.append(case)
    return cases


def report_cases(analysis: Path) -> list[dict]:
    cases = []
    for name, families, constraint, source in REPORT_CASES:
        document = analysis / "fixtures" / (name + ".docx")
        cases.append({
            "id": "android-print-report/" + name,
            "group": name.rsplit("-n", 1)[0] if name.startswith("dg-decide") else name.split("-")[0],
            "families": families,
            "config": {"id": "android-word-print-report", "platform": "android", "view": "print",
                       "requiredFamilies": ["Calibri"], "geometry": "document"},
            "document": {"path": rel(document), "sha256": digest(document) if document.is_file() else None},
            "reference": {"kind": "report-constraint", "path": "../word_analyse/" + source,
                          "constraint": constraint},
            "comparator": "acceptance.py report check (page count / next-page source start)",
            "evidence": "report-only",
            "evidenceReason": "NO_STRICT_PAGE_LINE_LOG",
        })
    return cases


def command_register(args) -> int:
    analysis = args.analysis_root.resolve()
    cases = (mac_cases(args.captures.resolve(), args.fixtures.resolve(), args.mac_baseline)
             + android_cases(analysis, args.android_baseline) + report_cases(analysis))
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        raise SystemExit("duplicate case ids")
    registry = {
        "schema": REGISTRY_SCHEMA, "version": args.version, "codeBaseline": args.code_baseline,
        "families": list(FAMILIES),
        "evidenceStates": {
            "strict": "raw Word reference with a strict comparator",
            "conditional": "raw reference compared under explicit historical assumptions",
            "report-only": "report-level constraint without a strict page/line log",
            "reference-invalid": "reference exists but is void",
            "binding-missing": "reference cannot be bound to a current document",
        },
        "familyTagging": "declared from each preregistration question or report topic",
        "cases": cases,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(registry, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("registered %d cases -> %s" % (len(cases), args.output))
    return 0


# ------------------------------------------------------------- run-reports

def command_run_reports(args) -> int:
    registry = load(args.cases)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary = args.trace_bin.resolve()
    manifest = {"schema": "rsword-acceptance-report-run/1", "createdAt": datetime.now(timezone.utc).isoformat(),
                "engineBinary": {"path": str(binary), "sha256": digest(binary)},
                "fonts": [{"path": str(f.resolve()), "sha256": digest(f)} for f in args.font],
                "results": []}
    for case in registry["cases"]:
        if case["evidence"] != "report-only":
            continue
        name = case["id"].split("/", 1)[1]
        out = output / name
        out.mkdir()
        document = resolve(case["document"]["path"])
        record = {"id": case["id"], "documentSha256": digest(document) if document.is_file() else None}
        command = [str(binary), "--metrics", "real", "--vertical-grid", "none",
                   "--platform", "android", "--view", "print", "--require", "Calibri", "--no-fallback"]
        for font in args.font:
            command += ["--font", str(font.resolve())]
        command += [str(document), str(out / "trace.json")]
        run = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout)
        (out / "stdout.txt").write_text(run.stdout, encoding="utf-8")
        (out / "stderr.txt").write_text(run.stderr, encoding="utf-8")
        record.update({"command": command, "exitCode": run.returncode})
        if run.returncode == 0:
            record["traceSha256"] = digest(out / "trace.json")
        manifest["results"].append(record)
        print("%s: exit %d" % (name, run.returncode), flush=True)
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return 0


# ------------------------------------------------------------------- panel

def story_inventory(document: Path | None) -> dict:
    """Visible content outside the main-story text the engine formats today.

    Read from the package itself so content the engine silently ignores is still
    counted. Separator/continuation notes (ids -1 and 0) are not user content.
    """
    found: dict[str, int] = {}
    if document is None or not document.is_file():
        return found
    try:
        with zipfile.ZipFile(document) as package:
            names = package.namelist()
            for part, key in (("word/footnotes.xml", "footnotes"), ("word/endnotes.xml", "endnotes")):
                if part in names:
                    ids = re.findall(rb"<w:(?:footnote|endnote)\b[^>]*w:id=\"(-?\d+)\"", package.read(part))
                    count = sum(int(i) > 0 for i in ids)
                    if count:
                        found[key] = count
            body = package.read("word/document.xml") if "word/document.xml" in names else b""
            for tag, key in ((rb"<w:headerReference\b", "headerReferences"),
                             (rb"<w:footerReference\b", "footerReferences"),
                             (rb"<w:drawing\b", "drawings"), (rb"<w:pict\b", "picts"),
                             (rb"<w:txbxContent\b", "textBoxes")):
                count = len(re.findall(tag, body))
                if count:
                    found[key] = count
    except (OSError, zipfile.BadZipFile, KeyError):
        found["unreadable"] = 1
    return found


def support_of(trace: dict | None, document: Path | None = None, *, evaluated: bool = True) -> dict:
    if not evaluated:
        return {"state": "not-evaluated"}
    if trace is None:
        return {"state": "unsupported", "reason": "NO_ENGINE_TRACE"}
    layout = trace.get("layoutInput") or {}
    skipped = layout.get("skippedBlocks", 0)
    diagnostics = layout.get("diagnostics") or []
    # The formatter lays out main-story paragraphs and supported tables only.
    unlaid = story_inventory(document)
    if skipped or diagnostics or unlaid:
        support = {"state": "partial", "skippedBlocks": skipped, "diagnostics": diagnostics}
        if unlaid:
            support["unlaidContent"] = unlaid
        return support
    return {"state": "full"}


def mac_layers(comparison: dict) -> list[dict]:
    body = comparison.get("comparison", {})
    structure = body.get("structure", {})
    tolerance = body.get("tolerancePt", 0.0)
    codes = Counter(f.get("code") for f in body.get("failures", []))
    layers = []
    if structure.get("candidateGlyphs") != structure.get("referenceGlyphs"):
        layers.append({"layer": "glyph-count", "candidateGlyphs": structure.get("candidateGlyphs"),
                       "referenceGlyphs": structure.get("referenceGlyphs")})
    if structure.get("candidatePages") != structure.get("referencePages"):
        layers.append({"layer": "pagination", "candidatePages": structure.get("candidatePages"),
                       "referencePages": structure.get("referencePages")})
    if codes.get("LINE_COUNT_MISMATCH"):
        layers.append({"layer": "line-structure", "pagesWithLineCountMismatch": codes["LINE_COUNT_MISMATCH"]})
    if codes.get("PAGE_UNDECIDABLE"):
        layers.append({"layer": "pairing-undecidable", "pages": codes["PAGE_UNDECIDABLE"]})
    start = comparison.get("decomposition", {}).get("lineStart", {})
    for axis, name in (("maxDy", "geometry-vertical"), ("maxDx", "geometry-horizontal")):
        value = body.get(axis)
        if value is not None and value > tolerance:
            key = "dy" if axis == "maxDy" else "dx"
            layers.append({"layer": name, "maxAbsPt": round(value, 6),
                           "lineStartMaxAbsPt": round(abs(start.get(key, {}).get("maxAbs", 0.0)), 6)})
    return layers


def evaluate_mac(case: dict, sweep: Path | None) -> dict:
    name = case["id"].split("/", 1)[1]
    result = load(sweep / name / "result.json") if sweep else None
    if result is None:
        return {"strict": "NOT_RUN", "reasons": ["NO_SWEEP_RESULT"], "support": support_of(None, evaluated=False)}
    out = sweep / name
    trace = load(out / "trace.json")
    fixture = result.get("binding", {}).get("fixture")
    evaluation = {"sweepState": result.get("state"), "sweepReason": result.get("reason"),
                  "support": support_of(trace, Path(fixture) if fixture else None,
                                        evaluated=result.get("traceRun") is not None)}
    binding = result.get("binding", {})
    if result.get("reason") == "CAPTURE_VOID" or case["evidence"] == "reference-invalid":
        return {**evaluation, "strict": "UNDECIDABLE", "reasons": ["REFERENCE_INVALID"]}
    if binding.get("state") != "OK":
        return {**evaluation, "strict": "UNDECIDABLE", "reasons": [result.get("reason") or "BINDING_FAILED"]}
    mismatches = []
    if binding.get("fixtureSha256") != case["document"]["sha256"]:
        mismatches.append("DOCUMENT_HASH_MISMATCH")
    if result.get("metaSha256") != case["reference"]["metaSha256"]:
        mismatches.append("REFERENCE_HASH_MISMATCH")
    if mismatches:
        return {**evaluation, "strict": "UNDECIDABLE", "reasons": mismatches}
    if result.get("reason") == "TRACE_FAILED" or trace is None:
        return {**evaluation, "strict": "FAIL", "reasons": ["TRACE_FAILED"]}
    selfcheck = load(out / "selfcheck.json") or {}
    comparison = load(out / "comparison.json") or {}
    evaluation["selfcheck"] = {
        "state": selfcheck.get("state", "NOT_RUN"),
        "notOk": [{"check": c.get("check"), "state": c.get("state"), "reason": c.get("reason")}
                  for c in selfcheck.get("checks", []) if c.get("state") != "OK"],
    }
    evaluation["comparisonState"] = comparison.get("state", "NOT_RUN")
    evaluation["layers"] = mac_layers(comparison) if comparison else []
    reasons = []
    if evaluation["comparisonState"] != "OK":
        reasons.append("COMPARISON_" + evaluation["comparisonState"])
    if evaluation["selfcheck"]["state"] != "OK":
        reasons.append("SELFCHECK_" + evaluation["selfcheck"]["state"])
    if evaluation["support"]["state"] != "full":
        reasons.append("SUPPORT_" + evaluation["support"]["state"].upper())
    if evaluation["comparisonState"] == "FAIL":
        strict = "FAIL"
    elif reasons:
        strict = "UNDECIDABLE"
    else:
        strict = "PASS"
    return {**evaluation, "strict": strict, "reasons": reasons}


def evaluate_android(case: dict, replay: dict | None, root: Path | None) -> dict:
    name = case["id"].split("/", 1)[1]
    result = next((r for r in (replay or {}).get("results", []) if r.get("fixture") == name), None)
    if result is None:
        return {"strict": "NOT_RUN", "conditional": "NOT_RUN", "reasons": ["NO_REPLAY_RESULT"],
                "support": support_of(None, evaluated=False)}
    trace_path = Path(result.get("trace", {}).get("path", "")) if result.get("trace") else None
    trace = load(trace_path) if trace_path else None
    comparison = result.get("comparison", {})
    boundary = comparison.get("boundary", {})
    reasons = ["REFERENCE_CONDITIONAL"]
    if result.get("inputs", {}).get("fixture", {}).get("sha256") != case["document"]["sha256"]:
        reasons.append("DOCUMENT_HASH_MISMATCH")
    if result.get("inputs", {}).get("capture", {}).get("sha256") != case["reference"]["sha256"]:
        reasons.append("REFERENCE_HASH_MISMATCH")
    evaluation = {"strict": "UNDECIDABLE", "conditional": comparison.get("state", "NOT_RUN"),
                  "reasons": reasons, "support": support_of(trace, resolve(case["document"]["path"])),
                  "lines": {"matched": boundary.get("matchedLines"), "word": boundary.get("wordLines"),
                            "engine": boundary.get("engineLines")}}
    if evaluation["conditional"] == "FAIL":
        evaluation["layers"] = [{"layer": "line-structure", **evaluation["lines"]}]
    return evaluation


def evaluate_report(case: dict, run: Path | None) -> dict:
    name = case["id"].split("/", 1)[1]
    reasons = ["REFERENCE_REPORT_ONLY"]
    trace = load(run / name / "trace.json") if run else None
    manifest = load(run / "manifest.json") if run else None
    record = next((r for r in (manifest or {}).get("results", []) if r.get("id") == case["id"]), None)
    evaluation = {"strict": "UNDECIDABLE", "reasons": reasons,
                  "support": support_of(trace, resolve(case["document"]["path"]), evaluated=record is not None)}
    if record is None:
        return {**evaluation, "report": {"state": "NOT_RUN"}}
    if record.get("documentSha256") != case["document"]["sha256"]:
        reasons.append("DOCUMENT_HASH_MISMATCH")
    if trace is None:
        stderr = (run / name / "stderr.txt").read_text(encoding="utf-8") if (run / name / "stderr.txt").is_file() else ""
        return {**evaluation, "report": {"state": "NO_LAYOUT", "exitCode": record.get("exitCode"),
                                         "stderrTail": stderr.strip().splitlines()[-1:] }}
    pages = trace.get("pages", [])
    observed = {"pages": len(pages)}
    if len(pages) > 1 and pages[1].get("lines"):
        observed["secondPageStart"] = pages[1]["lines"][0].get("sourceStart")
    constraint = case["reference"]["constraint"]
    matched = all(observed.get(key) == value for key, value in constraint.items())
    return {**evaluation, "report": {"state": "MATCH" if matched else "MISMATCH",
                                     "constraint": constraint, "observed": observed}}


def tally(rows: list[dict], key) -> dict:
    counts = Counter(key(row) for row in rows)
    return dict(sorted(counts.items()))


def build_panel(registry: dict, sweep: Path | None, replay_root: Path | None,
                report_run: Path | None, previous: dict | None) -> dict:
    replay = load(replay_root / "summary.json") if replay_root else None
    rows = []
    for case in registry["cases"]:
        kind = case["reference"]["kind"]
        if kind == "mac-pdf-glyph-bundle":
            evaluation = evaluate_mac(case, sweep)
        elif kind == "android-narrow-line-log":
            evaluation = evaluate_android(case, replay, replay_root)
        else:
            evaluation = evaluate_report(case, report_run)
        row = {"id": case["id"], "group": case["group"], "families": case["families"],
               "config": case["config"]["id"], "evidence": case["evidence"], **evaluation}
        baseline = case.get("baseline")
        if baseline:
            if kind == "mac-pdf-glyph-bundle":
                same = (baseline.get("state"), baseline.get("reason")) == (
                    evaluation.get("sweepState"), evaluation.get("sweepReason"))
            else:
                same = baseline.get("state") == evaluation.get("conditional") and \
                    baseline.get("matchedLines") == evaluation.get("lines", {}).get("matched")
            row["baselineReproduced"] = same
        rows.append(row)

    def strict_table(group_of) -> dict:
        table: dict[str, dict] = {}
        for row in rows:
            for group in group_of(row):
                entry = table.setdefault(group, {"cases": 0, **{s: 0 for s in STRICT_STATES}})
                entry["cases"] += 1
                entry[row["strict"]] += 1
        return dict(sorted(table.items()))

    previous_rows = {r["id"]: r for r in (previous or {}).get("cases", [])}
    regressions = [r["id"] for r in rows if previous_rows.get(r["id"], {}).get("strict") == "PASS"
                   and r["strict"] != "PASS"]
    changes = [{"id": r["id"], "before": previous_rows[r["id"]]["strict"], "after": r["strict"]}
               for r in rows if r["id"] in previous_rows and previous_rows[r["id"]]["strict"] != r["strict"]]
    by_family = strict_table(lambda r: r["families"] or ["untagged"])
    for family in FAMILIES:
        by_family.setdefault(family, {"cases": 0, **{s: 0 for s in STRICT_STATES}})
    return {
        "schema": PANEL_SCHEMA,
        "createdAt": datetime.now(timezone.utc).isoformat(),
        "registry": {"schema": registry["schema"], "version": registry["version"],
                     "codeBaseline": registry.get("codeBaseline")},
        "inputs": {"macSweep": rel(sweep) if sweep else None,
                   "androidReplay": rel(replay_root) if replay_root else None,
                   "reportRun": rel(report_run) if report_run else None},
        "northStar": {"strictPass": sum(r["strict"] == "PASS" for r in rows), "cases": len(rows),
                      "byFamily": dict(sorted(by_family.items())),
                      "byConfig": strict_table(lambda r: [r["config"]])},
        "guardrails": {
            "capability": tally(rows, lambda r: r["support"]["state"]),
            "evidence": tally(rows, lambda r: r["evidence"]),
            "conditional": tally([r for r in rows if "conditional" in r], lambda r: r["conditional"]),
            "report": tally([r for r in rows if "report" in r], lambda r: r["report"]["state"]),
            "failureLayers": dict(sorted(Counter(layer["layer"] for r in rows
                                                 for layer in r.get("layers", [])).items())),
            "selfcheckNotOk": dict(sorted(Counter(c["reason"] or c["check"] for r in rows
                                                  for c in r.get("selfcheck", {}).get("notOk", [])).items())),
            "stability": {"previous": (previous or {}).get("createdAt"),
                          "newRegressions": regressions, "strictChanges": changes},
            "baselineReproduced": tally([r for r in rows if "baselineReproduced" in r],
                                        lambda r: r["baselineReproduced"]),
        },
        "cases": rows,
    }


def markdown(panel: dict) -> str:
    star = panel["northStar"]
    lines = ["# Acceptance panel", "",
             "Registry v%s (code baseline `%s`). Strict pass requires a strict reference, a valid "
             "binding, full support, an OK comparison and an OK self-check." % (
                 panel["registry"]["version"], panel["registry"]["codeBaseline"]), "",
             "**Strict whole-document passes: %d / %d registered cases.**" % (star["strictPass"], star["cases"]),
             "", "## By family", "", "| Family | Cases | PASS | FAIL | UNDECIDABLE | NOT_RUN |",
             "| --- | --- | --- | --- | --- | --- |"]
    for name, entry in star["byFamily"].items():
        lines.append("| %s | %d | %d | %d | %d | %d |" % (name, entry["cases"], entry["PASS"], entry["FAIL"],
                                                        entry["UNDECIDABLE"], entry["NOT_RUN"]))
    lines += ["", "## By configuration", "", "| Configuration | Cases | PASS | FAIL | UNDECIDABLE | NOT_RUN |",
              "| --- | --- | --- | --- | --- | --- |"]
    for name, entry in star["byConfig"].items():
        lines.append("| %s | %d | %d | %d | %d | %d |" % (name, entry["cases"], entry["PASS"], entry["FAIL"],
                                                        entry["UNDECIDABLE"], entry["NOT_RUN"]))
    guard = panel["guardrails"]
    lines += ["", "## Guardrails", ""]
    for key in ("capability", "evidence", "conditional", "report", "failureLayers", "selfcheckNotOk",
                "baselineReproduced"):
        lines.append("- %s: %s" % (key, ", ".join("%s %s" % (k, v) for k, v in guard[key].items()) or "none"))
    stability = guard["stability"]
    lines.append("- stability: previous %s; new regressions %d; strict changes %d" % (
        stability["previous"], len(stability["newRegressions"]), len(stability["strictChanges"])))
    lines += ["", "## Cases", "", "| Case | Families | Evidence | Support | Strict | Attribution |",
              "| --- | --- | --- | --- | --- | --- |"]
    for row in panel["cases"]:
        detail = []
        for layer in row.get("layers", []):
            numbers = ", ".join("%s=%s" % (k, v) for k, v in layer.items() if k != "layer")
            detail.append("%s (%s)" % (layer["layer"], numbers))
        if "conditional" in row:
            detail.append("conditional %s %s/%s lines" % (row["conditional"], row["lines"]["matched"],
                                                          row["lines"]["word"]))
        if "report" in row:
            report = row["report"]
            detail.append("report %s %s" % (report["state"], json.dumps(report.get("observed", {}))))
        support = row["support"]
        if support["state"] == "partial":
            parts = []
            if support.get("skippedBlocks"):
                parts.append("skipped blocks %d" % support["skippedBlocks"])
            if support.get("diagnostics"):
                parts.append("diagnostics %d" % len(support["diagnostics"]))
            if support.get("unlaidContent"):
                parts.append("unlaid " + ", ".join("%s %d" % kv for kv in support["unlaidContent"].items()))
            detail.append("support: " + "; ".join(parts))
        detail += row["reasons"]
        lines.append("| %s | %s | %s | %s | %s | %s |" % (row["id"], ", ".join(row["families"]), row["evidence"],
                                                         row["support"]["state"], row["strict"], "; ".join(detail)))
    return "\n".join(lines) + "\n"


def command_panel(args) -> int:
    registry = load(args.cases)
    if not registry or registry.get("schema") != REGISTRY_SCHEMA:
        raise SystemExit("invalid registry")
    previous = load(args.previous) if args.previous else None
    panel = build_panel(registry, args.mac_sweep and args.mac_sweep.resolve(),
                        args.android_replay and args.android_replay.resolve(),
                        args.report_run and args.report_run.resolve(), previous)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    (output / "panel.json").write_text(json.dumps(panel, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    (output / "panel.md").write_text(markdown(panel), encoding="utf-8")
    star = panel["northStar"]
    print("strict %d / %d; new regressions %d" % (star["strictPass"], star["cases"],
                                                  len(panel["guardrails"]["stability"]["newRegressions"])))
    return 1 if panel["guardrails"]["stability"]["newRegressions"] else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    register = sub.add_parser("register")
    register.add_argument("--captures", type=Path, default=REPO / "captures")
    register.add_argument("--fixtures", type=Path, default=REPO / "fixtures")
    register.add_argument("--analysis-root", type=Path, default=REPO.parent / "word_analyse")
    register.add_argument("--mac-baseline", type=Path)
    register.add_argument("--android-baseline", type=Path)
    register.add_argument("--version", type=int, default=1)
    register.add_argument("--code-baseline", required=True)
    register.add_argument("--output", type=Path, required=True)
    reports = sub.add_parser("run-reports")
    reports.add_argument("--cases", type=Path, required=True)
    reports.add_argument("--trace-bin", type=Path, default=REPO / "target/debug/layout-trace")
    reports.add_argument("--font", type=Path, action="append", required=True)
    reports.add_argument("--timeout", type=float, default=60)
    reports.add_argument("--output", type=Path, required=True)
    panel = sub.add_parser("panel")
    panel.add_argument("--cases", type=Path, required=True)
    panel.add_argument("--mac-sweep", type=Path)
    panel.add_argument("--android-replay", type=Path)
    panel.add_argument("--report-run", type=Path)
    panel.add_argument("--previous", type=Path, help="Earlier panel.json for stability")
    panel.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    return {"register": command_register, "run-reports": command_run_reports,
            "panel": command_panel}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
