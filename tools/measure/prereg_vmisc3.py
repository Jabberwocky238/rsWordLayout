#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-vmisc3.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判四条：R0 栅格、R1 分节边界、R2 `atLeast` 首行、R3 字符缩放。
R1 / R2 的线索都取自已见数据，判据文已按 §7.5 声明为非独立检验。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import EPS, GRID_PT, eq, lines_with_tags  # noqa: E402


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


def check_r0(lines):
    out = {"prediction": "R0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_r1(lines, probes):
    out = {"prediction": "R1 分节边界那一步比常规多 0.24pt", "n": 1, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    gaps = {}
    for p in probes:
        if p["kind"] != "section":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        gaps[p["hasSection"]] = (members[1]["baseline"] - members[0]["baseline"]
                                 if len(members) >= 2 else None)
        if len(members) < 2:
            out["notes"].append({"tag": p["tag"], "hasSection": p["hasSection"],
                                 "reason": "该组行数不足（可能整页归行判不了）",
                                 "行数": len(members)})
    a, b = gaps.get(False), gaps.get(True)
    if a is None or b is None:
        out["undecidable"] = 1
        return out
    if eq(b - a, GRID_PT):
        out["ok"] = 1
    else:
        out["misses"].append({"无分节差": round(a, 4), "有分节差": round(b, 4),
                              "实测增量": round(b - a, 4), "预测": GRID_PT,
                              "偏离": round((b - a) - GRID_PT, 6)})
    return out


def check_r2(lines, probes):
    out = {"prediction": "R2 exact 首行 − atLeast 首行 = 3 格", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    first = {}
    for p in probes:
        if p["kind"] != "at-least-first":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        first[(p["lineTwips"], p["rule"])] = (
            int(round(members[0]["baseline"] / GRID_PT)) if members else None)
    for ln in sorted({k[0] for k in first}):
        out["n"] += 1
        a, e = first.get((ln, "atLeast")), first.get((ln, "exact"))
        if a is None or e is None:
            out["undecidable"] += 1
            out["notes"].append({"lineTwips": ln, "reason": "两种规则里有一边取不到首行"})
            continue
        if e - a == 3:
            out["ok"] += 1
        else:
            out["misses"].append({"lineTwips": ln, "atLeast 格": a, "exact 格": e,
                                  "实测差": e - a, "预测": 3})
    return out


def check_r3(lines, probes):
    out = {"prediction": "R3 Δ(sc,i) = Δ(100,i) × sc/100", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    xs = {}
    for p in probes:
        if p["kind"] != "scale":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        xs[p["scalePercent"]] = ([g["origin"][0] for g in members[0]["glyphs"]]
                                 if len(members) == 1 else None)
    base = xs.get(100)
    for sc, cur in sorted(xs.items()):
        if sc == 100:
            continue
        gaps = (len(base) - 1) if base else 0
        out["n"] += gaps
        if base is None or cur is None or len(cur) != len(base):
            out["undecidable"] += gaps
            out["notes"].append({"scale": sc, "reason": "字形数与对照不等",
                                 "对照": None if base is None else len(base),
                                 "本组": None if cur is None else len(cur)})
            continue
        for i in range(gaps):
            want = (base[i + 1] - base[i]) * sc / 100.0
            got = cur[i + 1] - cur[i]
            if eq(got, want):
                out["ok"] += 1
            elif len(out["misses"]) < 8:
                out["misses"].append({"scale": sc, "i": i, "实测": round(got, 4),
                                      "预测": round(want, 4),
                                      "偏离": round(got - want, 6)})
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
              "prereg": "docs/PREREG-2026-09-17-vmisc3.md",
              "clueIndependence": "R1 / R2 的线索取自已见数据，按 §7.5 记为非独立检验",
              "bundle": args.bundle, "bundleState": model.get("state"),
              "coverage": model.get("coverage"), "falsifiers": fired}

    if fired:
        result["verdict"] = "VOID"
        print(f"采集包 state={result['bundleState']}  判定=VOID")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        result["verdict"] = "EVALUATED"
        rs = {"R0": check_r0(lines), "R1": check_r1(lines, probes),
              "R2": check_r2(lines, probes), "R3": check_r3(lines, probes)}
        result["predictions"] = {k: {**v, "verdict": verdict(v)} for k, v in rs.items()}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in rs.items():
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
