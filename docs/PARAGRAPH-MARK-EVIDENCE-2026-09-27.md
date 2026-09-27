# 段落标记：已有 Word capture 的证据边界

本轮仅复核已有 DOCX ZIP、META、原始 glyph 提取和源扫描，没有操作 Word、重采或修改引擎。
未作废旧文件的具体输出仍可用于限定回归，但不等于规范输入的普遍规则；作废包仅留探索线索。
以下 CP 为 UTF-16 半开区间；PDF page/glyph index 为零基，Word scan page/Ln 为一基。
字号使用 `effectiveSizePt`，y 使用 `glyphOrigin[1]`，单位 pt、页面向下为正。
旧包的 `lineBaseline` 为 null、`boxAvailable=false`；这里不把 glyph 原点当成行盒占高。

## 来源绑定与输入限制

| DOCX 源 | SHA-256 |
| --- | --- |
| [vmisc2.docx](../fixtures/vmisc2.docx) | `808c58e18a67a0dcff1cc22852ee985d1366e58e5c46846a219402e4fe54fdab` |
| [probe-metrics.docx](../fixtures/probe-metrics.docx) | `faffc29559e479386869eb492d8296c52bf3f26ce7fb3ff95d05cbfe9844bc90` |
| [continuous40-break10-noBalance0.docx](../fixtures/column-balance-canonical-2026-09-27/continuous40-break10-noBalance0.docx) | `a3f7292eec22de2c21ae62d19e38c10364e1f2804bdbd5ba445ed24640b19eb3` |
| [continuous40-break10-noBalance1.docx](../fixtures/column-balance-canonical-2026-09-27/continuous40-break10-noBalance1.docx) | `71e9185dc245064db87d091d35de44da88c8c745b73e2184e4718dc7d4a3664b` |

前两份与各自 capture META 的 before/after 哈希一致；后两份与规范 capture META 一致，
PDF 哈希也匹配，两遍原始扫描一致。相关原包分别位于
[vmisc2 capture](../captures/vmisc2-2026-09-17/)、
[probe-metrics capture](../captures/probe-metrics-2026-09-17/) 和
[规范栏断 capture](../artifacts/column-balance-canonical-append-2026-09-27/)。

直接读取旧 ZIP，`pPr` 的顺序为 `[pageBreakBefore/]spacing → jc → widowControl → rPr`，
并非规范属性顺序；生成位置见 [make_probe_fixture.py:79](../tools/measure/make_probe_fixture.py:79)。
因此不能据旧输出断言 Word 对规范 `pPr/rPr` 一定采用、忽略或覆盖某属性。
这也不否定旧输入自身已有的可复核绘制现象。

## 可区别于最后 run 的段尾绘制

### vmisc2 的独立段尾字号

本节使用第二次采集：[VERDICT.json](../captures/vmisc2-2026-09-17/VERDICT.json) 为
`EVALUATED`、`falsifiers=[]`，不同于第一次 `-void` 包；其整体 `bundleState` 仍为
`UNDECIDABLE`，不能称全包通过。下列段尾字号是源区间与原始 glyph 限定的可用观察。

原 ZIP 中 `fssbbtt`、`hssbbtt` 的 mark 均声明 Luminari 13pt；最后正文 run 分别为
10pt、8pt，中间 `bb` 分别为 20pt、30pt；行距均为 `exact320`。

| 源与扫描 | [glyphs.json](../captures/vmisc2-2026-09-17/glyphs.json) 的实际读数 |
| --- | --- |
| `fssbbtt`，CP `[30,38)`，mark CP37；page6/Ln1 | PDF page5：最后两个 `t` 为 glyph5/6、10.08pt；尾空格为 glyph7、12.96pt；八个 glyph 的 y 均为 84.96 |
| `hssbbtt`，CP `[46,54)`，mark CP53；page8/Ln1 | PDF page7：最后两个 `t` 为 glyph5/6、7.92pt；尾空格为 glyph7、12.96pt；前八个 glyph 的 y 均为 84.96 |

两行正文自身没有空格或手动 break，尾空格可与唯一段末 mark 对应；字体族均为 Luminari。
这能反驳“非空段尾必然照搬最后 run 字号”的无条件模型，不能确定其字号来源的普遍优先级。
它也不证明独立 mark 会撑高：使用了 exact 行距，中间还有比 mark 更大的可见文字。

旧 [vmisc2 预注册:137](PREREG-2026-09-17-vmisc2.md:137) 称 8pt/30pt 组“不是恰好一行”。
但原 [sweep.json](../captures/vmisc2-2026-09-17/sweep.json) 的 CP `[46,54)` 全在 page8/Ln1，
上述八个 glyph 也位于同一 y。该页另有 CP `[54,56)` 的分页控制段；整页配对不可判
不能作为这七个可见字符换行的证据。此处纠正解释，不修改历史文件或追认其旧判据。

### 作废包 probe-metrics：仅保留探索线索

**以下三例不能作为 Word 验收依据。** [VERDICT.json](../captures/probe-metrics-2026-09-17/VERDICT.json)
为 `VOID`、`predictions={}`；[README:6](../captures/probe-metrics-2026-09-17/README.md:6)
明确整批作废。F-E 在 A16/A22/A24/A32/B0/B1/B2/F0 八组触发，例如 A16 的两次行距为
9.12/9.36pt，A24 为 13.68/13.92pt。判据见 [预注册:143](PREREG-2026-09-17-probe-metrics.md:143)，
[prereg_probe.py:218](../tools/measure/prereg_probe.py:218) 记录不等读数，447 行将整批判为 VOID。
既有 [replay result](../artifacts/paragraph-mark-inputs-2026-09-27/mac-replay/probe-metrics-2026-09-17/result.json)
也以 `CAPTURE_VOID` 跳过，未生成旧 trace。源哈希匹配不能取消作废状态。

