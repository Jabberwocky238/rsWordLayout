# 有效属性进入共享排版器

DOCX 的 CLI、SVG、CFFI、WASM 入口统一使用 `LoadedDocument::layout_document()`。
原始 `LoadedDocument.json` 仍是声明值 JSON；需要单节手工页面设置的调用方可用
`LoadedDocument::paragraphs()`。已有裸 JSON 桥接保留历史行为。

## 实现

使用钉住版本 rsWordParser `399e36a` 的 `Resolver`，没有升级依赖或修改外部项目。
字号、字体槽、隐藏状态、caps、字距/缩放、段距/行距、缩进、keep 系列和段前分页，
都从 docDefaults、默认段落样式、basedOn、段落/字符样式以及直接格式合成。
显式 false 由解析器的合成规则处理，正式入口不再猜测 `Heading1` 等名字的字号或间距。
主题字体解析为四槽实际名称；空段落使用有效段落标记字体。

制表位需要按位置叠加并处理 clear，不能使用解析器对整个数组的覆盖合并。
这里复用实际样式链，依次叠加 docDefaults、根到叶样式和段落直接制表位。

解析器编辑模型会把样式隐藏整段归为 `protected/invisible`，甚至包括段落/字符 toggle
合成后实际可见的内容。布局入口对这些候选使用同一 DOM 重建文本模型，节点号保持一致；
重建时仅屏蔽分类用的克隆样式 vanish，最终有效属性依然由未改变的原样式计算。
原始 JSON 不改，非文本的 invisible 图形不被转换，节块范围也不重新编号。

并排 `w:rPr` 的既有修复路径会同时重算有效属性映射，避免修复后节点号变化造成错配。
修复失败则同时使用原始 JSON 和原始有效映射。

## 验证边界

`crates/core/tests/effective_layout_props.rs` 通过真实 DOCX 包验证默认值、样式链、
显式关闭、主题字体、制表位 clear、继承段前分页、隐藏内容 UTF-16 源偏移，以及
相同有效属性的直接声明版/样式版几何等价。

这些是输入合成及共同排版逻辑回归，不是新的 Android Word toggle 行为采集。
最终有效属性专项 16 项、几何专项 22 项、含 Calibri 的隐藏文字专项 14 项全部通过。
核心/SVG/CFFI/WASM 完整测试、workspace 检查和 Clippy 通过；Android 历史假设回放
保持 186/186，25 组 Mac 轨迹的页面数据保持相同，既有失败与不可判项没有新增。
显示隐藏文字、非空段落的独立段落标记绘制与行高贡献、编号标记绘制及兼容开关仍待继续。
空东亚主题槽与 docDefaults 语言回填的字体来源也需要后续专项验证，不能把非空主题槽
测试的通过推广为所有主题字体组合都已对齐。
运行记录位于 `artifacts/section-core-2026-09-27/`。

## 后续：独立段落标记属性

`Para.mark: ParagraphMarkProperties` 现在保存两份独立输入：

| 访问器 | 来源 | 缺省含义 |
| --- | --- | --- |
| `declared()` | 当前主 part 段落的 `props.rpr` | 没有直接声明 |
| `effective()` | 同节点的原生 `Resolver::run` 结果，含解析后的主题字体槽 | 没有可用的解析结果 |

“保留”针对钉住版本的原生 JSON 投影，不表示原 XML 字节或逐字段来源均已保存。
两份输入不互相补齐，也不从最后一个文字 run 反推。空对象、显式 JSON null、非法原生
JSON 类型和缺失分别保留；有效结果即使只有空的 `fonts` 对象，也仍表示 resolver 已运行。
只带原生 JSON 的兼容入口保存直接声明，不伪造已完成样式继承的结果。

标记不进入 `Para.runs`，不会增加文字长度或再消费一个 CP。非空、空和全隐藏正文均保留
独立属性。隐藏段落恢复后从恢复的块读取直接声明；并排 `rPr` 修复后继续使用同一次解析
重建的节点号与有效映射，不拿旧节点或段落序号关联。

`LayoutDocument::trace_metadata()` 新增 `paragraphMarks`，按段序保存 `sourceNode`、
两份属性及 `declaredPresent` / `effectiveAvailable`。独立 presence 位避免 JSON null
把“未声明”和“显式 null”混在一起。`layoutPolicy` 说明当前应用边界。

输入接入片保持既有布局行为：正式 DOCX 的空段落仍用有效标记字体与抬升；非空或全隐藏段落
仍沿用原绘制和度量路径。颜色、隐藏标记、独立末行占高、分页符与标记分别绘制不是因为
属性被保留就已经实现。原始 `LoadedDocument.json` 不被写成有效值。
手动完整构造 `Para` 的 Rust 调用方需新增 `mark: Default::default()`；使用结构更新
`..Para::default()` 的调用方保持可用。

输入接入提交为 `1ebd1f5`，规范探针及旧采集复核为 `75de561`。
`paragraph_mark_props.rs` 的 10 项专项验证上述状态、真实 DOCX 继承/主题、隐藏恢复及修复映射；
workspace/fontenv 共 75 组、580 项通过、12 项忽略，全目标 Clippy `-D warnings` 通过。
8 份规范输入及清单逐字节可重现；实际 CLI 验证 24 个标记的直接/有效字体槽、字号、
可见性及有序源区间，解析警告为零。这些是输入验收，8 份仍没有 Word 测量结果。

