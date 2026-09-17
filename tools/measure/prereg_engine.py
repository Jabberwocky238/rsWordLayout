#!/usr/bin/env python3
"""把预注册的判据套到**引擎自己的轨迹**上（`docs/PREREG-2026-09-17-probe-metrics.md`）。

与 `prereg_probe.py` 是同一套判据、同一份夹具，换的只是读数来源：
那边读 Word 导出的 PDF，这边读 `layout-trace` 的输出。**不需要 Word。**

它回答的不是「Word 怎么做」——那只有采集能答。它回答的是
**「引擎当前实现的是哪一套规则」**，以及两边在哪一条上分家。
引擎这边判出「成立」也不是对 Word 的结论：两边可能一起错。

判据本身一个字都不重写：预测值全部从 `prereg_probe` 里 import，
免得哪天改了一边忘了另一边，两份判据悄悄分叉。

用法：

    cargo run --features fontenv --bin layout-trace -- \\
        --font ... --metrics real --vertical-grid mac \\
        fixtures/probe-metrics.docx /tmp/engine-probe.json
    ./.venv/bin/python prereg_engine.py /tmp/engine-probe.json \\
        --probes ../../fixtures/probe-metrics.probes.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import prereg_probe as P  # noqa: E402  预测值只此一份，两边共用
from wordmeasure import docxtext  # noqa: E402


def _lines(trace: dict, text: str) -> list[dict]:
    """轨迹 → 与 `prereg_probe.lines_with_tags` 同形状的行记录。

    轨迹里没有文本，只有 `sourceStart`/`sourceEnd`；文本从夹具推出来，
    偏移空间与 Word 的一致（见 `tools/measure/README.md`）。
    """
    out = []
    for page in trace["pages"]:
        for ln in page["lines"]:
            glyphs = ln["glyphs"]
            if not glyphs:
                continue
            out.append({
                "page": page["index"],
                "text": text[ln["sourceStart"]:ln["sourceEnd"]].strip("\r\x0b\x0c"),
                "baseline": glyphs[0]["origin"][1],
                "sizePt": glyphs[0]["sizeHalfPoints"] / 2.0,
                # 判据按 `sizePt` 取值，轨迹给的是半点——在这里换算，
                # 好让 prereg_probe 的判据函数一个字都不用改。
                "glyphs": [{**g, "sizePt": g["sizeHalfPoints"] / 2.0} for g in glyphs],
            })
    return out


def evaluate(trace_path: Path, probes_path: Path, docx: Path, fonts: Path) -> dict:
    trace = json.loads(trace_path.read_text())
    probes = json.loads(probes_path.read_text())["probes"]
    text = docxtext.content_text(docx)["text"]
    lines = _lines(trace, text)

    p1, _ = P.check_p1(lines, probes, fonts)
    p2 = P.check_p2(lines, probes, fonts)
    p3, p4 = P.check_p3_p4(lines, probes)
    p5 = P.check_p5(lines, probes)
    return {
        "schema": "rsword-layout-prereg-engine/1",
        "prereg": "docs/PREREG-2026-09-17-probe-metrics.md",
        "basis": "引擎轨迹，非 Word 读数。判的是「引擎实现了哪套规则」，不是「Word 怎么做」。",
        "trace": str(trace_path),
        "pages": len(trace["pages"]),
        "lines": len(lines),
        "predictions": {k: {**v, "verdict": P.verdict(v)} for k, v in
                        (("P1", p1), ("P2", p2), ("P3", p3), ("P4", p4), ("P5", p5))},
        "G9": P.collect_g9(lines, probes),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", help="layout-trace 写出的 rsword-layout-trace/1")
    parser.add_argument("--probes", required=True)
    parser.add_argument("--docx", default=None, help="夹具，默认由 --probes 同目录推出")
    parser.add_argument("--fonts", default=str(Path.home() / "Library/Fonts"))
    parser.add_argument("--output", "-o")
    args = parser.parse_args()

    probes_path = Path(args.probes)
    docx = Path(args.docx) if args.docx else probes_path.with_suffix("").with_suffix(".docx")
    result = evaluate(Path(args.trace), probes_path, docx, Path(args.fonts))

    text = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    if args.output:
        Path(args.output).write_text(text + "\n")

    print(f"引擎轨迹：{result['pages']} 页 / {result['lines']} 行")
    for key, p in result["predictions"].items():
        print(f"  {key} {p['verdict']:<11} 通过 {p['ok']}/{p['n']}  {p['prediction']}")
        for miss in p["misses"][:5]:
            print(f"        ✗ {json.dumps(miss, ensure_ascii=False)}")
        for note in p.get("notes", [])[:3]:
            print(f"        ? {json.dumps(note, ensure_ascii=False)}")
    print("  G-9 探索项（不作预测，只采数）：")
    for row in result["G9"]["rows"]:
        print(f"        {json.dumps(row, ensure_ascii=False)}")
    if args.output:
        print(f"已写出 {args.output}")


if __name__ == "__main__":
    raise SystemExit(main())
