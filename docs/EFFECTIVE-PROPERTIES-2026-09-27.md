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
显示隐藏文字、非空段落的独立段落标记样式、编号标记绘制及兼容开关仍待继续。
空东亚主题槽与 docDefaults 语言回填的字体来源也需要后续专项验证，不能把非空主题槽
测试的通过推广为所有主题字体组合都已对齐。
运行记录位于 `artifacts/section-core-2026-09-27/`。
