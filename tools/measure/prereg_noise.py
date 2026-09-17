#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-noise-floor.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

**这一批不判 Word，判量具**：本该逐位相同的读数，实测差多少。
判两条：N1 行间、N2 行内。两条都按容差 0 判，**同时报出实测最大偏差**——
后者才是这一批真正要的数。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import eq, lines_with_tags, paragraph_lines_for_tag  # noqa: E402


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


def check_n1(lines, probes):
    """N1：同一文本各行的字形 x 逐位相同。"""
    out = {"prediction": "N1 行间：同一文本各行的字形 x 逐位相同", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": [], "maxDev": 0.0, "perFamily": {}}
    for p in probes:
        if p["kind"] != "between-lines":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        if len(members) != p["lines"]:
            out["notes"].append({"tag": p["tag"], "reason": "行数不对", "实得": len(members)})
            continue
        base = [g["origin"][0] for g in members[0]["glyphs"]]
        dev = 0.0
        for row in members[1:]:
            xs = [g["origin"][0] for g in row["glyphs"]]
            for i, (a, b) in enumerate(zip(base, xs)):
                out["n"] += 1
                if eq(a, b):
                    out["ok"] += 1
                else:
                    dev = max(dev, abs(a - b))
                    if len(out["misses"]) < 6:
                        out["misses"].append({"tag": p["tag"], "family": p["family"],
                                              "字形": i, "第0行": a, "本行": b,
                                              "差": round(b - a, 8)})
        out["perFamily"][p["family"]] = dev
        out["maxDev"] = max(out["maxDev"], dev)
    return out


def check_n2(lines, probes):
    """N2：同一字符组各遍的内部间隔相同。"""
    out = {"prediction": "N2 行内：同一字符组各遍的内部间隔相同", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": [], "maxDev": 0.0, "perFamily": {}}
    for p in probes:
        if p["kind"] != "within-line":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        if len(members) != 1:
            out["notes"].append({"tag": p["tag"], "reason": "不是恰好一行", "实得": len(members)})
            continue
        gs = members[0]["glyphs"]
        # 跳过标签那一个字形，其后是 repeats 遍的 unit。
        unit = len(p["unit"])
        start = len(p["tag"])
        gaps = []
        for r in range(p["repeats"]):
            a = start + r * unit
            if a + 1 < len(gs):
                gaps.append(gs[a + 1]["origin"][0] - gs[a]["origin"][0])
        if len(gaps) < 2:
            out["notes"].append({"tag": p["tag"], "reason": "字形不够，取不出间隔"})
            continue
        dev = 0.0
        for r, g in enumerate(gaps[1:], start=1):
            out["n"] += 1
            if eq(g, gaps[0]):
                out["ok"] += 1
            else:
                dev = max(dev, abs(g - gaps[0]))
                if len(out["misses"]) < 6:
                    out["misses"].append({"tag": p["tag"], "family": p["family"],
                                          "第几遍": r, "第0遍": gaps[0], "本遍": g,
                                          "差": round(g - gaps[0], 8)})
        out["perFamily"][p["family"]] = dev
        out["maxDev"] = max(out["maxDev"], dev)
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
              "prereg": "docs/PREREG-2026-09-17-noise-floor.md",
              "purpose": "判量具自己的噪声底，不判 Word",
              "bundle": args.bundle, "bundleState": model.get("state"),
              "coverage": model.get("coverage"), "falsifiers": fired}

    if fired:
        result["verdict"] = "VOID"
        print(f"采集包 state={result['bundleState']}  判定=VOID")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        result["verdict"] = "EVALUATED"
        ns = {"N1": check_n1(lines, probes), "N2": check_n2(lines, probes)}
        result["predictions"] = {k: {**v, "verdict": verdict(v)} for k, v in ns.items()}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in ns.items():
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}  {p['prediction']}")
            print(f"        **实测最大偏差 {p['maxDev']:.9f} pt**"
                  f"   逐族：{ {k: round(v, 9) for k, v in p['perFamily'].items()} }")
            for m in p["misses"][:4]:
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
