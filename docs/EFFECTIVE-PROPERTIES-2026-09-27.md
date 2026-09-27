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

这一片保持既有布局行为：正式 DOCX 的空段落仍用有效标记字体与抬升；非空或全隐藏段落
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
旧 vmisc2 段尾字号不同于末 run，上/下标及 position 样本的段尾回到普通基线。
应继续分开处理 mark 与行内控制符的绘制样式和源位置；这些观察还不能决定独立 mark
如何贡献末行高度、页面占高或全隐藏段落行为。
