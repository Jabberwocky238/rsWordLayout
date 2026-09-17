"""采集链（方法 §9.1）。

一次采集产出一个**采集包**（目录），里面同时有结构通道与几何通道，外加环境指纹：

    <bundle>/
      META.json            环境指纹、Word build、字体 epoch、导出参数向量、夹具身份
      preflight.json       §6.1 采前核查结果（唯一防线）
      case.docx            夹具原件（按字节拷进来，采前采后核 sha256）
      case.pdf             Word 导出的 PDF
      sweep.json           行号扫描：逐字符位置的 (页, 行) 与段落区间
      glyphs.json          PDF 逐字形几何

Mac 通道的边界（§6.6）：**没有行盒**。所以本采集包里没有 `lines[].box`，
归行只能走 `pairing.assign_by_line_numbers`（推算）。Windows 通道若接上，
把行盒填进 `sweep.json` 的 `lines[].box`，配对器会自动改走盒包含。

导出参数（§2.2）：**必须逐位显式传，不要用默认值**——默认里有会改变输出的项。
用了哪个参数向量记进 `META.json` 的 `exportVector`。
"""

from __future__ import annotations

import json
import shutil
import time
from pathlib import Path

from . import fingerprint, pdfglyphs, preflight
from .applescript import literal, tell_word

# §2.2：导出参数向量逐位显式。Mac 没有 XPS，只有 save as + format PDF（§6.6）。
EXPORT_VECTOR = {
    "channel": "mac AppleScript `save as`",
    "fileFormat": "format PDF (WdSaveFormat 0x02310011)",
    "addToRecentFiles": False,
    "lockComments": False,
    "readOnlyRecommended": False,
    "embedTruetypeFonts": False,
    "note": "Mac Word 无 XPS 导出（§6.6）。ExportAsFixedFormat 的完整参数向量在 Mac 桥上不可达，"
            "这里记的是实际传出的每一位。",
}


def _sweep_script(doc_expr: str) -> str:
    """一次 AppleScript 跑完整段行号扫描。

    用 `first character line number` 配 `active end page number`（§2.1 的第三条通道）。
    返回的是**序数**（本页第几行 / 第几页），**不是长度，没有单位映射**——
    Mac 的 `Line` 返回 Long 且单位未映射，拿不到行盒（§6.6），所以这里只当行划分用。

    逐位置发一次 Apple event 太慢，且字符串累加是 O(n²)；用列表攒、末尾一次 join。
    """
    return f"""
set theDoc to {doc_expr}
set theRange to text object of theDoc
set eoc to end of content of theRange
set rows to {{}}
repeat with i from 0 to (eoc - 1)
    set r to create range theDoc start i end i
    set ln to (get range information r information type first character line number)
    set pg to (get range information r information type active end page number)
    set end of rows to ((i as text) & "," & (ln as text) & "," & (pg as text))
end repeat
set AppleScript's text item delimiters to linefeed
set out to rows as text
set AppleScript's text item delimiters to ""
return (eoc as text) & linefeed & "---" & linefeed & out
"""


def _paragraphs_script(doc_expr: str) -> str:
    """逐段的字符区间与文本（§2.1：`Document.Paragraphs`）。

    文本里的 `\\r` / `\\x0b` / `\\x0c` 是计数约定的输入（§4），所以必须原样带回来，
    不能让 AppleScript 的行处理把它们吃掉——用十六进制转义传回。
    """
    return f"""
set theDoc to {doc_expr}
set rows to {{}}
repeat with p in paragraphs of text object of theDoc
    set r to text object of p
    set s to start of content of r
    set e to end of content of r
    set end of rows to ((s as text) & "," & (e as text))
end repeat
set AppleScript's text item delimiters to linefeed
set out to rows as text
set AppleScript's text item delimiters to ""
return out
"""


def _content_text_script(doc_expr: str) -> str:
    """整篇正文文本。

    Word 的字符偏移空间就是这个串的下标，引擎侧的 `sourceStart` 要对齐到它
    （见 `tools/measure/README.md` 的「偏移空间」一节）。
    """
    return f"return content of text object of {doc_expr}"


def _decode_rows(text: str) -> list[tuple[int, ...]]:
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        rows.append(tuple(int(v) for v in line.split(",")))
    return rows


def open_document(path: Path, timeout: float = 180.0) -> str:
    """打开夹具，返回 Word 里的文档名。

    **不改文档**：只读，采完 `saving no` 关掉。
    Mac Word 的文件授权绑文件身份不绑路径（§6.6），同一路径换一份文件会重新弹授权框——
    所以这一步可能需要人工点一次授权，超时按未完成处理。
    """
    path = Path(path).resolve(strict=True)
    body = (
        "set theDoc to open file name %s add to recent files false\n"
        "return name of theDoc" % literal(str(path))
    )
    return tell_word(body, timeout=timeout).strip()


