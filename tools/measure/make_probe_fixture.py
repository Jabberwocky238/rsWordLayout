#!/usr/bin/env python3
"""生成一份**专门用来定未决项**的探针夹具。

它要回答的四条，都是前几轮量出来但**只有 1~2 个实例**、因而不敢下结论的：

| 记号 | 未决项 | 现有证据 |
| --- | --- | --- |
| W-2 | 行高 = 自然行高四舍五入到 1/300 英寸栅格 | n=2（12pt、13.92pt） |
| W-2 | 基线 = 行高 − 量化后的 descent | **n=1** |
| G-7 | 上下标：字号 0.66 em、升 0.34 em、降 0.08 em | **n=1**（一种字体、一个字号） |
| G-9 | 含抬升 run 的行，行高会变 | **n=1**（`w:position=8` → +4.32pt） |
| W-4 | 分节边界后那一行，行距多一格 | **n=1** |

设计要点（量具方法 §7.2）：**检验实例必须在被判的那一层上与设计实例不同**。
原来的读数全部来自 **Liberation Serif 12pt**，所以这里变的就是字号与字体本身——
在同一字号同一字体上再加一百个实例，那是构造不是证据。

每个探针点连排**三段**同设置的短段落：相邻基线之差即行距，一个探针点给两个读数，
彼此还能互相印证。段前段后间距一律 0、`lineRule=auto line=240`，
免得段落间距混进行距里。
"""

from __future__ import annotations

import argparse
import zipfile
from pathlib import Path

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

# 四个族的度量差得足够开，才能把「Word 常数」与「读字体表」分开。
FAMILIES = ["Liberation Serif", "Liberation Sans", "Liberation Mono", "Carlito"]

# 字号，半点。跨一个数量级，且包含 0.66 × size 落不到整半点的那些
# （如 11pt → 14.52 半点、18pt → 23.76 半点）。
SIZES_HALF_POINTS = [16, 18, 20, 22, 24, 28, 32, 36, 48]

# `w:position`，半点，正值向上。含奇数，好看出它是不是也落在栅格上。
RISES_HALF_POINTS = [2, 3, 4, 6, 8, 12]


