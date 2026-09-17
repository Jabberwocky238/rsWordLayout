"""采前核查（方法 §6.1 / §9.2）。**这道核查是唯一防线。**

实测教训：16 个字体装于 11:28，Word 进程启动于两天前，结果 2618 条字形**全被替换**，
而 `system_profiler` 与文件检查**都是通过的**。

所以这里核的是**Word 进程自己**能不能列出所需字体：

- 不是核文件在不在（§6.1）；
- 不是核别的进程看不看得见（§6.1）；
- 也不是核字体文件集哈希——那查的是机器上有什么，不是 Word 实际用了什么（§6.2）。

核查通过还有一个前提：进程启动时刻要**晚于**字体安装时刻。装完字体必须重启 Word。
本模块把进程启动时刻原样记进结果，让下游能自己判断。
"""

from __future__ import annotations

import struct

from . import fingerprint
from .applescript import literal, tell_word


def word_font_names(timeout: float = 60.0) -> list[str]:
    """问 Word 进程自己要字体族名。

    用 `return font names`——与 mac 侧既有批次同一条读法（那次返回 Liberation 0 / Carlito 0，
    正是靠这条读数发现字体没进 Word）。
    """
    body = "set AppleScript's text item delimiters to linefeed\nreturn (font names) as text"
    out = tell_word(body, timeout=timeout)
    names = [line.strip() for line in out.splitlines()]
    return [n for n in names if n]


def preflight(required_families: list[str], timeout: float = 60.0) -> dict:
    """核查 Word 进程能列出 `required_families` 里的每一个族。

    返回三态之一的 `result`：

    - `PASS`  —— 全部在列；
    - `FAIL`  —— 有族不在列，**不要采集**，先装字体再重启 Word；
    - `UNDECIDABLE` —— Word 没在跑，或读不到字体名。没读到不等于没有。
    """
    processes = fingerprint.word_processes()
    record = {
        "schema": "rsword-layout-font-preflight/1",
        "rule": "方法 §6.1：核 Word 进程自己能列出所需字体。不是文件在不在，不是别的进程看不看得见。",
        "method": "AppleScript: tell application \"Microsoft Word\" to return font names",
        "requiredFamilies": {family: None for family in required_families},
        "wordProcesses": processes,
        "fontNameCount": None,
        "result": "UNDECIDABLE",
        "note": None,
    }
    if not processes:
        record["note"] = "Word 未运行；字体名读数不存在。启动 Word 后重跑。"
        return record
    try:
        names = word_font_names(timeout=timeout)
    except Exception as error:  # 读不到就是判不了，不是通过，也不能记成不通过。
        record["note"] = "字体名读取失败：%r" % (error,)
        return record

    present = set(names)
    record["fontNameCount"] = len(names)
    record["requiredFamilies"] = {family: (family in present) for family in required_families}
    missing = [f for f, ok in record["requiredFamilies"].items() if not ok]
    record["result"] = "PASS" if not missing else "FAIL"
    if missing:
        record["note"] = (
            "Word 进程列不出：%s。装字体后**必须重启 Word**——"
            "文件在不在与此无关（§6.1）。" % ", ".join(missing)
        )
    return record


def assert_pass(record: dict) -> None:
    if record["result"] != "PASS":
        raise RuntimeError(
            "FONT_PREFLIGHT_%s: %s" % (record["result"], record.get("note") or "")
        )


def names_declared_by_font(path) -> set[str]:
    """字体文件**自己**声明的名字：族名(1)、全名(4)、PostScript 名(6)。

    为什么非读不可：族名与 PostScript 名可以毫无字面关系。
    `Bodoni 72 Smallcaps Book` 的 PostScript 名是 `BodoniSvtyTwoSCITCTT-Book`
    （Svty Two = Seventy Two = 72），而 PDF 的子集名用的是后者。
    只按族名做字符串包含，就会把**没被替换**的字体判成替换——实测栽过一次。
    """
    from pathlib import Path as _Path

    data = _Path(path).read_bytes()
    count = struct.unpack(">H", data[4:6])[0]
    tables = {}
    for i in range(count):
        at = 12 + 16 * i
        tables[data[at : at + 4].decode("latin1")] = struct.unpack(
            ">II", data[at + 8 : at + 16]
        )[0]
    if "name" not in tables:
        return set()
    base = tables["name"]
    n_records, storage = struct.unpack(">HH", data[base + 2 : base + 6])
    out: set[str] = set()
    for i in range(n_records):
        rec = base + 6 + 12 * i
        pid, _eid, _lid, nid, length, offset = struct.unpack(">HHHHHH", data[rec : rec + 12])
        if nid not in (1, 4, 6):
            continue
        raw = data[base + storage + offset : base + storage + offset + length]
        try:
            out.add(raw.decode("utf-16-be") if pid == 3 else raw.decode("latin1"))
        except UnicodeDecodeError:
            continue
    return out


