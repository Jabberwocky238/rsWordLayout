#!/usr/bin/env python3
"""Replay explicit mode-2 font adjustment checkpoints; not a Word layout capture."""

import argparse
import hashlib
import json
import os
from pathlib import Path

import dwrite_metrics
from wordmeasure import font_adjustment_reference as model

EVIDENCE = {
    "wordSha256": "b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c",
    "adjustedFontManifest": "3fd1611a399f9a2e4fda5f3167b56d1e1c0e437b21c1f30c1fc10bddaed90ed2",
}


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON key")
    return result


def reject_noninteger(value):
    raise ValueError("font adjustment inputs require integers, not floating-point numbers")


def project_file(path):
    data = path.read_bytes()
    inputs = json.loads(data, object_pairs_hook=unique_object,
                        parse_float=reject_noninteger, parse_constant=reject_noninteger)
    if (not isinstance(inputs, dict) or inputs.get("schema") != "rsword-font-adjustment-input/1"
            or not {"schema", "project"} <= inputs.keys()
            or inputs.keys() - {"schema", "project", "h2Source"}):
        raise ValueError("expected rsword-font-adjustment-input/1 with project and optional h2Source")
    if not isinstance(inputs["project"], dict):
        raise ValueError("project must be an object")
    factor = None
    if "h2Source" in inputs:
        if not isinstance(inputs["h2Source"], dict):
            raise ValueError("h2Source must be an object")
        factor = model.produce_mode2_h2(**inputs["h2Source"])
        if factor["h2"] != inputs["project"].get("h2"):
            raise ValueError("computed h2 does not match the explicit project checkpoint")
    result = model.project_mode2(**inputs["project"])
    sources = {"cli": Path(__file__), "model": Path(model.__file__),
               "publisher": Path(dwrite_metrics.__file__)}
    return dict(
        schema="rsword-font-adjustment-reference/1", status="ARITHMETIC_REFERENCE",
        scope="Explicit mode-2 checkpoints; no native execution, DOCX mapping or runtime branch selection",
        assumptions=["pre_scale is the record at entry to the mode-2 scaler",
                     "c8_correction explicitly selects the independently established post-scale gate",
                     "mode-2 output reaches the captured F-to-C copy and default wrapper update",
                     "unexpanded side effects and alternate wrapper selection are not simulated"],
        evidence=EVIDENCE, source={"path": str(path.resolve()), "sha256": hashlib.sha256(data).hexdigest()},
        inputs=inputs, h2Result=factor, result=result,
        codeSha256={name: hashlib.sha256(file.read_bytes()).hexdigest() for name, file in sources.items()},
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if os.path.lexists(args.out):
        parser.error("--out must be a new file")
    try:
        result = project_file(args.input)
        dwrite_metrics.publish(args.out, result)
    except (OSError, ValueError, KeyError, TypeError, RecursionError, OverflowError) as exc:
        parser.error(str(exc))
    print(f"ARITHMETIC_REFERENCE: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
