# 首个表格切片：table32-tail 形状（2026-09-27）

主文表格此前在布局中整块跳过，cell 正文完全不出现在页面上。本切片按
[结构流证据](STRUCTURED-FLOW-EVIDENCE-2026-09-27.md)的建议，只接入 `table32-tail.docx`
这一受限形状：有序正文块、row/cell 源身份、真实 cell 正文断行与绘制、固定行高行盒参与
分页续排，以及独立的表后段落。没有运行 Word、原生库或新增采集。

## 支持范围与拒绝规则

实现位于 `crates/core/src/table.rs`（投影与形状门槛）和
`layout/pagination.rs::format_table`（编排）。只有同时满足以下条件的表格才进入布局：

- 未声明表格样式；表格、行、单元格都没有修订，也没有 `tblPrEx`、`sdt` 等额外成员；
- `tblPr` 只含 `width`、`borders`、`cellMargins`；宽度为正的 `pct` 或 `dxa`；
- 左右 cell margin 显式为 0 dxa，上下为 0 或未声明（TableNormal 默认的左右边距非零）；
- 已声明的边框全部为 `nil` / `none`；
- 每行恰好一个单元格，行高 `hRule="exact"` 且为正；`trPr` 只含 `height`；
- 单元格只含段落，`tcPr` 只含 `width`（单格行由表宽决定，tcW 不使用）。

其它表格保持被跳过、计入 `skipped_blocks`，并输出
`block N: table not laid out: <原因>; omitted from the trace` 诊断，不用拟合高度代替。
table32、table32-noborder、tablepage、table30 因非 exact 行高被拒；table_cell、
talltable-80 因 auto 宽度被拒。

## 行为合同

- 行框横向从当前流区域左缘开始，`pct` 按当前区域（多栏时为栏）宽度换算。
- exact 行不拆分：放不下且区域非空（或位于部分栏带）时整行移到下一区域；空区域仍放置以保证推进。
- cell 段落使用共享 `break_paragraph_at` 在行框宽度内断行，段距按正文的非负折叠规则处理。
  超出行高的内容不裁剪、不撑高行，记录于 `TableCellBox.overflow_fine`。
- 表格前段落的段后距已计入游标；表格与相邻段落的段距不折叠，表后段落照常加段前距。
- 源投影：cell 内每段文本加 1 个段落标记单位，每格再加 1 个结束单位。
  只有单格单段行时它才与 `tablepage` 的 FormatLine 起点 0、35、70 一致；多格、行标记和
  嵌套仍未测。table32-tail 的 32 格覆盖 `[0,1120)`，尾段从 1120 开始。
- cell 段落的有效属性沿用主文 Resolver（docDefaults、段落/字符样式、直接属性）；
  表格样式层没有应用，因此声明样式的表格被拒。
- 已知限制，均有诊断：
  - 表格前段落的 keepNext 不与首行连接，引擎取消这条链（`keepNext ... into a table`）；
  - 表后缺少段落时，Word 会修复文档，引擎不模拟这次修复（`no following paragraph`）；
  - 多栏节的栏带里只要有表格行，就停用该栏带的平衡重放（`tables in multi-column sections`）。
    重放只格式化段落，不停用时会丢掉整张表的行（合成回归已证明）。

`Page.table_rows` 保存 `TableRowBox { table, row, column, rect, top_fine, height_fine, cells }`，
每个 `TableCellBox` 保存源区间、页内行号范围和溢出量。`layout-trace` 在存在受支持表格时
输出 `tableLayout` 与 `layoutInput.tables`；没有受支持表格的文档保持原轨迹结构，
方便逐字节比较历史回放。缺字扫描也会覆盖 cell 段落。

## 验证

`crates/core/tests/table_flow.rs` 的 11 项回归都走真实 DOCX 装载：

- table32-tail 同构：32 条标签全部绘制，行起点 `720+480k`，行源区间 `[35k,35k+34)`，
  尾段 `[1120,1121)` 位于 `(720+32×480)×5` fine，共 1 页；
- 正文高 10×480+100 时每页 10 行，4 页间行源连续、无丢失或重复，续行从正文顶开始；
- 表格前后段落的段距、半宽表格内换行、docDefaults 字号进入 cell run、超高内容溢出；
- 五种不支持形状继续跳过；无尾段和 keepNext 的诊断；
- keepNext 回归先确认去掉引擎修改会失败（标题被错误的链接挤到下一页）；
  多栏回归同样确认去掉平衡守卫会丢失 4 行。
- 真实 `../word_analyse/fixtures/table32-tail.docx`（ignored 用例，已运行）：
  1 页、32 行框、32 条标签。报告级 Word 约束同为 1 页。

真实 CLI 用 Calibri 运行 table32-tail，得到 1 页、33 行和 1089 个字形。table32-rowh
可以排版，但带有无尾段诊断；Word 报告为 2 页，差额来自未模拟的修复段，本引擎没有声称对齐。

护栏（相对 `e1c1a47`，core 自 `9fd8948` 后首次改动）：

- `RSWORD_TEST_CALIBRI=/tmp/wordfonts/calibri.ttf cargo test --offline --workspace
  --features rsword-layout-core/fontenv`：741 passed / 0 failed / 13 ignored；
  同配置下 workspace all-targets Clippy `-D warnings` 通过。
- Mac 离线回放（`artifacts/table-flow-mac-replay-2026-09-27/`，二进制 SHA-256
  `3c12168d3d922a3d448896639e15585f4d98b8de5824e38537c733646651474c`）：25 份 trace
  与 `autospace-consistency-mac-replay-2026-09-27/after` 逐字节相同，30 个 bundle 的
  状态与原因不变，仍为 0 OK / 23 FAIL / 7 UNDECIDABLE。这批样本没有表格，
  所以它只说明非表格文档未受影响，不能作为表格正例。
- Android 11 份条件回放（`artifacts/android-replay-table-flow-2026-09-27/`）：186/186 行区间保持。

页数相同只是附加约束，不代表表格几何已与 Word 对齐；行盒与续排都是引擎合同，不是 Word 读数。

## 后续

atLeast/auto 行高（需要内容高度决定行高）、多格行与 tblGrid、cell margins 和边框绘制、
表格样式层、行内拆分、重复标题行、表后修复段，以及表格进入 keepNext 链和栏平衡。
talltable-80 是第一个真实跨页表内续排的调查对象，其正文高为 13958。
