#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-cursor-unit.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判三条：R0 基线仍在栅格上、R1 单位是 twip（点预测）、R2 候选清单里恰好一个通过。
**R2 的门槛是全中，不是最高分**——按最高分选模型就是拟合。

用法：

    ./.venv/bin/python prereg_cursor.py <采集包> --probes fixtures/cursor-unit.probes.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import (EPS, GRID_PT, eq, hhea, lines_with_tags,  # noqa: E402
                          natural_pt, quantize)
from make_cursor_fixture import FONTS  # noqa: E402

# §3 的候选清单，**采集前钉死**。0.0 约定为「走实数、不取整」。
CANDIDATES = {
    "U1 twip 1/1440in": 0.05,
    "U2 半twip 1/2880in": 0.025,
    "U3 1/7200in": 0.01,
    "U4 1/600in": 0.12,
    "U5 1/1200in": 0.06,
    "U0 实数": 0.0,
}
POINT_PREDICTION = "U1 twip 1/1440in"


def step_of(natural: float, unit: float) -> float:
    return natural if unit == 0.0 else round(natural / unit) * unit


def groups_of(lines, probes):
    return [(p, [l for l in lines if l["text"].startswith(p["tag"])]) for p in probes]


def falsifiers(bundle, model, probes_doc):
    fired = []
    meta = bundle["META"]
    pre = (meta.get("preflight") or {}).get("result")
    if pre != "PASS":
        fired.append({"id": "F-A", "detail": f"采前核查 result={pre!r}"})

    subs = meta.get("fontSubstitution") or {}
    req = {f.replace("-", "").replace(" ", "").lower() for f in FONTS}
    unexpected = []
    for n in subs.get("pdfFontNames", []):
        b = n.split("+", 1)[-1].replace("-", "").replace(" ", "").lower()
        if not any(r in b or b in r for r in req):
            unexpected.append(n)
    if subs.get("result") != "PASS" or unexpected:
        fired.append({"id": "F-B", "detail": f"字体核查 {subs.get('result')!r}，额外字体={unexpected}"})

    fx = meta.get("fixture") or {}
    declared, actual = probes_doc.get("sha256"), (fx.get("before") or {}).get("sha256")
    if not fx.get("unchanged") or (declared and actual and declared != actual):
        fired.append({"id": "F-C", "detail": f"夹具身份不符 {str(declared)[:12]}… / {str(actual)[:12]}…"})

    if model.get("state") == UNDECIDABLE and "PAGE_SET_MISMATCH" in (model.get("reason") or ""):
        fired.append({"id": "F-D", "detail": model["reason"]})
    return fired


def check_r0(lines):
    out = {"prediction": "R0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 20:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def score_unit(lines, probes, unit: float) -> dict:
    """一个候选单位在全部步上的得分。判不了的组照样进分母（§7.4）。"""
    out = {"n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    for p, members in groups_of(lines, probes):
        steps = p["lines"] - 1
        out["n"] += steps
        if len(members) != p["lines"] or len({m["page"] for m in members}) != 1:
            out["undecidable"] += steps
            out["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除",
                                 "行数": len(members), "页": sorted({m["page"] for m in members})})
            continue
        m = hhea(Path(FONTS[p["family"]]))
        nat = natural_pt(m, p["sizeHalfPoints"] / 2.0)
        st = step_of(nat, unit)
        b0 = members[0]["baseline"]
        for n in range(1, p["lines"]):
            pred = quantize(b0 + n * st)
            got = members[n]["baseline"]
            if eq(got, pred):
                out["ok"] += 1
            elif len(out["misses"]) < 20:
                out["misses"].append({"tag": p["tag"], "n": n, "实测": got,
                                      "预测": round(pred, 4), "差": round(got - pred, 4)})
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
              "prereg": "docs/PREREG-2026-09-17-cursor-unit.md",
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
        r0 = check_r0(lines)
        scores = {name: score_unit(lines, probes, u) for name, u in CANDIDATES.items()}
        # R2 的门槛：**全中**。不是最高分。
        passed = [n for n, s in scores.items() if verdict(s) == OK]
        outcome = ("恰好一个通过" if len(passed) == 1 else
                   "一个都不通过" if not passed else "不止一个通过：本夹具分不开")
        result["predictions"] = {
            "R0": {**r0, "verdict": verdict(r0)},
            "R1": {**scores[POINT_PREDICTION], "prediction": f"R1 单位 = {POINT_PREDICTION}",
                   "verdict": verdict(scores[POINT_PREDICTION])},
        }
        result["R2"] = {"prediction": "R2 候选清单里恰好一个 752/752 通过",
                        "threshold": "全中，不是最高分",
                        "passed": passed, "outcome": outcome,
                        "scores": {n: {"ok": s["ok"], "n": s["n"],
                                       "undecidable": s["undecidable"]}
                                   for n, s in scores.items()}}

        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        print(f"  R0 {verdict(r0):<11} 通过 {r0['ok']}/{r0['n']}  {r0['prediction']}")
        for miss in r0["misses"][:5]:
            print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
        r1 = scores[POINT_PREDICTION]
        print(f"  R1 {verdict(r1):<11} 通过 {r1['ok']}/{r1['n']}（判不了 {r1['undecidable']}）"
              f"  点预测：单位 = {POINT_PREDICTION}")
        for miss in r1["misses"][:6]:
            print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
        print(f"  R2 结局：**{outcome}**（门槛：全中）")
        for name, s in scores.items():
            mark = "✔" if name in passed else " "
            print(f"      {mark} {name:<22} {s['ok']:>4}/{s['n']}"
                  f"{'  判不了 ' + str(s['undecidable']) if s['undecidable'] else ''}")
        for note in scores[POINT_PREDICTION]["notes"][:4]:
            print(f"      ? {json.dumps(note, ensure_ascii=False)}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
