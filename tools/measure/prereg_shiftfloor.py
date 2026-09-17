#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-shift-floor.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：U0 栅格、U1「存在实数 A 使 c(n) = ⌊A + n × step⌋」。

U1 **不是拟合**：它不挑「最好的 A」，只问 20 个半开区间的交集空不空——二值、精确。
全程有理数，一点浮点都不沾。
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
from make_shiftfloor_fixture import FONTS  # noqa: E402


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


def check_u0(lines):
    out = {"prediction": "U0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def interval_for_A(cells, step):
    """20 条基线把 A 限到的区间交集。返回 (lo, hi)，`lo < hi` 即存在。

    第 n 条要求 `c(n) ≤ A + n×step < c(n)+1`，即 `A ∈ [c(n) − n·step, c(n)+1 − n·step)`。
    """
    lo = Fraction(-10**12)
    hi = Fraction(10**12)
    for n, c in enumerate(cells):
        lo = max(lo, Fraction(c) - n * step)
        hi = min(hi, Fraction(c + 1) - n * step)
    return lo, hi


def check_u1(lines, probes, per_group):
    out = {"prediction": "U1 存在实数 A 使 c(n) = ⌊A + n × step⌋", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": [], "intervals": {}}
    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        out["n"] += 1
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除"})
            continue
        cells = [int(round(m["baseline"] / GRID_PT)) for m in members]
        step = Fraction(p["lineTwips"] * 10, 48)
        lo, hi = interval_for_A(cells, step)
        if lo < hi:
            out["ok"] += 1
            # 报出 A 的区间（模 1 之后更好读），供下一批写 A 的形式用。
            out["intervals"][f"{p['tag']} {p['family']} line={p['lineTwips']}"] = {
                "lo": str(lo), "hi": str(hi),
                "loMod1": float(lo % 1), "hiMod1": float(hi % 1)}
        else:
            out["misses"].append({"tag": p["tag"], "family": p["family"],
                                  "lineTwips": p["lineTwips"],
                                  "reason": "A 的区间交集为空", "lo": str(lo), "hi": str(hi)})
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
              "prereg": "docs/PREREG-2026-09-17-shift-floor.md",
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
        u0 = check_u0(lines)
        u1 = check_u1(lines, probes, per_group)
        result["predictions"] = {"U0": {**u0, "verdict": verdict(u0)},
                                 "U1": {**u1, "verdict": verdict(u1)}}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("U0", u0), ("U1", u1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:8]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        if u1["intervals"]:
            print("  A 的区间（另报，不作预测）：")
            for k, v in list(u1["intervals"].items())[:8]:
                print(f"      {k:<42}A mod 1 ∈ [{v['loMod1']:.4f}, {v['hiMod1']:.4f})")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
