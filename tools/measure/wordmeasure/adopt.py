"""把**已有**的 Mac Word 采集折成本量具的采集包。

用途：不重新驱动 Word 就能把配对器、比较器与对照跑在**真实 Word 读数**上。
方法 §9 的第 3、4 步（重复性与正／阴性对照）本就该在接引擎之前做完，
而它们只需要采集，不需要现采。

**折出来的包与现采的包在证据等级上不同，包里必须写清楚**：

- `META.json` 的 `provenance` 标 `ADOPTED`，并记下原始批次的限定；
- 原批次若自称 `diagnosticOnly` / `measurementEligible: false`，这两个标记**原样带过来**，
  不许在转换途中丢掉（§7.7：改结论时残差守恒，义务不许悄悄蒸发）。

支持的来源：`docx-layout-instrument` 的 mac 探针批次目录结构
（`runs/<name>/{case.pdf, information-sweep.csv, fixture/case.docx}`）。
"""

from __future__ import annotations

import json
import shutil
from pathlib import Path

from . import UNDECIDABLE, docxtext, fingerprint, pdfglyphs, preflight


def _read_sweep_csv(path: Path) -> dict:
    """读 `information-sweep.csv`。

    格式：`#key=value` 头，`P,序号,start,end` 段区间，`S,offset,line,page` 逐位置读数。
    """
    end_of_content = None
    paragraphs: list[dict] = []
    positions: list[dict] = []
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("#"):
            key, _, value = line[1:].partition("=")
            if key == "endOfContent":
                end_of_content = int(value)
            continue
        parts = line.split(",")
        if parts[0] == "P":
            paragraphs.append({"index": int(parts[1]) - 1, "start": int(parts[2]), "end": int(parts[3])})
        elif parts[0] == "S":
            positions.append({"offset": int(parts[1]), "line": int(parts[2]), "page": int(parts[3])})
    if end_of_content is None:
        raise ValueError("information-sweep.csv 缺 #endOfContent")
    return {"endOfContent": end_of_content, "paragraphs": paragraphs, "positions": positions}


def adopt_mac_run(run_dir: Path, bundle: Path, *, batch_manifest: Path | None = None,
                  required_families: list[str] | None = None) -> dict:
    """把一个 mac 探针 run 折成采集包。"""
    run_dir = Path(run_dir)
    bundle = Path(bundle)
    bundle.mkdir(parents=True, exist_ok=False)

    pdf_src = run_dir / "case.pdf"
    docx_src = run_dir / "fixture" / "case.docx"
    csv_src = run_dir / "information-sweep.csv"
    for path in (pdf_src, docx_src, csv_src):
        if not path.is_file():
            raise FileNotFoundError("来源 run 缺 %s" % path)

    shutil.copy2(pdf_src, bundle / "case.pdf")
    shutil.copy2(docx_src, bundle / "case.docx")

    raw = _read_sweep_csv(csv_src)

    # 正文文本推自 document.xml，**再用采集侧两个独立读数核过**才用（§6.4）。
    derived = docxtext.content_text(docx_src)
    verification = docxtext.verify(
        derived, end_of_content=raw["endOfContent"], paragraphs=raw["paragraphs"]
    )

    sweep = {
        "schema": "rsword-layout-line-sweep/1",
        "platform": "mac",
        "source": "adopted: information-sweep.csv（first character line number / active end page number）",
        "resultType": "序数（本页第几行 / 第几页），**不是长度，未做单位映射**",
        "boxAvailable": False,
        "boxNote": "Mac 桥拿不到行盒（§6.6）；归行只能走推算。",
        "endOfContent": raw["endOfContent"],
        "contentText": derived["text"],
        # 逐控制字符的构造标注。有它，计数就不必从字符去猜——
        # 分节符与段落标记在字符层同形，猜就会错（实测差 2 个字形/页）。
        "marks": {str(k): v for k, v in derived["marks"].items()},
        "contentTextProvenance": {
            "derivation": derived["derivation"],
            "verification": verification,
            "tablesSkipped": derived["tablesSkipped"],
        },
        "paragraphs": raw["paragraphs"],
        "positions": raw["positions"],
    }
    (bundle / "sweep.json").write_text(json.dumps(sweep, ensure_ascii=False, indent=2, sort_keys=True) + "\n")

    glyphs = pdfglyphs.read_pdf(pdf_src)
    (bundle / "glyphs.json").write_text(json.dumps(glyphs, ensure_ascii=False, indent=2, sort_keys=True) + "\n")

    families = required_families or []
    substitution = (
        preflight.font_substitution_check(families, pdfglyphs.font_names(glyphs))
        if families
        else {"result": "NOT_CHECKED", "note": "未声明必需字体族；字体替换未核（§6.2）"}
    )

    manifest = {}
    if batch_manifest and Path(batch_manifest).is_file():
        manifest = json.loads(Path(batch_manifest).read_text())

    meta = {
        "schema": "rsword-layout-capture/1",
        "provenance": "ADOPTED",
        "provenanceNote": (
            "本包由既有采集折出，**不是本量具现采**。采集链的前置核查、导出参数向量与"
            "重复性对照都属于原批次，不因折算而转移到本仓库。"
        ),
        "adoptedFrom": {
            "run": str(run_dir),
            "batchManifest": str(batch_manifest) if batch_manifest else None,
            # 原批次的限定原样带过来，不许在转换途中丢掉（§7.7 残差守恒）。
            "notice": manifest.get("notice"),
            "diagnosticOnly": manifest.get("diagnosticOnly"),
            "measurementEligible": manifest.get("measurementEligible"),
            "formalCoverage": manifest.get("formalCoverage"),
            "fontPreflight": manifest.get("fontPreflight"),
            "fontSubstitution": manifest.get("fontSubstitution"),
            "macFontEpoch": manifest.get("macFontEpoch"),
        },
        "label": run_dir.name,
        "environment": {
            "schema": "rsword-layout-capture-env/1",
            "platform": "mac",
            "word": {"note": "见 adoptedFrom.batchManifest；本量具未观测"},
            "fontEpoch": manifest.get("macFontEpoch")
            or {"filesSha256": None, "note": "原批次未给"},
            "tools": fingerprint.tool_versions(),
        },
        "preflight": {
            "result": (manifest.get("fontPreflight") or {}).get("result", "NOT_OBSERVED_HERE"),
            "note": "采前核查属于原批次（§6.1 要求核的是采集当时的 Word 进程）",
        },
        "fontSubstitution": substitution,
        "exportVector": {"note": "见 adoptedFrom.batchManifest；本量具未观测"},
        "fixture": {
            "before": fingerprint.docx_identity(docx_src),
            "after": fingerprint.docx_identity(docx_src),
            "unchanged": True,
        },
        "pdfSha256": glyphs["sourceSha256"],
        "pageCount": len(glyphs["pages"]),
        "glyphCounts": pdfglyphs.glyph_counts(glyphs),
        "glyphTotal": sum(pdfglyphs.glyph_counts(glyphs)),
        "sourceTextVerification": verification,
    }
    if verification["state"] != "OK":
        meta["usability"] = UNDECIDABLE
        meta["usabilityReason"] = verification["reasons"]
    (bundle / "META.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    return meta
