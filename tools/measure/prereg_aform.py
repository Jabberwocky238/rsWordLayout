#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-a-form.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：Z0 栅格、Z1「存在常数 k 使 `c(n) = ⌊300 + (k+n) × L⌋`」。

Z1 **不是拟合**：不挑「最好的 k」，只问 160 条基线给出的区间交集空不空。
而且 k 是**跨全部 16 组共享的一个常数**——16 个不同的 `w:line` 必须被同一个 k 解释。
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


def check_z0(lines):
    out = {"prediction": "Z0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_z1(lines, probes, per_group):
    """Z1：存在常数 k 使 c(n) = ⌊300 + (k+n) × L⌋。按顺序累积区间交集。"""
    out = {"prediction": "Z1 存在常数 k 使 c(n) = ⌊300 + (k+n) × L⌋",
           "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": [],
           "kInterval": None}
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
            # c ≤ 300 + (k+n)·L < c+1  ⟹  k ∈ [(c−300)/L − n, (c+1−300)/L − n)
            nlo = Fraction(c - TOP_CELLS, 1) / L - n
            nhi = Fraction(c + 1 - TOP_CELLS, 1) / L - n
            new_lo, new_hi = max(lo, nlo), min(hi, nhi)
            if new_lo < new_hi:
                lo, hi = new_lo, new_hi
                out["ok"] += 1
            elif len(out["misses"]) < 6:
                out["misses"].append({
                    "tag": p["tag"], "lineTwips": p["lineTwips"], "n": n,
                    "reason": "k 的区间在这一条上变空",
                    "已满足条数": out["ok"],
                    "此前交集": f"[{float(lo):.6f}, {float(hi):.6f})",
                    "本条要求": f"[{float(nlo):.6f}, {float(nhi):.6f})"})
    if out["ok"] == out["n"] and out["n"]:
        out["kInterval"] = {"lo": str(lo), "hi": str(hi),
                            "loFloat": float(lo), "hiFloat": float(hi)}
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
              "prereg": "docs/PREREG-2026-09-17-a-form.md",
              "clueIndependence": "形式的线索取自已见数据（判据文 §1），按 §7.5 记为非独立检验",
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
        z0 = check_z0(lines)
        z1 = check_z1(lines, probes, per_group)
        result["predictions"] = {"Z0": {**z0, "verdict": verdict(z0)},
                                 "Z1": {**z1, "verdict": verdict(z1)}}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("Z0", z0), ("Z1", z1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:6]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        if z1["kInterval"]:
            ki = z1["kInterval"]
            print(f"  k 的区间（另报，不作预测）：[{ki['loFloat']:.6f}, {ki['hiFloat']:.6f})")
            print(f"      前三批（已见数据）给的是 [0.806897, 0.813559)，"
                  f"{'相交' if ki['loFloat'] < 0.813559 and ki['hiFloat'] > 0.806897 else '**不相交**'}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
