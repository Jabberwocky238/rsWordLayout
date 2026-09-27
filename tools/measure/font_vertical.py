#!/usr/bin/env python3
"""Replay measured Face1 inputs through the explicit mode-2 simple vertical path."""

import argparse
import hashlib
import json
import os
from pathlib import Path

import dwrite_metrics
import font_record
from wordmeasure import font_adjustment_reference as adjustment
from wordmeasure import font_vertical_reference as model

EVIDENCE = {
    "wordSha256": "b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c",
    "rawProviderManifest": "8de482d8a316622f3d20b2b1c71494089a1996d9e5ae907a975a785d46a7425f",
    "adjustedFontManifest": "3fd1611a399f9a2e4fda5f3167b56d1e1c0e437b21c1f30c1fc10bddaed90ed2",
}


def project_file(path, measurements):
    data = path.read_bytes()
    inputs = json.loads(data, object_pairs_hook=font_record.unique_object,
                        parse_float=font_record.finite_number, parse_constant=font_record.finite_number)
    if (not isinstance(inputs, dict) or inputs.get("schema") != "rsword-font-vertical-input/1"
            or set(inputs) != {"schema", "font", "vertical"}):
        raise ValueError("expected rsword-font-vertical-input/1 with font and vertical")
    if (not isinstance(inputs["font"], dict)
            or set(inputs["font"]) != {"lf_height", "width_scale", "escapement"}):
        raise ValueError("font must contain exactly lf_height, width_scale and escapement")
    if not isinstance(inputs["vertical"], dict) or "t_prefix_words" in inputs["vertical"]:
        raise ValueError("vertical must be an object; T is computed from the measured source")
    record = font_record.project_report(measurements, **inputs["font"])
    result = model.project_simple_mode2(t_prefix_words=record["result"]["tPrefixWords"],
                                        **inputs["vertical"])
    sources = {"cli": Path(__file__), "model": Path(model.__file__),
               "adjustmentModel": Path(adjustment.__file__),
               "fontRecordCli": Path(font_record.__file__),
               "fontRecordModel": Path(font_record.model.__file__),
               "publisherAndMeasurementProtocol": Path(dwrite_metrics.__file__)}
    return dict(
        schema="rsword-font-vertical-reference/1", status="ARITHMETIC_REFERENCE",
        scope="Measured Face1 inputs to conditional mode-2 vertical fields and three updated M words",
        assumptions=["installed RTARC/RTDWRITEFONT path forwards V to the bound provider T builder",
                     "numeric T words remain valid at the modeled read points; fallback and V postprocessing excluded",
                     "mode 2 and the simple adjustment tail are selected; supplied flags are explicit checkpoints",
                     "dy is unchanged between factor production and scaling; later F-to-C/default-M update is reached",
                     "no intermediate mutation, alternate wrapper selection or LS aggregation is modeled",
                     "no DOCX font-size, property-bit, unit or runtime branch mapping is inferred"],
        evidence=EVIDENCE, source={"path": str(path.resolve()), "sha256": hashlib.sha256(data).hexdigest()},
        inputs=inputs, fontRecord=record, result=result,
        codeSha256={name: hashlib.sha256(file.read_bytes()).hexdigest() for name, file in sources.items()},
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--measurements", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if os.path.lexists(args.out):
        parser.error("--out must be a new file")
    try:
        result = project_file(args.input, args.measurements)
        dwrite_metrics.publish(args.out, result)
    except (OSError, ValueError, KeyError, TypeError, RecursionError, OverflowError) as exc:
        parser.error(str(exc))
    print(f"ARITHMETIC_REFERENCE: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
