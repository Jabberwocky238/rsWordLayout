#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-accumulation.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判三条：Q0 基线都在栅格上、Q1 累加律、Q2 每页第一条基线。
证否条件只剩量具自身那四条——**没有「两个读数不等即作废」**，
上一批正是那一条把真实现象判成了故障。

用法：

    ./.venv/bin/python prereg_accum.py <采集包> --probes fixtures/accumulation.probes.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import (CONTENT_TOP_PT, EPS, GRID_PT, eq, hhea,  # noqa: E402
                          lines_with_tags, natural_pt, quantize)

FONT_FILES = {
    "Georgia": "/System/Library/Fonts/Supplemental/Georgia.ttf",
    "Verdana": "/System/Library/Fonts/Supplemental/Verdana.ttf",
    "Tahoma": "/System/Library/Fonts/Supplemental/Tahoma.ttf",
    "Comic Sans MS": "/System/Library/Fonts/Supplemental/Comic Sans MS.ttf",
    "Trebuchet MS": "/System/Library/Fonts/Supplemental/Trebuchet MS.ttf",
}


def ascent_pt(m: dict, size_pt: float) -> float:
    """ascent + lineGap，即基线到行顶。"""
    return (m["ascent"] + m["lineGap"]) / m["upem"] * size_pt


def groups_of(lines: list[dict], probes: list[dict]) -> list[tuple[dict, list[dict]]]:
    """按标签把行归组，保持文档顺序。"""
    out = []
    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        out.append((p, members))
    return out


def falsifiers(bundle: dict, model: dict, probes_doc: dict) -> list[dict]:
    """§4：只剩量具自身那四条。"""
    fired = []
    meta = bundle["META"]

    pre = (meta.get("preflight") or {}).get("result")
    if pre != "PASS":
        fired.append({"id": "F-A", "detail": f"采前核查 result={pre!r}"})

    subs = meta.get("fontSubstitution") or {}
    required = {f.replace("-", "").replace(" ", "").lower() for f in FONT_FILES}
    unexpected = [n for n in subs.get("pdfFontNames", [])
                  if not any(r in n.split("+", 1)[-1].replace("-", "").replace(" ", "").lower()
                             or n.split("+", 1)[-1].replace("-", "").replace(" ", "").lower() in r
                             for r in required)]
    if subs.get("result") != "PASS" or unexpected:
        fired.append({"id": "F-B", "detail": f"字体核查 {subs.get('result')!r}，"
                                             f"申请之外的字体名={unexpected}"})

    fixture = meta.get("fixture") or {}
    declared, actual = probes_doc.get("sha256"), (fixture.get("before") or {}).get("sha256")
    if not fixture.get("unchanged") or (declared and actual and declared != actual):
        fired.append({"id": "F-C", "detail": f"夹具身份不符：清单 {str(declared)[:12]}… "
                                             f"采集 {str(actual)[:12]}…"})

    if model.get("state") == UNDECIDABLE and "PAGE_SET_MISMATCH" in (model.get("reason") or ""):
        fired.append({"id": "F-D", "detail": model["reason"]})
    return fired


def check_q0(lines) -> dict:
    """Q0：每一条基线都落在 0.24pt 栅格上。"""
    out = {"prediction": "Q0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        else:
            out["misses"].append({"text": l["text"][:8], "baseline": y,
                                  "余数": round(y % GRID_PT, 6)})
    return out


def check_q1(lines, probes, per_group: int) -> dict:
    """Q1：以实测首条基线为锚，第 n 条 = quantize(b0 + n × natural)。"""
    out = {"prediction": "Q1 第 n 条基线 = quantize(b₀ + n × 自然行高)", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for p, members in groups_of(lines, probes):
        steps = per_group - 1
        out["n"] += steps
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            out["undecidable"] += steps
            out["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除",
                                 "行数": len(members),
                                 "页": sorted({m["page"] for m in members})})
            continue
        m = hhea(Path(FONT_FILES[p["family"]]))
        nat = natural_pt(m, p["sizeHalfPoints"] / 2.0)
        b0 = members[0]["baseline"]
        for n in range(1, per_group):
            pred = quantize(b0 + n * nat)
            got = members[n]["baseline"]
            if eq(got, pred):
                out["ok"] += 1
            elif len(out["misses"]) < 40:
                out["misses"].append({"tag": p["tag"], "n": n, "实测": got,
                                      "预测": round(pred, 4),
                                      "差": round(got - pred, 4)})
            else:
                out["misses"].append({"tag": p["tag"], "n": n, "略": True})
    return out


def check_q2(lines, probes, per_group: int) -> dict:
    """Q2：每页第一条基线 = quantize(正文顶 + ascent + lineGap)。"""
    out = {"prediction": "Q2 b₀ = quantize(72 + (ascender + lineGap)/upem × 字号)",
           "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    for p, members in groups_of(lines, probes):
        out["n"] += 1
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页"})
            continue
        m = hhea(Path(FONT_FILES[p["family"]]))
        size = p["sizeHalfPoints"] / 2.0
        pred = quantize(CONTENT_TOP_PT + ascent_pt(m, size))
        got = members[0]["baseline"]
        if eq(got, pred):
            out["ok"] += 1
        else:
            out["misses"].append({"tag": p["tag"], "family": p["family"], "sizePt": size,
                                  "实测": got, "预测": round(pred, 4),
                                  "差": round(got - pred, 4)})
    return out


def verdict(p: dict) -> str:
    if p["misses"]:
        return FAIL
    if p["undecidable"] or p["ok"] < p["n"]:
        return UNDECIDABLE
    return OK


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle")
    parser.add_argument("--probes", required=True)
    parser.add_argument("--output", "-o")
    args = parser.parse_args()

    bundle = capture.load_bundle(Path(args.bundle))
    model = wordmodel.build(bundle)
    probes_doc = json.loads(Path(args.probes).read_text())
    probes, per_group = probes_doc["probes"], probes_doc["linesPerGroup"]
    lines = lines_with_tags(model)

    fired = falsifiers(bundle, model, probes_doc)
    result = {
        "schema": "rsword-layout-prereg/1",
        "prereg": "docs/PREREG-2026-09-17-accumulation.md",
        "bundle": args.bundle,
        "bundleState": model.get("state"),
        "coverage": model.get("coverage"),
        "lineDenominators": model.get("lineDenominators"),
        "falsifiers": fired,
    }
    if fired:
        result["verdict"] = "VOID"
        result["predictions"] = {}
    else:
        result["verdict"] = "EVALUATED"
        result["predictions"] = {
            k: {**v, "verdict": verdict(v)} for k, v in (
                ("Q0", check_q0(lines)),
                ("Q1", check_q1(lines, probes, per_group)),
                ("Q2", check_q2(lines, probes, per_group)),
            )
        }

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")

    print(f"采集包 state={result['bundleState']}  判定={result['verdict']}")
    if fired:
        print("§4 证否条件触发，本批读数全部作废：")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        for key, p in result["predictions"].items():
            print(f"  {key} {p['verdict']:<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for miss in p["misses"][:8]:
                print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
    if args.output:
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
