# 预注册：first-line 夹具（2026-09-17）

**本文在采集之前写死并提交。** 采集后只允许追加「结论」一节（量具方法 §7.1）。

夹具：`fixtures/first-line.docx`，sha256
`a9f42e6a0c1f9aa892b9c13d67f1ce7a355133855f69085f4f11fafcd17497cd`
判据的可执行形式：`tools/measure/prereg_firstline.py`（与本文同一次提交）。

## 0 为什么要这一份

前三批把问题收敛到了一处：

| | 状态 |
| --- | --- |
| 0.24pt 显示栅格 | **已确立**（三份独立夹具、1080 条基线、零例外） |
| 「游标走更细的整数单位」 | **已排除**（cursor-unit R2，得分随单位变粗单调下降） |
| 「以某条基线为锚、常数步长累加」 | **已排除**（cursor-unit 的退化对照：周期对、相位差 4） |
| **每页第一条基线** | **从头到尾没对过**（accumulation 的 Q2 只有 9/12） |

`accumulation` 那份预注册在**采集之前**就写下了下一个待判的形式：

> 若 Q2 判假，下一个待判的形式是 `quantize(正文区顶 + natural − 量化后的 descent)`。

Q2 确实判假了。所以本批判的这条**不是看了数据才想出来的**——它在上一批的数据
存在之前就登记好了，这一点一条 `git log` 就能核。

## 1 容差

**0.0000pt**，逐位相等。唯一例外：PDF 读数的十进制舍入取 **1e-6 pt**。

## 2 记号

```
grid     = 0.24pt
quantize(v) = 四舍五入(v / grid) × grid
natural  = (hhea.ascender + |hhea.descender| + hhea.lineGap) / upem × 字号pt
descent  = |hhea.descender| / upem × 字号pt
正文区顶 = 上边距 = 72pt（本身就在栅格上：72 / 0.24 = 300）
```

## 3 预测

### T0 — 每条基线仍落在栅格上

**判为假**：任何一条基线不是 0.24pt 的整数倍。分母 = **72**。

六个族（Impact / Microsoft Sans Serif / Luminari / Herculanum / Apple Chancery /
Bodoni 72 Smallcaps Book）与前三批**无一重合**，`upem` 有 1000 与 2048 两种，
所以这是第**四**次独立检验。

### T1 — 每页第一条基线（**本批的点预测**）

```
预测 b₀ = quantize(72 + natural − quantize(descent))
```

**判为假的读数**：任何一页的实测 b₀ ≠ 预测值。分母 = **24**（24 组，每组独占一页）。

## 4 对照（**不作预测**，只报得分）

同一批读数上，把已判假的旧形式一并算出来报出：

```
旧形式（accumulation 的 Q2） b₀ = quantize(72 + (ascender + lineGap)/upem × 字号)
```

**这不是候选选择。** T1 是本批唯一的点预测；旧形式只是为了让「新形式有没有比
旧的好」这句话有数可依。**不许因为旧形式分高就改判它成立**——它已经在
accumulation 上判假，要翻案得有它自己的预注册。

## 5 采集前声明的排除项

某组的 3 行没有全部落在同一页上时，该页的 T1 读数**判不了**，记入分母但不算通过。

## 6 证否条件（量具自身）

| | |
| --- | --- |
| **F-A** | 采前核查不是 `PASS` |
| **F-B** | 采后 PDF 里出现申请之外的字体名 |
| **F-C** | 夹具 sha256 与本文声明的不一致 |
| **F-D** | 行号扫描给出的页数与 PDF 页数不符 |

依旧**没有**「两个读数不等即作废」那一条。

## 7 采集条件

- **Mac**，Word for Mac，**兼容性模式**。结论只能写「在 Mac Word 兼容性模式下观测到」。
- 字体族用 **Word 自己列出来的名字**：`Impact` / `Microsoft Sans Serif` /
  `Luminari` / `Herculanum` / `Apple Chancery` / `Bodoni 72 Smallcaps Book`。

## 8 结论

*（留空。采集后追加，不改动上面任何内容。）*
