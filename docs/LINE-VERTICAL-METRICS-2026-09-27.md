# 行间推进与页面占高

`PendingLine` 的单一 `height_fine` 已替换为私有 `VerticalExtent`：

- `advance_fine` 决定下一行或下一段的起点。
- `required_fine` 决定从本行起点到必须容纳的最远底边。

两者都用现有的 1/7200 英寸整数单位。当前 `auto`、`atLeast`、`exact` 仍由原行高函数
产生相等的两个值；这一步不引入 docGrid 公式，也不改变字体基线或绘制位置。

## 组合规则

把后继 B 放在 A 的推进终点时，组合为：

```text
advance(A + B) = advance(A) + advance(B)
required(A + B) = max(required(A), advance(A) + required(B))
```

因此，推进 100、占高 40 的两行只需 140 个单位，不需 200；推进 40、占高 100 的两行
也需 140，不能只检查最终推进量 80。更早的行若延伸到 250，后面的较矮行不能把这条边界
缩小到自身末端。段间空白只改变后继起点，不把末尾空白单独当成内容。

负段距可能把后继移回前面，组合仍需保留早先内容的底边。旧实现只累计最终游标，
这类输入可能低估占高；新规则修正该边界。因此等价目标针对既有回放及正常非负段距，
不声称保留负段距导致的错误预留。首个后继平移时允许其相对底边为负，
不能凭空在段间原点加入一个零占位。

没有后继使用 `Option::None`。实际零占高的后继仍需其起点在页内，所以不能把默认零值
追加到末行来表示“不需要预留”，否则会把本可超出占高的尾部推进重新算进页面。

## 接入范围

`page_line_quota`、keepLines 整组检查、keepNext 链及孤行前缀预留都使用同一组合规则。
格式化后继段落时从累计推进量开始，检查是否放得下时读取最大占高，实际页面游标只加推进量。
显式分页仍截断当前候选组，超长组仍保留已有的前进保证。

`place_line` 的基线、字形位置和源坐标保持原规则。
推进/占高分离的初版保留了 `break_paragraph_at` 内的粗 twips 环绕游标；
同日后续修复已将它接到实际精细推进量，见下面的环绕接入记录。

## 验证与后续

私有模块测试直接验证不等推进/占高、前行最大延伸、正负段距、精细单位和组合分组；
格式器测试用真实断行得到候选行，再注入不同纵向量，检查页底边界、keep 预留、硬分页及基线。
这些是数学和控制流测试，不声称对应新增 Word 实测。

本轮验收：core/SVG/CFFI/WASM 全量 465 项通过、8 项按既有配置忽略，包含新增的
8 项组合测试与 11 项格式器测试；随后新增负段距整链回归，并使用 Calibri 执行段落保留
专项，23 项全部通过。workspace check 与相关 crate 全目标 Clippy `-D warnings` 通过。
Android 既有窄路径保持 186/186 条件匹配；Mac 25 份有效轨迹页面内容与上一提交一致，
原有 25 包 FAIL、5 包采集无效不可判保持原状。记录在
`artifacts/vertical-extent-2026-09-27/`，未提交产物。

docGrid 输入与夹具检查见 [输入记录](DOCGRID-INPUTS-2026-09-27.md)，
拟合公式的证据缺口见 [页数证据](ANDROID-PAGE-EVIDENCE-2026-09-27.md)。
后续仍需确定网格步长和边界量如何来自字体、字号与段落属性，不能将 T=298 或某个拟合边界
值写成所有文档的规则。

## 后续：环绕查询使用实际推进游标

段内自动换行和软回车原先每行加整数行高，而分页/落位累加 `advance_fine`。
当一行推进 195.6 twips、整数度量为 196 时，第五行实际起点是 782.4，旧环绕查询却在
784。若排除区到 783 结束，旧代码过早恢复全宽，使这一行多容纳字符。
段落入口先舍入当前页游标，也会把之前段落留下的小数误差继续带入后续查询。

现在 `break_paragraph` / `break_paragraph_at` 接收精细原点；每行收下的同一个
`VerticalExtent.advance_fine` 同时用于推进断行查询与实际放置。只有调用既有整数
环绕接口时才转换查询位置，不把其结果累加回游标。

同页连续栏组的顶部也可能为小数 twip。私有 `FlowRegion` 同时携带区域矩形及
`top_fine`，供 keepLines、keepNext、widow 尾段及换栏/换页预排使用：当前栏组保留精细
顶部，新物理页取新正文顶部。显示用 `Rect.y` 不再成为这些预排路径的计算原点。
段前后距、源游标、硬断控制以及正常/平衡试排仍共用原有控制流。

`tests/fine_wrap.rs` 覆盖自动换行、软回车、前段小数推进，以及连续单栏转双栏后的
keepLines 搬家；检查源区间、x 和实际基线。前三项在修复前失败，修复后通过。
第四项要求右栏预排使用 195.6 的顶部，不能取显示矩形的 196。
这些是内部一致性回归，不计为新增 Word 实测。

环绕查询的高度估值和公开坐标仍为整数 twips，遮挡整行时的回退行为也未修改。
本次没有决定 docGrid 的步长、占高或基线相位；未来若行高度依赖绝对网格相位，
还须重新审计 widow 检查中“同区域且无环绕”的快捷路径。

修复提交为 `7f8dce4`。完整 workspace/fontenv 回归 74 组、569 项通过、12 项忽略；
当时包括前三项新回归。随后添加的跨栏预排用例与前三项一起执行，4 项全部通过。
workspace 全目标 Clippy `-D warnings` 通过，指定真实 TNR 字体的多栏专项 13 项通过
（其中一项读取 7 份规范 DOCX）。首次手动启用该测试遗漏字体环境变量的失败日志保留，
补上 `RSWORD_TEST_TIMES_NEW_ROMAN` 后通过，没有修改测试预期。

