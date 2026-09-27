#!/usr/bin/env python3
"""Project measured Face1 metrics to an explicit initial host font record."""

import argparse
import base64
import hashlib
import json
import math
import os
from pathlib import Path
import struct

import dwrite_metrics
from wordmeasure import font_record_reference as model

EVIDENCE = {
    "wordSha256": "b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c",
    "sourceProviderManifest": "03f6e799d02ecf84f153f1167a6c0df3d24595bd50bc73f4ce71f70aa18a439d",
    "scalarImportManifest": "3ef120400f674a9ab7a19ebca399b981f4f1b34e0ff6c658ab60ce50ab6dc9d7",
    "faceContractManifest": "3b3c4a2e298786e456c640edb31818a2acf13216c1d4bf9cd916fb11b800a3b5",
}


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON key")
    return result


def finite_number(value):
    result = float(value)
    if not math.isfinite(result):
        raise ValueError("non-finite JSON number")
    return result


def xavg_from_truetype(data):
    if len(data) < 12 or data[:4] not in (b"\0\1\0\0", b"true"):
        raise ValueError("only a single-face TrueType sfnt is supported")
    count = struct.unpack_from(">H", data, 4)[0]
    if 12 + 16 * count > len(data):
        raise ValueError("truncated sfnt directory")
    matches = []
    for index in range(count):
        tag, _, offset, length = struct.unpack_from(">4sIII", data, 12 + 16 * index)
        if tag == b"OS/2":
            matches.append((offset, length))
    if len(matches) != 1:
        raise ValueError("expected one OS/2 table")
    offset, length = matches[0]
    if length < 0x4e or offset + length > len(data):
        raise ValueError("OS/2 must contain at least 78 valid bytes for this host path")
    return dict(offset=offset, length=length, xAvgCharWidthI16=struct.unpack_from(">h", data, offset + 2)[0])


def project_report(path, *, lf_height, width_scale, escapement):
    data = path.read_bytes()
    report = json.loads(data, object_pairs_hook=unique_object, parse_float=finite_number,
                        parse_constant=finite_number)
    if not isinstance(report, dict) or report.get("schema") != "rsword-dwrite-metrics/1":
        raise ValueError("expected a DirectWrite measurement report")
    if (report.get("status") != "MEASURED" or type(report.get("exitCode")) is not int
            or report["exitCode"] != 0 or report.get("timedOut") is not False or report.get("errors") != []):
        raise ValueError("measurement must have completed successfully")
    bound, sizes = report["inputsBefore"], report["emSizes"]
    if not isinstance(bound, dict) or set(bound) != {"font", "framework", "tool"}:
        raise ValueError("incomplete measurement input bindings")
    if bound != report["inputsAfter"] or bound["framework"]["sha256"] != dwrite_metrics.FRAMEWORK_SHA256:
        raise ValueError("measurement input hashes changed or framework is unsupported")
    if not isinstance(sizes, list) or not sizes or any(
            type(size) is not float or not math.isfinite(size) or size <= 0 or model.f32(size) != size for size in sizes):
        raise ValueError("measurement sizes must be positive finite binary32 values")
    records = dwrite_metrics.protocol(base64.b64decode(report["stdoutBase64"], validate=True), bound, sizes)
    analysis = next(row for row in records if row["stage"] == "analysis")
    if (analysis["fileType"], analysis["faceType"], analysis["count"]) != (2, 1, 1):
        raise ValueError("reference currently requires a single-face TrueType measurement")
    font_path = Path(bound["font"]["path"])
    font = font_path.read_bytes()
    if hashlib.sha256(font).hexdigest() != bound["font"]["sha256"]:
        raise ValueError("font bytes no longer match the measured input")
    table = xavg_from_truetype(font)
    base = next(row["metrics"] for row in records if row["stage"] == "metrics")
    units = base["designUnitsPerEm"]
    model.integer(lf_height, model.MIN_I32 + 1, -1, "lf_height")
    size = model.f32(-lf_height)

    def sample(em_size):
        matches = [row["metrics"] for row in records if row["stage"] == "gdiMetrics" and row["emSize"] == em_size]
        if not matches or any(value != matches[0] for value in matches):
            raise ValueError(f"need consistent measured GDI metrics at exact emSize {em_size}")
        if matches[0]["designUnitsPerEm"] != units:
            raise ValueError("measurement records disagree on designUnitsPerEm")
        return matches[0]

    initialized = sample(float(units))
    requested = None if size == model.f32(units) else sample(size)
    inputs = dict(initialized=initialized, requested=requested, lf_height=lf_height,
                  xavg_width=table["xAvgCharWidthI16"], width_scale=width_scale, escapement=escapement)
    result = model.project_face1(**inputs)
    source_paths = {"cli": Path(__file__), "model": Path(model.__file__),
                    "measurementProtocol": Path(dwrite_metrics.__file__)}
    return dict(schema="rsword-font-record-reference/1", status="ARITHMETIC_REFERENCE",
                scope="Explicit Face1/non-CFF read-point inputs to U fields, numeric T prefix and initial M; not Word layout",
                floatSemantics="IEEE binary32/binary64 round-to-nearest ties-to-even; Word FPCR not observed",
                assumptions=["successful Face1 initialization and non-CFF path", "valid OS/2 source table",
                             "explicit host read-point values; no proof of unexpanded callee purity",
                             "no later adjusted-wrapper mutation or LS aggregation"],
                evidence=EVIDENCE, measurement={"path": str(path.resolve()), "sha256": hashlib.sha256(data).hexdigest(),
                                               "inputs": bound}, fontTable=table, inputs=inputs, result=result,
                codeSha256={name: hashlib.sha256(file.read_bytes()).hexdigest() for name, file in source_paths.items()})


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measurements", type=Path, required=True)
    parser.add_argument("--lf-height", type=int, required=True)
    parser.add_argument("--width-scale", type=float, required=True)
    parser.add_argument("--escapement", type=int, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if os.path.lexists(args.out):
        parser.error("--out must be a new file")
    try:
        result = project_report(args.measurements, lf_height=args.lf_height,
                                width_scale=args.width_scale, escapement=args.escapement)
        dwrite_metrics.publish(args.out, result)
    except (OSError, ValueError, KeyError, TypeError, RecursionError, OverflowError) as exc:
        parser.error(str(exc))
    print(f"ARITHMETIC_REFERENCE: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
