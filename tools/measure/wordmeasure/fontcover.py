"""字体能不能画出某些字符——读 `cmap`，不猜。

# 为什么要有这一层

量具方法 §6.2 说：**几何自检查不出字体替换，只有字体名能。** 但那是**采集之后**
才能查的事。更早的一道关在夹具这边：**夹具的标签字符，申请的字体画得出来吗？**

画不出来，Word 会静静换一个回退字体去画（实测：Kokonor 与 Gurmukhi MN 没有
拉丁字形，标签 `"d00"` 被 Cambria 代画了）。那一批读数整批作废——
而作废是**采集之后**才知道的，一次采集就白跑了。

这个函数把那道关前移到**生成夹具时**：画不出来就根本不要放进夹具。
"""

from __future__ import annotations

import struct
from pathlib import Path


def _table_offsets(data: bytes) -> dict[str, int]:
    count = struct.unpack(">H", data[4:6])[0]
    out = {}
    for i in range(count):
        at = 12 + 16 * i
        out[data[at : at + 4].decode("latin1")] = struct.unpack(">II", data[at + 8 : at + 16])[0]
    return out


def covered_chars(path, chars: str | set[str]) -> set[str]:
    """`chars` 里哪些字符在这个字体里有字形。

    只看 Unicode 子表（3,1 / 3,10 / 0,x），支持 format 4 与 12——
    桌面字体里这两种覆盖了绝大多数情况。读不出 `cmap` 时返回空集合，
    **宁可报「画不出」也不报「画得出」**：这道关宁严勿松。
    """
    want = set(chars)
    data = Path(path).read_bytes()
    tables = _table_offsets(data)
    if "cmap" not in tables:
        return set()
    base = tables["cmap"]
    n_tables = struct.unpack(">H", data[base + 2 : base + 4])[0]

    # **按优先级挑子表，不是「循环到最后一个」**。
    # 原来那样写会被排在后面的子表覆盖掉正确的那个：Zapfino 因此被读成
    # 「一个 ASCII 字形都没有」，而换一组待查字符又读成「只缺 012345」——
    # 同一个字体两次给出互相矛盾的答案，正是这个 bug 露出来的样子。
    PREF = ((3, 10), (3, 1), (0, 6), (0, 4), (0, 3), (0, 1), (0, 0))
    found: dict[tuple[int, int], int] = {}
    for i in range(n_tables):
        rec = base + 4 + 8 * i
        pid, eid, off = struct.unpack(">HHI", data[rec : rec + 8])
        found.setdefault((pid, eid), base + off)
    sub = next((found[k] for k in PREF if k in found), None)
    if sub is None:
        return set()

    fmt = struct.unpack(">H", data[sub : sub + 2])[0]
    got: set[str] = set()
    if fmt == 4:
        seg_x2 = struct.unpack(">H", data[sub + 6 : sub + 8])[0]
        seg = seg_x2 // 2
        ends = [struct.unpack(">H", data[sub + 14 + 2 * i : sub + 16 + 2 * i])[0] for i in range(seg)]
        starts = [
            struct.unpack(">H", data[sub + 16 + seg_x2 + 2 * i : sub + 18 + seg_x2 + 2 * i])[0]
            for i in range(seg)
        ]
        for ch in want:
            cp = ord(ch)
            if any(s <= cp <= e for s, e in zip(starts, ends)):
                got.add(ch)
    elif fmt == 12:
        groups = struct.unpack(">I", data[sub + 12 : sub + 16])[0]
        for g in range(groups):
            rec = sub + 16 + 12 * g
            start, end, _gid = struct.unpack(">III", data[rec : rec + 12])
            for ch in want:
                if start <= ord(ch) <= end:
                    got.add(ch)
    return got


def assert_can_draw(fonts: dict[str, str], chars: str) -> None:
    """夹具生成时的守门：任一字体画不出任一标签字符就直接报错。

    宁可在生成时报错，也不要采完才发现整批作废。
    """
    bad = {}
    for family, path in fonts.items():
        missing = set(chars) - covered_chars(path, chars)
        if missing:
            bad[family] = "".join(sorted(missing))
    if bad:
        lines = "\n".join(f"    {f}：缺 {m!r}" for f, m in bad.items())
        raise ValueError(
            "FONT_CANNOT_DRAW_LABELS：下列字体画不出夹具的标签字符，"
            "Word 会拿回退字体代画，整批读数作废（实测栽过一次）：\n" + lines
        )
