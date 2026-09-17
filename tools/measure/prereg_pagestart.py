#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-page-start.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：P0 基线仍在栅格上、P1 起页方式不影响首行基线。
**P1 不依赖任何公式**——它只问「强制分页起的首行」与「自然溢出起的首行」是否相等。
另报 T1 在两类首行上的得分（**不作预测**）。
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
from make_pagestart_fixture import FONTS  # noqa: E402


def t1_form(m, size_pt):
    """目前最好、但已判假的形式，只作对照。"""
    nat = natural_pt(m, size_pt)
    des = m["descent"] / m["upem"] * size_pt
    return quantize(CONTENT_TOP_PT + nat - quantize(des))


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
    """P1：同组内，自然溢出起的首行 − 强制分页起的首行 = 0。"""
    p1 = {"prediction": "P1 起页方式不影响首行基线（两者之差 = 0.0000pt）",
          "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    ctl = {"form": "对照（不作预测）：T1 在两类首行上的命中",
           "forcedOk": 0, "forcedN": 0, "flowOk": 0, "flowN": 0}

    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        p1["n"] += 1
        pages = sorted({m["page"] for m in members})
        if len(pages) != 2:
            p1["undecidable"] += 1
            p1["notes"].append({"tag": p["tag"], "reason": "该组没有恰好跨 2 页，按采前声明排除",
                                "页": pages, "段数": len(members)})
            continue
        # 每页的首行 = 该页里文档顺序最靠前的那条。
        first = {}
        for m_ in members:
            first.setdefault(m_["page"], m_)
        forced, flow = first[pages[0]], first[pages[1]]
        diff = round(flow["baseline"] - forced["baseline"], 6)
        if eq(diff, 0.0):
            p1["ok"] += 1
        else:
            p1["misses"].append({"tag": p["tag"], "family": p["family"],
                                 "sizePt": p["sizeHalfPoints"] / 2.0,
                                 "强制分页起": forced["baseline"],
                                 "自然溢出起": flow["baseline"], "差": diff})

        m = hhea(Path(FONTS[p["family"]]))
        pred = t1_form(m, p["sizeHalfPoints"] / 2.0)
        ctl["forcedN"] += 1
        ctl["flowN"] += 1
        ctl["forcedOk"] += eq(forced["baseline"], pred)
        ctl["flowOk"] += eq(flow["baseline"], pred)
    return p1, ctl


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
              "prereg": "docs/PREREG-2026-09-17-page-start.md",
              "clueIndependence": "线索独立于已见读数（判据文 §0）",
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
        p0 = check_p0(lines)
        p1, ctl = check_p1(lines, probes)
        result["predictions"] = {"P0": {**p0, "verdict": verdict(p0)},
                                 "P1": {**p1, "verdict": verdict(p1)}}
        result["control"] = ctl

        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("P0", p0), ("P1", p1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for miss in p["misses"][:10]:
                print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
            for note in p["notes"][:6]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        print(f"  对照（不作预测）T1 命中：强制分页起 {ctl['forcedOk']}/{ctl['forcedN']}，"
              f"自然溢出起 {ctl['flowOk']}/{ctl['flowN']}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
