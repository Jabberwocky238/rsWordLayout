#!/usr/bin/env python3
"""Replay the recovered docGrid integer helper with explicit native inputs.

This is an arithmetic reference, not a DOCX layout policy or a Word execution.
See docs/DOCGRID-ALGORITHM-EVIDENCE-2026-09-27.md for its scope and provenance.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

from wordmeasure import docgrid_reference

I32_MIN, I32_MAX = -(1 << 31), (1 << 31) - 1
INPUT_SCHEMA = "rsword-docgrid-helper-input/1"
MODEL_SHA256 = "10cbf5b056719da29e6bee97afcb5691c93b47239d8f0d385a940baa4763fb7e"
EVIDENCE = {
    "wordSha256": "b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c",
    "function": "0x100a656bc..0x100a65900",
    "frozenManifestSha256": "5cc1909764ca4d1965c8569865aefd1f9d29809dfb5b9302fecf76bf54c1930d",
}
CASE_FIELDS = {"id", "components", "tag", "valueU16", "ratioU16", "scaleU32",
               "periodI32", "useInputHeight"}


def _integer(value: object, low: int, high: int, name: str) -> int:
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")
    return value


def apply_grid(components: list[int], *, tag: int, value_u16: int, ratio_u16: int,
               scale_u32: int, period_i32: int, use_input_height: bool) -> dict:
    """Transform six native words; record diagnostics assuming they return.

    The first five words are signed i32; word 5 is opaque unsigned bits.
    Scale and period are explicit inputs, with no implied unit or XML mapping.
    Tags other than 0/2 are outside this recovered caller domain.
    """
    if not isinstance(components, list) or len(components) != 6:
        raise ValueError("components must contain five signed i32 and one opaque u32")
    for index, value in enumerate(components):
        _integer(value, I32_MIN if index < 5 else 0,
                 I32_MAX if index < 5 else 0xffffffff, f"components[{index}]")
    _integer(tag, 0, 2, "tag")
    if tag not in (0, 2):
        raise ValueError("only native parameter tags 0 and 2 are supported")
    _integer(value_u16, 0, 65535, "valueU16")
    _integer(ratio_u16, 0, 65535, "ratioU16")
    _integer(scale_u32, 0, 0xffffffff, "scaleU32")
    _integer(period_i32, I32_MIN, I32_MAX, "periodI32")
    if type(use_input_height) is not bool:
        raise ValueError("useInputHeight must be a boolean")

    return docgrid_reference.apply_tuple(components, tag, value_u16, ratio_u16,
                                        scale_u32, period_i32, use_input_height)


def replay(document: object) -> list[dict]:
    if not isinstance(document, dict) or set(document) != {"schema", "cases"}:
        raise ValueError("input requires exactly schema and cases")
    if document["schema"] != INPUT_SCHEMA:
        raise ValueError(f"schema must be {INPUT_SCHEMA}")
    cases = document["cases"]
    if not isinstance(cases, list) or not cases:
        raise ValueError("cases must be a nonempty array")
    seen, results = set(), []
    for case in cases:
        if not isinstance(case, dict) or set(case) != CASE_FIELDS:
            raise ValueError(f"each case requires exactly {', '.join(sorted(CASE_FIELDS))}")
        identity = case["id"]
        if not isinstance(identity, str) or not identity.strip() or identity in seen:
            raise ValueError("case ids must be nonempty, unique strings")
        seen.add(identity)
        result = apply_grid(case["components"], tag=case["tag"], value_u16=case["valueU16"],
                            ratio_u16=case["ratioU16"], scale_u32=case["scaleU32"],
                            period_i32=case["periodI32"], use_input_height=case["useInputHeight"])
        results.append({"input": case, "result": result})
    return results


def _unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="Explicit native input JSON; no inferred defaults")
    parser.add_argument("--output", type=Path, required=True, help="New result JSON file")
    args = parser.parse_args(argv)
    try:
        source = args.input.expanduser().resolve()
        output = args.output.expanduser()
        if output.exists() or output.is_symlink():
            raise ValueError("--output must be a new file")
        output = output.resolve()
        model_hash = hashlib.sha256(Path(docgrid_reference.__file__).read_bytes()).hexdigest()
        if model_hash != MODEL_SHA256:
            raise ValueError("reference model differs from the pinned frozen source")
        raw = source.read_bytes()
        results = replay(json.loads(raw, object_pairs_hook=_unique_object))
        report = {"schema": "rsword-docgrid-helper-replay/1", "state": "ARITHMETIC_REFERENCE",
                  "input": {"path": str(source), "sha256": hashlib.sha256(raw).hexdigest()},
                  "implementationSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  "modelSha256": model_hash,
                  "evidence": EVIDENCE, "nativeExecuted": False, "engineExecuted": False,
                  "limits": ["Input origin, scale units and runtime branch are unverified",
                             "Diagnostic calls are assumed to return without mutating inputs",
                             "No caller projection, font selection, baseline or page-fit policy"],
                  "cases": results}
        serialized = json.dumps(report, indent=2, allow_nan=False) + "\n"
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=output.parent,
                                             prefix=".docgrid-replay-", delete=False) as stream:
                temporary = Path(stream.name)
                stream.write(serialized)
            # Publish a complete file and atomically refuse an existing destination.
            os.link(temporary, output)
        finally:
            if temporary is not None:
                temporary.unlink()
    except (OSError, ValueError, TypeError, RuntimeError) as exc:
        parser.error(str(exc))
    print(f"ARITHMETIC_REFERENCE: {len(results)} cases; report={output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
