#!/usr/bin/env python3
"""Evaluate the frozen vertical-precision experiment without fitting observations.

Source spans and the verified original PDF order determine correspondence. A
failed prediction is evidence against that candidate, not an unusable capture.
The six Hs check within-run consistency; each sup/sub run counts once.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from fractions import Fraction
from itertools import combinations
from pathlib import Path

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, pairing, source_binding, sweep_stability, wordmodel

EPS_PT = 1e-6
MATRIX_EPS = 1e-10
GRID = Fraction(6, 25)
SPAN_NAMES = ("tag", "anchorA", "normal", "anchorB", "superscript", "anchorC", "subscript", "anchorD")
METRICS = ("paintX", "paintY", "advance", "offset")


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


def _hash(data):
    return hashlib.sha256(data).hexdigest()


def _json(path):
    data = Path(path).read_bytes()
    return json.loads(data), _hash(data)


def _integer(value):
    return isinstance(value, int) and not isinstance(value, bool)


def _finite(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def quantize(value, grid=GRID, mode="nearest"):
    """Exact rational grid rounding, with nearest ties away from zero."""
    value = value if isinstance(value, Fraction) else Fraction(str(value))
    units = value / grid
    if mode == "floor":
        count = math.floor(units)
    elif mode == "ceil":
        count = math.ceil(units)
    elif mode == "nearest":
        count = math.floor(abs(units) + Fraction(1, 2)) * (-1 if units < 0 else 1)
    else:
        raise ValueError("Unknown quantization mode: %s" % mode)
    return count * grid


def _size_candidates(size, font, subject):
    raw = size * Fraction(33, 50)
    prefix = "ySuperscript" if subject == "superscript" else "ySubscript"
    x = size * Fraction(font["OS2"][prefix + "XSize"], font["upem"])
    y = size * Fraction(font["OS2"][prefix + "YSize"], font["upem"])
    isotropic = {"ratio_0.66": raw, "ratio_2_over_3": size * Fraction(2, 3),
                 "ratio_0.66_half_point_nearest": quantize(raw, Fraction(1, 2))}
    for mode in ("nearest", "floor", "ceil"):
        isotropic["ratio_0.66_grid_0.24_" + mode] = quantize(raw, mode=mode)
    return {**{name: {"x": value, "y": value} for name, value in isotropic.items()},
            "OS2_XY": {"x": x, "y": y}, "OS2_Y_isotropic": {"x": y, "y": y}}


def _offset_candidates(size, font, subject):
    superscript = subject == "superscript"
    raw = size * (Fraction(17, 50) if superscript else Fraction(2, 25))
    prefix = "ySuperscript" if superscript else "ySubscript"
    result = {"empirical_unquantized": raw,
              "OS2_YOffset": size * Fraction(font["OS2"][prefix + "YOffset"], font["upem"])}
    for mode in ("nearest", "floor", "ceil"):
        result["magnitude_grid_0.24_" + mode] = quantize(raw, mode=mode)
    return result


def _check(observed, expected):
    observed = list(observed)
    expected = [float(x) for x in expected]
    residuals = [actual - prediction for actual, prediction in zip(observed, expected)]
    _require(len(observed) == len(expected) and observed, "EMPTY_OR_UNEQUAL_PREDICTION")
    return {"state": OK if all(abs(r) <= EPS_PT for r in residuals) else FAIL,
            "expectedPt": expected, "residualsPt": residuals,
            "maxAbsResidualPt": max(abs(r) for r in residuals)}


def _metric(observed, predictions, reason=None):
    if not reason and (not observed or not all(_finite(value) for value in observed)):
        reason = "NONFINITE_OR_MISSING_OBSERVATION"
    if reason:
        return {"state": UNDECIDABLE, "reasons": [reason], "observedPt": observed,
                "candidates": {name: {"state": UNDECIDABLE, "reasons": [reason]}
                               for name in predictions}, "matchingCandidates": []}
    checks = {name: _check(observed, values) for name, values in predictions.items()}
    return {"state": OK, "observedPt": observed, "candidates": checks,
            "matchingCandidates": [name for name, check in checks.items() if check["state"] == OK]}


def _validate_manifest(probes, source_sha, font_sha, fonts):
    _require(probes["schema"] == "rsword-layout-vertical-probes/1", "MANIFEST_SCHEMA_MISMATCH")
    _require(probes["fixtureSha256"] == source_sha, "MANIFEST_FIXTURE_SHA_MISMATCH")
    _require(probes["fontInputsSha256"] == font_sha, "FONT_INPUTS_SHA_MISMATCH")
    _require(_integer(probes["expectedPages"]) and probes["expectedPages"] > 0,
             "MANIFEST_PAGE_COUNT_INVALID")
    cases = probes["cases"]
    _require(_integer(probes["expectedLines"]) and probes["expectedLines"] == len(cases) and cases,
             "MANIFEST_LINE_COUNT_INVALID")
    ids, cursor = set(), 0
    for case in cases:
        name, text = case["id"], case["text"]
        _require(isinstance(name, str) and name and name not in ids, "MANIFEST_DUPLICATE_OR_INVALID_ID")
        ids.add(name)
        _require(isinstance(text, str) and text.isascii() and text.endswith("\r")
                 and all(32 <= ord(char) <= 126 for char in text[:-1]), "MANIFEST_NON_ASCII_OR_CONTROL_TEXT")
        _require(_integer(case["sourceStart"]) and _integer(case["sourceEnd"])
                 and case["sourceStart"] == cursor and case["sourceEnd"] == cursor + len(text),
                 "MANIFEST_SOURCE_RANGE_INVALID: " + name)
        _require(_integer(case["sizeHalfPoints"]) and case["sizeHalfPoints"] > 0, "MANIFEST_SIZE_INVALID")
        _require(case["family"] in fonts, "FONT_INPUTS_FAMILY_MISSING: " + case["family"])
        _require(set(case["spans"]) == set(SPAN_NAMES), "MANIFEST_SPAN_KEYS_INVALID")
        for span_name in SPAN_NAMES:
            span = case["spans"][span_name]
            _require(isinstance(span, list) and len(span) == 2 and all(_integer(x) for x in span)
                     and span[0] == cursor and span[1] > cursor and span[1] < case["sourceEnd"],
                     "MANIFEST_SPAN_RANGE_INVALID: %s/%s" % (name, span_name))
            part = text[span[0] - case["sourceStart"]:span[1] - case["sourceStart"]]
            expected = name + ":" if span_name == "tag" else (span_name[-1] if span_name.startswith("anchor") else "HHHHHH")
            _require(part == expected, "MANIFEST_SPAN_TEXT_INVALID: %s/%s" % (name, span_name))
            cursor = span[1]
        _require(cursor == case["sourceEnd"] - 1, "MANIFEST_SPAN_COVERAGE_INVALID")
        cursor = case["sourceEnd"]


def _fonts(inputs):
    _require(inputs["schema"] == "rsword-layout-font-inputs/1", "FONT_INPUTS_SCHEMA_MISMATCH")
    result = {}
    for font in inputs["fonts"]:
        _require(font["family"] not in result, "FONT_INPUTS_DUPLICATE_FAMILY")
        _require(_integer(font["upem"]) and font["upem"] > 0, "FONT_INPUTS_UPEM_INVALID")
        _require(isinstance(font["postscript"], str) and font["postscript"], "FONT_INPUTS_POSTSCRIPT_MISSING")
        _require(_integer(font["H"]["hmtxAdvance"]) and font["H"]["hmtxAdvance"] > 0, "FONT_INPUTS_H_ADVANCE_INVALID")
        for subject in ("ySuperscript", "ySubscript"):
            for field in ("XSize", "YSize", "YOffset"):
                value = font["OS2"][subject + field]
                _require(_integer(value) and (field == "YOffset" or value > 0), "FONT_INPUTS_OS2_INVALID")
        result[font["family"]] = font
    return result


def _verify_stability(bundle):
    record = bundle["META"].get("sweepStability", {})
    _require(record.get("schema") == "rsword-layout-sweep-stability/1" and record.get("state") == OK,
             "SWEEP_STABILITY_UNVERIFIED")
    scans = []
    for name in ("first", "repeat"):
        receipt = record["scans"][name]["rawReceipt"]
        _require(receipt["file"] == "sweep-%s.raw.txt" % name, "SWEEP_RECEIPT_PATH_INVALID")
        data = (Path(bundle["path"]) / receipt["file"]).read_bytes()
        _require(_hash(data) == receipt["sha256"] and len(data) == receipt["bytes"], "SWEEP_RECEIPT_HASH_MISMATCH")
        scan = sweep_stability.parse_receipt(data.decode("utf-8"))
        scan["rawReceipt"] = receipt
        scans.append(scan)
    verified = sweep_stability.verify(*scans, bundle["sweep"]["contentText"])
    _require(verified["state"] == OK, "SWEEP_STABILITY_RECHECK_FAILED: " + "; ".join(verified["reasons"]))
    _require(scans[0]["positions"] == bundle["sweep"]["positions"]
             and scans[0]["endOfContent"] == bundle["sweep"]["endOfContent"], "SWEEP_RECEIPT_CONTENT_MISMATCH")
    return verified


def _common_admission(bundle, probes, source_docx, fonts):
    families = {case["family"] for case in probes["cases"]}
    meta = bundle["META"]
    pdf_sha = _hash((Path(bundle["path"]) / "case.pdf").read_bytes())
    _require(pdf_sha == meta.get("pdfSha256") == bundle["glyphs"].get("sourceSha256"),
             "PDF_HASH_MISMATCH")
    _require(meta.get("preflight", {}).get("result") == "PASS", "FONT_PREFLIGHT_NOT_PASS")
    _require(all(meta["preflight"].get("requiredFamilies", {}).get(f) is True for f in families),
             "FONT_PREFLIGHT_FAMILY_UNVERIFIED")
    _require(meta.get("fontSubstitution", {}).get("result") == "PASS", "FONT_SUBSTITUTION_NOT_PASS")
    _require(all(meta["fontSubstitution"].get("requiredFamiliesSeenInPdf", {}).get(f) is True for f in families),
             "FONT_SUBSTITUTION_FAMILY_UNVERIFIED")
    stability = _verify_stability(bundle)
    bound = source_binding.bind_source(bundle, source_docx)
    source_text = "".join(case["text"] for case in probes["cases"])
    captured = bound["sweep"]["contentText"]
    _require(len(source_text) == len(captured) and all(
        a == b or (a == "\r" and b == "\n" and bound["sweep"].get("platform") == "mac")
        for a, b in zip(source_text, captured)), "MANIFEST_SOURCE_TEXT_MISMATCH")
    model = wordmodel.build(bound)
    _require(model["state"] == OK, "WORD_MODEL_NOT_OK: " + str(model.get("reason", model.get("lineDenominators"))))
    _require(len(model["pages"]) == len(bundle["glyphs"]["pages"]) == probes["expectedPages"], "PAGE_COUNT_MISMATCH")
    lines = []
    for page, raw_page in zip(model["pages"], bundle["glyphs"]["pages"]):
        _require(page["state"] == OK and page["assignment"]["state"] == OK
                 and page["assignment"]["method"] == "DERIVED_FROM_LINE_NUMBERS", "PAGE_ASSIGNMENT_UNVERIFIED")
        cursor = 0
        for line in page["lines"]:
            length = line["sourceEnd"] - line["sourceStart"]
            raw = raw_page["glyphs"][cursor:cursor + length]
            _require(line["state"] == OK and line["mode"] == pairing.MODE_ALL and line["expected"] == length
                     and len(line["glyphs"]) == len(raw) == length and line["identityMismatched"] == 0,
                     "LINE_PAIRING_UNVERIFIED")
            _require(len(lines) < len(probes["cases"]), "EXTRA_SOURCE_LINE")
            case = probes["cases"][len(lines)]
            _require((line["sourceStart"], line["sourceEnd"]) == (case["sourceStart"], case["sourceEnd"]),
                     "LINE_SOURCE_RANGE_MISMATCH: " + case["id"])
            for ordinal, (glyph, modeled) in enumerate(zip(raw, line["glyphs"])):
                _require(glyph["index"] == cursor + ordinal and glyph["glyphOrigin"] == modeled["origin"]
                         and glyph["text"] == modeled["text"], "PDF_GLYPH_ORDER_MISMATCH")
                if ordinal < length - 1:
                    _require(glyph["textStatus"] == "mapped" and glyph["text"] == case["text"][ordinal],
                             "VISIBLE_CHARACTER_IDENTITY_MISMATCH: %s/%d" % (case["id"], ordinal))
                    _require(glyph["fontName"].split("+", 1)[-1] == fonts[case["family"]]["postscript"],
                             "CASE_FONT_IDENTITY_MISMATCH: " + case["id"])
            lines.append((case, raw, page["index"], cursor))
            cursor += length
        _require(cursor == len(raw_page["glyphs"]), "UNASSIGNED_PDF_GLYPHS")
    _require(len(lines) == probes["expectedLines"], "LINE_COUNT_MISMATCH")
    return lines, bound["sourceAnnotation"], stability, model["lineDenominators"]


def _horizontal_geometry(glyphs):
    for glyph in glyphs:
        matrix = glyph.get("matrix")
        if (glyph.get("geometryStatus") != "RAW_HORIZONTAL" or not isinstance(matrix, list)
                or len(matrix) != 6 or not all(_finite(v) for v in matrix)
                or abs(matrix[1]) >= MATRIX_EPS or abs(matrix[2]) >= MATRIX_EPS
                or matrix[0] <= 0 or matrix[3] <= 0):
            return "RAW_HORIZONTAL_GEOMETRY_UNVERIFIED"
    return None


def _paint_geometry(glyphs):
    horizontal_reason = _horizontal_geometry(glyphs)
    if horizontal_reason:
        return horizontal_reason
    for glyph in glyphs:
        if (not _finite(glyph.get("fontSize")) or glyph["fontSize"] <= 0
                or not _finite(glyph.get("scaling")) or glyph["scaling"] <= 0):
            return "PAINT_SIZE_UNVERIFIED"
        if not all(math.isfinite(_paint(glyph, axis)) for axis in ("x", "y")):
            return "PAINT_SIZE_NONFINITE"
    return None


def _origin_geometry(glyphs):
    horizontal_reason = _horizontal_geometry(glyphs)
    if horizontal_reason:
        return horizontal_reason
    for glyph in glyphs:
        origin = glyph.get("glyphOrigin")
        if not isinstance(origin, list) or len(origin) != 2 or not all(_finite(v) for v in origin):
            return "ORIGIN_UNVERIFIED"
    return None


def _paint(glyph, axis):
    return glyph["fontSize"] * glyph["matrix"][0 if axis == "x" else 3] * (glyph["scaling"] if axis == "x" else 1)


def _one_case(case, raw, page_index, first_index, font):
    start = case["sourceStart"]
    spans = {name: raw[a - start:b - start] for name, (a, b) in case["spans"].items()}
    normal = spans["normal"]
    anchors = [spans["anchor" + letter][0] for letter in "ABCD"]
    size = Fraction(case["sizeHalfPoints"], 2)
    result = {"id": case["id"], "family": case["family"], "sizeHalfPoints": case["sizeHalfPoints"],
              "sourceStart": start, "sourceEnd": case["sourceEnd"], "pageIndex": page_index,
              "firstPdfGlyphIndex": first_index, "admission": {"state": OK, "reasons": []},
              "observations": {"spans": {name: {"sourceRange": case["spans"][name], "glyphs": glyphs}
                                                   for name, glyphs in spans.items()}}, "conditions": []}
    normal_geometry = _origin_geometry(normal)
    normal_paint_geometry = _paint_geometry(normal)
    anchor_geometry = _origin_geometry(anchors)
    anchor_ys = [glyph["glyphOrigin"][1] for glyph in anchors] if not anchor_geometry else None
    baseline_ok = anchor_ys is not None and max(anchor_ys) - min(anchor_ys) <= EPS_PT
    result["observations"]["anchorBaselinesPt"] = anchor_ys
    result["observations"]["anchorsShareBaseline"] = baseline_ok
    valid_origins = [g for g in anchors + normal + spans["superscript"] + spans["subscript"] if not _origin_geometry([g])]
    result["observations"]["absoluteYGridDiagnostic"] = [
        {"pdfGlyphIndex": g["index"], "yPt": g["glyphOrigin"][1],
         "nearestGridPt": float(quantize(g["glyphOrigin"][1])),
         "residualPt": g["glyphOrigin"][1] - float(quantize(g["glyphOrigin"][1])),
         "onGrid": abs(g["glyphOrigin"][1] - float(quantize(g["glyphOrigin"][1]))) <= EPS_PT}
        for g in valid_origins]
    result["observations"]["normalPaintPt"] = {
        axis: [_paint(g, axis) for g in normal] if not normal_paint_geometry else None for axis in ("x", "y")}
    normal_span = None if normal_geometry or _origin_geometry(spans["anchorB"]) else spans["anchorB"][0]["glyphOrigin"][0] - normal[0]["glyphOrigin"][0]
    result["observations"]["normalAdvanceSpanPt"] = normal_span
    result["observations"]["normalHmtxSpanPredictionPt"] = float(6 * size * Fraction(font["H"]["hmtxAdvance"], font["upem"]))
    for subject, next_anchor in (("superscript", "anchorC"), ("subscript", "anchorD")):
        marked = spans[subject]
        geometry = _origin_geometry(marked)
        paint_geometry = _paint_geometry(marked)
        sizes = _size_candidates(size, font, subject)
        metrics = {}
        for axis, metric_name in (("x", "paintX"), ("y", "paintY")):
            observed = [_paint(g, axis) for g in marked] if not paint_geometry else None
            predictions = {"absolute/" + name: [value[axis]] * 6 for name, value in sizes.items()}
            normal_paint = result["observations"]["normalPaintPt"][axis]
            if normal_paint is not None:
                predictions["baseline_scaled/ratio_0.66"] = [p * .66 for p in normal_paint]
                predictions["baseline_scaled/ratio_2_over_3"] = [p * (2 / 3) for p in normal_paint]
                for mode in ("nearest", "floor", "ceil"):
                    predictions["baseline_scaled/ratio_0.66_grid_0.24_" + mode] = [quantize(Fraction(str(p)) * Fraction(33, 50), mode=mode) for p in normal_paint]
                predictions["baseline_scaled/ratio_0.66_half_point_nearest"] = [quantize(Fraction(str(p)) * Fraction(33, 50), Fraction(1, 2)) for p in normal_paint]
            metrics[metric_name] = _metric(observed, predictions, paint_geometry)
            if normal_paint_geometry:
                for name in ("ratio_0.66", "ratio_2_over_3", "ratio_0.66_grid_0.24_nearest", "ratio_0.66_grid_0.24_floor", "ratio_0.66_grid_0.24_ceil", "ratio_0.66_half_point_nearest"):
                    metrics[metric_name]["candidates"]["baseline_scaled/" + name] = {"state": UNDECIDABLE, "reasons": [normal_paint_geometry]}
        advance_reason = geometry or normal_geometry or _origin_geometry(spans[next_anchor]) or _origin_geometry(spans["anchorB"])
        target_span = None if advance_reason else spans[next_anchor][0]["glyphOrigin"][0] - marked[0]["glyphOrigin"][0]
        if not advance_reason and (not math.isfinite(normal_span) or normal_span <= 0):
            advance_reason = "INVALID_NORMAL_ADVANCE_DENOMINATOR"
        if not advance_reason and not math.isfinite(target_span):
            advance_reason = "NONFINITE_TARGET_ADVANCE_SPAN"
        advance_predictions = {name: [float(value["x"] / size) * normal_span if normal_span is not None else 0] for name, value in sizes.items()}
        metrics["advance"] = _metric([target_span] if target_span is not None else None, advance_predictions, advance_reason)
        metrics["advance"]["normalSpanPt"] = normal_span
        metrics["advance"]["ratio"] = target_span / normal_span if not advance_reason else None
        metrics["advance"]["advanceEquivalentSizePt"] = float(size) * target_span / normal_span if not advance_reason else None
        metrics["advance"]["note"] = "First H origin to following baseline anchor; equivalent size is not Word internal logical size."
        offset_reason = geometry or normal_geometry or anchor_geometry
        ys = [glyph["glyphOrigin"][1] for glyph in marked] if not geometry else None
        if not offset_reason and (not baseline_ok or any(abs(g["glyphOrigin"][1] - anchor_ys[0]) > EPS_PT for g in normal)):
            offset_reason = "NORMAL_ANCHORS_OR_HS_DO_NOT_SHARE_BASELINE"
        offsets = None if geometry or anchor_geometry else [(anchor_ys[0] - y) * (1 if subject == "superscript" else -1) for y in ys]
        offset_predictions = {name: [value] * 6 for name, value in _offset_candidates(size, font, subject).items()}
        metrics["offset"] = _metric(offsets, offset_predictions, offset_reason)
        metrics["offset"]["direction"] = "rise" if subject == "superscript" else "drop"
        metrics["offset"]["empiricalRatio"] = .34 if subject == "superscript" else .08
        metrics["offset"]["withinRunConstant"] = max(ys) - min(ys) <= EPS_PT if ys is not None else None
        metrics["offset"]["note"] = "Signed top-down origin delta; grid candidates quantize the positive predicted displacement magnitude."
        isotropic = (not paint_geometry and all(abs(g["scaling"] - 1) <= MATRIX_EPS and abs(_paint(g, "x") - _paint(g, "y")) <= EPS_PT for g in marked))
        result["conditions"].append({"subject": subject, "sourceRange": case["spans"][subject],
                                     "paintScalarApplicable": isotropic,
                                     "metrics": metrics})
    return result


def _summarize(result):
    conditions = [condition for case in result["cases"] for condition in case.get("conditions", [])]
    total = result["denominators"]["expectedConditions"]
    summary = {}
    for name in METRICS:
        metrics = [condition["metrics"][name] for condition in conditions]
        candidates = sorted({key for metric in metrics for key in metric["candidates"]})
        summary[name] = {"conditions": total, "readable": sum(m["state"] == OK for m in metrics),
                         "undecidable": total - sum(m["state"] == OK for m in metrics), "candidates": {}}
        for candidate in candidates:
            counts = {"conditions": total, "ok": 0, "fail": 0, "undecidable": total - len(metrics)}
            for metric in metrics:
                state = metric["candidates"].get(candidate, {"state": UNDECIDABLE})["state"]
                counts[{OK: "ok", FAIL: "fail", UNDECIDABLE: "undecidable"}[state]] += 1
            summary[name]["candidates"][candidate] = counts
        summary[name]["candidatePairs"] = {}
        for left, right in combinations(candidates, 2):
            counts = {"conditions": total, "comparable": 0, "coincidentPredictions": 0,
                      "distinguishablePredictions": 0, "leftOnlyMatches": 0, "rightOnlyMatches": 0,
                      "neitherMatches": 0, "bothMatch": 0, "undecidable": total}
            for metric in metrics:
                a, b = (metric["candidates"].get(key, {}) for key in (left, right))
                if a.get("state") not in (OK, FAIL) or b.get("state") not in (OK, FAIL):
                    continue
                counts["comparable"] += 1
                counts["undecidable"] -= 1
                if all(abs(x - y) <= EPS_PT for x, y in zip(a["expectedPt"], b["expectedPt"])):
                    counts["coincidentPredictions"] += 1
                    continue
                counts["distinguishablePredictions"] += 1
                key = ("bothMatch" if a["state"] == b["state"] == OK else
                       "leftOnlyMatches" if a["state"] == OK else
                       "rightOnlyMatches" if b["state"] == OK else "neitherMatches")
                counts[key] += 1
            summary[name]["candidatePairs"][left + " vs " + right] = counts
    result["metrics"] = summary
    result["denominators"]["admittedCases"] = sum(case["admission"]["state"] == OK for case in result["cases"])
    result["denominators"]["undecidableCases"] = result["denominators"]["expectedCases"] - result["denominators"]["admittedCases"]


def _json_safe(value):
    if isinstance(value, float) and not math.isfinite(value):
        return str(value)
    if isinstance(value, dict):
        return {key: _json_safe(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_json_safe(item) for item in value]
    return value


def evaluate(bundle_path: Path, probes_path: Path, source_docx: Path, font_inputs_path: Path) -> dict:
    result = {"schema": "rsword-layout-vertical-probe-evaluation/1", "state": UNDECIDABLE,
              "admission": {"state": UNDECIDABLE, "reasons": []},
              "inputs": {"bundle": str(bundle_path), "probes": str(probes_path), "sourceDocx": str(source_docx), "fontInputs": str(font_inputs_path)},
              "criteria": {"epsPt": EPS_PT, "matrixEps": MATRIX_EPS, "gridPt": float(GRID),
                           "gridNearest": "rational round-half-away-from-zero",
                           "pairing": "Manifest UTF-16 source spans, verified line counts, original PDF order; no text or y sorting",
                           "sampleUnit": "One superscript or subscript run per case; six Hs are not independent samples",
                           "predictionPolicy": "Report every frozen candidate; no nearest winner, fitting, or tolerance expansion",
                           "limitations": "Admission does not prove Word quiescence or internal logical size. OS/2 fields are competing predictions, not evidence of Word usage."},
              "denominators": {"expectedCases": 0, "expectedConditions": 0}, "cases": []}
    try:
        probes, probes_sha = _json(probes_path)
        cases = probes["cases"]
        result["denominators"].update(expectedCases=len(cases), expectedConditions=2 * len(cases),
                                       expectedPages=probes["expectedPages"], expectedLines=probes["expectedLines"])
        source_sha = _hash(Path(source_docx).read_bytes())
        inputs, font_sha = _json(font_inputs_path)
        result["inputs"].update(probesSha256=probes_sha, fixtureSha256=source_sha, fontInputsSha256=font_sha)
        fonts = _fonts(inputs)
        _validate_manifest(probes, source_sha, font_sha, fonts)
        bundle = capture.load_bundle(bundle_path)
        result["inputs"]["captureSha256"] = {name: _hash((Path(bundle_path) / name).read_bytes()) for name in ("META.json", "sweep.json", "glyphs.json")}
        lines, annotation, stability, model_counts = _common_admission(bundle, probes, source_docx, fonts)
        result.update(sourceAnnotation=annotation, sweepStability=stability, modelLineDenominators=model_counts)
        result["cases"] = [_one_case(case, raw, page, first, fonts[case["family"]]) for case, raw, page, first in lines]
        result["state"] = OK
        result["admission"] = {"state": OK, "reasons": []}
    except (ValueError, OSError, KeyError, TypeError, IndexError, OverflowError) as exc:
        reason = "%s: %s" % (type(exc).__name__, exc)
        result["admission"]["reasons"] = [reason]
        result["cases"] = [{"id": case.get("id"), "admission": {"state": UNDECIDABLE, "reasons": [reason]}, "conditions": []}
                           for case in locals().get("cases", []) if isinstance(case, dict)]
    _summarize(result)
    return _json_safe(result)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--probes", type=Path, required=True)
    parser.add_argument("--source-docx", type=Path, required=True)
    parser.add_argument("--font-inputs", type=Path, required=True)
    parser.add_argument("--output", "-o", type=Path)
    args = parser.parse_args(argv)
    result = evaluate(args.bundle, args.probes, args.source_docx, args.font_inputs)
    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False) + "\n"
    if args.output:
        args.output.write_text(text)
    else:
        print(text, end="")
    return 0 if result["state"] == OK else 2


if __name__ == "__main__":
    raise SystemExit(main())
