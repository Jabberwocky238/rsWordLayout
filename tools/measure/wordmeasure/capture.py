"""采集链（方法 §9.1）。

一次采集产出一个**采集包**（目录），里面同时有结构通道与几何通道，外加环境指纹：

    <bundle>/
      META.json            环境指纹、Word build、字体 epoch、导出参数向量、夹具身份
      preflight.json       §6.1 采前核查结果（唯一防线）
      case.docx            夹具原件（按字节拷进来，采前采后核 sha256）
      case.pdf             Word 导出的 PDF
      sweep.json           行号扫描：逐字符位置的 (页, 行) 与段落区间
      sweep-first.raw.txt   首次扫描的原始 AppleScript 回执
      sweep-repeat.raw.txt  紧接复扫的原始回执（不覆盖首次扫描）
      sweep-repeat.json    复扫读数与验证结果；META.sweepStability 记录比较
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

from . import OK, UNDECIDABLE, docxtext, fingerprint, pdfglyphs, preflight, sweep_stability
from .applescript import AppleScriptError, literal, tell_word

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

# 导出 PDF 的**固定**落点。与取件槽同一条理由，只是方向相反：
# 槽是「Word 要读的路径」，这是「Word 要写的路径」。两者都必须固定。
#
# 实测：Word 的**目录**授权不可靠地覆盖新建的子目录——前五批采集包目录都没弹框，
# 第六批换了个新目录就弹了（多半有容量淘汰）。每份采集包一个新目录，
# 就等于每次都可能要人点一次。所以先导到这一个固定文件，采完再搬进采集包。
DEFAULT_WORK_PDF = DEFAULT_SLOT.parent / "_word-export.pdf"


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
    font_files: list[Path] | None = None,
    work_pdf: Path | None = DEFAULT_WORK_PDF,
    optional_families: list[str] | None = None,
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
    # Word 写的是那个**固定**落点，不是采集包里的新目录——见 `DEFAULT_WORK_PDF`。
    pdf_path = Path(work_pdf) if work_pdf else bundle / "case.pdf"
    pdf_path.parent.mkdir(parents=True, exist_ok=True)
    if work_pdf and pdf_path.exists():
        pdf_path.unlink()

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

        scans = []
        for scan_label, timing in (("first", "sweepSeconds"), ("repeat", "repeatSweepSeconds")):
            started = time.monotonic()
            read_error = None
            if scans and scans[-1].get("readError"):
                raw = ""
                read_error = "NOT_RUN_AFTER_FIRST_SWEEP_ERROR"
            else:
                try:
                    raw = tell_word(_sweep_script(doc_expr), timeout=1800.0)
                except AppleScriptError as exc:
                    raw = exc.stdout
                    read_error = str(exc)
            timings[timing] = time.monotonic() - started
            receipt_path = bundle / ("sweep-%s.raw.txt" % scan_label)
            receipt_bytes = raw.encode("utf-8")
            receipt_path.write_bytes(receipt_bytes)
            scan = sweep_stability.parse_receipt(raw)
            scan["rawReceipt"] = {"file": receipt_path.name,
                                  "sha256": fingerprint.sha256_bytes(receipt_bytes),
                                  "bytes": len(receipt_bytes),
                                  "encoding": "UTF-8 of unchanged AppleScript stdout"}
            if read_error:
                scan["readError"] = read_error
            scans.append(scan)
    finally:
        try:
            close_document(doc_expr)
        except Exception:
            pass  # 关不掉不改采集结果；留给下游从进程状态判断。

    first_scan, repeat_scan = scans
    stability = sweep_stability.verify(first_scan, repeat_scan, content)
    end_of_content = first_scan["endOfContent"]
    (bundle / "sweep-repeat.json").write_text(json.dumps({
        "schema": "rsword-layout-line-sweep-repeat/1", "platform": "mac",
        **repeat_scan, "validation": stability["scans"]["repeat"],
        "sourceTextReference": "sweep.json:contentText (read once before PDF export)",
    }, ensure_ascii=False, indent=2, sort_keys=True) + "\n")

    # 从 `document.xml` 推出逐字符的构造标注，**并用采集侧的独立读数核过**（§6.4）。
    #
    # 没有它，`counting` 只能按字符猜，而 `\x0c` 在 `Range.Text` 里
    # **分节符与手动分页符同形**——猜不出来就只能报判不了，整页归行跟着废掉。
    # vmisc2 的分页符三页、vmisc3 的分节页，全是这么丢的。
    #
    # 核不过就**不写**：拿一份对不上的推导去算计数，只会得到看着像对的错答案。
    derived = docxtext.content_text(docx)
    marks_check = docxtext.verify(
        derived, end_of_content=end_of_content,
        paragraphs=[{"index": i, "start": s0, "end": e0}
                    for i, (s0, e0) in enumerate(paragraphs)],
    ) if end_of_content is not None else {
        "state": UNDECIDABLE, "reasons": ["SWEEP_END_OF_CONTENT_UNAVAILABLE"],
    }
    marks = ({str(k): v for k, v in derived["marks"].items()}
             if marks_check["state"] == "OK" else None)

    sweep = {
        "schema": "rsword-layout-line-sweep/1",
        "marks": marks,
        "marksCheck": marks_check,
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
        "positions": first_scan["positions"],
        "rawReceipt": first_scan["rawReceipt"],
        "validation": stability["scans"]["first"],
    }
    (bundle / "sweep.json").write_text(
        json.dumps(sweep, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )

    if work_pdf:
        # 采完立刻搬进采集包：固定落点只是过道，读数得跟采集包待在一起。
        shutil.move(str(pdf_path), str(bundle / "case.pdf"))
        pdf_path = bundle / "case.pdf"

    glyphs = pdfglyphs.read_pdf(pdf_path)
    (bundle / "glyphs.json").write_text(
        json.dumps(glyphs, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    )

    # §6.2 采后核字体名。几何自检发现不了字体替换，只有字体名能。
    substitution = preflight.font_substitution_check(
        required_families, pdfglyphs.font_names(glyphs), font_files=font_files,
        optional_families=optional_families,
    )

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
        "sweepStability": stability,
    }
    if stability["state"] != OK:
        meta["usability"] = UNDECIDABLE
        meta["usabilityReason"] = ["SWEEP_STABILITY_UNVERIFIED: " + reason
                                   for reason in stability["reasons"]]
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
