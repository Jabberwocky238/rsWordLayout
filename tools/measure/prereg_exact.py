#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-exact-spacing.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：X0 基线仍在栅格上、X1 固定行距下的累加律。
X1 里**没有字体度量**——`exact` 行距是固定值，这正是本批的用意：
把「累加机制」与「natural 算得对不对」分开。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import EPS, GRID_PT, eq, lines_with_tags, quantize  # noqa: E402
from make_exact_fixture import FONTS  # noqa: E402


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


def check_x0(lines):
    out = {"prediction": "X0 每条基线都在 0.24pt 栅格上（非默认行距下）", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y,
                                  "余数": round(y % GRID_PT, 6)})
    return out


def check_x1(lines, probes, per_group):
    """X1：以实测 b₀ 为锚，第 n 条 = quantize(b₀ + n × line/20)。**不含字体度量。**"""
    out = {"prediction": "X1 第 n 条基线 = quantize(b₀ + n × w:line/20)", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": [], "perGroup": {}}
    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        steps = per_group - 1
        out["n"] += steps
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            out["undecidable"] += steps
            out["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除",
                                 "行数": len(members), "页": sorted({m["page"] for m in members})})
            continue
        step = p["lineTwips"] / 20.0
        b0 = members[0]["baseline"]
        hit = 0
        for n in range(1, per_group):
            pred = quantize(b0 + n * step)
            got = members[n]["baseline"]
            if eq(got, pred):
                out["ok"] += 1
                hit += 1
            elif len(out["misses"]) < 20:
                out["misses"].append({"tag": p["tag"], "family": p["family"],
                                      "lineTwips": p["lineTwips"], "n": n, "实测": got,
                                      "预测": round(pred, 4), "差": round(got - pred, 4)})
        out["perGroup"][f"{p['tag']} {p['family']} line={p['lineTwips']}"] = f"{hit}/{steps}"
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
    probes, per_group = probes_doc["probes"], probes_doc["linesPerGroup"]
    lines = lines_with_tags(model)

    fired = falsifiers(bundle, model, probes_doc)
    result = {"schema": "rsword-layout-prereg/1",
              "prereg": "docs/PREREG-2026-09-17-exact-spacing.md",
              "purpose": "把累加机制与 natural 分开：exact 行距不含字体度量",
              "bundle": args.bundle, "bundleState": model.get("state"),
              "coverage": model.get("coverage"),
              "lineDenominators": model.get("lineDenominators"), "falsifiers": fired}

    if fired:
        result["verdict"] = "VOID"
        print(f"采集包 state={result['bundleState']}  判定=VOID")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        result["verdict"] = "EVALUATED"
        x0 = check_x0(lines)
        x1 = check_x1(lines, probes, per_group)
        result["predictions"] = {"X0": {**x0, "verdict": verdict(x0)},
                                 "X1": {**x1, "verdict": verdict(x1)}}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("X0", x0), ("X1", x1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for miss in p["misses"][:10]:
                print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        print("  逐组：")
        for k, v in x1["perGroup"].items():
            print(f"      {k:<40}{v}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
