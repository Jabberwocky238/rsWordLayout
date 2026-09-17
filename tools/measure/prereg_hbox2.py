#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-hbox2.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判三条：M0 栅格、M1 两端对齐、M2 字距（单位 **twips**）。
三条横向预测**都不含字体度量**——K1/K2 判「行尾贴不贴右边界」（常数），
K3 判「有无字距的差」（推进量相减时消掉）。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import (EPS, GRID_PT, eq, lines_with_tags,  # noqa: E402
                          paragraph_lines_for_tag)

RIGHT_EDGE_PT = 72.0 + 9026 / 20.0          # 72 + 451.3 = 523.3pt


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


def right_edge(line):
    """行尾的右缘 = 末字形 x + 末字形 advance。两者都是实测量。

    末尾那个字形是段落标记画的空格（§4），它也占推进量，所以要跳过它——
    Word 的行尾对齐看的是**可见内容**的右缘。这里取倒数第二个非空格字形。
    """
    gs = [g for g in line["glyphs"] if (g.get("text") or "").strip()]
    if not gs:
        return None
    last = gs[-1]
    return last["origin"][0] + last["advance"][0]


def check_k0(lines):
    out = {"prediction": "M0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_k1(lines, probes):
    out = {"prediction": f"K1 右对齐：行尾右缘 = {RIGHT_EDGE_PT}pt", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for p in probes:
        if p["kind"] != "right":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        out["n"] += 1
        if len(members) != 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": p["tag"], "reason": "该段不是恰好一行", "行数": len(members)})
            continue
        got = right_edge(members[0])
        if got is not None and eq(got, RIGHT_EDGE_PT):
            out["ok"] += 1
        else:
            out["misses"].append({"tag": p["tag"], "实测右缘": got,
                                  "预测": RIGHT_EDGE_PT,
                                  "差": None if got is None else round(got - RIGHT_EDGE_PT, 4)})
    return out


def check_k2(lines, probes, model=None, bundle=None):
    out = {"prediction": f"M1 两端对齐：非末行右缘 = {RIGHT_EDGE_PT}pt", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for p in probes:
        if p["kind"] != "justify":
            continue
        # 段落会换行，必须按源区间归行——按前缀只找得到第一行。
        members = paragraph_lines_for_tag(model, bundle, p["tag"])
        if len(members) < 2:
            out["notes"].append({"tag": p["tag"], "reason": "该组没有换行，没有非末行可判",
                                 "行数": len(members)})
            continue
        for i, line in enumerate(members[:-1]):        # 末行按采集前声明不判
            out["n"] += 1
            got = right_edge(line)
            if got is not None and eq(got, RIGHT_EDGE_PT):
                out["ok"] += 1
            else:
                out["misses"].append({"tag": p["tag"], "行": i, "实测右缘": got,
                                      "预测": RIGHT_EDGE_PT,
                                      "差": None if got is None else round(got - RIGHT_EDGE_PT, 4)})
    return out


def check_k3(lines, probes):
    out = {"prediction": "M2 字距：Δ(sp,i) − Δ(0,i) = sp/20 pt（twips）", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    xs = {}
    for p in probes:
        if p["kind"] != "letter-spacing":
            continue
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        xs[p["spacingHalfPoints"]] = (
            [g["origin"][0] for g in members[0]["glyphs"]] if len(members) == 1 else None)
    base = xs.get(0)
    for sp, cur in sorted(xs.items()):
        if sp == 0:
            continue
        gaps = (len(base) - 1) if base else 0
        out["n"] += gaps
        if base is None or cur is None or len(cur) != len(base):
            out["undecidable"] += gaps
            out["notes"].append({"spacing": sp, "reason": "字形数与对照不等，无法逐位相减",
                                 "对照": None if base is None else len(base),
                                 "本组": None if cur is None else len(cur)})
            continue
        for i in range(gaps):
            d = (cur[i + 1] - cur[i]) - (base[i + 1] - base[i])
            if eq(d, sp / 20.0):
                out["ok"] += 1
            elif len(out["misses"]) < 8:
                out["misses"].append({"spacing": sp, "i": i, "实测差": round(d, 4),
                                      "预测": sp / 20.0, "偏离": round(d - sp / 20.0, 6)})
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
    probes = json.loads(Path(args.probes).read_text())["probes"]
    probes_doc = json.loads(Path(args.probes).read_text())
    lines = lines_with_tags(model)

    fired = falsifiers(bundle, model, probes_doc)
    result = {"schema": "rsword-layout-prereg/1",
              "prereg": "docs/PREREG-2026-09-17-hbox2.md",
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
        ks = {"M0": check_k0(lines), "M1": check_k2(lines, probes, model, bundle),
              "M2": check_k3(lines, probes)}
        result["predictions"] = {k: {**v, "verdict": verdict(v)} for k, v in ks.items()}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in ks.items():
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:6]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
