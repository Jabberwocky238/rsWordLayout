"""采集环境指纹（方法 §9.1）。

每次采集都要记下：Word build、字体文件集哈希、夹具身份。
理由见 §6.2：**度量兼容克隆的字体替换在几何上完全不可见**——
2618 条字形记录 max|Δ| = 0.000000pt，只有字体名能发现。所以指纹里字体名与文件集哈希都要有，
而且必须清楚这两者查的是**不同的东西**：文件集哈希查「机器上有什么」，
字体名查「Word 实际用了什么」（后者见 `preflight.py`，那才是防线）。
"""

from __future__ import annotations

import hashlib
import plistlib
import subprocess
import zipfile
from pathlib import Path

WORD_APP = Path("/Applications/Microsoft Word.app")

# §9.1 要求记下字体文件集哈希。根目录与扩展名沿用 docx-layout-instrument 的 mac 口径，
# 这样两边算出来的 epochId 可以直接比。
FONT_ROOTS = (
    "~/Library/Fonts",
    "/Library/Fonts",
    "/System/Library/Fonts",
    "/System/Library/Fonts/Supplemental",
    "/Network/Library/Fonts",
)
FONT_EXTENSIONS = (".dfont", ".otc", ".otf", ".ttc", ".ttf")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def word_app_identity(app: Path = WORD_APP) -> dict:
    """Word 应用身份。读 Info.plist，不启动 Word。"""
    app = app.expanduser().resolve(strict=True)
    info_path = app / "Contents/Info.plist"
    info = plistlib.loads(info_path.read_bytes())
    if info.get("CFBundleIdentifier") != "com.microsoft.Word":
        raise ValueError("WORD_APP_IDENTITY: %s 不是 Microsoft Word" % app)
    return {
        "path": str(app),
        "infoPlistSha256": sha256_file(info_path),
        "version": info["CFBundleShortVersionString"],
        "build": info["CFBundleVersion"],
    }


def font_epoch(roots=FONT_ROOTS, extensions=FONT_EXTENSIONS) -> dict:
    """字体文件集指纹。

    公式与 mac 侧既有批次一致：对按路径排序的文件列表取 `path + '\\t' + sha256` 的换行拼接，
    再 sha256。**这个哈希挡不住字体替换**（§6.2）——它查的是机器上有什么，
    不是 Word 实际用了什么。留它是为了「同批次」这个限定成立。
    """
    files = []
    for root in roots:
        base = Path(root).expanduser()
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if path.is_file() and path.suffix.lower() in extensions:
                files.append(path)
    files.sort(key=str)
    records = [{"path": str(p), "sha256": sha256_file(p), "bytes": p.stat().st_size} for p in files]
    joined = "\n".join("%s\t%s" % (r["path"], r["sha256"]) for r in records)
    return {
        "schema": "mac-font-environment/1",
        "roots": list(roots),
        "extensions": list(extensions),
        "fileCount": len(records),
        "filesSha256": hashlib.sha256(joined.encode("utf-8")).hexdigest(),
        "formula": "sha256 of '\\n'.join(path + '\\t' + sha256) over files sorted by path",
        "files": records,
    }


def docx_identity(path: Path) -> dict:
    """夹具身份。

    除整包 sha256 外**单独记 `word/document.xml` 的 sha256**，理由是 §6.3：
    实测 63 份探针夹具只有 41 份不同的 `document.xml`，12 个共用组覆盖 54%。
    判 `document.xml` 构造的规则时，同组「变体」不是检验实例——按构造不可能给出不同结论。
    这个字段就是用来当场发现「你以为的检验实例其实与设计实例同构」。
    """
    path = Path(path)
    record = {
        "path": str(path),
        "sha256": sha256_file(path),
        "bytes": path.stat().st_size,
        "documentXmlSha256": None,
        "parts": None,
    }
    try:
        with zipfile.ZipFile(path) as archive:
            names = sorted(archive.namelist())
            record["parts"] = names
            if "word/document.xml" in names:
                record["documentXmlSha256"] = sha256_bytes(archive.read("word/document.xml"))
    except zipfile.BadZipFile:
        record["parts"] = "NOT_A_ZIP"
    return record


def word_processes(app: Path = WORD_APP) -> list[dict]:
    """当前 Word 进程（pid 与启动时刻）。

    进程启动时刻是 §6.1 的关键读数：**装完字体必须重启 Word**，
    而判断「装字体时这个进程在不在」靠的就是它。
    """
    found = subprocess.run(
        ["/usr/bin/pgrep", "-x", "Microsoft Word"], capture_output=True, text=True, timeout=20
    )
    if found.returncode not in (0, 1):
        raise RuntimeError("PROCESS_ENUMERATION_FAILED")
    processes = []
    for line in found.stdout.split():
        pid = int(line)
        details = subprocess.run(
            ["/bin/ps", "-p", str(pid), "-o", "lstart=", "-o", "comm="],
            capture_output=True,
            text=True,
            timeout=20,
        )
        if details.returncode != 0:
            continue  # 进程在枚举与读取之间退出了；不算失败，但也不记。
        value = details.stdout.strip()
        # lstart 固定占 24 个 ASCII 字符，其后是可执行文件路径。
        processes.append({"pid": pid, "startedAt": value[:24], "executable": value[24:].strip()})
    return processes


def tool_versions() -> dict:
    import importlib.metadata
    import sys

    return {
        "python": sys.version.split()[0],
        "pdfminer.six": importlib.metadata.version("pdfminer.six"),
    }


def capture_environment(app: Path = WORD_APP, include_font_files: bool = False) -> dict:
    """一次采集的完整环境指纹。"""
    epoch = font_epoch()
    if not include_font_files:
        epoch = {k: v for k, v in epoch.items() if k != "files"}
    return {
        "schema": "rsword-layout-capture-env/1",
        "platform": "mac",
        "word": word_app_identity(app),
        "wordProcesses": word_processes(app),
        "fontEpoch": epoch,
        "tools": tool_versions(),
    }
