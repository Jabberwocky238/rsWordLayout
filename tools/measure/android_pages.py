#!/usr/bin/env python3
"""Audit one archived Android PGIDX log and optionally compare supplied page counts.

This command does not run Word or the layout engine. Input files remain read only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

from wordmeasure.android_pages import compare_page_count, parse_page_log, trace_page_count

REPO = Path(__file__).resolve().parents[2]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--binding", type=Path, help="Optional android-page-binding/1 JSON sidecar")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--trace", type=Path, help="Existing engine trace; engine execution is not inferred")
    source.add_argument("--engine-page-count", type=int, help="User-supplied count, not an engine execution")
    parser.add_argument("--assume-legacy-print", action="store_true")
    parser.add_argument("--output", type=Path, required=True, help="New JSON file outside word_analyse")
    args = parser.parse_args(argv)
    output = args.output.expanduser().resolve()
    protected = (REPO.parent / "word_analyse").resolve()
    if output == protected or protected in output.parents:
        parser.error("--output must be outside the read-only word_analyse project")
    if output.exists():
        parser.error("--output must be a new file")
    inputs = {}

    def read(path: Path) -> bytes:
        resolved = path.expanduser().resolve()
        raw = resolved.read_bytes()
        inputs[str(resolved)] = hashlib.sha256(raw).hexdigest()
        return raw

    try:
        raw = read(args.log)
        fixture_hash = hashlib.sha256(read(args.fixture)).hexdigest()
        binding = json.loads(read(args.binding)) if args.binding else None
        audit = parse_page_log(raw)
        count, trace_problems, assumptions = args.engine_page_count, [], []
        candidate = {"source": "user-supplied count" if count is not None else "none"}
        if args.trace:
            trace = json.loads(read(args.trace))
            count, trace_problems, assumptions = trace_page_count(
                trace, assume_legacy_print=args.assume_legacy_print)
            if isinstance(trace, dict):
                if "docSha256" in trace:
                    if trace["docSha256"] != fixture_hash:
                        trace_problems.append("ENGINE_FIXTURE_HASH_MISMATCH")
                elif args.assume_legacy_print:
                    assumptions.append("supplied engine trace belongs to the supplied fixture SHA-256")
                else:
                    trace_problems.append("ENGINE_FIXTURE_BINDING_UNVERIFIED")
            candidate = {"source": "supplied trace", "path": str(args.trace.expanduser().resolve())}
        comparison = compare_page_count(audit, count, fixture_hash, binding,
                                        assume_legacy_print=args.assume_legacy_print)
        comparison["assumptions"] += assumptions
        if trace_problems:
            problems = trace_problems + comparison.get("problems", [])
            comparison.update(state="UNDECIDABLE", reason=problems[0], problems=problems)
        report = {"schema": "rsword-android-page-audit/1", "inputs": inputs,
                  "candidate": candidate, "audit": audit, "comparison": comparison,
                  "engineExecuted": False}
        serialized = json.dumps(report, indent=2, allow_nan=False) + "\n"
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("x", encoding="utf-8") as stream:
            stream.write(serialized)
    except (OSError, ValueError, TypeError) as exc:
        parser.error(str(exc))
    print(f"{comparison['state']}: final recorded count={audit['finalCount']}; report={output}")
    return {"OK": 0, "FAIL": 1, "UNDECIDABLE": 2}[comparison["state"]]


if __name__ == "__main__":
    sys.exit(main())
