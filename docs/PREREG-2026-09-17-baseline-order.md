# 预注册：baseline-order 夹具（2026-09-17）

**本文在采集之前写死并提交。** 采集后只允许追加「结论」一节（量具方法 §7.1）。

夹具：`fixtures/baseline-order.docx`，sha256
`9acad2f4905fb14e06fa5c23bbe5b4271cb4c234f173045e15f6230885123c98`
判据的可执行形式：`tools/measure/prereg_baseline_order.py`（与本文同一次提交）。

## 0 先说一个采集前就得到的结果：量化次序已经穷尽

本轮原本要判「行底先落栅格」的两种新次序：

```
V1 = quantize(72 + natural) − quantize(descent)
V2 = quantize(quantize(72 + natural) − descent)
V3 = quantize(72 + natural − quantize(descent))     ← 这就是 first-line 的 T1，已判假
```

**用算术（不碰任何读数）算出来：三者恒等。** 因为 `quantize(descent)` 是栅格的
整数倍，而 72 本身就在栅格上（72 / 0.24 = 300），所以

```
quantize(72 + natural − quantize(descent)) ≡ 72 + quantize(natural) − quantize(descent)
```

V1、V2 展开后是同一个式子。在本夹具的 24 组上逐组核对，**0 / 24 不同**。

所以「换量化次序」这条路**到此为止**：它给不出新形式。first-line 判掉 T1，
就等于把这一整族一起判掉了。真正要换的是**那个量本身**。

## 1 本批要判什么，以及线索是从哪来的（**必须先读这一段**）

Word 的纵向度量有两张表可读：`hhea` 的 `ascender/descender`，与 OS/2 的
`usWinAscent/usWinDescent`。**前四批从没把这两张表分开过**——那些夹具用的族
几乎清一色 `hhea == win`，两者在数据上塌缩成同一个。

本批判：**Word 读的是 OS/2 的 `usWinAscent`。**

**线索的来源要说清楚，它不是纯净的。** 有两部分：

1. 「有两张表、从没被区分过」——这是**字体文件的事实**，与任何读数无关；
2. first-line 那批两处失败里，**有一处（Apple Chancery）正是那六个族里唯一
   `hhea ≠ win` 的**——这一条**来自已经看过的数据**。

有第 2 条在，本批**不是完全独立的检验**，按量具方法 §7.5 记为**「线索取自已见数据」**。
它能做到的是：把一个 n=1 的巧合，放到一份**新夹具、新字体、新字号**上去，
且其中 **4 / 6 个族 `hhea ≠ win`**，24 组里有 **12 组**能把两张表分开。
成立也只算「初步」，要确立还得再来一份线索独立的夹具。

## 2 容差

**0.0000pt**，逐位相等。唯一例外：PDF 读数的十进制舍入取 **1e-6 pt**。

## 3 预测

```
grid = 0.24pt，quantize(v) = 四舍五入(v / grid) × grid，正文区顶 = 72pt
```

### U0 — 每条基线仍落在栅格上

**判为假**：任何一条基线不是 0.24pt 的整数倍。分母 = **72**。

六个族（Skia / Sathu / Plantagenet Cherokee / STIX Two Text / Trattatello /
Zapfino）与前四批无一重合，`upem` 含 **400**（Zapfino）与 **1274**（Sathu）
两个前四批没有过的值。这是第**五**次独立检验。

### U1 — 每页第一条基线，用 OS/2 的 `usWinAscent`

```
预测 b₀ = quantize(72 + usWinAscent / upem × 字号pt)
```

**判为假的读数**：任何一页的实测 b₀ ≠ 预测值。分母 = **24**。

## 4 对照（**不作预测**）

同一批读数上，把已判假的 `hhea` 形式一并算出来报出：

```
对照 b₀ = quantize(72 + (hhea.ascender + hhea.lineGap) / upem × 字号pt)
```

24 组里 **12 组**两者给出不同预测（Skia 4 组、Plantagenet Cherokee 4 组、
STIX Two Text 4 组），另 12 组两者同解（Sathu / Trattatello / Zapfino），
那 12 组判的是「U1 这个形式本身对不对」，不是「两张表谁对」。

**分高不改判。** U1 是本批唯一的点预测；对照已在 accumulation 与 first-line
两批判假，要翻案得有它自己的预注册。

## 5 采集前声明的排除项

某组的 3 行没有全部落在同一页上时，该页的 U1 读数**判不了**，记入分母但不算通过。

## 6 证否条件（量具自身）

| | |
| --- | --- |
| **F-A** | 采前核查不是 `PASS` |
| **F-B** | 采集包里的字体核查不是 `PASS`（判据**直接采信采集包的结论**，不自己再算一遍） |
| **F-C** | 夹具 sha256 与本文声明的不一致 |
| **F-D** | 行号扫描给出的页数与 PDF 页数不符 |

依旧**没有**「两个读数不等即作废」那一条。

## 7 采集条件

- **Mac**，Word for Mac，**兼容性模式**。
- 字体族用 **Word 自己列出来的名字**：`Skia` / `Sathu` / `Plantagenet Cherokee` /
  `STIX Two Text` / `Trattatello` / `Zapfino`。

## 8 结论

*（留空。采集后追加，不改动上面任何内容。）*
