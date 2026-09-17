#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-indent-align.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判三条：G0 栅格、G1 缩进、G2 对齐。
G1 / G2 **都不依赖字体度量**——G2 靠三种对齐相减把行宽消掉。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import EPS, GRID_PT, eq, lines_with_tags  # noqa: E402

LEFT_MARGIN_PT = 72.0


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


def check_g0(lines):
    out = {"prediction": "G0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def first_x(line):
    return line["glyphs"][0]["origin"][0]


def check_g1(lines, probes):
    """G1：行首 x = 72pt + 左缩进（首行再加首行缩进）。"""
    out = {"prediction": "G1 行首 x = 72pt + 左缩进（+ 首行缩进）", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for p in probes:
        if p["kind"] != "indent":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        for k in range(p["lines"]):
            out["n"] += 1
            if k >= len(members):
                out["undecidable"] += 1
                out["notes"].append({"tag": p["tag"], "行": k, "reason": "缺这一行"})
                continue
            # 每段就是一行，所以第 k 段的那一行既是段首行。
            pred = LEFT_MARGIN_PT + (p["indLeft"] + p["indFirst"]) / 20.0
            got = first_x(members[k])
            if eq(got, pred):
                out["ok"] += 1
            else:
                out["misses"].append({"tag": p["tag"], "行": k,
                                      "indLeft": p["indLeft"], "indFirst": p["indFirst"],
                                      "实测": got, "预测": round(pred, 4),
                                      "差": round(got - pred, 4)})
    return out


def check_g2(lines, probes):
    """G2：x_right − x_left = 2 × (x_center − x_left)。行宽自己消掉。"""
    out = {"prediction": "G2 x_right − x_left = 2 × (x_center − x_left)", "n": 0,
           "ok": 0, "undecidable": 0, "misses": [], "notes": [], "slack": {}}
    by_text = {}
    for p in probes:
        if p["kind"] != "align":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        by_text.setdefault(p["textIndex"], {})[p["align"]] = (
            first_x(members[0]) if len(members) == 1 else None)
    for idx, xs in sorted(by_text.items()):
        out["n"] += 1
        if any(v is None for v in xs.values()) or len(xs) != 3:
            out["undecidable"] += 1
            out["notes"].append({"textIndex": idx, "reason": "该段没有恰好三种对齐各一行"})
            continue
        dl = xs["right"] - xs["left"]
        dc = xs["center"] - xs["left"]
        if eq(dl, 2 * dc):
            out["ok"] += 1
            out["slack"][idx] = {"avail−w": round(dl, 4), "居中偏移": round(dc, 4)}
        else:
            out["misses"].append({"textIndex": idx, "x_left": xs["left"],
                                  "x_center": xs["center"], "x_right": xs["right"],
                                  "右−左": round(dl, 4), "2×(中−左)": round(2 * dc, 4),
                                  "差": round(dl - 2 * dc, 4)})
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
              "prereg": "docs/PREREG-2026-09-17-indent-align.md",
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
        g0 = check_g0(lines)
        g1 = check_g1(lines, probes)
        g2 = check_g2(lines, probes)
        result["predictions"] = {k: {**v, "verdict": verdict(v)}
                                 for k, v in (("G0", g0), ("G1", g1), ("G2", g2))}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("G0", g0), ("G1", g1), ("G2", g2)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:8]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        if g2["slack"]:
            print("  对齐余量（另报，不作预测）：")
            for idx, v in g2["slack"].items():
                print(f"      文本 {idx}: {json.dumps(v, ensure_ascii=False)}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
