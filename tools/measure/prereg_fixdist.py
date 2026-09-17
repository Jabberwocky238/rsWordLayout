#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-fixed-distance.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：D0 栅格、D1「存在常数 d 使 `c(n) = ⌊300 + (n+1) × L − d⌋`」。

D1 **不是拟合**：不挑「最好的 d」，只问 160 条基线给出的区间交集空不空。
d 是**跨全部 16 组共享的一个常数**——16 种行高必须被同一个 d 解释。
"""

from __future__ import annotations

import argparse
import json
import sys
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import EPS, GRID_PT, lines_with_tags  # noqa: E402

TOP_CELLS = 300          # 72pt / 0.24pt = 300 格，正文区顶


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


def check_d0(lines):
    out = {"prediction": "D0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_d1(lines, probes, per_group):
    """D1：存在常数 d 使 c(n) = ⌊300 + (n+1) × L − d⌋。按顺序累积区间交集。"""
    out = {"prediction": "D1 存在常数 d 使 c(n) = ⌊300 + (n+1) × L − d⌋",
           "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": [],
           "dInterval": None}
    lo, hi = Fraction(-10**9), Fraction(10**9)
    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            out["undecidable"] += per_group
            out["n"] += per_group
            out["notes"].append({"tag": p["tag"], "lineTwips": p["lineTwips"],
                                 "reason": "该组未整组落在同一页，按采前声明排除"})
            continue
        L = Fraction(p["lineTwips"] * 10, 48)
        for n, m in enumerate(members):
            out["n"] += 1
            c = int(round(m["baseline"] / GRID_PT))
            # c ≤ 300 + (n+1)·L − d < c+1
            #   ⟹ d ∈ ((n+1)·L + 300 − c − 1, (n+1)·L + 300 − c]
            # 取半开区间 [lo, hi) 的写法：d ∈ [X − 1, X) 其中 X = (n+1)L + 300 − c，
            # 边界方向与上式相反，故用 (X−1, X]；这里按 [X−1, X) 近似不行，
            # 改为对 −d 做同样的半开处理：令 e = −d，则
            #   e ∈ [c − 300 − (n+1)·L, c + 1 − 300 − (n+1)·L)
            # 最后把 e 的区间取负还原成 d。
            nlo = Fraction(c - TOP_CELLS, 1) - (n + 1) * L
            nhi = Fraction(c + 1 - TOP_CELLS, 1) - (n + 1) * L
            new_lo, new_hi = max(lo, nlo), min(hi, nhi)
            if new_lo < new_hi:
                lo, hi = new_lo, new_hi
                out["ok"] += 1
            elif len(out["misses"]) < 6:
                out["misses"].append({
                    "tag": p["tag"], "lineTwips": p["lineTwips"], "n": n,
                    "reason": "d 的区间在这一条上变空",
                    "已满足条数": out["ok"],
                    "此前交集": f"[{float(lo):.6f}, {float(hi):.6f})",
                    "本条要求": f"[{float(nlo):.6f}, {float(nhi):.6f})"})
    if out["ok"] == out["n"] and out["n"]:
        # 上面累积的是 e = −d 的区间，还原成 d。
        out["dInterval"] = {"lo": str(-hi), "hi": str(-lo),
                            "loFloat": float(-hi), "hiFloat": float(-lo),
                            "loPt": float(-hi) * GRID_PT, "hiPt": float(-lo) * GRID_PT}
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
              "prereg": "docs/PREREG-2026-09-17-fixed-distance.md",
              "clueIndependence": "形式的线索取自 a-form 的失败走向（已见数据），按 §7.5 记为非独立检验",
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
        d0 = check_d0(lines)
        d1 = check_d1(lines, probes, per_group)
        result["predictions"] = {"D0": {**d0, "verdict": verdict(d0)},
                                 "D1": {**d1, "verdict": verdict(d1)}}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("D0", d0), ("D1", d1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:6]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        if d1["dInterval"]:
            di = d1["dInterval"]
            print(f"  d 的区间（另报，不作预测）："
                  f"[{di['loFloat']:.6f}, {di['hiFloat']:.6f}) 格 "
                  f"= [{di['loPt']:.4f}, {di['hiPt']:.4f}) pt")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
