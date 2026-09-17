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
set acc to {{}}
repeat with i from 0 to (eoc - 1)
    set r to create range theDoc start i end i
    set ln to (get range information r information type first character line number)
    set pg to (get range information r information type active end page number)
    set end of acc to ((i as text) & "," & (ln as text) & "," & (pg as text))
end repeat
set AppleScript's text item delimiters to linefeed
set out to acc as text
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
set acc to {{}}
set n to count of paragraphs of text object of theDoc
repeat with i from 1 to n
    set r to text object of (paragraph i of text object of theDoc)
    set s to start of content of r
    set e to end of content of r
    set end of acc to ((s as text) & "," & (e as text))
end repeat
set AppleScript's text item delimiters to linefeed
set out to acc as text
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


def open_document(path: Path, timeout: float = 1800.0) -> str:
    """打开夹具，返回 Word 里的文档名。

    **不改文档**：只读，采完 `saving no` 关掉。
    这一步**可能要人点一次授权框**，所以超时按「人的尺度」给（1800 秒），
    与 `export_pdf` 同一条理由：180 秒不够一个人走到电脑前，而超时会把采集打断，
    框也跟着消失，下次还得从头再来。

    授权绑的是**路径**，不是文件内容——实测：把已授权路径上的文件内容整个换掉，
    Word 再开它不弹框，且读到的是新内容。（**不是**绑 inode：把已授权文件硬链接到
    新路径，新路径照样弹框。）`fill_slot` 就是靠这条规律做的，见那里的说明。
    """
    path = Path(path).resolve(strict=True)
    body = (
        "set theDoc to open file name %s add to recent files false\n"
        "return name of theDoc" % literal(str(path))
    )
    return tell_word(body, timeout=timeout).strip()


def export_pdf(doc_expr: str, out_pdf: Path, timeout: float = 1800.0) -> None:
    """导出 PDF。

    超时给得很长，理由与 `open_document` 同一条（§6.6）：Mac Word 的沙箱按
    **文件夹**授权，采集包目录第一次用会弹授权框，而那个框要人点。
    300 秒不够一个人走到电脑前——超时把采集打断，框也跟着没了，
    下一次还得从头再来一遍。宁可等。
    """
    body = (
        "save as %s file name %s file format format PDF "
        "add to recent files false lock comments false "
        "read only recommended false embed truetype fonts false"
        % (doc_expr, literal(str(out_pdf)))
    )
    tell_word(body, timeout=timeout)


def close_document(doc_expr: str, timeout: float = 120.0) -> None:
    tell_word("close %s saving no" % doc_expr, timeout=timeout)


# 「取件槽」：一个**固定身份**的文件，每次采集把夹具的字节**原地**写进去，让 Word 开它。
DEFAULT_SLOT = Path.home() / "Documents" / "rsword-captures" / "_word-slot.docx"


def fill_slot(docx: Path, slot: Path) -> dict:
    """把 `docx` 的字节原地写进 `slot`，返回身份记录。

    # 为什么要这么绕

    Mac Word 的沙箱授权绑**文件身份**，不绑路径，也**不随目录授权传递**——
    实测：把一个新文件放进已授权的目录里，开它照样弹框。所以每做一份新夹具，
    就得有人去点一次「授予文件访问权限」。

    但同一份实测也表明：**把已授权文件的内容整个换掉，授权仍然有效**
    （inode 不变，Word 开它不弹框，读到的是新内容）。

    于是：留一个固定的槽文件，**只授权它一次**，以后每次采集把夹具的字节
    `r+b` + `truncate` 原地灌进去——inode 不变，授权就一直在。
    只有**第一次**创建这个槽需要人点一下。

    # 为什么要核对哈希

    槽是「Word 实际读到的那份」。它若与夹具不一致，量出来的就是**另一份文档**的排版，
    而几何上完全看不出来——这正是 §6.3 要挡的那类错。所以灌完必须逐字节核对，
    不一致就拒绝采集，而不是继续跑。
    """
    slot = Path(slot)
    slot.parent.mkdir(parents=True, exist_ok=True)
    data = Path(docx).read_bytes()
    created = not slot.exists()
    if created:
        slot.write_bytes(data)
    else:
        # **原地**改写：不能用 shutil.copy2 之外的「先删后建」，那会换 inode，授权就没了。
        with open(slot, "r+b") as handle:
            handle.truncate(0)
            handle.write(data)
    written = slot.read_bytes()
    if written != data:
        raise ValueError("SLOT_MISMATCH: 槽里的字节与夹具不一致，拒绝采集：%s" % slot)
    return {
        "path": str(slot),
        "sha256": fingerprint.sha256_bytes(written),
        "created": created,
        "note": ("槽是新建的，Word 会为它弹一次授权框；此后同一个槽不再弹。"
                 if created else "复用已授权的槽，不弹授权框。"),
    }


def capture(
    docx: Path,
    bundle: Path,
    *,
    required_families: list[str],
    label: str | None = None,
    include_font_files: bool = False,
    slot: Path | None = DEFAULT_SLOT,
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

    # 让 Word 开固定身份的槽，而不是夹具本身——否则每做一份新夹具就要人点一次授权。
    # 顺带还有个好处：Word 碰不到夹具本身，连改坏的可能都没有。
    slot_identity = fill_slot(docx, slot) if slot else None
    to_open = Path(slot_identity["path"]) if slot_identity else docx

    timings = {}
    doc_expr = "theDoc"
    name = open_document(to_open)
    doc_expr = "document %s" % literal(name)
    try:
        started = time.monotonic()
        # 这两条与扫描同一个量级的超时：它们都**按段落数线性增长**，
        # 而夹具的段落数是设计变量。762 段的段落区间枚举就超过了默认的 120 秒——
        # 默认值是按「几十段的夹具」定的，夹具一长就不够用，且失败得莫名其妙。
        content = tell_word(_content_text_script(doc_expr), timeout=1800.0)
        paragraphs = _decode_rows(tell_word(_paragraphs_script(doc_expr), timeout=1800.0))
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
        # 槽的哈希与夹具的哈希必须一致（`fill_slot` 已逐字节核过）。记在这里，
        # 是为了让「Word 到底读的是哪份字节」在采集包里可核，而不是靠相信。
        "slot": slot_identity,
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
