#!/usr/bin/env python3
"""Replay the alternate font-height tail and optional explicit mode-2 projection."""

import argparse
import hashlib
import json
import os
from pathlib import Path

import dwrite_metrics
import font_adjustment
from wordmeasure import font_adjustment_reference as arithmetic
from wordmeasure import font_tail_reference as model
from wordmeasure import font_vertical_reference as vertical

EVIDENCE = {
    "wordSha256": "b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c",
    "alternateTailManifest": "d9d550272d707658b793f4f5710dcda6f07256324625b50ac5a06f9676e89741",
    "adjustedFontManifest": "3fd1611a399f9a2e4fda5f3167b56d1e1c0e437b21c1f30c1fc10bddaed90ed2",
}


def project_file(path):
    data = path.read_bytes()
    inputs = json.loads(data, object_pairs_hook=font_adjustment.unique_object,
                        parse_float=font_adjustment.reject_noninteger,
                        parse_constant=font_adjustment.reject_noninteger)
    schemas = {"rsword-font-tail-input/1": model.project_alternate_tail,
               "rsword-font-tail-mode2-input/1": model.project_alternate_mode2}
    if (not isinstance(inputs, dict) or not isinstance(inputs.get("schema"), str)
            or inputs["schema"] not in schemas
            or set(inputs) != {"schema", "project"}):
        raise ValueError("expected rsword-font-tail-input/1 or rsword-font-tail-mode2-input/1 with project")
    if not isinstance(inputs["project"], dict):
        raise ValueError("project must be an object")
    mode2 = inputs["schema"] == "rsword-font-tail-mode2-input/1"
    result = schemas[inputs["schema"]](**inputs["project"])
    sources = {"cli": Path(__file__), "model": Path(model.__file__),
               "integerArithmetic": Path(arithmetic.__file__),
               "jsonParser": Path(font_adjustment.__file__),
               "publisher": Path(dwrite_metrics.__file__)}
    if mode2:
        sources["verticalModel"] = Path(vertical.__file__)
    assumptions = ["numeric V words and post-rewrite charset are supplied at the tail read point",
                   "P/V values remain stable throughout the modeled wrapper and alternate tail",
                   "no DOCX size mapping, OS/2 charset inference or active Word branch is claimed"]
    if mode2:
        assumptions += ["C/F mode 2 is selected and unchanged; V prefix and dy survive to later reads",
                        "four tail fields reach the scaler unchanged; pre_scale_cc is independently supplied",
                        "h2 is retained; completed copy/default-wrapper update reaches the three modeled M words",
                        "auxiliary processing and its stackResult consumption are not simulated",
                        "no further metric mutation, alternate wrapper or LS aggregation is modeled"]
    else:
        assumptions += ["mode is the caller's masked value in 0..3; property words and scales are explicit",
                        "independent stack output is preserved; F.cc is not inferred or modified",
                        "downstream helpers, scaling, wrapper selection and LS aggregation are not composed"]
    return dict(
        schema="rsword-font-tail-mode2-reference/1" if mode2 else "rsword-font-tail-reference/1",
        status="ARITHMETIC_REFERENCE",
        scope=("Conditional alternate tail to mode-2 fields and three updated M words; not Word layout"
               if mode2 else "Explicit host selector and alternate-tail fields; not Word layout or final font metrics"),
        assumptions=assumptions,
        evidence=EVIDENCE, source={"path": str(path.resolve()), "sha256": hashlib.sha256(data).hexdigest()},
        inputs=inputs, result=result,
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
