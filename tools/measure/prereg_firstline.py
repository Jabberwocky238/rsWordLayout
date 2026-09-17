#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-first-line.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：T0 基线仍在栅格上、T1 每页第一条基线的形式（本批的点预测）。
另报一个**不作预测**的对照：accumulation 里已判假的旧形式，只为让
「新的有没有比旧的好」有数可依——分高也不改判它成立。

用法：

    ./.venv/bin/python prereg_firstline.py <采集包> --probes fixtures/first-line.probes.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import (CONTENT_TOP_PT, EPS, GRID_PT, descent_pt, eq,  # noqa: E402
                          hhea, lines_with_tags, natural_pt, quantize)
from make_firstline_fixture import FONTS  # noqa: E402


def falsifiers(bundle, model, probes_doc):
    fired = []
    meta = bundle["META"]
    pre = (meta.get("preflight") or {}).get("result")
    if pre != "PASS":
        fired.append({"id": "F-A", "detail": f"采前核查 result={pre!r}"})

    # F-B **直接采信采集包里的核查结论**，不在这里另算一遍。
    #
    # 判据文说的是「采后 PDF 里出现申请之外的字体名」。「叫什么」这件事只有字体文件
    # 说了算：`Bodoni 72 Smallcaps Book` 声明的 PostScript 名是
    # `BodoniSvtyTwoSCITCTT-Book`，而 PDF 子集名用的就是 PostScript 名。
    # 判据脚本自己拿族名做字符串包含，就会把没被替换的字体判成替换——
    # 第一次采集正是这么废掉的（见 `captures/first-line-2026-09-17-void/`）。
    #
    # 采集侧的核查读字体文件的 name 表，并且**多查一问**：PDF 里有没有账面之外的名字。
    # 那一问才是「有没有被替换」。这里读它的结论，重复实现只会再错一次。
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


def check_t0(lines):
    out = {"prediction": "T0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def first_line_scores(lines, probes):
    """T1（点预测）与对照旧形式，在同一批读数上各算一遍。"""
    def new_form(m, size):
        return quantize(CONTENT_TOP_PT + natural_pt(m, size) - quantize(descent_pt(m, size)))

    def old_form(m, size):
        return quantize(CONTENT_TOP_PT + (m["ascent"] + m["lineGap"]) / m["upem"] * size)

    t1 = {"prediction": "T1 b₀ = quantize(72 + natural − 量化后的 descent)",
          "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    old = {"form": "对照（不作预测）：quantize(72 + (ascender + lineGap)/upem × 字号)",
           "n": 0, "ok": 0}

    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        t1["n"] += 1
        old["n"] += 1
        if len(members) != p["lines"] or len({m["page"] for m in members}) != 1:
            t1["undecidable"] += 1
            t1["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除",
                                "行数": len(members), "页": sorted({m["page"] for m in members})})
            continue
        m = hhea(Path(FONTS[p["family"]]))
        size = p["sizeHalfPoints"] / 2.0
        got = members[0]["baseline"]
        if eq(got, new_form(m, size)):
            t1["ok"] += 1
        else:
            t1["misses"].append({"tag": p["tag"], "family": p["family"], "sizePt": size,
                                 "实测": got, "预测": round(new_form(m, size), 4),
                                 "差": round(got - new_form(m, size), 4)})
        if eq(got, old_form(m, size)):
            old["ok"] += 1
    return t1, old


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
              "prereg": "docs/PREREG-2026-09-17-first-line.md",
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
        t0 = check_t0(lines)
        t1, old = first_line_scores(lines, probes)
        result["predictions"] = {"T0": {**t0, "verdict": verdict(t0)},
                                 "T1": {**t1, "verdict": verdict(t1)}}
        result["control"] = old

        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("T0", t0), ("T1", t1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for miss in p["misses"][:8]:
                print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        print(f"  对照（不作预测）{old['ok']}/{old['n']}  {old['form']}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
