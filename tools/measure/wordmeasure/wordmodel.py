"""把采集包折成「页 → 行 → 字形」，喂给比较器。

这一层做三件事，每件都可能出「判不了」，而且**判不了必须原样传下去**（§7.3）：

1. 从行号扫描把源字符分到 (页, 行)；
2. 把 PDF 字形归行——有行盒走盒包含（§3.3），没有就走推算（Mac，§6.6）；
3. 逐行按读序配对（§3.2），并报 `COUNT_OK` / `COUNT_MISMATCH` / 判不了（§9.5）。

**8.5% 的字形不在行划分覆盖内**（§5：2,988 / 35,087，集中在抬升 run、脚注、表格页）。
目前的处理与方法一致：**显式写进验收定义里排除**，不是给出归属（§8）。
本模块把这类页标成 `UNDECIDABLE` 并记下分母，不猜归属。
"""

from __future__ import annotations

from . import OK, UNDECIDABLE, counting, pairing


def _dedupe(reasons: list[str]) -> list[str]:
    """同一条理由重复多次没有信息量，只会淹掉别的理由。保序去重并记次数。"""
    seen: dict[str, int] = {}
    for reason in reasons:
        seen[reason] = seen.get(reason, 0) + 1
    return [r if n == 1 else "%s ×%d" % (r, n) for r, n in seen.items()]


def lines_from_sweep(sweep: dict) -> list[dict]:
    """把逐字符的 (页, 行) 读数折成行。

    行的边界就是 (页, 行) 这对序数发生变化的地方。相邻位置同属一行即并入。
    **不做任何几何推断**——这些是序数，没有单位（§2.1 / §6.6）。
    """
    content = sweep["contentText"]
    positions = sorted(sweep["positions"], key=lambda p: p["offset"])
    lines: list[dict] = []
    for record in positions:
        key = (record["page"], record["line"])
        if lines and (lines[-1]["page"], lines[-1]["line"]) == key:
            lines[-1]["end"] = record["offset"] + 1
        else:
            lines.append(
                {
                    "page": record["page"],
                    "line": record["line"],
                    "start": record["offset"],
                    "end": record["offset"] + 1,
                }
            )
    for line in lines:
        line["text"] = content[line["start"] : line["end"]]
    return lines


def line_marks(sweep: dict, line: dict) -> dict[int, str] | None:
    """这一行的逐字符构造标注（行内下标 → `counting.RULES` 的键）。

    标注来自采集包里的源侧推导（`sweep.marks`，由 `docxtext` 给出），**不是从字符猜的**。
    分页符还要按 §4 定位置，那要同时看段落与行，所以在这里做：

    - 「行首独占、自成一条行记录」是行的性质；
    - 「紧跟段落标记」「段中」是段落里的位置。

    采集包没带标注时返回 None，调用方退回按字符猜（猜不出即判不了）。
    """
    marks = sweep.get("marks")
    if not marks:
        return None
    text = sweep["contentText"]
    paragraphs = sweep.get("paragraphs") or []
    start, end = line["start"], line["end"]

    out: dict[int, str] = {}
    for at in range(start, end):
        mark = marks.get(str(at), marks.get(at))
        if mark is None:
            continue
        if mark == "PAGE_BREAK":
            para = next((p for p in paragraphs if p["start"] <= at < p["end"]), None)
            if para is None:
                out[at - start] = "PAGE_BREAK"  # 定位不了段落 → 让 counting 判不了
                continue
            para_text = text[para["start"] : para["end"]]
            alone = line["text"].strip("\r\x0b") == "\x0c"
            mark = counting.resolve_page_break(
                para_text, at - para["start"], alone_on_line=alone
            )
            if mark == "UNKNOWN":
                mark = "PAGE_BREAK"
        out[at - start] = mark
    return out


