"""比较器（方法 §9.6）。

页 → 行 → 字形三层配对。**任何一层数目对不上即结构失败，不修配对再比。**
判定用欧氏距离。输出**先给 `state` 再给 `maxAbs`**。

最后一条不是格式洁癖，是 §6.5 的难例：两份计数层完全相同、只是行长排列不同的文档，
比较时 3 行结构失败、**剩下 1 行 8 个字形距离 0.0000pt**。
只读 `maxAbs` 会把「配对失败」读成「完全一致」。所以 `state` 在 dict 里排第一，
`summary()` 也先印 state——**写进比较器的输出契约，不只写进文档**。

噪声底是 **0.0000pt**（34 对采集、38,845 对字形，同批次内）。
所以默认容差是 0：引擎对 Word 出现任何非零差，要么是引擎错，要么是配对错，
**不可能是「量得不准」**。给非零 `tolerance` 时报告会标出来，免得悄悄放水。
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

from . import FAIL, OK, UNDECIDABLE

# §5：同条件重复采集的噪声底。范围：同机、同 build、同字体 epoch、**同批次**。
# 跨批次跨 build 未测（§8）。
NOISE_FLOOR_PT = 0.0


@dataclass
class GlyphDiff:
    page: int
    line: int
    index: int
    reference: tuple[float, float]
    candidate: tuple[float, float]
    text: str | None

    @property
    def dx(self) -> float:
        return self.candidate[0] - self.reference[0]

    @property
    def dy(self) -> float:
        return self.candidate[1] - self.reference[1]

    @property
    def distance(self) -> float:
        return math.hypot(self.dx, self.dy)


@dataclass
class Comparison:
    """比较结果。字段顺序即阅读顺序：state 先于任何数值。"""

    state: str
    structure: dict = field(default_factory=dict)
    pairs: int = 0
    max_abs: float | None = None
    max_dx: float | None = None
    max_dy: float | None = None
    worst: list[GlyphDiff] = field(default_factory=list)
    failures: list[dict] = field(default_factory=list)
    tolerance_pt: float = NOISE_FLOOR_PT

    def to_dict(self) -> dict:
        # dict 顺序即输出顺序：state 第一，maxAbs 在它后面（§6.5）。
        out = {
            "state": self.state,
            "structure": self.structure,
            "pairs": self.pairs,
            "tolerancePt": self.tolerance_pt,
            "toleranceNote": (
                "噪声底 0.0000pt（§5）：非零差不可能是量得不准。"
                if self.tolerance_pt == 0
                else "**非零容差**：调用方显式放宽到 %.6fpt，不是方法给的。" % self.tolerance_pt
            ),
            "structurallySound": self.structurally_sound,
            "maxAbs": self.max_abs,
            "maxDx": self.max_dx,
            "maxDy": self.max_dy,
            "worst": [
                {
                    "page": d.page,
                    "line": d.line,
                    "index": d.index,
                    "text": d.text,
                    "reference": list(d.reference),
                    "candidate": list(d.candidate),
                    "dx": d.dx,
                    "dy": d.dy,
                    "distance": d.distance,
                }
                for d in self.worst
            ],
            "failures": self.failures,
        }
        return out

    # 只表示「几何比过了」的失败码。其余失败码意味着某一层没配上，
    # 那时 maxAbs 只覆盖侥幸配上的那部分，不能当作整体读数（§6.5）。
    GEOMETRY_ONLY_FAILURES = frozenset({"GEOMETRY_EXCEEDS_TOLERANCE"})

    @property
    def structurally_sound(self) -> bool:
        """三层配对有没有全程对上。

        这与 `state == OK` 是**两件事**：结构全对但几何超容差时，
        `state` 是 FAIL 而结构是好的，`maxAbs` 此时可读。
        反过来结构没对上时 `maxAbs` 只覆盖侥幸配上的那部分——§6.5 的难例就死在这里。
        """
        return all(f["code"] in self.GEOMETRY_ONLY_FAILURES for f in self.failures)

    def summary(self) -> str:
        head = "state=%s  pairs=%d" % (self.state, self.pairs)
        numbers = "maxAbs=%.4fpt  maxDx=%.4fpt  maxDy=%.4fpt" % (
            self.max_abs or 0.0,
            self.max_dx or 0.0,
            self.max_dy or 0.0,
        )
        if self.state == OK:
            return "%s  %s" % (head, numbers)
        detail = "; ".join(dict.fromkeys(f["code"] for f in self.failures))[:120] or "—"
        if self.structurally_sound and self.max_abs is not None:
            # 结构全程对上，只是几何超了容差：这时 maxAbs 是整体读数，可读。
            return "%s  失败=%s  %s" % (head, detail, numbers)
        # 有层没配上：先读 state，maxAbs 只覆盖侥幸配上的那部分，不可当整体读数。
        return "%s  失败=%s  （结构未全程对上，maxAbs 不可作整体读数）" % (head, detail)


def exclude_glyphs(model: dict, rules: set[str]) -> tuple[dict, dict]:
    """按 §4 的约定名从验收范围里**显式排除**某类字形。

    这是方法自己用过的处置（§8）：8.5% 的字形不在行划分覆盖内，
    「目前的处理是**显式写进验收定义里排除**，不是给出归属」。

    与放宽容差的区别是关键，不是措辞：

    - 排除改的是**验收定义的范围**，被排除的东西在报告里逐条点名、给分母；
    - 放宽容差改的是**判据本身**，而噪声底是 0（§5），放宽就是在给引擎的错找地方藏。

    返回 (排除后的模型, 排除记录)。排除记录必须随结果一起印出来——
    否则下游会把「在缩小的范围上通过」读成「通过」。
    """
    if not rules:
        return model, {"excludedRules": [], "excludedGlyphs": 0, "totalGlyphs": 0}

    total = 0
    removed = 0
    per_rule: dict[str, int] = {}
    pages = []
    for page in model.get("pages", []):
        lines = []
        for line in page.get("lines", []):
            kept = []
            for glyph in line.get("glyphs", []):
                total += 1
                rule = glyph.get("rule")
                if rule in rules:
                    removed += 1
                    per_rule[rule] = per_rule.get(rule, 0) + 1
                else:
                    kept.append(glyph)
            lines.append({**line, "glyphs": kept})
        pages.append({**page, "lines": lines})

    record = {
        "basis": "§8：不在覆盖内的字形，处置是显式写进验收定义里排除，不是给出归属。",
        "excludedRules": sorted(rules),
        "excludedGlyphs": removed,
        "totalGlyphs": total,
        "perRule": per_rule,
        "warning": (
            "**验收范围已缩小。** 下面的 state 与 maxAbs 只在剩下的 %d / %d 个字形上成立。"
            % (total - removed, total)
        ),
    }
    return {**model, "pages": pages}, record


def compare(reference: dict, candidate: dict, *, tolerance_pt: float = NOISE_FLOOR_PT,
            worst_n: int = 10) -> Comparison:
    """比较两份「页 → 行 → 字形」结构。

    `reference` 通常是 Word 侧，`candidate` 是引擎侧；两者都要是
    `{"pages":[{"lines":[{"glyphs":[{"origin":[x,y], ...}]}]}]}` 形状，单位点。

    结构失败**不再往下比几何**：修配对再比出来的数不是测量，是构造。
    """
    result = Comparison(state=OK, tolerance_pt=tolerance_pt)

    ref_pages, cand_pages = reference["pages"], candidate["pages"]
    result.structure = {
        "referencePages": len(ref_pages),
        "candidatePages": len(cand_pages),
        "referenceGlyphs": sum(len(l.get("glyphs", [])) for p in ref_pages for l in p.get("lines", [])),
        "candidateGlyphs": sum(len(l.get("glyphs", [])) for p in cand_pages for l in p.get("lines", [])),
    }

    if len(ref_pages) != len(cand_pages):
        result.state = FAIL
        result.failures.append(
            {
                "code": "PAGE_COUNT_MISMATCH",
                "reference": len(ref_pages),
                "candidate": len(cand_pages),
                "note": "第一层就对不上；不修配对再比（§9.6）",
            }
        )
        return result

    diffs: list[GlyphDiff] = []
    undecidable = 0

    for pi, (ref_page, cand_page) in enumerate(zip(ref_pages, cand_pages)):
        ref_lines, cand_lines = ref_page.get("lines", []), cand_page.get("lines", [])

        if ref_page.get("state") == UNDECIDABLE or cand_page.get("state") == UNDECIDABLE:
            undecidable += 1
            result.failures.append(
                {
                    "code": "PAGE_UNDECIDABLE",
                    "page": pi,
                    "note": ref_page.get("reason") or cand_page.get("reason"),
                }
            )
            continue

        if len(ref_lines) != len(cand_lines):
            result.state = FAIL
            result.failures.append(
                {
                    "code": "LINE_COUNT_MISMATCH",
                    "page": pi,
                    "reference": len(ref_lines),
                    "candidate": len(cand_lines),
                }
            )
            continue

        for li, (ref_line, cand_line) in enumerate(zip(ref_lines, cand_lines)):
            ref_glyphs = ref_line.get("glyphs", [])
            cand_glyphs = cand_line.get("glyphs", [])

            if ref_line.get("state") == UNDECIDABLE:
                undecidable += 1
                result.failures.append(
                    {
                        "code": "LINE_UNDECIDABLE",
                        "page": pi,
                        "line": li,
                        "note": ref_line.get("reasons"),
                    }
                )
                continue

            if len(ref_glyphs) != len(cand_glyphs):
                result.state = FAIL
                result.failures.append(
                    {
                        "code": "GLYPH_COUNT_MISMATCH",
                        "page": pi,
                        "line": li,
                        "reference": len(ref_glyphs),
                        "candidate": len(cand_glyphs),
                        "referenceText": ref_line.get("text"),
                        "candidateText": cand_line.get("text"),
                    }
                )
                continue

            for gi, (ref_glyph, cand_glyph) in enumerate(zip(ref_glyphs, cand_glyphs)):
                diffs.append(
                    GlyphDiff(
                        page=pi,
                        line=li,
                        index=gi,
                        reference=tuple(ref_glyph["origin"]),
                        candidate=tuple(cand_glyph["origin"]),
                        text=ref_glyph.get("text"),
                    )
                )

    result.pairs = len(diffs)

    if undecidable and result.state == OK:
        # 判不了是独立出口，不能折叠进 FAIL，也不能当成通过（§7.3）。
        result.state = UNDECIDABLE

    if not diffs:
        if result.state == OK:
            result.state = UNDECIDABLE
            result.failures.append(
                {"code": "NO_PAIRS", "note": "一对字形都没配上；没有可比的量（§7.3 判不了）"}
            )
        return result

    result.max_abs = max(d.distance for d in diffs)
    result.max_dx = max(abs(d.dx) for d in diffs)
    result.max_dy = max(abs(d.dy) for d in diffs)
    result.worst = sorted(diffs, key=lambda d: -d.distance)[:worst_n]

    if result.state == OK and result.max_abs > tolerance_pt:
        result.state = FAIL
        result.failures.append(
            {
                "code": "GEOMETRY_EXCEEDS_TOLERANCE",
                "maxAbs": result.max_abs,
                "tolerancePt": tolerance_pt,
                "note": "噪声底为 0（§5）：这不是量得不准，是引擎错或配对错。",
            }
        )
    return result
