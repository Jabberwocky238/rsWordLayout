#!/usr/bin/env python3
"""Replay explicit PTS Simple fit inputs; not a Word pagination capture."""

import argparse
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path

import dwrite_metrics
from wordmeasure import simple_fit_reference as model

EVIDENCE = {
    "ptlsSha256": "cf7699ab47748bdf16ad1a5e8ba06803d99f7e4c4533bd57974ba9bbf2c89492",
    "simpleRegionManifest": "5a720498d3c8639e842f00be73907e6569bbf17249b0fe4b9776fe3d175d46ca",
    "simpleBottomSpaceManifest": "40d0418bc292bebf12676f481be3e1221beda28647895f3972a3257a097d68c5",
}


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON key")
    return result


def reject_noninteger(value):
    raise ValueError("Simple fit inputs require integers, not floating-point numbers")


def project_file(path):
    data = path.read_bytes()
    inputs = json.loads(data, object_pairs_hook=unique_object,
                        parse_float=reject_noninteger, parse_constant=reject_noninteger)
    if (not isinstance(inputs, dict) or inputs.get("schema") != "rsword-simple-fit-input/1"
            or not {"schema", "fit"} <= inputs.keys()
            or inputs.keys() - {"schema", "fit", "allowOverhang"}):
        raise ValueError("expected rsword-simple-fit-input/1 with fit and optional allowOverhang")
    if not isinstance(inputs["fit"], dict):
        raise ValueError("fit must be an object")
    if "allowOverhang" in inputs and type(inputs["allowOverhang"]) is not bool:
        raise ValueError("allowOverhang must be a Boolean when supplied")
    decision = model.classify(**inputs["fit"])
    resolved = decision.decision == "NEEDS_OVERHANG" and "allowOverhang" in inputs
    result = model.resolve(decision, inputs["allowOverhang"]) if resolved else decision
    sources = {"cli": Path(__file__), "model": Path(model.__file__),
               "publisher": Path(dwrite_metrics.__file__)}
    return dict(
        schema="rsword-simple-fit-reference/1", status="ARITHMETIC_REFERENCE",
        scope="Local Simple fit decision with explicit i32 inputs; no Word execution or page layout",
        assumptions=["the captured Simple branch has been selected",
                     "s comes from a successful bottom-space query or explicit zero on the no-query path",
                     "u and v include preceding adjustments; s is not inferred from font or grid fields",
                     "limit is the supplied remaining amount in the same native units",
                     "allowOverhang is used only when the local decision needs it",
                     "the overhang query does not otherwise change the supplied element values",
                     "missing allowOverhang leaves the decision unresolved; query failures are not simulated"],
        evidence=EVIDENCE, source={"path": str(path.resolve()), "sha256": hashlib.sha256(data).hexdigest()},
        inputs=inputs, classification=asdict(decision), overhangResolved=resolved, result=asdict(result),
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
