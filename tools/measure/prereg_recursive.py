#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-recursive.md`）。

与判据文同一次提交，**在采集之前**（量具方法 §7.1）。

判三条：R0 栅格、R1 前提检验（偶数步）、R2 点预测（半数进位）。
**R1 判假则 R2 判不了**——这一条采集前就写死，免得把「式子错了」与
「进位规则错了」两种失败混为一谈。
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402
from prereg_probe import EPS, GRID_PT, eq, lines_with_tags  # noqa: E402
from make_recursive_fixture import FONTS  # noqa: E402


# 四种进位规则，输入是**有理数** (num, den) 表示的格数，输出整数格数。
# 用有理数而不是浮点：判的正是「恰好在半格」这类边界，浮点会把边界糊掉。


def q_half_up(num, den):     # 半数进位（远离零）——本批的点预测
    return (num * 2 + den) // (den * 2)


def q_half_even(num, den):   # 半数取偶（banker's）
    lo, r = divmod(num, den)
    if r * 2 < den:
        return lo
    if r * 2 > den:
        return lo + 1
    return lo if lo % 2 == 0 else lo + 1


def q_floor(num, den):       # 截断
    return num // den


def q_ceil(num, den):        # 向上
    return -((-num) // den)


RULES = {"进位": q_half_up, "取偶": q_half_even, "截断": q_floor, "向上": q_ceil}
POINT = "递推·进位"


def series(b0, p, per_group, rule, recursive):
    """一组里从第 1 到第 per_group-1 条基线的预测（格数，整数）。

    `recursive=True` 走递推：**用预测值往下递推**，不是每步拿实测值续算——
    那样每步只预测一步，太容易中，判不出机制。
    """
    g0 = int(round(b0 / GRID_PT))
    step_num, step_den = p["lineTwips"] * 10, 48
    out = []
    if recursive:
        y = g0
        for _ in range(1, per_group):
            y = rule(y * step_den + step_num, step_den)
            out.append(y)
    else:
        for k in range(1, per_group):
            out.append(rule(g0 * step_den + k * step_num, step_den))
    return out


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


def cells_exact(b0, p, k):
    """第 k 条基线的精确位置，以**格**为单位，返回 (分子, 分母=48)。

    全程整数，**一点浮点都不用**：`b₀` 落在栅格上（栅格七批成立），
    记它是第 g₀ 格；增量 `k × line/20` pt 换成格是 `k × line × 10 / 48`。
    于是精确位置 = (g₀ × 48 + k × line × 10) / 48。

    非整数算不可：判据要区分的正是「恰好落在格点上」与「差一点」，
    而浮点误差的量级恰好能把这两者混起来——自测里 floor / ceil 就是这么
    把本该同解的步判成不同解的。
    """
    g0 = int(round(b0 / GRID_PT))
    return g0 * 48 + k * p["lineTwips"] * 10, 48


def on_grid_exactly(p, k):
    """第 k 步的精确增量是不是正落在栅格点上。整数判断，无浮点。"""
    return (k * p["lineTwips"] * 10) % 48 == 0


def score(lines, probes, per_group, rule, recursive):
    """一个候选（闭式/递推 × 一种进位规则）在全部步上的得分。"""
    n = ok = und = 0
    misses, notes = [], []
    for p in probes:
        members = [l for l in lines if l["text"].startswith(p["tag"])]
        n += per_group - 1
        if len(members) != per_group or len({m["page"] for m in members}) != 1:
            und += per_group - 1
            notes.append({"tag": p["tag"], "reason": "该组未整组落在同一页，按采前声明排除"})
            continue
        pred_cells = series(members[0]["baseline"], p, per_group, rule, recursive)
        for k, cells in enumerate(pred_cells, start=1):
            pred = cells * GRID_PT
            got = members[k]["baseline"]
            if eq(got, pred):
                ok += 1
            elif len(misses) < 10:
                misses.append({"tag": p["tag"], "lineTwips": p["lineTwips"], "n": k,
                               "实测": got, "预测": round(pred, 4),
                               "差": round(got - pred, 4)})
    return {"n": n, "ok": ok, "undecidable": und, "misses": misses, "notes": notes}


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
              "prereg": "docs/PREREG-2026-09-17-recursive.md",
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
        s0 = check_r0(lines)
        cands = {}
        for name, rule in RULES.items():
            for mode, rec in (("闭式", False), ("递推", True)):
                cands[f"{mode}·{name}"] = score(lines, probes, per_group, rule, rec)
        s1 = cands[POINT]
        passed = [k for k, v in cands.items() if verdict(v) == OK]
        outcome = ("恰好一个通过" if len(passed) == 1 else
                   "一个都不通过" if not passed else "不止一个通过：本夹具分不开")
        result["predictions"] = {"S0": {**s0, "verdict": verdict(s0)},
                                 "S1": {**s1, "prediction": f"S1 {POINT}",
                                        "verdict": verdict(s1)}}
        result["S2"] = {"prediction": "S2 八个候选里恰好一个全中",
                        "threshold": "全中，不是最高分", "passed": passed,
                        "outcome": outcome,
                        "scores": {k: f"{v['ok']}/{v['n']}" for k, v in cands.items()}}

        print(f"采集包 state={result['bundleState']}  判定=EVALUATED")
        print(f"  S0 {verdict(s0):<11} 通过 {s0['ok']}/{s0['n']}  {s0['prediction']}")
        for m in s0["misses"][:6]:
            print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
        print(f"  S1 {verdict(s1):<11} 通过 {s1['ok']}/{s1['n']}  点预测：{POINT}")
        for m in s1["misses"][:8]:
            print(f"        ✗ {json.dumps(m, ensure_ascii=False)}")
        print(f"  S2 结局：**{outcome}**（门槛：全中）")
        for k, v in cands.items():
            mark = "✔" if k in passed else (" ←点预测" if k == POINT else "")
            print(f"      {k:<12}{v['ok']:>4}/{v['n']}  {mark}")

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")
        print(f"已写出 {args.output}")
    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