Android 旧采集保持 186/186 条件源行区间匹配；Mac 保持 25 FAIL / 5 UNDECIDABLE，
25 份有效轨迹的完整页面及比较统计与上一轮精细环绕产物逐项相同。
记录位于 `artifacts/paragraph-mark-inputs-2026-09-27/`，`baseline-diff.json` SHA-256 为
`8fecad5c77f878572dfd0a50783a27da6e56e6da15b8676234f2a86d0cbbffd8`；
356 文件清单 `durable-hashes.json` SHA-256 为
`01b0ca9ab3b627b3f0d9bfdd7e79d535869890f337a1f5c6b1dca9b3f100c944`。

下一片的具体反例见 [段落标记证据](PARAGRAPH-MARK-EVIDENCE-2026-09-27.md)：
未作废的 vmisc2 第二次采集提供段尾字号不同于末 run 的限定观察；上/下标及 position
的三个回普通基线读数来自整批 VOID 的 probe-metrics，只保留为探索线索，不能作为验收依据。
应继续分开处理 mark 与行内控制符的绘制样式和源位置；现有有效观察仍不能决定独立 mark
如何贡献末行高度、页面占高或全隐藏段落行为。

## 后续：独立标记绘制

尾部结构提交 `5103a4e` 将控制字符与段落末尾保存为有序项，每项有明确的 UTF-16 源区间、
替代空格数量及绘制样式。源范围不再从字形数量倒推，隐藏内容和对象留下的源缺口以空片段
保留。源区间连续、样式相同且替代文本等长时仍合并整形；不同样式的后续尾项从前一组实际
推进位置开始。多组尾项跟在 tab 后时分开绘制，避免用孤立空格宽度猜测上下文中的 tab 字形。
该结构提交的 61 项定向测试通过、1 项既有条件测试忽略，25 份旧 trace 的完整页面逐项相同。

绘制片仅在真正的 `ParagraphMark` 上使用独立 `FontSpec` 和精细位移，分页符、栏断、
软回车及无字形的分节末尾不被覆盖。JSON 投影集中在 bridge，排版代码只读取私有 typed
样式；`declared()` / `effective()` 仍原样保留。可应用条件为有效结果包含非空 ascii 或
hAnsi 字体、正 u32 字号；如声明了 position，必须可解析为 parser 使用的 i32；如声明了
vertAlign，必须为 baseline/superscript/subscript。字体槽选择顺序与正文投影一致。
缺少这些必需字段或值非法时保留旧绘制近似，不借末 run 填齐，也不把宿主默认值称作 Word
缺省。`paintStyleAvailable` 单独报告能否投影，不能代替 `effectiveAvailable`。

有效 position/vertAlign 缺省时使用现有 run 投影的正常基线；其余字体字段也复用既有 run
投影，包括其中未完成的上下标、缩放、字距近似。颜色、隐藏标记、空行度量、行高、页底
容量及对齐宽度维持原规则；正式 DOCX 空段落已有的标记字体度量路径继续保留。这一片不把
“mark 不贡献行高”定为 Word 规则，也未验证 Android/Windows 的独立标记几何。

11 项新的公开入口测试检查独立属性、稀疏/非法回退、双尾项、不同视图、空/全隐藏正文、
soft return 空末行、源代理对和对象缺口、正文整形及连续栏组重放。真实 DOCX 规范输入也
进入测试，但仍是引擎行为回归，不是新增 Word 测量。测试和离线轨迹记录在
`artifacts/paragraph-mark-paint-2026-09-27/`。

绘制实现提交为 `5c62d5c`。workspace/fontenv 最终 77 组、600 项通过、12 项忽略，
全目标 Clippy `-D warnings` 通过。首次全量运行暴露 C 入口测试将正文与 mark 视为同一
片段的旧断言；C/Wasm 测试现分别验证正文的 20-twip 字距、mark 的独立 0 字距及各自 CP，
入口生产代码未改。原失败日志保留，最终成功日志为 `workspace-tests-after-entry-assertions.log`。

新 CLI 在 8 份规范输入中验证 24 个 mark 的精确源 CP、字体文件身份和独立字号，仍无
解析警告；Android 保持 186/186 条件源行区间。Mac 仍为 25 FAIL / 5 UNDECIDABLE。
25 份有效 trace 的 356 页、3834 行及全部源/终止符/已有栏元数据、18150 个字形的数量
完全不变；仅 5 份文件的 20 个真段落标记改变，其余字形逐项一致，没有浮点末位差异。
其中 9 项只改变 advance，9 项只改变 y，2 项同时改变字号和 advance。
`audit-paint-replay.json` SHA-256 为
`15fea7a36d4a252ef3d1722843a829a5e0251c08bcc9828de69ed3df63845caf`。

vmisc2 的 CP37/53 分别从沿用正文的 10pt/8pt 改为标记自身的名义 13pt，且与同行正常
正文 glyph 的相对 y 保持 0。Word PDF 读数仍是 12.96pt；字号残差 0.04pt、绝对 y 残差
6.72/16.32pt 未消除，不能称几何通过。其他变化的旧包限定复核见
[独立位移与推进](PARAGRAPH-MARK-EVIDENCE-2026-09-27.md#旧包中的独立位移与推进)。

本片 404 个产物的冻结清单 `durable-hashes.json` SHA-256 为
`6d9733c284ab70dec13090f91f84fffe932664ce4e37e36ffc796a530d166f89`；
实际回放二进制 SHA-256 为
`5b2c76367e726ce34cc51d35d5b3fc16f14b0a25d421c8cc509154a105856483`。
