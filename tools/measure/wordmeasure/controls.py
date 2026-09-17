"""对照与证否条件（方法 §9.3 / §9.4 / §7.1）。

§9 的搭建顺序里，**对照排在配对器之前**：

    3. 重复性对照：同一份夹具同条件采两次，比 glyphOrigin。应当逐位相同。
       **不相同就先别往下走**——那说明采集链本身不确定。
    4. 配对器：先跑正对照（同条件两次采集，预期 Δ = 0）与
       阴性对照（两份不同夹具，预期配对失败）。四条证否条件一条都不该触发。

证否条件在**看数据之前**就写死在这里（§7.1）。判据的可执行形式与判据文一起提交，
事后有没有动手，一条 `git diff` 就能核。
"""

from __future__ import annotations

from . import FAIL, OK, UNDECIDABLE, compare, wordmodel

# 四条证否条件。**预注册**：先写死再看数据。任一条触发即量具本身不可信，
# 此时引擎的任何读数都不该被引用。
FALSIFIERS = {
    "F1_POSITIVE_CONTROL_NONZERO": (
        "同条件两次采集之间出现任何非零 glyphOrigin 差。"
        "噪声底是 0.0000pt（34 对采集、38,845 对字形，§5），"
        "所以非零即采集链本身不确定，不是量得不准。"
    ),
    "F2_NEGATIVE_CONTROL_PASSES": (
        "两份不同夹具比出 state=OK。"
        "§6.5 的难例：两份只是行长排列不同的文档，3 行结构失败、"
        "剩下 1 行 8 个字形距离 0.0000pt——只读 maxAbs 就会把配对失败读成完全一致。"
    ),
    "F3_IDENTITY_MISMATCH_ON_OK_LINE": (
        "某行配对器判 OK，但配对后的字符身份核对出现不符。"
        "身份不参与配对（§3.1），但配对对了身份就该对得上；不符说明读序前提出了问题。"
    ),
    "F4_UNDECIDABLE_SILENTLY_ABSORBED": (
        "存在判不了的页或行，而顶层 state 却是 OK。"
        "「判不了」是独立出口，取消它这套东西只会输出「成立」（§7.3）。"
    ),
}


def glyph_origins(bundle: dict) -> list[tuple[int, int, float, float]]:
    """采集包里全部字形原点，按 (页, 序号) 排好。逐位比较用。"""
    out = []
    for page in bundle["glyphs"]["pages"]:
        for glyph in page["glyphs"]:
            out.append((page["index"], glyph["index"], glyph["glyphOrigin"][0], glyph["glyphOrigin"][1]))
    return out


def repeatability(bundle_a: dict, bundle_b: dict) -> dict:
    """§9.3 重复性对照：同一份夹具同条件采两次，`glyphOrigin` 应逐位相同。

    **不相同就先别往下走。** 返回的 `maxAbs` 只有在 `state != FAIL` 时才有解读意义（§6.5）。
    """
    a, b = glyph_origins(bundle_a), glyph_origins(bundle_b)

    def same(*path):
        """比两个包里同一条指纹。任一边缺就返回 None——「未核」不是「核过相同」（§7.4）。"""
        def dig(bundle):
            node = bundle["META"]
            for key in path:
                if not isinstance(node, dict) or key not in node:
                    return None
                node = node[key]
            return node
        left, right = dig(bundle_a), dig(bundle_b)
        return None if left is None or right is None else left == right

    record = {
        "state": OK,
        "check": "§9.3 重复性对照",
        "countA": len(a),
        "countB": len(b),
        "sameFixture": same("fixture", "before", "sha256"),
        "sameFontEpoch": same("environment", "fontEpoch", "filesSha256"),
        "sameWordBuild": same("environment", "word", "build"),
        "maxAbs": None,
        "bitwiseIdentical": None,
        "falsifiers": [],
    }
    unchecked = [k for k in ("sameFontEpoch", "sameWordBuild") if record[k] is None]
    if unchecked:
        # 同批次是噪声底 0 成立的前提（§5）。核不了就得说核不了。
        record["unverifiedConditions"] = unchecked
        record["conditionNote"] = (
            "噪声底 0.0000pt 只在同机、同 build、同字体 epoch、**同批次**内成立（§5）；"
            "这些条件本次未能核（采集包没带该字段）。"
        )
    if record["sameFixture"] is not True:
        record["state"] = UNDECIDABLE
        record["reason"] = "两次采集的夹具 sha256 不同；这不是重复性对照。"
        return record
    if len(a) != len(b):
        record["state"] = FAIL
        record["reason"] = "字形总数不同：%d vs %d" % (len(a), len(b))
        record["falsifiers"].append("F1_POSITIVE_CONTROL_NONZERO")
        return record

    deltas = [max(abs(x1 - x2), abs(y1 - y2)) for (_, _, x1, y1), (_, _, x2, y2) in zip(a, b)]
    record["maxAbs"] = max(deltas) if deltas else 0.0
    record["bitwiseIdentical"] = all(ra == rb for ra, rb in zip(a, b))
    if not record["bitwiseIdentical"]:
        record["state"] = FAIL
        record["reason"] = (
            "同条件两次采集的 glyphOrigin 不是逐位相同（max|Δ| = %.6fpt）。"
            "采集链本身不确定，先别往下走（§9.3）。" % record["maxAbs"]
        )
        record["falsifiers"].append("F1_POSITIVE_CONTROL_NONZERO")
    return record


