"""osascript 调用层。

Mac 通道的能力边界（方法 §6.6）：

- Mac Word **没有 XPS 导出**；
- AppleScript 桥缺术语，`Line` 返回 `Long` 且单位未映射——**拿不到行盒**，
  所以 §3.3 的「按行盒纵向包含归行」在 Mac 上做不了；
- 文件授权**绑文件身份不绑路径**：同一路径换一份文件会重新弹授权框。

可用的替代通道是 §2.1 的第三条：`first character line number`（配 `active end page number`），
按字符位置扫出 (页, 行)。它给的是**行划分**，不是行盒——用途仅限于把源字符分到行，
不能拿它推任何几何量。
"""

from __future__ import annotations

import subprocess

WORD_BUNDLE = "/Applications/Microsoft Word.app"


class AppleScriptError(RuntimeError):
    def __init__(self, code: str, stdout: str, stderr: str, returncode: int):
        super().__init__("%s: rc=%d stderr=%s" % (code, returncode, stderr.strip()[:400]))
        self.code = code
        self.stdout = stdout
        self.stderr = stderr
        self.returncode = returncode


def literal(text: str) -> str:
    """AppleScript 字符串字面量。禁掉换行与 NUL：它们会把脚本拆开。"""
    if any(c in text for c in "\r\n\x00"):
        raise ValueError("APPLESCRIPT_ARGUMENT: 参数含换行或 NUL")
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def run(source: str, timeout: float = 120.0) -> str:
    """跑一段 AppleScript，返回 stdout。

    超时不当成「失败但 Word 没事」：Word 可能仍在处理，调用方要按未完成处理。
    """
    try:
        process = subprocess.run(
            ["/usr/bin/osascript", "-"],
            input=source,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise AppleScriptError("APPLESCRIPT_TIMEOUT_WORD_UNCONFIRMED", "", str(exc), -1) from exc
    if process.returncode != 0:
        raise AppleScriptError("APPLESCRIPT_FAILED", process.stdout, process.stderr, process.returncode)
    return process.stdout


def tell_word(body: str, timeout: float = 120.0) -> str:
    return run('tell application %s\n%s\nend tell' % (literal(WORD_BUNDLE), body), timeout=timeout)
