#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-vmisc.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判四条：P0 栅格、P1 段落间距、P2 `w:position`、P3 `atLeast`。
三条实质判据**都用差值绕开了尚未解出的 A**。
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


def check_p0(lines):
    out = {"prediction": "P0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_p1(lines, probes):
    """P1：Δ(sp) − Δ(0) = sp/20 pt。A 在相减时消掉。"""
    out = {"prediction": "P1 段落间距：Δ(sp) − Δ(0) = sp/20 pt", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    deltas = {}
    for p in probes:
        if p["kind"] != "space-after":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        if len(members) != p["lines"] or len({m["page"] for m in members}) != 1:
            deltas[p["spaceAfter"]] = None
            out["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页"})
            continue
        deltas[p["spaceAfter"]] = [members[k + 1]["baseline"] - members[k]["baseline"]
                                   for k in range(len(members) - 1)]
    base = deltas.get(0)
    for sp, cur in sorted(deltas.items()):
        if sp == 0:
            continue
        pairs = len(base) if base else 0
        out["n"] += pairs
        if base is None or cur is None:
            out["undecidable"] += pairs
            continue
        for i, (a, b) in enumerate(zip(base, cur)):
            if eq(b - a, sp / 20.0):
                out["ok"] += 1
            else:
                out["misses"].append({"spaceAfter": sp, "对": i,
                                      "无间距差": round(a, 4), "有间距差": round(b, 4),
                                      "实测增量": round(b - a, 4), "预测": sp / 20.0,
                                      "偏离": round((b - a) - sp / 20.0, 6)})
    return out


def check_p2(lines, probes):
    """P2：同一行内 y_normal − y_raised = val/2 pt。一行之内 A 不参与。"""
    out = {"prediction": "P2 w:position：y_normal − y_raised = val/2 pt", "n": 0,
           "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    for p in probes:
        if p["kind"] != "position" or p["positionHalfPoints"] == 0:
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        out["n"] += 1
        if len(members) != 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "不是恰好一行"})
            continue
        ys = [g["origin"][1] for g in members[0]["glyphs"]]
        # 标签 + "nn" 是普通 run，其后 "rr" 是抬升 run。
        head = len(p["tag"]) + 2
        if len(ys) < head + 2:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "字形不够"})
            continue
        got = ys[0] - ys[head]
        pred = p["positionHalfPoints"] / 2.0
        if eq(got, pred):
            out["ok"] += 1
        else:
            out["misses"].append({"tag": p["tag"], "val": p["positionHalfPoints"],
                                  "y_normal": ys[0], "y_raised": ys[head],
                                  "实测差": round(got, 4), "预测": pred,
                                  "偏离": round(got - pred, 6)})
    return out


def check_p3(lines, probes):
    """P3：line 够大时 atLeast 与 exact 给出同一条序列。"""
    out = {"prediction": "P3 atLeast：line 大于自然行高时与 exact 同解", "n": 0,
           "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    seqs = {}
    for p in probes:
        if p["kind"] != "at-least":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        seqs[(p["lineTwips"], p["rule"])] = (
            [int(round(m["baseline"] / GRID_PT)) for m in members]
            if len(members) == p["lines"] and len({m["page"] for m in members}) == 1
            else None)
    for ln in sorted({k[0] for k in seqs}):
        a, b = seqs.get((ln, "atLeast")), seqs.get((ln, "exact"))
        rows = len(b) if b else (len(a) if a else 0)
        out["n"] += rows
        if a is None or b is None or len(a) != len(b):
            out["undecidable"] += rows
            out["notes"].append({"lineTwips": ln, "reason": "两种规则的行数对不上或未整组同页"})
            continue
        for n, (x, y) in enumerate(zip(a, b)):
            if x == y:
                out["ok"] += 1
            elif len(out["misses"]) < 8:
                out["misses"].append({"lineTwips": ln, "行": n,
                                      "atLeast 格数": x, "exact 格数": y, "差": x - y})
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
              "prereg": "docs/PREREG-2026-09-17-vmisc.md",
              "bundle": args.bundle, "bundleState": model.get("state"),
              "coverage": model.get("coverage"), "falsifiers": fired}

    if fired:
        result["verdict"] = "VOID"
        print(f"采集包 state={result['bundleState']}  判定=VOID")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        result["verdict"] = "EVALUATED"
        ps = {"P0": check_p0(lines), "P1": check_p1(lines, probes),
              "P2": check_p2(lines, probes), "P3": check_p3(lines, probes)}
        result["predictions"] = {k: {**v, "verdict": verdict(v)} for k, v in ps.items()}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in ps.items():
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
