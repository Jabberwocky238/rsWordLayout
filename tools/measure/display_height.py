#!/usr/bin/env python3
"""Replay explicit Word host display-height inputs without executing Word."""

import argparse
import hashlib
import json
import os
from pathlib import Path

import dwrite_metrics
from wordmeasure import display_height_reference as model
from wordmeasure import font_adjustment_reference as muldiv

EVIDENCE = {
    "wordSha256": "b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c",
    "callerMappingManifest": "cfb325c4af4d335dcb9fe14e508d13ec2a0eba87b1b2ef535167526c456f248f",
    "lineHeightForwarderManifest": "743dde772e90b77ae1cdc71fffc3765c940a836bf56005210014ad3418c75677",
    "displayPointSourceManifest": "c5bd14d9a1c3656dbd2095a4043a78ef0613a300bf306025aff20d89f58c57f1",
}


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON key")
    return result


def reject_noninteger(value):
    raise ValueError("display-height inputs require integers, not floating-point numbers")


def project_file(path):
    data = path.read_bytes()
    inputs = json.loads(data, object_pairs_hook=unique_object,
                        parse_float=reject_noninteger, parse_constant=reject_noninteger)
    if (not isinstance(inputs, dict) or inputs.get("schema") != "rsword-display-height-input/1"
            or set(inputs) != {"schema", "conversion", "record", "hasLine"}):
        raise ValueError("expected rsword-display-height-input/1 with conversion, record, hasLine")
    result = model.project(conversion=inputs["conversion"], record=inputs["record"],
                           has_line=inputs["hasLine"])
    sources = {"cli": Path(__file__), "model": Path(model.__file__),
               "muldiv": Path(muldiv.__file__), "publisher": Path(dwrite_metrics.__file__)}
    return dict(
        schema="rsword-display-height-reference/1", status=model.STATUS,
        scope="Endpoint conversion and direct host height adjustment; not a Word layout capture",
        assumptions=["the captured reconciliation path has been selected",
                     "origin and R.b8 are supplied at de750 after preceding conditional updates",
                     "source_scale is the selected slot620 word; target_scale is the slot628 word",
                     "record fields stay stable through the normal line getter until forwarding",
                     "hasLine explicitly supplies the normal getter's nonnull result",
                     "native height-modification calls and their effects are not simulated",
                     "directStores report instruction writes, not final fields after opaque calls",
                     "no font, OOXML unit, direction, grid policy, baseline or PDF origin is inferred"],
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
