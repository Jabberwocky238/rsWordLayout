"""量具命令行。

子命令对应方法 §9 的搭建顺序，**每一步都能独立验收**：

    rules       §4  打印字形计数约定表（带分母与范围）
    preflight   §9.2 采前核查：Word 进程自己能不能列出所需字体
    capture     §9.1 采集链：COM/AppleScript 结构 + PDF 几何，进同一个包
    glyphs      §2.2 单读一份 PDF 的逐字形几何
    model       §3  把采集包折成「页 → 行 → 字形」，逐行报三态
    control     §9.3/§9.4 重复性、正对照、阴性对照
    selfcheck   只查轨迹与契约对不对得上，不需要 Word 采集
    compare     §9.6 引擎轨迹 vs Word 采集包
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import FAIL, OK, UNDECIDABLE, capture as capture_mod
from . import compare as compare_mod
from . import adopt, controls, counting, pdfglyphs, preflight, selfcheck, wordmodel

DEFAULT_FAMILIES = ["Liberation Serif", "Liberation Sans", "Carlito"]


def _dump(obj, path: str | None):
    text = json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True, allow_nan=False)
    if path:
        Path(path).write_text(text + "\n")
        print("已写出 %s" % path)
    else:
        print(text)


def _load_trace(path: Path) -> dict:
    """读引擎轨迹并核契约（`rsword-layout-trace/1`）。"""
    doc = json.loads(Path(path).read_text())
    if doc.get("schema") != "rsword-layout-trace/1":
        raise SystemExit("引擎轨迹 schema 不是 rsword-layout-trace/1：%s" % doc.get("schema"))
    if doc.get("unit") != "pt":
        raise SystemExit("引擎轨迹单位不是 pt：%s" % doc.get("unit"))
    return doc


def cmd_rules(args):
    table = counting.rules_table()
    if args.json:
        _dump({"schema": "rsword-layout-counting-rules/1", "rules": table}, args.output)
        return 0
    print("字形计数约定（方法 §4）。范围：**拉丁文本、简单 TrueType 字体**。")
    print("仍在范围外：制表符、跨页表格行、自动编号、行内对象、合成字体（含 CJK 位图化）。\n")
    width = max(len(r["name"]) for r in table)
    for rule in table:
        glyphs = "范围外" if rule["glyphs"] is None else str(rule["glyphs"])
        print("  %-*s  %-6s  %-22s  %s" % (width, rule["name"], glyphs, rule["checked"], rule["platform"]))
        if rule["note"]:
            print("  %s  ↳ %s" % (" " * width, rule["note"]))
    return 0


def cmd_preflight(args):
    record = preflight.preflight(args.font or DEFAULT_FAMILIES)
    _dump(record, args.output)
    print("\n前置核查：%s" % record["result"], file=sys.stderr)
    if record.get("note"):
        print(record["note"], file=sys.stderr)
    return 0 if record["result"] == "PASS" else 1


def cmd_capture(args):
    meta = capture_mod.capture(
        Path(args.docx),
        Path(args.bundle),
        required_families=args.font or DEFAULT_FAMILIES,
        label=args.label,
        include_font_files=args.include_font_files,
    )
    print("已采：%s" % args.bundle)
    print("  页 %d · 字形 %d · 逐页 %s" % (meta["pageCount"], meta["glyphTotal"], meta["glyphCounts"]))
    print("  字体替换核查：%s" % meta["fontSubstitution"]["result"])
    if not meta["fixture"]["unchanged"]:
        print("  **夹具在采集过程中变了，读数作废**", file=sys.stderr)
        return 1
    return 0


def cmd_adopt(args):
    meta = adopt.adopt_mac_run(
        Path(args.run),
        Path(args.bundle),
        batch_manifest=Path(args.manifest) if args.manifest else None,
        required_families=args.font,
    )
    print("已折出：%s  （provenance=%s）" % (args.bundle, meta["provenance"]))
    print("  页 %d · 字形 %d" % (meta["pageCount"], meta["glyphTotal"]))
    verification = meta["sourceTextVerification"]
    print("  正文文本推导核查：%s" % verification["state"])
    for reason in verification.get("reasons", []):
        print("    %s" % reason)
    return 0 if verification["state"] == OK else 2


def cmd_glyphs(args):
    doc = pdfglyphs.read_pdf(args.pdf)
    if args.summary:
        print("页 %d · 逐页字形 %s" % (len(doc["pages"]), pdfglyphs.glyph_counts(doc)))
        print("字体：%s" % ", ".join(pdfglyphs.font_names(doc)))
        for i in range(len(doc["pages"])):
            print("  p%-3d %s" % (i, repr(pdfglyphs.page_text(doc, i))[:150]))
        return 0
    _dump(doc, args.output)
    return 0


def cmd_model(args):
    bundle = capture_mod.load_bundle(Path(args.bundle))
    model = wordmodel.build(bundle)
    hits = controls.scan_falsifiers(model)
    if args.output or args.json:
        _dump({**model, "falsifierHits": hits}, args.output)
        return 0

    # 先印 state，再印任何数值（§6.5）。
    print("state=%s" % model["state"])
    if model.get("reason"):
        print("  %s" % model["reason"])
    print("覆盖：%s" % json.dumps(model["coverage"], ensure_ascii=False))
    if "lineDenominators" in model:
        d = model["lineDenominators"]
        print("行分母：N=%d  成立=%d  不成立=%d  判不了=%d" % (d["lines"], d["ok"], d["fail"], d["undecidable"]))
    for page in model["pages"]:
        if page["state"] != OK:
            reason = page.get("reason") or ""
            if not isinstance(reason, str):
                reason = json.dumps(reason, ensure_ascii=False)
            print("  p%-3d %s  %s" % (page["index"], page["state"], reason[:220]))
            continue
        for line in page["lines"]:
            if line["state"] != OK or args.verbose:
                print(
                    "  p%d L%-3d %-12s 字形=%d 预期=%s %s"
                    % (
                        page["index"],
                        line["index"],
                        line["state"],
                        len(line["glyphs"]),
                        line["expected"],
                        "; ".join(line["reasons"])[:160],
                    )
                )
    if hits:
        print("\n**证否条件命中**（§9.4）：")
        for hit in hits:
            print("  %s  %s" % (hit["falsifier"], json.dumps(hit, ensure_ascii=False)[:200]))
    return 0 if model["state"] == OK else (2 if model["state"] == UNDECIDABLE else 1)


def cmd_control(args):
    a = capture_mod.load_bundle(Path(args.bundle_a))
    b = capture_mod.load_bundle(Path(args.bundle_b))
    runner = {
        "repeat": controls.repeatability,
        "positive": controls.positive_control,
        "negative": controls.negative_control,
    }[args.kind]
    record = runner(a, b)
    if args.output or args.json:
        _dump(record, args.output)
    else:
        print("state=%s  （%s）" % (record["state"], record["check"]))
        if record.get("reason"):
            print("  %s" % record["reason"])
        if record.get("maxAbs") is not None:
            print("  maxAbs=%.6fpt  bitwiseIdentical=%s" % (record["maxAbs"], record.get("bitwiseIdentical")))
        if record.get("comparison"):
            print("  %s" % json.dumps(record["comparison"]["structure"], ensure_ascii=False))
        for name in record["falsifiers"]:
            print("  **证否条件命中** %s：%s" % (name, controls.FALSIFIERS[name]))
    return 0 if record["state"] == OK else (2 if record["state"] == UNDECIDABLE else 1)


def cmd_selfcheck(args):
    trace = _load_trace(Path(args.trace))
    result = selfcheck.run(trace)
    if args.output or args.json:
        _dump(result, args.output)
        return 0 if result["state"] == OK else (2 if result["state"] == UNDECIDABLE else 1)

    print("state=%s   %s" % (result["state"], args.trace))
    for check in result["checks"]:
        print("  %-9s %s" % (check["state"], check["check"]))
        if check.get("reason"):
            print("            %s" % check["reason"])
        for conflict in check.get("conflicts", [])[: args.limit]:
            print("            p%d L%d %s：自报 %d 个，实画 %d 个（glyphId %s）"
                  % (conflict["page"], conflict["line"], conflict["terminator"],
                     conflict["declaredExpected"], conflict["actuallyEmitted"],
                     conflict["glyphIds"]))
        for sample in check.get("sample", [])[: args.limit]:
            print("            p%d L%d 接着上一条的 %d" % (sample["page"], sample["line"], sample["continuesFrom"]))
    return 0 if result["state"] == OK else (2 if result["state"] == UNDECIDABLE else 1)


def cmd_compare(args):
    bundle = capture_mod.load_bundle(Path(args.bundle))
    reference = wordmodel.build(bundle)
    candidate = _load_trace(Path(args.trace))
    reference, exclusion = compare_mod.exclude_glyphs(reference, set(args.exclude_rule or []))
    result = compare_mod.compare(reference, candidate, tolerance_pt=args.tolerance)
    hits = controls.scan_falsifiers(reference, result)

    if args.output or args.json:
        _dump(
            {
                "schema": "rsword-layout-acceptance/1",
                # state 第一（§6.5）。
                "state": result.state,
                "reference": {
                    "bundle": str(args.bundle),
                    "state": reference["state"],
                    "coverage": reference["coverage"],
                    "lineDenominators": reference.get("lineDenominators"),
                    "premises": reference["premises"],
                },
                "candidate": {
                    "trace": str(args.trace),
                    "engine": candidate.get("engine"),
                    "metrics": candidate.get("metrics"),
                    "glyphOriginMethod": candidate.get("glyphOriginMethod"),
                },
                "scopeExclusion": exclusion,
                "comparison": result.to_dict(),
                "decomposition": compare_mod.decompose(result),
                "falsifierHits": hits,
            },
            args.output,
        )
        return 0 if result.state == OK else (2 if result.state == UNDECIDABLE else 1)

    if exclusion["excludedRules"]:
        # 缩小过范围就必须先说，再给数（§6.5 的同一条道理：先读 state，再读数）。
        print("验收范围已缩小：排除 %s —— %d / %d 个字形不参与比较"
              % (", ".join(exclusion["excludedRules"]), exclusion["excludedGlyphs"],
                 exclusion["totalGlyphs"]))
    print(result.summary())
    print("Word 侧：state=%s  %s" % (reference["state"], json.dumps(reference["coverage"], ensure_ascii=False)))
    print("引擎侧：%s / %s" % (candidate.get("engine"), candidate.get("metrics")))
    for failure in result.failures[: args.limit]:
        print("  %s  %s" % (failure["code"], json.dumps(failure, ensure_ascii=False)[:240]))
    if result.state == OK or args.show_worst:
        for diff in result.worst[: args.limit]:
            print(
                "  p%d L%d #%d %r  Δ=(%+.3f, %+.3f)  d=%.3fpt"
                % (diff.page, diff.line, diff.index, diff.text, diff.dx, diff.dy, diff.distance)
            )
    breakdown = compare_mod.decompose(result)
    if "lineStart" in breakdown:
        start, everything = breakdown["lineStart"], breakdown["all"]
        print("  分解：每行首字形（n=%d） Δx maxAbs=%+.4f 中位=%+.4f | Δy maxAbs=%+.4f 中位=%+.4f"
              % (start["dx"]["n"], start["dx"]["maxAbs"], start["dx"]["median"],
                 start["dy"]["maxAbs"], start["dy"]["median"]))
        print("        全部字形（n=%d）  Δx maxAbs=%+.4f 中位=%+.4f | Δy maxAbs=%+.4f 中位=%+.4f"
              % (everything["dx"]["n"], everything["dx"]["maxAbs"], everything["dx"]["median"],
                 everything["dy"]["maxAbs"], everything["dy"]["median"]))
        worst_page = max(breakdown["perPageMaxAbs"].items(), key=lambda kv: kv[1])
        print("        最差的页：p%s max|Δ|=%.4fpt" % worst_page)
    for hit in hits:
        print("  **证否条件命中** %s" % hit["falsifier"])
    return 0 if result.state == OK else (2 if result.state == UNDECIDABLE else 1)


def build_parser():
    parser = argparse.ArgumentParser(prog="wm", description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    def common(p):
        p.add_argument("--output", "-o", help="写到文件而不是标准输出")
        p.add_argument("--json", action="store_true", help="输出 JSON")
        return p

    p = common(sub.add_parser("rules", help="打印 §4 字形计数约定表"))
    p.set_defaults(func=cmd_rules)

    p = common(sub.add_parser("preflight", help="§9.2 采前核查（唯一防线）"))
    p.add_argument("--font", action="append", help="必须在列的字体族，可重复")
    p.set_defaults(func=cmd_preflight)

    p = sub.add_parser("capture", help="§9.1 跑一次采集")
    p.add_argument("docx")
    p.add_argument("bundle")
    p.add_argument("--font", action="append")
    p.add_argument("--label")
    p.add_argument("--include-font-files", action="store_true", help="把逐个字体文件哈希也写进包（很大）")
    p.set_defaults(func=cmd_capture)

    p = sub.add_parser("adopt", help="把既有 mac 采集折成采集包（不驱动 Word）")
    p.add_argument("run")
    p.add_argument("bundle")
    p.add_argument("--manifest", help="原批次 MANIFEST.json，限定会原样带进包里")
    p.add_argument("--font", action="append")
    p.set_defaults(func=cmd_adopt)

    p = common(sub.add_parser("glyphs", help="§2.2 读一份 PDF 的逐字形几何"))
    p.add_argument("pdf")
    p.add_argument("--summary", action="store_true")
    p.set_defaults(func=cmd_glyphs)

    p = common(sub.add_parser("model", help="§3 采集包 → 页/行/字形，逐行三态"))
    p.add_argument("bundle")
    p.add_argument("--verbose", "-v", action="store_true", help="连成立的行也列出来")
    p.set_defaults(func=cmd_model)

    p = common(sub.add_parser("control", help="§9.3/§9.4 对照"))
    p.add_argument("kind", choices=["repeat", "positive", "negative"])
    p.add_argument("bundle_a")
    p.add_argument("bundle_b")
    p.set_defaults(func=cmd_control)

    p = common(sub.add_parser("selfcheck", help="只查轨迹与契约对不对得上，不需要 Word 采集"))
    p.add_argument("trace")
    p.add_argument("--limit", type=int, default=4)
    p.set_defaults(func=cmd_selfcheck)

    p = common(sub.add_parser("compare", help="§9.6 引擎轨迹 vs Word 采集包"))
    p.add_argument("bundle")
    p.add_argument("trace")
    p.add_argument(
        "--tolerance",
        type=float,
        default=compare_mod.NOISE_FLOOR_PT,
        help="容差（点）。默认 0——噪声底就是 0（§5），非零差不可能是量得不准。",
    )
    p.add_argument(
        "--exclude-rule",
        action="append",
        help="按 §4 约定名把某类字形排出验收范围（如 PARAGRAPH_MARK）。"
             "排除会逐条印在报告里；它改的是范围，不是判据——不要拿它当容差用。",
    )
    p.add_argument("--limit", type=int, default=12)
    p.add_argument("--show-worst", action="store_true")
    p.set_defaults(func=cmd_compare)

    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