def export_pdf(doc_expr: str, out_pdf: Path, timeout: float = 300.0) -> None:
    body = (
        "save as %s file name %s file format format PDF "
        "add to recent files false lock comments false "
        "read only recommended false embed truetype fonts false"
        % (doc_expr, literal(str(out_pdf)))
    )
    tell_word(body, timeout=timeout)


def close_document(doc_expr: str, timeout: float = 120.0) -> None:
    tell_word("close %s saving no" % doc_expr, timeout=timeout)


def capture(
    docx: Path,
    bundle: Path,
    *,
    required_families: list[str],
    label: str | None = None,
    include_font_files: bool = False,
) -> dict:
    """跑一次完整采集，写出采集包。返回 META.json 的内容。

    顺序是有讲究的（§9.1 / §9.2）：**先核字体，再打开文档**。
    核查不过就不采——采了也是废数据，而且废得看不出来（§6.2）。
    """
    docx = Path(docx).resolve(strict=True)
    bundle = Path(bundle)
    bundle.mkdir(parents=True, exist_ok=False)

    # 采前夹具身份（§6.3：单独记 document.xml 的 sha256）。
    identity_before = fingerprint.docx_identity(docx)

    # §9.2 前置核查。这道核查是唯一防线。
    pre = preflight.preflight(required_families)
    (bundle / "preflight.json").write_text(
        json.dumps(pre, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )
    preflight.assert_pass(pre)

    env = fingerprint.capture_environment(include_font_files=include_font_files)

    shutil.copy2(docx, bundle / "case.docx")
    pdf_path = bundle / "case.pdf"

    timings = {}
    doc_expr = "theDoc"
    name = open_document(docx)
    doc_expr = "document %s" % literal(name)
    try:
        started = time.monotonic()
        content = tell_word(_content_text_script(doc_expr))
        paragraphs = _decode_rows(tell_word(_paragraphs_script(doc_expr)))
        timings["structureSeconds"] = time.monotonic() - started

        started = time.monotonic()
        export_pdf(doc_expr, pdf_path)
        timings["exportSeconds"] = time.monotonic() - started

        started = time.monotonic()
        raw = tell_word(_sweep_script(doc_expr), timeout=1800.0)
        timings["sweepSeconds"] = time.monotonic() - started
    finally:
        try:
            close_document(doc_expr)
        except Exception:
            pass  # 关不掉不改采集结果；留给下游从进程状态判断。

    head, _, body = raw.partition("\n---\n")
    end_of_content = int(head.strip())
    sweep_rows = _decode_rows(body)

    sweep = {
        "schema": "rsword-layout-line-sweep/1",
        "platform": "mac",
        "source": "AppleScript get range information; "
                  "information type = first character line number / active end page number",
        "resultType": "text（sdef 声明 result type text）；按 int 解析；"
                      "序数（本页第几行 / 第几页），**不是长度，未做单位映射**",
        "boxAvailable": False,
        "boxNote": "Mac 桥拿不到行盒（§6.6）；归行只能走推算，见 pairing.PREMISES。",
        "endOfContent": end_of_content,
        "contentText": content,
        "paragraphs": [{"index": i, "start": s, "end": e} for i, (s, e) in enumerate(paragraphs)],
        "positions": [{"offset": o, "line": ln, "page": pg} for o, ln, pg in sweep_rows],
    }
    (bundle / "sweep.json").write_text(
        json.dumps(sweep, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )

    glyphs = pdfglyphs.read_pdf(pdf_path)
    (bundle / "glyphs.json").write_text(
        json.dumps(glyphs, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )

    # §6.2 采后核字体名。几何自检发现不了字体替换，只有字体名能。
    substitution = preflight.font_substitution_check(required_families, pdfglyphs.font_names(glyphs))

    identity_after = fingerprint.docx_identity(docx)

    meta = {
        "schema": "rsword-layout-capture/1",
        "label": label or docx.stem,
        "capturedAtUtc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "environment": env,
        "preflight": pre,
        "fontSubstitution": substitution,
        "exportVector": EXPORT_VECTOR,
        "fixture": {
            "before": identity_before,
            "after": identity_after,
            # 采前采后夹具字节必须相同；不同就说明采集过程中动了夹具，读数作废。
            "unchanged": identity_before["sha256"] == identity_after["sha256"],
        },
        "pdfSha256": glyphs["sourceSha256"],
        "pageCount": len(glyphs["pages"]),
        "glyphCounts": pdfglyphs.glyph_counts(glyphs),
        "glyphTotal": sum(pdfglyphs.glyph_counts(glyphs)),
        "timings": timings,
        "wordDocumentName": name,
    }
    (bundle / "META.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )
    return meta


def load_bundle(bundle: Path) -> dict:
    """读一个采集包。缺件即报错，不用默认值补。"""
    bundle = Path(bundle)
    out = {}
    for name in ("META.json", "sweep.json", "glyphs.json"):
        path = bundle / name
        if not path.is_file():
            raise FileNotFoundError("采集包缺 %s：%s" % (name, bundle))
        out[name.removesuffix(".json")] = json.loads(path.read_text())
    out["path"] = str(bundle)
    return out
