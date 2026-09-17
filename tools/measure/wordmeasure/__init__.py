"""Word 排版验收量具。

方法见 `docs/WORD-LAYOUT-MEASUREMENT-METHOD.md`（本仓库 `tools/measure/README.md` 有平台裁剪说明）。

一句话：不测 Word 的内部量（行基线、行盒），只测**每个字形被画在哪里**——
把引擎输出的字形原点与 Word 导出 PDF 里同一字形的原点逐个相比。
"""

__version__ = "0.1.0"

# 三态出口（方法 §7.3）。这个枚举贯穿配对器与比较器：
# 「判不了」是**独立出口**，不能被折叠进 FAIL，否则这套东西只会输出「成立」。
OK = "OK"
FAIL = "FAIL"
UNDECIDABLE = "UNDECIDABLE"

STATES = (OK, FAIL, UNDECIDABLE)