def build(bundle: dict) -> dict:
    """采集包 → 比较器输入。"""
    sweep, glyphs_doc = bundle["sweep"], bundle["glyphs"]

    # 采集包自己说不可用，就不要往下算。算出来的数会长得很像能用的数——
    # 那正是最危险的一种（§6.5）。
    meta = bundle.get("META", {})
    if meta.get("usability") == UNDECIDABLE:
        return {
            "schema": "rsword-layout-word-model/1",
            "state": UNDECIDABLE,
            "reason": "BUNDLE_NOT_USABLE: %s" % "; ".join(meta.get("usabilityReason") or []),
            "premises": dict(pairing.PREMISES),
            "coverage": {},
            "pages": [],
        }
    all_lines = lines_from_sweep(sweep)

    # Word 的页序数从 1 起；PDF 页从 0 起。按出现顺序映到 PDF 页下标，
    # 不假设「Word 第 N 页 = PDF 第 N−1 页」，而是核过再用。
    word_pages = sorted({line["page"] for line in all_lines})
    pdf_pages = glyphs_doc["pages"]

    out = {
        "schema": "rsword-layout-word-model/1",
        "unit": "pt",
        "source": bundle["path"],
        "premises": dict(pairing.PREMISES),
        "coverage": {
            "wordPagesInSweep": len(word_pages),
            "pdfPages": len(pdf_pages),
            "sweepPositions": len(sweep["positions"]),
            "linesFromSweep": len(all_lines),
        },
        "pages": [],
    }

    if len(word_pages) != len(pdf_pages):
        # 页数就对不上：不往下走。常见原因是脚注/表格页的字形不进行划分（§5 的 8.5%）。
        out["state"] = UNDECIDABLE
        out["reason"] = (
            "PAGE_SET_MISMATCH: 行号扫描给出 %d 页，PDF 有 %d 页。"
            "8.5%% 的字形本就不在行划分覆盖内（§5），此处按验收定义排除，不猜归属（§8）。"
            % (len(word_pages), len(pdf_pages))
        )
        out["pages"] = [{"index": i, "state": UNDECIDABLE, "reason": out["reason"], "lines": []}
                        for i in range(len(pdf_pages))]
        return out

    out["state"] = OK
    counts = {"lines": 0, "ok": 0, "fail": 0, "undecidable": 0}

    for pdf_index, (word_page, pdf_page) in enumerate(zip(word_pages, pdf_pages)):
        page_lines = [l for l in all_lines if l["page"] == word_page]
        page_glyphs = pdf_page["glyphs"]
        marks = {i: m for i, l in enumerate(page_lines) if (m := line_marks(sweep, l)) is not None}

        if sweep.get("boxAvailable") and all("box" in l for l in page_lines):
            # Windows 通道：按行盒纵向包含归行（§3.3）。
            grouped, outside = pairing.assign_by_box(page_glyphs, [l["box"] for l in page_lines])
            assignment = {
                "state": OK,
                "method": "BOX_CONTAINMENT",
                "premise": pairing.PREMISES["BOX_CONTAINMENT"],
                "outsideBox": len(outside),
            }
            if outside:
                # 八份采集实测 outsideBox 为 0；非 0 是信号，不静默丢。
                assignment["state"] = UNDECIDABLE
                assignment["detail"] = "%d 个字形落在所有行盒之外或命中多盒" % len(outside)
                grouped = None
        else:
            grouped, assignment = pairing.assign_by_line_numbers(
                page_glyphs, page_lines, marks=marks
            )

        if grouped is None:
            counts["undecidable"] += 1
            out["pages"].append(
                {
                    "index": pdf_index,
                    "wordPage": word_page,
                    "state": UNDECIDABLE,
                    "reason": assignment.get("detail") or assignment.get("reason"),
                    "assignment": assignment,
                    "lines": [],
                }
            )
            continue

        lines_out = []
        for li, line in enumerate(page_lines):
            result = pairing.pair_line(
                page=pdf_index,
                line=li,
                text=line["text"],
                source_start=line["start"],
                glyphs=grouped[li],
                marks=marks.get(li),
            )
            counts["lines"] += 1
            counts[{OK: "ok", UNDECIDABLE: "undecidable"}.get(result.state, "fail")] += 1
            lines_out.append(
                {
                    "index": li,
                    "state": result.state,
                    "text": line["text"],
                    "sourceStart": line["start"],
                    "sourceEnd": line["end"],
                    "mode": result.mode,
                    "expected": result.expected,
                    "reasons": _dedupe(result.reasons),
                    "backtest": result.backtest,
                    "identityChecked": result.identity_checked,
                    "identityMismatched": result.identity_mismatched,
                    "glyphs": [
                        {
                            "rule": (
                                result.glyph_rules[gi]
                                if gi < len(result.glyph_rules)
                                else "UNKNOWN"
                            ),
                            "origin": g["glyphOrigin"],
                            "advance": g["advanceVector"],
                            "text": g["text"],
                            "textStatus": g["textStatus"],
                            "fontName": g["fontName"],
                            "sizePt": g["effectiveSizePt"],
                            "rise": g["rise"],
                        }
                        for gi, g in enumerate(grouped[li])
                    ],
                }
            )

        out["pages"].append(
            {
                "index": pdf_index,
                "wordPage": word_page,
                "state": OK,
                "assignment": assignment,
                "width": pdf_page["width"],
                "height": pdf_page["height"],
                "lines": lines_out,
            }
        )

    # 分母照 §7.4 给全：N / M / N−M，且「未核」与「核过无发现」分开。
    out["lineDenominators"] = counts
    if counts["undecidable"] and out["state"] == OK:
        out["state"] = UNDECIDABLE
    return out
