# Android 页数证据审计与 docGrid 边界

后续更新（2026-10-04）：本文已指出九份夹具没有声明字体与字号。进一步的对照显示，字号阶梯、
docGrid 阈值与 P0 轮的像素度量三组读数共同指向「手机缺省 11pt、单倍约 1.352 em 的字体」，
下文按 Calibri 12pt 自然高度做的预测因此不是这批夹具的正确输入，见
[P0 轮对齐](WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md) §4。

本轮只读 `../word_analyse` 的夹具和既有日志，没有启动 Word、采集新读数或修改原始材料。
新增 `tools/measure/wordmeasure/android_pages.py` 与单日志入口
`tools/measure/android_pages.py`，把页容器读数作为独立证据处理。
该模块不修改既有 Android 行边界比较器，也没有实现拟合网格公式。

## 读数与条件比较

`parse_page_log(raw: bytes)` 保存原始文件 SHA-256、全部 PGIDX 读数、连续 count 分段、
最后一次 count、全部 ENTER 读数与宽度频次，以及最后 PGIDX 之前最近的 ENTER 和之后
所有 ENTER。`finalCount` 表示最后一次记录值，不能仅凭它声称布局已稳定。

`compare_page_count(audit, engine_page_count, fixture_sha256, binding=None,
assume_legacy_print=False)` 比较页容器数量，不比较页内边界、行推进或几何位置。
目标夹具 SHA-256 必须有效，候选数量必须为正整数，布尔值、NaN 和零不被接受。

原始 PGIDX 记录没有目标文档哈希、线程、布局通道与完成标记。
因此严格模式为 `UNDECIDABLE`，即使日志出现纸页宽度、外部侧车哈希匹配也不能升级。
ENTER 有线程信息，但它与 PGIDX 缺乏关联标识；相邻时间顺序不等于同一布局通道。

显式 `--assume-legacy-print` 才允许条件比较，并逐项记录：

- 文件名和采集配方将日志绑定到当前夹具 SHA-256。
- 最后一段 PGIDX 属于目标文档的打印通道。
- 该段已经稳定，采集完成后不存在未记录的重排。

原设备移动路径的已知宽度是 5329 twips。最后 PGIDX 前最近的 ENTER 或其后的任一
ENTER 出现该宽度时，与上述 legacy 配方冲突，不能得到条件 `OK`。
10466、9026 或其他宽度均不被当作打印视图的证明。
哈希冲突、损坏记录、缺失计数序列开头、序号不连续和缺少末尾换行均保持不可判。
原探针 `tools/layout-probe/lineprobe.c:713` 只记录 `n < 80` 的 PGIDX。
一旦观测到 `n=79`，就已经到达采样上限；随后即使还有 ENTER，PGIDX 也不再写入。
审计记录这个已知上限，legacy 参数同样不能将采样耗尽升级成稳定读数。

可选侧车的结构为：

```json
{
  "schema": "android-page-binding/1",
  "docSha256": "目标夹具的64位小写十六进制SHA-256",
  "logSha256": "原始日志的64位小写十六进制SHA-256",
  "mode": "print",
  "modeSource": "说明模式证据来自哪个采集记录或配方"
}
```

侧车只描述外部来源，不能补造旧 PGIDX 本身没有的身份信息。

```sh
tools/measure/.venv/bin/python tools/measure/android_pages.py \
  ../word_analyse/reports/diff/print/dg-decide-139-n38.print.live.log \
  --fixture ../word_analyse/fixtures/dg-decide-139-n38.docx \
  --trace /path/to/existing-engine-trace.json \
  --assume-legacy-print \
  --output artifacts/page-audit.json
```

省略 legacy 参数可生成严格审计。CLI 不运行引擎；`--trace` 是调用者提供的既有轨迹，
`--engine-page-count` 是调用者提供的数量。轨迹必须是 `rsword-layout-trace/1`，数量
来自 `pages` 数组；如果另带 `pageCount`，必须与数组长度一致。显式非 print 模式或
文档哈希冲突不能被假设覆盖。旧轨迹 `metrics` 的固定格式后缀如果明确记录 `mobile`，
同样阻止 print 假设，即使顶层 `mode` 写着 print。缺乏结构化模式和文档哈希时，
必须另列候选侧假设。

## 九份真实日志的复核

原始报告是 `../word_analyse/reports/diff/docgrid-decided.md`；夹具和日志按表中名称
分别位于该项目的 `fixtures/` 与 `reports/diff/print/`。
以下为原日志复读结果，三档每档的段数为容量减一、容量、容量加一。

| pitch | 段数 | 完整连续 count 读数段 | 最后 count | 当前候选页数 | 条件比较 |
| --- | --- | --- | --- | --- | --- |
| 139 | 36 | 1 x 50 | 1 | 1 | OK |
| 139 | 37 | 1 x 54 | 1 | 1 | OK |
| 139 | 38 | 1 x 32, 2 x 48 | 2 | 1 | UNDECIDABLE，采样耗尽 |
| 188 | 40 | 1 x 58 | 1 | 1 | OK |
| 188 | 41 | 1 x 62 | 1 | 1 | OK |
| 188 | 42 | 1 x 34, 2 x 46 | 2 | 1 | UNDECIDABLE，采样耗尽 |
| 220 | 34 | 1 x 50 | 1 | 1 | OK |
| 220 | 35 | 1 x 54 | 1 | 1 | OK |
| 220 | 36 | 1 x 38, 2 x 42 | 2 | 1 | UNDECIDABLE，采样耗尽 |