def positive_control(bundle_a: dict, bundle_b: dict) -> dict:
    """§9.4 正对照：同条件两次采集，经**完整配对器与比较器**走一遍，预期 Δ = 0。

    与 `repeatability` 的区别：那条只比原始读数，这条把配对器也放进回路——
    配对器出错时原始读数照样逐位相同，但比较结果会崩。
    """
    model_a, model_b = wordmodel.build(bundle_a), wordmodel.build(bundle_b)
    result = compare.compare(model_a, model_b)
    record = {
        "state": result.state,
        "check": "§9.4 正对照（同条件两次采集，预期 Δ = 0）",
        "comparison": result.to_dict(),
        "falsifiers": [],
    }
    if result.state == OK and (result.max_abs or 0.0) > 0.0:
        record["falsifiers"].append("F1_POSITIVE_CONTROL_NONZERO")
        record["state"] = FAIL
    if result.state == FAIL:
        record["falsifiers"].append("F1_POSITIVE_CONTROL_NONZERO")
    return record


def negative_control(bundle_a: dict, bundle_b: dict) -> dict:
    """§9.4 阴性对照：两份**不同**夹具，预期配对失败。

    这条是防自欺的关键：一个只会说「一致」的比较器在正对照上也是满分。
    """
    if bundle_a["META"]["fixture"]["before"]["sha256"] == bundle_b["META"]["fixture"]["before"]["sha256"]:
        return {
            "state": UNDECIDABLE,
            "check": "§9.4 阴性对照",
            "reason": "两个采集包是同一份夹具；这不是阴性对照。",
            "falsifiers": [],
        }
    model_a, model_b = wordmodel.build(bundle_a), wordmodel.build(bundle_b)
    result = compare.compare(model_a, model_b)
    record = {
        "state": OK if result.state != OK else FAIL,
        "check": "§9.4 阴性对照（两份不同夹具，预期配对失败）",
        "expected": "state != OK",
        "observed": result.state,
        "comparison": result.to_dict(),
        "falsifiers": [],
    }
    if result.state == OK:
        record["falsifiers"].append("F2_NEGATIVE_CONTROL_PASSES")
    return record


def scan_falsifiers(model: dict, comparison: compare.Comparison | None = None) -> list[dict]:
    """在一份模型（与可选的比较结果）上扫 F3 / F4。"""
    hits = []
    for page in model.get("pages", []):
        for line in page.get("lines", []):
            if line.get("state") == OK and line.get("identityMismatched", 0) > 0:
                hits.append(
                    {
                        "falsifier": "F3_IDENTITY_MISMATCH_ON_OK_LINE",
                        "page": page["index"],
                        "line": line["index"],
                        "identityChecked": line["identityChecked"],
                        "identityMismatched": line["identityMismatched"],
                        "text": line.get("text"),
                    }
                )
    has_undecidable = any(
        page.get("state") == UNDECIDABLE
        or any(l.get("state") == UNDECIDABLE for l in page.get("lines", []))
        for page in model.get("pages", [])
    )
    if has_undecidable and model.get("state") == OK:
        hits.append({"falsifier": "F4_UNDECIDABLE_SILENTLY_ABSORBED", "scope": "model"})
    if comparison is not None:
        absorbed = any(f["code"].endswith("UNDECIDABLE") for f in comparison.failures)
        if absorbed and comparison.state == OK:
            hits.append({"falsifier": "F4_UNDECIDABLE_SILENTLY_ABSORBED", "scope": "comparison"})
    return hits
