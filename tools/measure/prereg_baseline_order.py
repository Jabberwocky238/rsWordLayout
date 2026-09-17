#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-baseline-order.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判两条：U0 基线仍在栅格上、U1 每页第一条基线用 OS/2 `usWinAscent`。
另报一个**不作预测**的对照（hhea 形式，已在两批判假）。

**线索不纯净**：本批假设一部分来自已见数据，判据文 §1 写明了，结论只能算初步。
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import (CONTENT_TOP_PT, EPS, GRID_PT, eq, hhea,  # noqa: E402
                          lines_with_tags, quantize)
from make_baseline2_fixture import FONTS  # noqa: E402


def os2_win(path) -> tuple[int, int]:
    """OS/2 的 `usWinAscent` / `usWinDescent`。直接读表，不经任何库。"""
    data = Path(path).read_bytes()
    count = struct.unpack(">H", data[4:6])[0]
    tables = {}
    for i in range(count):
        at = 12 + 16 * i
        tables[data[at : at + 4].decode("latin1")] = struct.unpack(
            ">II", data[at + 8 : at + 16]
        )[0]
    o = tables["OS/2"]
    return struct.unpack(">HH", data[o + 74 : o + 78])


def falsifiers(bundle, model, probes_doc):
    fired = []
    meta = bundle["META"]
    pre = (meta.get("preflight") or {}).get("result")
    if pre != "PASS":
        fired.append({"id": "F-A", "detail": f"采前核查 result={pre!r}"})

    # F-B 直接采信采集包的核查结论——它读字体文件的 name 表，
    # 判据脚本自己再算一遍只会把族名与 PostScript 名的差别再错一次
    # （见 `captures/first-line-2026-09-17-void/`）。
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


def check_u0(lines):
    out = {"prediction": "U0 每条基线都在 0.24pt 栅格上", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for l in lines:
        out["n"] += 1
        y = l["baseline"]
        if abs(y / GRID_PT - round(y / GRID_PT)) <= EPS:
            out["ok"] += 1
        elif len(out["misses"]) < 12:
            out["misses"].append({"text": l["text"][:6], "baseline": y})
    return out


def check_u1(lines, probes):
    u1 = {"prediction": "U1 b₀ = quantize(72 + usWinAscent/upem × 字号)",
          "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": []}
    ctl = {"form": "对照（不作预测）：quantize(72 + (hhea.ascender + lineGap)/upem × 字号)",
           "n": 0, "ok": 0, "separating": 0, "separatingOk": 0}

    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        u1["n"] += 1
        ctl["n"] += 1
        if len(members) != p["lines"] or len({m["page"] for m in members}) != 1:
            u1["undecidable"] += 1
            u1["notes"].append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除"})
            continue
        path = FONTS[p["family"]]
        m = hhea(Path(path))
        wa, _wd = os2_win(path)
        size = p["sizeHalfPoints"] / 2.0
        pred = quantize(CONTENT_TOP_PT + wa / m["upem"] * size)
        old = quantize(CONTENT_TOP_PT + (m["ascent"] + m["lineGap"]) / m["upem"] * size)
        got = members[0]["baseline"]

        if not eq(pred, old):
            ctl["separating"] += 1
            if eq(got, pred):
                ctl["separatingOk"] += 1
        if eq(got, pred):
            u1["ok"] += 1
        else:
            u1["misses"].append({"tag": p["tag"], "family": p["family"], "sizePt": size,
                                 "实测": got, "预测": round(pred, 4),
                                 "差": round(got - pred, 4),
                                 "两表同解": eq(pred, old)})
        if eq(got, old):
            ctl["ok"] += 1
    return u1, ctl


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
              "prereg": "docs/PREREG-2026-09-17-baseline-order.md",
              "clueIndependence": "线索部分取自已见数据（判据文 §1），按 §7.5 记为非独立检验",
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
        u0 = check_u0(lines)
        u1, ctl = check_u1(lines, probes)
        result["predictions"] = {"U0": {**u0, "verdict": verdict(u0)},
                                 "U1": {**u1, "verdict": verdict(u1)}}
        result["control"] = ctl

        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        for key, p in (("U0", u0), ("U1", u1)):
            print(f"  {key} {verdict(p):<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for miss in p["misses"][:10]:
                print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
            for note in p["notes"][:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
        print(f"  对照（不作预测）{ctl['ok']}/{ctl['n']}  {ctl['form']}")
        print(f"  其中能分开两张表的 {ctl['separating']} 组里，U1 中 {ctl['separatingOk']}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
