# 文档兼容项、孤行和段落保留

共享 PTS/LS 路线第 2 片的首个文档兼容项，以及第 5 片的段落保留规则已接入。
这批改动使用同一个源流和断行器，没有按桌面、Android 各写一套分页算法。

## 文档兼容项

`LayoutDocument.compatibility` 和 `Engine::with_compatibility` 接收
`DocumentCompatibility`。`split_page_break_and_para_mark` 保留 `None`、`Some(false)`、
`Some(true)`，轨迹 `layoutInput.compatibility` 同步记录。

钉住的解析器 `399e36a` 把 `w:compat` 的布尔子项放在 `raw_unmodeled`，没有投影到原生 JSON。
因此 `LoadedDocument` 从 settings 关系指向的原始 DOM 按 QName 和 OnOff codec 读取，
原始 `.json` 保持不变。`document_from_json` 默认不带该项，手工调用方可以显式设置。

打印视图在该项为真时，把段末手动分页符与段落标记拆成两行；移动视图沿用自身的拆行规则。
显式 false 不关闭移动视图规则。分节符不被当成段落标记，段内还有文字时仍按原分页控制流处理。
多次使用同一个 Engine 不会把上一份文档的开关泄漏到下一份。

## 页内候选行与保留规则

`Para.widow_control` 读取有效 `widowControl`。未指定时沿用宿主 false，样式和 docDefaults
可以启用，直接 false 可以关闭继承值；不把一个无样式夹具的省略行为推成所有 Word 文档默认。

自动分页先选择可提交的候选行数，再落位。启用孤行控制时，正常情况下断页两侧各保留至少两行；
三行段落不能拆成 1+2 或 2+1。页面或环绕区域改变时，重排下一页的尾部再检查，不能只数旧缓存行。
空页仍装不下、或保留约束互相冲突时，至少提交一行，保证源游标前进。

keepNext 链能装进新页时，在提交首段之前整体搬移。链末端按合法前缀预留：普通段至少一行，
孤行段至少两行（仅三行的段保留全段），keepLines 段保留到首个手动分页之前。
段后距和下一段段前距计入预留；下一段 pageBreakBefore 或需要另起页的节会截断 keepNext 链。
超页长链退化为直接后继的最小前缀，不在空页上无限重试。

手动分页强于自动保留。keepLines 只预留到首个硬分页处，避免把含手动分页的整段提前搬走。
该组在新页也装不下时，放宽 keepLines 并继续落位，避免破坏与上一段仍可满足的 keepNext 关系。
末端孤行段跨页后如果因更宽的区域从两行变成一行，keepNext 预留也会重新计算，
避免首段已经落页、后段却因孤行规则独自移页。

## 证据与验收

`paragraph_keep.rs` 包含合成边界、继承属性和原 DOCX 回测。
用本机 Android Calibri，现有报告的结果被复现：

| 输入 | 每页行数 | 第二页首 CP |
| --- | --- | --- |
| widow-split | 32、1 | 1933 |
| widow-on | 31、2 | 1834 |
| keep-next | 31、2 | 1054 |
| keep-lines | 31、2 | 1054 |

这些期望来自 `word_analyse/reports/rsword-diff/widow.md` 等报告。
原始 widow/keep 采集日志未在本机找到，不能把回测升级成重新审核过原始日志或新 Word 采集。
多行 keepNext 链、极小页面上的退化以及复杂环绕组合属于明确标记的合成约束测试。

`document_compat.rs` 验证开关三态、平台/视图组合、标准 OnOff 值、不同 XML 前缀与命名空间、
非默认 settings 关系路径、无关同名元素隔离及属性修复后设置保留，没有新增 Word 采集。
运行记录位于 `artifacts/paragraph-flow-2026-09-27/`。

本切片专项包含 22 项段落保留、10 项文档兼容、11 项精确分页测试，全部通过；
core/SVG/CFFI/WASM 的 Clippy `-D warnings` 通过。Android 既有窄路径回放仍为
186/186（带原历史假设）；Mac 最终回放的 25 份有效轨迹页面内容与改动前一致，
既有 25 包 FAIL、5 包采集无效不可判保持原状，没有升级为 Word 对齐通过。

docGrid 尚未实现。其九份页数判别只确定三档容量，不能确定网格阈值的字体来源；
例如直接拿 Calibri 12pt 自然行高 292.969 twips 套公式，能通过九份判别，
却会在 pitch 297 上预测 51 行而报告只有 26 行。
[页数证据审计](ANDROID-PAGE-EVIDENCE-2026-09-27.md) 和
[推进/占高接口](LINE-VERTICAL-METRICS-2026-09-27.md) 已接入，实际网格量仍需确认。
