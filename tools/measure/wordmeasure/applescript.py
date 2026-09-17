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
    """把 `body` 发给 Word。

    外面套一层 `with timeout of N seconds`，**这一层不是多余的**：

    - `run()` 的 `timeout` 管的是 `osascript` 这个**进程**能跑多久；
    - AppleScript 自己对每个 Apple event 另有一个超时，**默认 120 秒**，
      到点就抛 -1712，与进程超时毫无关系。

    两者不设成一致时，长活儿会在 120 秒整被 AppleScript 掐掉，而调用方以为
    自己给了半小时。实测就是这么栽的：采集包目录第一次用会弹文件夹授权框，
    人还没走到键盘前，事件已经超时，采集失败，框也跟着消失。

    取 `timeout + 1` 是让**进程**超时晚于**事件**超时：这样超时总是以
    AppleScript 的 -1712 形式回来（Word 状态明确），而不是进程被杀
    （Word 可能还在干活，状态不明）。
    """
    inner = int(timeout) + 1
    return run(
        "with timeout of %d seconds\n"
        "tell application %s\n%s\nend tell\n"
        "end timeout" % (inner, literal(WORD_BUNDLE), body),
        timeout=timeout + 30.0,
    )