Android 11 份旧采集仍为 186/186 条件源行区间匹配。Mac 仍为 25 FAIL / 5 UNDECIDABLE；
与上一轮连续栏平衡产物比较，25 份有效轨迹的完整 `pages`（含栏、行与字形）逐项相同，
比较统计及分类也相同。没有运行 Word 或取得新的几何证据。
记录保存在 `artifacts/fine-wrap-2026-09-27/`；`baseline-diff.json` SHA-256 为
`5c294f208de42846b75cfe61e6681f526fca0d09048c633efda378d807e2fc22`。
328 个文件的清单 `durable-hashes.json` SHA-256 为
`6953b3fdcc701a8245f432be583113c5b39d4b95d8d592673bd244c52d4acd62`。

## 后续：保留最终纵向落位诊断

提交 `2e8e088` 将最终行顶、推进、占高、量化前基线偏移及量化后绝对基线，以整数
1/7200 英寸保存在 `LinePlacement`。它按页内行号穿过布局、绘制与轨迹层，输出为
`engineVerticalDiagnostic`；手工页面缺少这些记录时输出 null，不从字形坐标反推。
值在 run/mark 位移及整形偏移之前记录，试排随整个页面克隆并在失败时丢弃。
这为后续 exact/docGrid 修正提供可核对输入，没有修改现行行度量公式。

公开 `Page` / `PaintPage` 字面量需提供 `line_placements`，手工构造可用空 Vec；
`LineRecord` 新增可空 `placement`。既有构造函数及 `LineRecord::default()` 会保留未知状态。
`required` 是引擎用于放置判断的预留量，不是实测 Word 行盒或字形墨迹边界。
新字段不参与 Word 验收；具体反例与证据限制见
[exact 纵向复核](EXACT-VERTICAL-EVIDENCE-2026-09-27.md)。

workspace/fontenv 回归 78 组、605 项通过、12 项忽略，Clippy 全目标通过。
随后加强同页已有单栏前缀、后组双栏平衡及再转单栏的测试，5 项定向测试全部通过。
Android 11 份旧采集仍为 186/186 条件源区间匹配。25 份 Mac 轨迹的 3834 行均带完整
精细诊断；只移除该新增字段后，旧完整页面内容逐项相同。没有新增 Word 读数，也没有
把既有几何失败改成通过。验证产物位于 `artifacts/line-placement-2026-09-27/`。
118 个文件的冻结清单 SHA-256 为
`602d459ecc5a96984233cbffcbc27974a9a78351ede3126c02485282079ff796`；
Mac 完整旧字段比较报告 SHA-256 为
`731e2705e9494ede5fbe71e1ae2dea204c7d0d4db4336261ba36677583c3eddf`。

## 后续：基线偏移保留精细字体度量

`FontMetrics::ascent_fine(text, font, measured)` 新增可覆盖的字体 ascent 出口，单位为
1/7200 英寸。调用方传入已经取得的 measure、fit 或 empty_line_metrics 结果；默认仅把
该 ascent 乘以 5，不增加一次度量或整形，也不改变 `TextMetrics` 的 C ABI。
它描述字体贡献，不承载段落 exact/docGrid 策略。RealMetrics 与 SimpleMetrics 暂用默认值。

`PendingLine` 改存 `baseline_offset_fine: i64`，按实际接受的正文片段和 tab 累计最大值。
空段、全隐藏段、显式控制符、宽度收行和段末收行全部接入；跨 run 回退或截短片段时，
重新计算剩余贡献，每次收行清零。独立段落标记仍按既有合同参与绘制，不因这次接口
增加而进入行高度量。原整数 ascent/descent/content 继续供行高与环绕使用。

落位直接计算 `quantize(top_fine + baseline_offset_fine)`，之后才应用 run/mark 位移；
`LinePlacement` 保留同一个未量化偏移。不能先把偏移舍入回 twips，也不能分别量化
行顶和偏移后相加。公开的粗粒度 `Line.baseline` 未改。这一步消除传递精度限制，
没有选定新的 exact 或网格公式，也没有修复已记录的 Word 几何反例。

新增 11 项定向测试覆盖四种收行、空行/tab、回退移走最大贡献、mark 位移、量化顺序，
以及 auto/atLeast/exact 下仅改变 fine ascent 时推进/占高/分页/源区间不变。
计数型度量验证默认出口复用已有空行结果，不额外 measure。首轮一项手算预期错误的
失败日志保留；修正测试选值后 11 项通过，生产代码未因该算术错误调整。

workspace/fontenv 回归 79 组、616 项通过、12 项忽略，workspace 全目标 Clippy 通过。
25 份旧 Mac、12 份新 exact 输入及无网格输入的两种字体量化模式共 39 份轨迹、3900 行，
完整 JSON 与改造前逐项相同，包括纵向诊断，没有删字段再比较。Android 旧窄路径仍为
11 份、186/186 条件源区间匹配。上述兼容验证不升级已有 Word 几何判定。

产物在 `artifacts/fine-baseline-2026-09-27/`；146 文件冻结清单 SHA-256 为
`c91a3ced65e577769329d264cd45f3824ac8d9ae5da6ac8e84dc562ddde25185`。
完整轨迹比较摘要 SHA-256 为
`13663b8a190429339fcc8627e22d291e2f930f3beeb98583041d40d823f05d43`；
新 CLI 二进制 SHA-256 为 `cab09884fc04db2599edc93c4b95b2b2dab4696943e50ede62d3354daae3fe66`。
