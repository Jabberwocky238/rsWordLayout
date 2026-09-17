#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-breaks-sections.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判三条：S0 栅格、S1 分页符三分的计数、S2 分节边界多一格。
两项此前都卡在量具的构造标注缺口上，`9dc3077` 修好后解锁。
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


def check_s0(lines):
    out = {"prediction": "S0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_s1(model, probes):
    """S1：涉及分页符的行记录，计数模型与实测相符。"""
    out = {"prediction": "S1 分页符三分：计数模型与实测相符", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": [], "byKind": {}}
    tags = {p["tag"]: p["kind"] for p in probes if p["kind"].startswith("break-")}
    for page in model.get("pages", []):
        for line in page.get("lines", []):
            text = line.get("text") or ""
            kind = next((k for t, k in tags.items() if text.startswith(t)), None)
            # 分页符那一行本身可能不带标签（独占一行时文本只有 \x0c）。
            if kind is None and "\x0c" not in text:
                continue
            out["n"] += 1
            key = kind or "break-line-itself"
            slot = out["byKind"].setdefault(key, {"ok": 0, "fail": 0, "und": 0})
            if line.get("state") == OK:
                out["ok"] += 1
                slot["ok"] += 1
            elif line.get("state") == UNDECIDABLE:
                out["undecidable"] += 1
                slot["und"] += 1
                if len(out["notes"]) < 4:
                    out["notes"].append({"text": repr(text[:10]), "kind": key,
                                         "reasons": line.get("reasons")})
            else:
                slot["fail"] += 1
                if len(out["misses"]) < 6:
                    out["misses"].append({"text": repr(text[:10]), "kind": key,
                                          "expected": line.get("expected"),
                                          "实得": len(line.get("glyphs") or []),
                                          "reasons": line.get("reasons")})
    return out


def check_s2(lines, probes):
    """S2：分节边界那一步比常规多 0.24pt。三对。"""
    out = {"prediction": "S2 分节边界那一步比常规多 0.24pt", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    gaps = {}
    for p in probes:
        if p["kind"] != "section":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        gaps[(p["pairIndex"], p["hasSection"])] = (
            members[1]["baseline"] - members[0]["baseline"] if len(members) >= 2 else None)
        if len(members) < 2:
            out["notes"].append({"tag": p["tag"], "pair": p["pairIndex"],
                                 "hasSection": p["hasSection"],
                                 "reason": "该组行数不足", "行数": len(members)})
    for pair in sorted({k[0] for k in gaps}):
        out["n"] += 1
        a, b = gaps.get((pair, False)), gaps.get((pair, True))
        if a is None or b is None:
            out["undecidable"] += 1
            continue
        if eq(b - a, GRID_PT):
            out["ok"] += 1
        else:
            out["misses"].append({"pair": pair, "无分节差": round(a, 4),
                                  "有分节差": round(b, 4),
                                  "实测增量": round(b - a, 4), "预测": GRID_PT,
                                  "偏离": round((b - a) - GRID_PT, 6)})
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
              "prereg": "docs/PREREG-2026-09-17-breaks-sections.md",
              "clueIndependence": "S2 的线索取自已见数据（n=1 旧观察），按 §7.5 记为非独立检验",
              "bundle": args.bundle, "bundleState": model.get("state"),
              "coverage": model.get("coverage"),
              "lineDenominators": model.get("lineDenominators"),
              "marksCheck": (bundle["sweep"].get("marksCheck") or {}).get("state"),
              "falsifiers": fired}

    if fired:
        result["verdict"] = "VOID"
        print(f"采集包 state={result['bundleState']}  判定=VOID")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        result["verdict"] = "EVALUATED"
        ss = {"S0": check_s0(lines), "S1": check_s1(model, probes),
              "S2": check_s2(lines, probes)}
        result["predictions"] = {k: {**v, "verdict": verdict(v)} for k, v in ss.items()}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED"
              f"   构造标注核查={result['marksCheck']}")
        for key, p in ss.items():
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            if key == "S1" and p.get("byKind"):
                for k, v in p["byKind"].items():
                    print(f"        {k:<20}{v}")
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