九份日志均没有语法问题，但三个 n+1 样本均恰好达到 80 条 PGIDX 上限。
严格比较全部 `UNDECIDABLE`；显式 legacy 比较为 6 个 `OK`、3 个 `UNDECIDABLE`。
原报告的三组容量 37/41/35 继续作为报告级约束保存，本轮不能把它们认证为完整原始日志
已证明的最终容量。当前候选没有实现网格分页，不能因低段数控制组相同而报完成。

候选由本轮快照的 `layout-trace` 实际执行，Android/print、RealMetrics、vertical-grid=none、
显式 Calibri 字体，采用文档自身纸张和边距。二进制、字体哈希、每份命令、输出日志、
轨迹哈希在本机 `artifacts/android-pages-2026-09-27/manifest.json` 和相邻案例目录中保存。
加入采样上限检查后的严格/条件审计位于
`artifacts/android-pages-2026-09-27-reviewed/manifest.json` 和相邻案例目录。
前一个目录中的初版审计未考虑采样上限，应以 reviewed 审计结论为准。
这些产物只记录本轮快照结果，不是后续代码版本的自动通过证明。

九份 DOCX 均仅含内容类型、根关系和正文三个 ZIP 部件；没有 `rPr`、styles 或 settings，
没有声明字体和字号。Calibri 12pt 是旧研究使用的环境解释，不能作为这些文件的显式属性。
该事实必须随后续真实字体/字号采集一并验证。

## 为什么九份通过仍不足以完成网格

旧研究的十八个 pitch 读数见
`../word_analyse/findings/rules/d6-docgrid-analysis.md`。它们属于报告级行容量约束，
本轮没有为每个点建立独立的新原始采集身份。

取当前 Calibri 的字体自然高度 `Hfont = 2500 / 2048 * 240 = 292.96875 twips`，
假设 `advance = pitch * ceil(Hfont / pitch)`、页面边界占高也等于 `Hfont`，则预测：
`capacity = 1 + floor((15398 - Hfont) / advance)`。

| pitch | 报告行容量 | 上述字体公式预测 |
| --- | --- | --- |
| 60 | 51 | 51 |
| 75 | 51 | 51 |
| 99 | 39 | 51 |
| 100 | 51 | 51 |
| 125 | 41 | 41 |
| 150 | 51 | 51 |
| 151 | 50 | 51 |
| 200 | 38 | 38 |
| 240 | 32 | 32 |
| 280 | 27 | 27 |
| 290 | 26 | 27 |
| 293 | 26 | 52 |
| 296 | 26 | 52 |
| 297 | 26 | 51 |
| 298 | 51 | 51 |
| 300 | 51 | 51 |
| 312 | 49 | 49 |
| 315 | 48 | 48 |

该公式在 pitch 139/188/220 上恰好得到 37/41/35，因此会与上述三组报告页数全部吻合，
同时在十八点中仍有六处错误。只实现一个更容易通过九份夹具的公式不能证明目标行为。

本机 Calibri 文件的 UPEM=2048；hhea/typo 为 ascent=1536、descent=-512、gap=452；
win ascent=1950、descent=550，两种自然高度之和均为 2500 字体单位。
现有自然高度无法直接给出旧报告中 297/298 的阈值。

条件模型 `advance = pitch * ceil(T / pitch)` 与
`capacity = 1 + floor((15398 - B) / advance)` 在取 T=298 时，对十八点和三个新增点
共同允许 `318 < B <= 350` twips，整数写法为 319..350。
新三点没有收窄这个区间；B 是首行占位、末行占位还是其他边界量仍未确定。
`longpage-narrow100-grid65/85/96` 的现有日志没有任何 PGIDX 读数，且只有移动路径，
不能拿来进一步收窄 B。拟合区间较宽不是字体公式已被推导出来的证明。

## 下一步代码边界

`PendingLine` 已分成推进量 `advance_fine` 与需要容纳的底边 `required_fine`，
无网格时保持现有数值不变。整段保留累计 `max(prefix_advance + required)`，
不直接相加所有占高，也不只看末行，因为之前行的下延伸可能更大。
实现与边界测试见 [纵向度量](LINE-VERTICAL-METRICS-2026-09-27.md)。

网格投影分别接入节 `docGrid` 与段 `snapToGrid`，不混入字体度量的 Mac 基线量化选项。
自动行距的网格步长和边界应接受字体度量及段落属性产生的输入；exact 对照保持原行为。
在可区分字体、字号、首末行与多行段的证据到位前，不把 T=298 或 B 区间中某个值写进
通用实现，也不对 docGrid 宣称 Word 对齐。

新增测试覆盖严格与条件分流、多次布局、最后值不同于最大值、末端移动路径冲突、
损坏/截断日志、无 PGIDX、哈希冲突、候选数量类型和轨迹模式/数量一致性。
执行：`tools/measure/.venv/bin/python -m pytest -q tools/measure/tests/test_android_pages.py`。