def esc(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def rpr(size: int, family: str, extra: str = "") -> str:
    fonts = (
        f'<w:rFonts w:ascii="{family}" w:hAnsi="{family}" '
        f'w:eastAsia="{family}" w:cs="{family}"/>'
    )
    return f"<w:rPr>{fonts}<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>{extra}</w:rPr>"


def para(runs: str, size: int, family: str, sect: str = "", page_break: bool = False) -> str:
    """一个段落。间距钉死为 0，行距 auto 单倍——否则行距里会混进段落间距。

    `page_break` 走 **`w:pageBreakBefore`（段落属性）**，不是 `w:br`：它不产生任何
    字符，所以 §4 的「分页符位置三分」在这里根本不用判——计数模型一个分支都不碰。
    """
    ppr = (
        "<w:pPr>"
        f"{'<w:pageBreakBefore/>' if page_break else ''}"
        '<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'
        '<w:jc w:val="left"/><w:widowControl w:val="0"/>'
        f"{rpr(size, family)}"
        f"{sect}"
        "</w:pPr>"
    )
    return f"<w:p>{ppr}{runs}</w:p>"


def run(text: str, size: int, family: str, extra: str = "") -> str:
    return f"{'<w:r>'}{rpr(size, family, extra)}<w:t xml:space=\"preserve\">{esc(text)}</w:t></w:r>"


def sect_pr(kind: str) -> str:
    return (
        f'<w:sectPr><w:type w:val="{kind}"/>'
        '<w:pgSz w:w="11906" w:h="16838"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="720" w:footer="720" w:gutter="0"/>'
        "</w:sectPr>"
    )


def build_body() -> tuple[str, list[dict]]:
    """返回 (body XML, 探针清单)。清单供预注册脚本按标签定位读数。"""
    parts: list[str] = []
    probes: list[dict] = []

    def new_page() -> bool:
        """每组独占一页——**除了第一组**。

        Word 会忽略首段的 `w:pageBreakBefore`；若它哪天不忽略，就会多出一张空白页，
        页数对不上（F-D）。把第一组排除掉，这个分歧就不存在了。
        """
        return len(parts) > 0

    def triple(tag: str, size: int, family: str, extra: str = "", label: str = ""):
        """连排三段同设置的短段落——相邻基线之差即行距，一个探针给两个读数。

        每组起一新页：这样**每一页的首行都是一条已知设置的普通行**，
        P2（首行基线）的分母才等于页数。挤在一页里，P2 就只剩 1 个读数。
        """
        for i in range(3):
            parts.append(
                para(run(f"{tag}{i}", size, family, extra), size, family,
                     page_break=(i == 0 and new_page()))
            )
        probes.append(
            {"tag": tag, "kind": label or "pitch", "sizeHalfPoints": size, "family": family}
        )

    # A 字号扫描（Liberation Serif）：定 W-2 的「行高 = 四舍五入到栅格」。
    for size in SIZES_HALF_POINTS:
        triple(f"A{size:02d}", size, "Liberation Serif", label="size-sweep")

    # B 字体扫描（12pt）：把「Word 常数」与「读字体表」分开。
    for i, family in enumerate(FAMILIES):
        triple(f"B{i}", 24, family, label="family-sweep")

    # C 上下标：定 G-7 的三个比例。每组前后各一段正文，好读出基线差。
    for si, size in enumerate([20, 24, 36]):
        for kind in ("superscript", "subscript"):
            tag = f"C{si}{kind[:3]}"
            parts.append(para(run(f"{tag}base", size, "Liberation Serif"), size,
                              "Liberation Serif", page_break=new_page()))
            parts.append(
                para(
                    run(f"{tag}x", size, "Liberation Serif")
                    + run("Xy", size, "Liberation Serif", f'<w:vertAlign w:val="{kind}"/>'),
                    size,
                    "Liberation Serif",
                )
            )
            parts.append(para(run(f"{tag}end", size, "Liberation Serif"), size, "Liberation Serif"))
            probes.append(
                {"tag": tag, "kind": kind, "sizeHalfPoints": size, "family": "Liberation Serif",
                 # 带 vertAlign 的是哪一行——P2 要按行排除，不按组排除。
                 "markedLineSuffix": "x"}
            )

    # D `w:position` 扫描：定 G-9（含抬升 run 的行，行高变多少）。
    for rise in RISES_HALF_POINTS:
        tag = f"D{rise:02d}"
        parts.append(para(run(f"{tag}a", 24, "Liberation Serif"), 24, "Liberation Serif",
                          page_break=new_page()))
        parts.append(
            para(
                run(f"{tag}b", 24, "Liberation Serif")
                + run("Up", 24, "Liberation Serif", f'<w:position w:val="{rise}"/>'),
                24,
                "Liberation Serif",
            )
        )
        parts.append(para(run(f"{tag}c", 24, "Liberation Serif"), 24, "Liberation Serif"))
        probes.append(
            {"tag": tag, "kind": "position", "riseHalfPoints": rise,
             "sizeHalfPoints": 24, "family": "Liberation Serif", "markedLineSuffix": "b"}
        )

    # E 分节边界：定 W-4。连续两组，看那「多出一格」是否复现。
    for i, kind in enumerate(["continuous", "continuous"]):
        tag = f"E{i}"
        parts.append(para(run(f"{tag}a", 24, "Liberation Serif"), 24, "Liberation Serif",
                          page_break=new_page()))
        parts.append(
            para(run(f"{tag}b", 24, "Liberation Serif"), 24, "Liberation Serif", sect_pr(kind))
        )
        parts.append(para(run(f"{tag}c", 24, "Liberation Serif"), 24, "Liberation Serif"))
        parts.append(para(run(f"{tag}d", 24, "Liberation Serif"), 24, "Liberation Serif"))
        # 字号与字体也记下：判据要按它算这一组的常规行距，缺了就只能猜。
        probes.append({"tag": tag, "kind": "section-boundary", "sectionKind": kind,
                       "sizeHalfPoints": 24, "family": "Liberation Serif"})

    # F 收尾组：让文末的 body 级 sectPr 不紧挨着 E 组。同时也是一个正常的行距探针。
    triple("F0", 24, "Liberation Serif", label="size-sweep")

    # 文末必须有一个 body 级 sectPr，否则 Word 认不出页面设置。
    #
    # 它是 **continuous**，不是 nextPage——这一条是判据能不能成立的关键。
    # OOXML 里 `w:sectPr` 的 `w:type` 说的是**它所定义的那一节怎么开始**，
    # 不是它所结束的那一节怎么结束。所以 E1b 与 E1c 之间那一步，归**文末**
    # 这个 sectPr 管；写成 nextPage，E1 组就被劈到两页上，P5 的读数直接作废。
    # （引擎侧实测确认：E0 不翻页、E1 翻页——引擎是对的，错的是原来的夹具。）
    body = "".join(parts) + sect_pr("continuous")
    return f"<w:body>{body}</w:body>", probes


def write_docx(path: Path) -> list[dict]:
    body, probes = build_body()
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W}" xmlns:r="{R}">{body}</w:document>'
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    )

    path.parent.mkdir(parents=True, exist_ok=True)
    # 逐位可复现：固定时间戳与压缩方式，好让 sha256 只随内容变。
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in (
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("word/document.xml", document),
        ):
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, data)
    return probes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", help="写出的 .docx 路径")
    args = parser.parse_args()

    path = Path(args.output)
    probes = write_docx(path)

    import hashlib
    import json

    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    print(f"已写出 {path}")
    print(f"sha256 {digest}")
    print(f"探针 {len(probes)} 组")
    counts: dict[str, int] = {}
    for p in probes:
        counts[p["kind"]] = counts.get(p["kind"], 0) + 1
    print("按类别：", json.dumps(counts, ensure_ascii=False))
    (path.parent / (path.stem + ".probes.json")).write_text(
        json.dumps({"sha256": digest, "probes": probes}, ensure_ascii=False, indent=2) + "\n"
    )


if __name__ == "__main__":
    main()
