#!/usr/bin/env python3
"""预注册判据的**可执行形式**（`docs/PREREG-2026-09-17-probe-metrics.md`）。

量具方法 §7.1 要求「判据的可执行形式与判据文一起提交」——事后有没有动手，
一条 `git diff` 就能核。所以本文件与那份预注册同一次提交，**在采集之前**。

它只做两件事：

1. 先跑 §4 的证否条件（F-A..F-E）。任何一条触发，**整批读数作废**，不往下判。
2. 再把 §2 的五条预测逐条判成立／不成立／判不了，并给全分母。

**不做任何拟合**——G-9 是探索项（§3），这里只把读数原样列出来，不套模型。

用法：

    ./.venv/bin/python prereg_probe.py <采集包> --probes fixtures/probe-metrics.probes.json
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from wordmeasure import FAIL, OK, UNDECIDABLE, capture, wordmodel  # noqa: E402

# 1/300 英寸，实测的纵向栅格（两份夹具 56/56 个基线落在上面）。
GRID_PT = 0.24
# 只吸收 PDF 读数的十进制舍入，不吸收任何真实差异（预注册 §1）。
EPS = 1e-6
# 正文区顶 = 上边距 1440 twips。
CONTENT_TOP_PT = 72.0

SUPERSUB_SIZE_RATIO = 0.66
SUPERSCRIPT_RISE_EM = 0.34
SUBSCRIPT_DROP_EM = 0.08

FONT_FILES = {
    "Liberation Serif": "LiberationSerif-Regular.ttf",
    "Liberation Sans": "LiberationSans-Regular.ttf",
    "Liberation Mono": "LiberationMono-Regular.ttf",
    "Carlito": "Carlito-Regular.ttf",
}


def _round_half_up(x: float) -> float:
    """四舍五入（半数远离零）。

    **不用内建 `round`**：它是半数取偶，而本预注册的 P3/P4 恰好会落在整半数上
    （10pt 上标 0.66×10/0.24 = 27.5、18pt 的 49.5 与 25.5）。
    两种规则在这几点上碰巧同解，但让判据依赖这种巧合是拿运气当纪律。
    """
    return math.floor(x + 0.5) if x >= 0 else math.ceil(x - 0.5)


def quantize(value: float) -> float:
    return _round_half_up(value / GRID_PT) * GRID_PT


def eq(a: float, b: float) -> bool:
    return abs(a - b) <= EPS


# ---------------------------------------------------------------- 字体表

def hhea(path: Path) -> dict:
    """直接读 `head`/`hhea`，不经任何库——期望值必须独立于被测实现。"""
    data = path.read_bytes()
    count = struct.unpack(">H", data[4:6])[0]
    tables = {}
    for i in range(count):
        at = 12 + 16 * i
        tag = data[at : at + 4].decode("latin1")
        offset, length = struct.unpack(">II", data[at + 8 : at + 16])
        tables[tag] = (offset, length)
    head, _ = tables["head"]
    upem = struct.unpack(">H", data[head + 18 : head + 20])[0]
    h, _ = tables["hhea"]
    ascender, descender, line_gap = struct.unpack(">hhh", data[h + 4 : h + 10])
    return {"upem": upem, "ascent": ascender,
            "descent": abs(descender), "lineGap": line_gap}


def natural_pt(m: dict, size_pt: float) -> float:
    return (m["ascent"] + m["descent"] + m["lineGap"]) / m["upem"] * size_pt


def descent_pt(m: dict, size_pt: float) -> float:
    return m["descent"] / m["upem"] * size_pt


# ---------------------------------------------------------------- 读数提取

def lines_with_tags(model: dict) -> list[dict]:
    """每条行记录连同文本标签、首字形基线与字号。"""
    out = []
    for page in model.get("pages", []):
        for line in page.get("lines", []):
            glyphs = line.get("glyphs") or []
            if not glyphs:
                continue
            out.append({
                "page": page["index"],
                "text": (line.get("text") or "").strip("\r\x0b\x0c"),
                "baseline": glyphs[0]["origin"][1],
                "sizePt": glyphs[0].get("sizePt"),
                "glyphs": glyphs,
            })
    return out


def find(lines: list[dict], prefix: str) -> list[dict]:
    return [l for l in lines if l["text"].startswith(prefix)]


def settings_by_tag(probes: list[dict]) -> list[tuple[str, str, float, str]]:
    """(标签, 字体族, 字号pt, 探针类别)，按标签长度降序——长标签先匹配。"""
    rows = []
    for p in probes:
        # 缺字号的探针不往下猜——宁可让它在 P2 里判不了，也不编一个默认值。
        if "sizeHalfPoints" not in p:
            continue
        marked = p["tag"] + p["markedLineSuffix"] if p.get("markedLineSuffix") else None
        rows.append((p["tag"], p.get("family", "Liberation Serif"),
                     p["sizeHalfPoints"] / 2.0, p["kind"], marked))
    return sorted(rows, key=lambda r: -len(r[0]))


# ---------------------------------------------------------------- §4 证否条件

def falsifiers(bundle: dict, model: dict, probes_doc: dict, lines, probes) -> list[dict]:
    """§4：任何一条触发，本批采集不可用，所有读数作废。"""
    fired = []
    meta = bundle["META"]

    pre = (meta.get("preflight") or {}).get("result")
    if pre != "PASS":
        fired.append({"id": "F-A", "detail": f"采前核查 result={pre!r}，不是 PASS"})

    subs = meta.get("fontSubstitution") or {}
    required = {f.replace("-", "").replace(" ", "").lower() for f in FONT_FILES}
    unexpected = []
    for name in subs.get("pdfFontNames", []):
        base = name.split("+", 1)[-1].replace("-", "").replace(" ", "").lower()
        if not any(r in base or base in r for r in required):
            unexpected.append(name)
    if subs.get("result") != "PASS" or unexpected:
        fired.append({"id": "F-B", "detail": f"字体核查 result={subs.get('result')!r}，"
                                             f"申请之外的字体名={unexpected}"})

    fixture = meta.get("fixture") or {}
    if not fixture.get("unchanged"):
        fired.append({"id": "F-C", "detail": "夹具采前采后 sha256 不一致"})
    declared = probes_doc.get("sha256")
    actual = (fixture.get("before") or {}).get("sha256")
    if declared and actual and declared != actual:
        fired.append({"id": "F-C", "detail": f"采的不是预注册那份夹具："
                                             f"清单 {declared[:12]}… ≠ 采集 {actual[:12]}…"})

    if model.get("state") == UNDECIDABLE and "PAGE_SET_MISMATCH" in (model.get("reason") or ""):
        fired.append({"id": "F-D", "detail": model["reason"]})

    for probe in probes:
        if probe["kind"] not in ("size-sweep", "family-sweep"):
            continue
        group = find(lines, probe["tag"])
        pitches = [round(b["baseline"] - a["baseline"], 6)
                   for a, b in zip(group, group[1:]) if a["page"] == b["page"]]
        if len(pitches) == 2 and not eq(pitches[0], pitches[1]):
            fired.append({"id": "F-E", "detail": f"探针 {probe['tag']} 两个行距不等：{pitches}"})
    return fired


# ---------------------------------------------------------------- §2 预测

def check_p1(lines, probes, fonts_dir):
    """P1 行高 = round(自然行高 / 0.24) × 0.24。分母 = 26 个行距读数。"""
    out = {"prediction": "P1 行高 = round(自然行高 / 0.24) × 0.24", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    pitch_by_tag = {}
    for probe in probes:
        if probe["kind"] not in ("size-sweep", "family-sweep"):
            continue
        family = probe.get("family", "Liberation Serif")
        size_pt = probe["sizeHalfPoints"] / 2.0
        metrics = hhea(Path(fonts_dir) / FONT_FILES[family])
        predicted = quantize(natural_pt(metrics, size_pt))

        group = find(lines, probe["tag"])
        pitches = [round(b["baseline"] - a["baseline"], 6)
                   for a, b in zip(group, group[1:]) if a["page"] == b["page"]]
        # 一个探针申报两个读数；拿不到的照样进分母，只是判不了（§7.4）。
        out["n"] += 2
        if len(pitches) < 2:
            out["undecidable"] += 2 - len(pitches)
            out["notes"].append({"tag": probe["tag"], "reason": "同页相邻行不足，判不了",
                                 "got": pitches})
        if not pitches:
            continue
        if len(set(pitches)) > 1:
            # F-E 已在上面触发；这里只保证不把它算成通过。
            out["undecidable"] += len(pitches)
            continue
        pitch_by_tag[probe["tag"]] = (pitches[0], predicted, metrics, size_pt)
        for _ in pitches:
            if eq(pitches[0], predicted):
                out["ok"] += 1
            else:
                out["misses"].append({"tag": probe["tag"], "family": family, "sizePt": size_pt,
                                      "measured": pitches[0], "predicted": predicted})
    return out, pitch_by_tag


def check_p2(lines, probes, fonts_dir):
    """P2 每页首行基线偏移 = 预测行距 − round(descent / 0.24) × 0.24。

    排除项（**采集前声明**，§8 的口径排除，不是放宽容差）：首行**自己**带
    `w:position` 或 `w:vertAlign` 的页不判——那两样改的是行高本身，归 G-9 与 P4，
    拿它们来判 P2 等于把别的题的错算到这题头上。

    按**行**排除，不按**组**排除：夹具里每组第一段都是普通行，带标记的在第二、
    第三段，所以正常情况下这条排除一次也不该触发。它触发了，说明分页跟设计的不一样，
    那本身就是要看见的事——所以排除项会原样打印出来，而不是悄悄少算一个分母。
    """
    out = {"prediction": "P2 首行基线偏移 = 行高 − round(descent / 0.24) × 0.24",
           "n": 0, "ok": 0, "undecidable": 0, "misses": [], "notes": [], "excluded": []}
    settings = settings_by_tag(probes)

    first_of_page = {}
    for line in lines:
        first_of_page.setdefault(line["page"], line)

    for page, line in sorted(first_of_page.items()):
        out["n"] += 1
        match = next((s for s in settings if line["text"].startswith(s[0])), None)
        if match is None:
            out["undecidable"] += 1
            out["notes"].append({"page": page, "reason": "首行不属于任何探针",
                                 "text": line["text"]})
            continue
        tag, family, size_pt, kind, marked = match
        sizes = {g["sizePt"] for g in line["glyphs"] if g["sizePt"] is not None}
        if (marked and line["text"].startswith(marked)) or len(sizes) > 1:
            out["undecidable"] += 1
            out["excluded"].append({"page": page, "tag": tag, "kind": kind,
                                    "text": line["text"], "sizes": sorted(sizes),
                                    "reason": "首行自身含抬升／上下标，按采前声明排除"})
            continue
        metrics = hhea(Path(fonts_dir) / FONT_FILES[family])
        predicted = quantize(natural_pt(metrics, size_pt)) - quantize(descent_pt(metrics, size_pt))
        measured = round(line["baseline"] - CONTENT_TOP_PT, 6)
        if eq(measured, predicted):
            out["ok"] += 1
        else:
            out["misses"].append({"page": page, "tag": tag, "sizePt": size_pt,
                                  "measured": measured, "predicted": predicted})
    return out


def check_p3_p4(lines, probes):
    """P3 上下标字号 = 0.66 × 正文（**不量化**）；P4 偏移**量化**到栅格。"""
    p3 = {"prediction": "P3 上下标字号 = 0.66 × 正文字号（不量化）", "n": 0, "ok": 0,
          "undecidable": 0, "misses": [], "notes": []}
    p4 = {"prediction": "P4 上标升 0.34 em / 下标降 0.08 em，量化到栅格", "n": 0, "ok": 0,
          "undecidable": 0, "misses": [], "notes": []}

    for probe in probes:
        if probe["kind"] not in ("superscript", "subscript"):
            continue
        size_pt = probe["sizeHalfPoints"] / 2.0
        p3["n"] += 1
        p4["n"] += 1
        target = next((l for l in lines if l["text"].startswith(probe["tag"] + "x")), None)
        if target is None:
            p3["undecidable"] += 1
            p4["undecidable"] += 1
            p3["notes"].append({"tag": probe["tag"], "reason": "没找到带上下标的行"})
            continue

        base = [g for g in target["glyphs"] if eq(g["sizePt"] or 0.0, size_pt)]
        marked = [g for g in target["glyphs"] if not eq(g["sizePt"] or 0.0, size_pt)]
        if not base or not marked:
            p3["undecidable"] += 1
            p4["undecidable"] += 1
            p3["notes"].append({"tag": probe["tag"], "reason": "同一行里分不出正文与上下标",
                                "sizes": sorted({g["sizePt"] for g in target["glyphs"]})})
            continue

        measured_size = marked[0]["sizePt"]
        predicted_size = SUPERSUB_SIZE_RATIO * size_pt
        if eq(measured_size, predicted_size):
            p3["ok"] += 1
        else:
            p3["misses"].append({"tag": probe["tag"], "sizePt": size_pt,
                                 "measured": measured_size, "predicted": predicted_size})

        # 页顶向下：上标 y 更小、下标 y 更大，所以正数即「抬升」。
        shift = round(base[0]["origin"][1] - marked[0]["origin"][1], 6)
        em = SUPERSCRIPT_RISE_EM if probe["kind"] == "superscript" else -SUBSCRIPT_DROP_EM
        predicted_shift = quantize(abs(em) * size_pt) * (1 if em > 0 else -1)
        if eq(shift, predicted_shift):
            p4["ok"] += 1
        else:
            p4["misses"].append({"tag": probe["tag"], "kind": probe["kind"], "sizePt": size_pt,
                                 "measured": shift, "predicted": round(predicted_shift, 6)})
    return p3, p4


def check_p5(lines, probes):
    """P5 分节边界后那一行，行距 = 常规 + 0.24pt。分母 2。"""
    out = {"prediction": "P5 分节边界后行距 = 常规 + 0.24pt", "n": 0, "ok": 0,
           "undecidable": 0, "misses": [], "notes": []}
    for probe in probes:
        if probe["kind"] != "section-boundary":
            continue
        out["n"] += 1
        group = find(lines, probe["tag"])
        if len(group) < 4:
            out["undecidable"] += 1
            out["notes"].append({"tag": probe["tag"], "reason": "该组行数不足，判不了",
                                 "got": len(group)})
            continue
        a, b, c, d = group[:4]
        if len({a["page"], b["page"], c["page"], d["page"]}) > 1:
            out["undecidable"] += 1
            out["notes"].append({"tag": probe["tag"], "reason": "该组跨页，行距不可比"})
            continue
        normal = round(d["baseline"] - c["baseline"], 6)   # 边界之后的常规行距
        across = round(c["baseline"] - b["baseline"], 6)   # 跨过 sectPr 那一步
        if eq(across, normal + GRID_PT):
            out["ok"] += 1
        else:
            out["misses"].append({"tag": probe["tag"], "normal": normal, "across": across,
                                  "predicted": round(normal + GRID_PT, 6)})
    return out


def collect_g9(lines, probes):
    """G-9 是**探索项**：只把读数原样列出来，不套任何模型（预注册 §3）。"""
    rows = []
    for probe in probes:
        if probe["kind"] != "position":
            continue
        group = find(lines, probe["tag"])
        if len(group) < 3 or len({l["page"] for l in group[:3]}) > 1:
            rows.append({"tag": probe["tag"], "note": "跨页或行数不足，不采"})
            continue
        a, b, c = group[:3]
        rows.append({
            "tag": probe["tag"],
            "risePt": probe["riseHalfPoints"] / 2.0,
            "pitchIntoRisedLine": round(b["baseline"] - a["baseline"], 6),
            "pitchOutOfRisedLine": round(c["baseline"] - b["baseline"], 6),
        })
    return {"exploratory": True, "note": "不作预测，只采数（预注册 §3）。"
            "采完若能形成规则，按 §7.5 标「回测」，不当独立检验。", "rows": rows}


# ---------------------------------------------------------------- 主程序

def verdict(p: dict) -> str:
    """三态出口（§7.3）：「判不了」是独立出口，不折叠进 FAIL 也不折叠进 OK。"""
    if p["misses"]:
        return FAIL
    if p["undecidable"] or p["ok"] < p["n"]:
        return UNDECIDABLE
    return OK


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle")
    parser.add_argument("--probes", required=True,
                        help="make_probe_fixture.py 写出的 .probes.json")
    parser.add_argument("--fonts", default=str(Path.home() / "Library/Fonts"))
    parser.add_argument("--output", "-o")
    args = parser.parse_args()

    bundle = capture.load_bundle(Path(args.bundle))
    model = wordmodel.build(bundle)
    probes_doc = json.loads(Path(args.probes).read_text())
    probes = probes_doc["probes"]
    lines = lines_with_tags(model)

    fired = falsifiers(bundle, model, probes_doc, lines, probes)

    result = {
        "schema": "rsword-layout-prereg/1",
        "prereg": "docs/PREREG-2026-09-17-probe-metrics.md",
        "bundle": args.bundle,
        "bundleState": model.get("state"),
        "coverage": model.get("coverage"),
        "lineDenominators": model.get("lineDenominators"),
        "falsifiers": fired,
    }

    if fired:
        # §4：触发即整批作废。**不出任何预测判定**——不许挑着用。
        result["verdict"] = "VOID"
        result["predictions"] = {}
        result["G9"] = {"exploratory": True, "rows": [], "note": "整批作废，不采"}
    else:
        p1, _ = check_p1(lines, probes, args.fonts)
        p2 = check_p2(lines, probes, args.fonts)
        p3, p4 = check_p3_p4(lines, probes)
        p5 = check_p5(lines, probes)
        result["verdict"] = "EVALUATED"
        result["predictions"] = {
            k: {**v, "verdict": verdict(v)}
            for k, v in (("P1", p1), ("P2", p2), ("P3", p3), ("P4", p4), ("P5", p5))
        }
        result["G9"] = collect_g9(lines, probes)

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")

    print(f"采集包 state={result['bundleState']}  判定={result['verdict']}")
    if fired:
        print("§4 证否条件触发，**本批读数全部作废**：")
        for f in fired:
            print(f"  ✗ {f['id']}  {f['detail']}")
    else:
        for key, p in result["predictions"].items():
            print(f"  {key} {p['verdict']:<11} 通过 {p['ok']}/{p['n']}"
                  f"（判不了 {p['undecidable']}）  {p['prediction']}")
            for miss in p["misses"][:6]:
                print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
            for note in p.get("notes", [])[:4]:
                print(f"        ? {json.dumps(note, ensure_ascii=False)}")
            for ex in p.get("excluded", [])[:4]:
                print(f"        − {json.dumps(ex, ensure_ascii=False)}")
        print("  G-9 探索项（不作预测，只采数）：")
        for row in result["G9"]["rows"]:
            print(f"        {json.dumps(row, ensure_ascii=False)}")
    if args.output:
        print(f"已写出 {args.output}")

    return 0 if result["verdict"] == "EVALUATED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
