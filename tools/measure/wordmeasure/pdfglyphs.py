"""PDF 几何通道（方法 §2.2）。

逐字形取 `glyphOrigin[x, y]`、`advanceVector`、`fontName`、`fontSize`、`text`、
`textStatus`、`matrix`、`rise`。

坐标口径：`glyphOrigin` 已翻成**页顶向下**、相对 cropBox 左上角，单位点。
这与引擎输出的页内坐标同向同原点，可以直接相减。原始的 PDF 设备坐标一并留在
`deviceGlyphOrigin`，好让人能回推。

两条 Mac 限定（§6.6）：

- Mac 通道的 PDF `fontSize` 逐条恒为 **1**（字号在文本矩阵里），**不可作字号读数**。
  本模块照读照记，但另给 `effectiveSizePt`（从矩阵还原）并标明它是推算量。
- Mac 与 Windows 是**两套字体度量**，读数不能互相替代。

字符身份（`text`）只作**配对之后的核对手段**，不作配对依据——见 §3.1：
PDF 里的字形可能没有 ToUnicode 映射（合成字体、CJK 子集）。
"""

from __future__ import annotations

import hashlib
import importlib.metadata
import math
from pathlib import Path

from pdfminer.converter import PDFPageAggregator
from pdfminer.pdfinterp import PDFPageInterpreter, PDFResourceManager
from pdfminer.pdfpage import PDFPage
from pdfminer.utils import apply_matrix_pt


def _font_name(font) -> str:
    name = font.fontname
    return name.decode("latin1") if isinstance(name, bytes) else str(name)


def _text_of(font, cid) -> tuple[str | None, str]:
    """字符身份与它的状态。

    `textStatus` 三态：`mapped`（拿到了码位）、`unmapped`（字体没给映射）、
    `error`（映射表本身出错）。**不要把 unmapped 当成空字符串**——
    那会让「没身份」和「身份是空」混成一件事。
    """
    try:
        return font.to_unichr(cid), "mapped"
    except Exception:
        return None, "unmapped"


class _GlyphExtractor(PDFPageAggregator):
    def __init__(self, manager):
        super().__init__(manager)
        self.pages: list[dict] = []
        self._textstate = None

    def begin_page(self, page, ctm):
        super().begin_page(page, ctm)
        x0, y0, x1, y1 = page.cropbox
        corners = [apply_matrix_pt(ctm, p) for p in ((x0, y0), (x0, y1), (x1, y0), (x1, y1))]
        self._crop_left = min(p[0] for p in corners)
        self._crop_top = max(p[1] for p in corners)
        self.current = {
            "index": len(self.pages),
            "rotation": page.rotate,
            "mediaBox": list(page.mediabox),
            "cropBox": list(page.cropbox),
            "pageMatrix": list(ctm),
            "width": max(p[0] for p in corners) - self._crop_left,
            "height": self._crop_top - min(p[1] for p in corners),
            "glyphs": [],
        }
        self.pages.append(self.current)

    def render_string(self, textstate, seq, ncs, graphicstate):
        self._textstate = {
            "textMatrix": list(textstate.matrix),
            "charSpace": textstate.charspace,
            "wordSpace": textstate.wordspace,
        }
        return super().render_string(textstate, seq, ncs, graphicstate)

    def render_char(self, matrix, font, fontsize, scaling, rise, cid, ncs, graphicstate):
        advance = super().render_char(matrix, font, fontsize, scaling, rise, cid, ncs, graphicstate)
        glyph = apply_matrix_pt(matrix, (0, rise))
        unraised = apply_matrix_pt(matrix, (0, 0))
        end = apply_matrix_pt(matrix, (advance, rise))
        # 只有无旋转无斜切的横排字形才有「页顶向下的 y 就是基线」这层意义。
        horizontal = (
            abs(matrix[1]) < 1e-10
            and abs(matrix[2]) < 1e-10
            and matrix[0] > 0
            and matrix[3] > 0
            and not font.is_vertical()
        )
        values = (*matrix, fontsize, scaling, rise, advance, *glyph)
        if not all(math.isfinite(v) for v in values):
            raise ValueError("PDF_NONFINITE")
        text, status = _text_of(font, cid)
        self.current["glyphs"].append(
            {
                "index": len(self.current["glyphs"]),
                # 页顶向下、相对 cropBox 左上角，单位点。与引擎坐标同向同原点。
                "glyphOrigin": [glyph[0] - self._crop_left, self._crop_top - glyph[1]],
                "unraisedOrigin": [unraised[0] - self._crop_left, self._crop_top - unraised[1]],
                "advanceVector": [end[0] - glyph[0], end[1] - glyph[1]],
                "deviceGlyphOrigin": list(glyph),
                "advanceTextSpace": advance,
                "fontName": _font_name(font),
                "fontSize": fontsize,
                # Mac 通道 fontSize 恒为 1，字号在矩阵里（§6.6）。这是推算量，不是读数。
                "effectiveSizePt": fontsize * matrix[0] if horizontal else None,
                "scaling": scaling,
                "rise": rise,
                "matrix": list(matrix),
                "textState": self._textstate,
                "cid": cid,
                "text": text,
                "textStatus": status,
                "geometryStatus": "RAW_HORIZONTAL" if horizontal else "UNMEASURABLE",
                # 行基线在 PDF 里**不能唯一确定**（§1）：同一份文档两种排布下五个字形的
                # yBaseline 全是 300.0，而真实行基线分别是 300 与 304。所以这里不给。
                "lineBaseline": None,
                "lineMeasurementStatus": "UNMEASURABLE",
            }
        )
        return advance


def read_pdf(path) -> dict:
    """解析 PDF，返回逐页逐字形的原始几何。"""
    path = Path(path)
    manager = PDFResourceManager()
    device = _GlyphExtractor(manager)
    interpreter = PDFPageInterpreter(manager, device)
    with path.open("rb") as stream:
        for page in PDFPage.get_pages(stream):
            interpreter.process_page(page)
    if not device.pages:
        raise ValueError("PDF_NO_PAGES")
    return {
        "schema": "rsword-layout-pdf-glyphs/1",
        "status": "UNCALIBRATED",
        "unit": "pt",
        "originFrame": "page-top-down, relative to cropBox top-left",
        "sourceSha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "tool": {
            "name": "wordmeasure.pdfglyphs",
            "pdfminer.six": importlib.metadata.version("pdfminer.six"),
        },
        "pages": device.pages,
    }


def font_names(pdf: dict) -> list[str]:
    return sorted({g["fontName"] for page in pdf["pages"] for g in page["glyphs"]})


def glyph_counts(pdf: dict) -> list[int]:
    return [len(page["glyphs"]) for page in pdf["pages"]]


def page_text(pdf: dict, index: int) -> str:
    """按内容流顺序拼出一页的字符身份。**只用于核对，不用于配对**（§3.1）。"""
    return "".join(g["text"] or "�" for g in pdf["pages"][index]["glyphs"])
