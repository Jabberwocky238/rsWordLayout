#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-vmisc2.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判四条：Q0 栅格、Q1 抬升量量化、Q2 同行多字号共用基线、Q3 分页符计数。
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import EPS, GRID_PT, eq, lines_with_tags  # noqa: E402


def quantize_half_up(v: float) -> float:
    return math.floor(v / GRID_PT + 0.5) * GRID_PT if v >= 0 else \
        -(math.floor(-v / GRID_PT + 0.5) * GRID_PT)


def falsifiers(bundle, model, probes_doc):
    fired = []
    meta = bundle["META"]
    if (meta.get("preflight") or {}).get("result") != "PASS":
        fired.append({"id": "F-A", "detail": f"采前核查 {(meta.get('preflight') or {}).get('result')!r}"})
    subs = meta.get("fontSubstitution") or {}
    if subs.get("result") != "PASS":
        fired.append({"id": "F-B", "detail": f"字体核查 {subs.get('result')!r}，"
                                             f"账外字体={subs.get('unexpectedNames')}"})
    fx = meta.get("fixture") or {}
    declared, actual = probes_doc.get("sha256"), (fx.get("before") or {}).get("sha256")
    if not fx.get("unchanged") or (declared and actual and declared != actual):
        fired.append({"id": "F-C", "detail": f"夹具身份不符 {str(declared)[:12]}… / {str(actual)[:12]}…"})
    if model.get("state") == UNDECIDABLE and "PAGE_SET_MISMATCH" in (model.get("reason") or ""):
        fired.append({"id": "F-D", "detail": model["reason"]})
    return fired


def check_q0(lines):
    out = {"prediction": "Q0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_q1(lines, probes):
    out = {"prediction": "Q1 抬升量 = quantize(val/2) 到 0.24pt 栅格", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for p in probes:
        if p["kind"] != "rise-quantised":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        out["n"] += 1
        if len(members) != 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "不是恰好一行"})
            continue
        ys = [g["origin"][1] for g in members[0]["glyphs"]]
        head = len(p["tag"]) + 2
        if len(ys) < head + 2:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "字形不够"})
            continue
        got = ys[0] - ys[head]
        pred = quantize_half_up(p["positionHalfPoints"] / 2.0)
        if eq(got, pred):
            out["ok"] += 1
        else:
            out["misses"].append({"tag": p["tag"], "val": p["positionHalfPoints"],
                                  "原始": p["positionHalfPoints"] / 2.0,
                                  "实测": round(got, 4), "预测": round(pred, 4),
                                  "偏离": round(got - pred, 6)})
    return out


def check_q2(lines, probes):
    out = {"prediction": "Q2 同一行内各字号共用一条基线", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for p in probes:
        if p["kind"] != "mixed-size":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        out["n"] += 1
        if len(members) != 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "不是恰好一行"})
            continue
        ys = {round(g["origin"][1], 6) for g in members[0]["glyphs"]}
        sizes = sorted({g.get("sizePt") for g in members[0]["glyphs"]})
        if len(ys) == 1:
            out["ok"] += 1
        else:
            out["misses"].append({"tag": p["tag"], "small": p["small"], "big": p["big"],
                                  "各字号": sizes, "出现的基线": sorted(ys)})
    return out


def check_q3(model, probes):
    """Q3：涉及分页符的行记录，量具的计数模型都给 OK。"""
    out = {"prediction": "Q3 分页符按位置画 0 或 1 个字形（计数模型对得上）", "n": 0,
           "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    tags = {p["tag"] for p in probes if p["kind"].startswith("break-")}
    for page in model.get("pages", []):
        for line in page.get("lines", []):
            text = (line.get("text") or "")
            if not any(text.startswith(t) or t in text[:6] for t in tags):
                continue
            out["n"] += 1
            if line.get("state") == OK:
                out["ok"] += 1
            elif line.get("state") == UNDECIDABLE:
                out["undecidable"] += 1
                out["notes"].append({"text": text[:10], "state": line.get("state"),
                                     "reasons": line.get("reasons")})
            else:
                out["misses"].append({"text": text[:10], "state": line.get("state"),
                                      "expected": line.get("expected"),
                                      "实得": len(line.get("glyphs") or []),
                                      "reasons": line.get("reasons")})
    return out


def verdict(p):
    if p["misses"]:
        return FAIL
    if p["undecidable"] or p["ok"] < p["n"]:
        return UNDECIDABLE
    return OK


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("bundle")
    ap.add_argument("--probes", required=True)
    ap.add_argument("--output", "-o")
    args = ap.parse_args()

    bundle = capture.load_bundle(Path(args.bundle))
    model = wordmodel.build(bundle)
    probes_doc = json.loads(Path(args.probes).read_text())
    probes = probes_doc["probes"]
    lines = lines_with_tags(model)

    fired = falsifiers(bundle, model, probes_doc)
    result = {"schema": "rsword-layout-prereg/1",
              "prereg": "docs/PREREG-2026-09-17-vmisc2.md",
              "clueIndependence": "Q1 的半数进位方向取自 vmisc 的读数，按 §7.5 记为线索不独立",
              "bundle": args.bundle, "bundleState": model.get("state"),
              "coverage": model.get("coverage"), "falsifiers": fired}

    if fired:
        result["verdict"] = "VOID"
        print(f"采集包 state={result['bundleState']}  判定=VOID")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        result["verdict"] = "EVALUATED"
        qs = {"Q0": check_q0(lines), "Q1": check_q1(lines, probes),
              "Q2": check_q2(lines, probes), "Q3": check_q3(model, probes)}
        result["predictions"] = {k: {**v, "verdict": verdict(v)} for k, v in qs.items()}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in qs.items():
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:6]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:3]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