原 ZIP 的下列段落均为 `auto240`。正文和 mark 同为 Liberation Serif；mark 未声明
`vertAlign` 或 `position`，最后 run 则声明上标、下标或抬升。

| 源与扫描 | [glyphs.json](../captures/probe-metrics-2026-09-17/glyphs.json) 的实际读数 |
| --- | --- |
| `C0supxXy`，CP `[193,202)`，mark CP201；page14/Ln2 | PDF page13：`Xy` 为 glyph16/17，6.48pt、y=89.28；尾空格 glyph18 为 10.08pt、y=92.88，与前面正常文字同 y |
| `C0subxXy`，CP `[221,230)`，mark CP229；page15/Ln2 | PDF page14：`Xy` 为 glyph16/17，6.48pt、y=93.36；尾空格 glyph18 为 10.08pt、y=92.88 |
| `D02bUp`，CP `[356,363)`，mark CP362；page20/Ln2 | PDF page19：`Up` 为 glyph9/10，12pt、y=96.96；尾空格 glyph11 为 12pt、y=97.92，与正常文字同 y |

最后一例源 `position=2` 半点，实际原点差为 0.96pt。前两例 mark 声明 10pt，
末例声明 12pt。这些作废读数仅提示后续探针可以区分段尾和最后 run 的缩放、纵向位移，
不能据此验证“不继承位移”的规则，也不能转成三项有效样本或精度验收。
段中还已有正常字号文字，不能隔离 mark 对自然行高、最后一行 advance 或 required extent 的贡献。

## break 与 mark 仍须分开取证

旧 [vmisc2 预注册:87](PREREG-2026-09-17-vmisc2.md:87) 将 TNR 空格归为仅出现在独占
分页符段的 mark，并进一步断言指定字体管不住、默认字体必然参与。原始 glyph 不支持这些归因：

| PDF 原始位置 | 实际读数与对应源区域 |
| --- | --- |
| page7/glyph8 | TNR 空格，12.96pt，`(72,100.8)`；相邻 glyph9 是 Luminari 空格，`(216,100.8)`；源 CP `[54,56)` 为 break 加 mark |
| page8/glyph14 | TNR 空格，12.96pt，`(112.13,100.8)`；相邻 glyph15 是 Luminari 空格，`(256.13,100.8)`；源 CP `[63,72)` 为 `jbefore` 加 break、mark |

第二例明确包含可见正文，已超出“只含 break 的段落”。这些 PDF 空格没有直接 CP 映射，
不能唯一命名为 break glyph 或 mark glyph；原分页场景的计数模型也保持不可判。
因此它们不能证明所有空段落默认使用 TNR，更不能证明 mark 不参与撑高。

规范硬栏断两份输入则有可用的源行证据：CP49 是栏断，Ln10 为 `[45,50)`；
CP50 是 mark，Ln11 为 `[50,51)`；C010 位于 Ln12 `[51,56)`，均为 Word page1。
两份 PDF page0 的 C009 为 glyph45..48，y=271.2；glyph49 是右栏顶空格，
`(315.6,55.2)`、12pt；C010 从 glyph50 开始，y=79.2。
原生 content 字节明确为 `C009\x0e\rC010\r`，派生 JSON 的 LF 是文本读取归一化。
这能区别源 break/mark 及其行归属，仍不把 PDF 空格 index 直接当 CP，亦不推导通用控制 glyph 数量。

## 尚缺的鉴别输入

本轮逐份核对的是 26 个名字不带 `-void` 的 `captures/` 目录，并非 26 个有效包：
其中 `probe-metrics` 已整批 VOID。各源文件均匹配 META，正文 XML 均没有 `vanish`；
这些输入事实不等于采集有效性，不能按目录名省略 VERDICT/README 状态检查。
21 份规范 docGrid 加 7 份规范 columns 源也没有 `vanish`，每个正文 run 与 mark 的
`rPr` 相同；其 `pPr` 为 `widowControl → [snapToGrid →] spacing → jc → rPr [→ sectPr]`。
这些规范输入可验证别的行为，不能区分独立 mark 字体与沿用正文的模型。

目前没有规范的正文/mark 不同字体族实测，没有全隐藏段落的 Word capture，也没有
只改变 mark 字号的 auto/atLeast 对照或页底容量对照。旧 scan 没有行盒，PDF 原点和
后继基线差不能直接充当末行占高。现有材料足以支持后续分离 mark 绘制属性的调查，
尚未证明独立 mark 如何改变末行占高或所有输入的默认字体，不能把“mark 不撑高”固化为规则。

同日新增的 [8 份规范输入](../fixtures/paragraph-mark-canonical-2026-09-27/README.md)
分别变化正文/mark 字号、字体族，以及空段、全隐藏正文和 soft return 后空末行。
每个测试段后附两个显式 TNR12 参考段，供观察后继推进；源清单明确记录 CR/VT 的 UTF-16
位置。它们尚未进入 Word 采集，不预填行距、页数、基线或控制 glyph 数量。
当前桌面锁屏仍阻止已有 docGrid 探针继续打开，不能把新输入或本地解析当成新增实测。