def font_substitution_check(
    required_families: list[str],
    pdf_font_names: list[str],
    font_files: list | None = None,
    optional_families: list[str] | None = None,
) -> dict:
    """采后核字体名（§6.2）。

    Liberation Serif 是 Times New Roman 的**度量兼容**克隆（Sans↔Arial、Carlito↔Calibri、
    Caladea↔Cambria）。替换后 `glyphOrigin` 与 `advanceVector` **一字不差**，
    2618 条记录 max|Δ| = 0.000000pt。**任何几何自检都发现不了字体被换过，只有字体名能。**

    所以采完必须回头看 PDF 里的子集名，确认用的是申请的族。
    """
    def norm(s: str) -> str:
        return s.replace("-", "").replace(" ", "").lower()

    # PDF 子集名形如 `ABCDEF+LiberationSerif`；去前缀与空格后做包含判断。
    normalized = [norm(name.split("+", 1)[-1]) for name in pdf_font_names]

    # 申请的每个族「可以叫什么」：族名本身，**加上字体文件自己声明的名字**。
    # 不读文件就只能拿族名去猜，而族名与 PostScript 名可以毫无字面关系。
    declared: dict[str, set[str]] = {}
    for path in font_files or []:
        try:
            declared[str(path)] = {norm(n) for n in names_declared_by_font(path)}
        except (OSError, struct.error, KeyError):
            declared[str(path)] = set()
    def related(a: str, b: str) -> bool:
        return a in b or b in a

    findings = {}
    for family in list(required_families) + list(optional_families or []):
        needle = norm(family)
        acceptable = {needle}
        # **按文件归并**：一个文件里的族名、全名、PostScript 名说的是同一个字体，
        # 所以只要其中任何一个与申请的族名对得上，这个文件的**全部**名字都算数。
        # 逐个名字去跟族名比是不行的——PostScript 名与族名可以毫无字面关系
        # （`Bodoni 72 Smallcaps` ↔ `BodoniSvtyTwoSCITCTT-Book`），那样反而把
        # 真正出现在 PDF 里的那个名字排除掉。
        for names in declared.values():
            if any(related(needle, d) for d in names):
                acceptable |= names
        findings[family] = any(any(related(a, n) for a in acceptable) for n in normalized)

    # 反向：PDF 里出现了账面之外的名字吗？这一问才是「有没有被替换」。
    known = {norm(f) for f in list(required_families) + list(optional_families or [])}
    for names in declared.values():
        known |= names
    unexpected = sorted(
        name
        for name in set(pdf_font_names)
        if not any(related(k, norm(name.split("+", 1)[-1])) for k in known)
    )

    return {
        "schema": "rsword-layout-font-substitution/2",
        "basis": "方法 §6.2：几何自检查不出字体替换，只有字体名能。"
                 "可接受的名字取自**字体文件自己声明的** name 表（族名/全名/PostScript 名），"
                 "不靠族名字符串去猜。",
        "pdfFontNames": sorted(set(pdf_font_names)),
        "fontFilesRead": sorted(declared),
        "requiredFamiliesSeenInPdf": findings,
        "optionalFamilies": sorted(optional_families or []),
        "unexpectedNames": unexpected,
        # **只有申请的（required）才必须出现**。可选的（optional）是「允许出现」，
        # 用不到很正常——例如 Word 只在段落没有可见 run 时才拿默认字体画段落标记。
        # 把两者混为一谈，会在夹具恰好没触发那种情形时误报替换（实测栽过一次）。
        "result": "PASS" if all(findings[f] for f in required_families) and not unexpected
                  else "SUBSTITUTION_SUSPECTED",
    }
