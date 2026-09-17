#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-size-free.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：Y0 栅格、Y1「`exact` 行距下基线位置与**字号**无关」。
Y1 **不依赖任何公式**——只比同一 (`line`, 行序号) 上四个字号的基线格数是否全等。
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
from prereg_shiftfloor import interval_for_A  # noqa: E402


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


def check_y0(lines):
    out = {"prediction": "Y0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_y1(lines, probes, per_group):
    """Y1：同一 (line, 行序号) 上，各**字号**的基线格数必须全等。"""
    out = {"prediction": "Y1 exact 行距下基线位置与字号无关", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": [], "intervals": {}}
    # 每组的格数序列
    seqs = {}
    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            out["notes"].append({"tag": p["tag"], "sizePt": p["sizeHalfPoints"]/2,
                                 "lineTwips": p["lineTwips"],
                                 "reason": "该组未整组落在同一页，按采前声明排除"})
            seqs[(p["lineTwips"], p["sizeHalfPoints"])] = None
            continue
        seqs[(p["lineTwips"], p["sizeHalfPoints"])] = [
            int(round(m["baseline"] / GRID_PT)) for m in members]
        lo, hi = interval_for_A(seqs[(p["lineTwips"], p["sizeHalfPoints"])],
                               Fraction(p["lineTwips"] * 10, 48))
        out["intervals"].setdefault(p["lineTwips"], {})[f'{p["sizeHalfPoints"]/2}pt'] = (
            f"[{float(lo % 1):.4f}, {float(hi % 1):.4f})" if lo < hi else "空")

    by_line = {}
    for (line, size), seq in seqs.items():
        by_line.setdefault(line, {})[f"{size/2}pt"] = seq

    for line, fams in sorted(by_line.items()):
        for n in range(per_group):
            out["n"] += 1
            vals = {f: (s[n] if s else None) for f, s in fams.items()}
            if any(v is None for v in vals.values()):
                out["undecidable"] += 1
                continue
            if len(set(vals.values())) == 1:
                out["ok"] += 1
            elif len(out["misses"]) < 10:
                out["misses"].append({"lineTwips": line, "n": n, "各字号格数": vals})
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
              "prereg": "docs/PREREG-2026-09-17-size-free.md",
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
        y0 = check_y0(lines)
        y1 = check_y1(lines, probes, per_group)
        result["predictions"] = {"Y0": {**y0, "verdict": verdict(y0)},
                                 "Y1": {**y1, "verdict": verdict(y1)}}
        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("Y0", y0), ("Y1", y1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for m in p["misses"][:8]:
                print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
            for note in p["notes"][:6]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        print("  A 的区间（另报，不作预测）：")
        for line, fams in sorted(y1["intervals"].items()):
            uniq = set(fams.values())
            print(f"      line={line:<5}{'各字号一致 ' + next(iter(uniq)) if len(uniq)==1 else fams}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
