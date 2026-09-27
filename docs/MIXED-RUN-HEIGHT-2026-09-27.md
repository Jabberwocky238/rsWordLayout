# 混合字体拆分 run 的行高反例

真实字体的纵向分量现按整行已接纳内容合并，再统一取整，修复了仅拆分 run 就改变
行高、基线和分页的问题。原有单 run 的量化策略保留；自定义字体度量仍可独立提供
精细高度和基线。下面先保留修复前反例，再说明生产实现与验证。
这是引擎内部一致性修复，不是新的 Word 实测，也不证明 docGrid 的字体输入映射。

## 修复前结果

使用仓库的 Liberation Sans/Serif 字体，12pt，ascii 槽为 Sans、hAnsi 槽为 Serif，
两个相同段落各含 `Aα`，auto/240 行距、零页边距。所有 run 使用同一个 FontSpec。
`VerticalGrid::None` 下：

| 输入 | 每行推进，1/100pt | 基线偏移，1/100pt | 正文高 556 twips 时页数 |
| --- | ---: | ---: | ---: |
| 一个 run：`Aα` | 1397 | 1085 | 2 |
| 两个 run：`A`、`α` | 1380 | 1085 | 1 |

两版源 UTF-16 区间均为 `[0,3)`、`[3,6)`，含各自段落标记。基线偏移一致，第二行
的推进却相差 0.17pt，足以改变页面归属。Sans/Droid 的拉丁字母与汉字组合也复现。
这里的 None 是字体指标的量化设置；输入没有 DOCX docGrid。

Mono/Serif 还暴露粗指标下限的影响：None 下单 run/拆 run 的推进为 1481/1430；
Mac 指标量化模式下为 1490/1465。仅调整自然高度总量的最大值不能完整修复这些例子。

## 原因与修复范围

`RealMetrics::vertical_raw` 在一次测量内，对各个实际字体的未取整 ascent、descent
和 line gap 分别取最大值，再将总和取整。修复前的 `layout.rs` 在多个已接纳 run/piece 之间
却只对各自的自然高度总值取最大值。分量峰值来自不同字体时，这两种运算结果不同。

现有 `line_height_fine` 还把自然高度与分别聚合的粗 ascent+descent 比较。Mac 模式
在一次测量内由量化后的总量重建 ascent，此前拆 run 会改变这条下限。因此简单增加
三个已取整 fine 分量仍可能留下差异，或破坏当前单 run 的一次舍入行为。

新增 `FontMetrics::line_metrics` 接收已接纳的 `MeasuredFontSpan` 列表，输出
`LineFontMetrics`。默认实现保留原来五项独立最大值，复用已有粗指标并继续调用
`natural_height_fine` / `ascent_fine`。它不以粗 ascent 反推精细自然高度。

`RealMetrics` 覆盖该方法，对每个片段按其 FontSpec 选择实际字体，只读取纵向量，
不重新整形；跨片段合并未取整 a/d/g 后，复用单请求的粗指标投影，精细自然高度则
对总和取整一次。None 模式仍分别取整粗 a/d/g；Mac 模式仍量化总和与 descent，再
相减得到 ascent。内容下限仍使用这些投影后的 ascent+descent，未改成对原始内容
总和重新取整的另一种规则。此处 coarse 单位为 twips，fine 单位为 1/100pt。

`LinePiece` 缓存实际 measure/fit 结果。完整片段、拟合前缀和 tab 三个入口均保存
对应结果；tab 的纵向输入仍为空格。部分截短只更新该片段的缓存，整片删除不再
贡献高度。显式换行、宽度收行和段末共用同一个整行计算入口，只对最终保留片段
计算一次。空段、全 hidden 段、空控制行和对象行继续使用既有 empty 指标及两个
fine hook，保留有符号值；绘制用段落标记、rise 和源位置缺口不额外加入行高。

分页、keep/widow、分栏试排和后续环绕查询消费同一份收行结果。字体指标的 Mac
量化策略和 DOCX docGrid 仍是不同输入，本修复没有启用新的生产文档网格算法。

## 可重放材料

[反例包](../artifacts/mixed-run-height-counterexample-2026-09-27/README.md) 保存 Rust
探针、实际命令、源码/字体/rlib/可执行文件哈希及输出。探针链接现有 fontenv rlib，
编译与执行均 exit 0；没有运行 Cargo 或 Word。源码哈希记录当时文件，rlib 哈希
绑定实际执行实现，不以二者的并列记录伪造完整 Cargo 构建来源。

10 成员包已审核并冻结，清单 SHA-256：
`108eb0b571e3ffa4aa337e9206a53b46bddb55fdbf4e163851d65cc81bb30ee8`。
重放须使用新输出目录；现有包继续保留修复前结果，作为回归对照。

## 修复验证

新增真实字体集成测试 9 项，覆盖三组字体、两种指标量化模式，以及整片删除和片段
截短两类回退。合并/拆分版本同时比较行顶、推进、required、基线偏移、量化后基线、
完整 UTF-16 行区间与相邻页面容量阈值。原有单 run 的六组高度与基线数值保持。
拟合前缀测试还确认未接纳的高字体不污染当前行。

首轮旧生产代码的 7 项测试中，六组拆 run 用例失败，单 run 基准通过。加入基线
断言和两项前缀/回退防回归后，最终 9 项再次链接旧 rlib，结果为 6 失败、3 通过；
修复版本的 9 项全部通过。最终旧版检查保存了测试源码快照和实际 rlib 哈希，
没有用修改后的生产源码冒充旧库来源。两项前缀/回退测试属于防回归控制。

另有 5 项不依赖 fontenv 的自定义 provider 测试，验证默认五项独立最大值、trait
object 调用、fit 指标保留、回退缓存和空/hidden 段的负基线。现有精细基线、环绕、
行高漂移及跨 run 回退测试均通过。完整 workspace/fontenv 共 636 项测试通过，
12 项既有测试忽略。core 默认功能测试 511 项通过，workspace/all-targets Clippy
开启 fontenv 并使用 `-D warnings` 通过。新构建的 CLI 保持 Android 11 份既有窄
路径采集的 186/186 有序行边界；11 个完整引擎轨迹与前一片逐字节相同。Android
结论仍带明确的历史视图/CP 假设，不代表新的打印分页或几何对齐。

日志、命令、源码快照和二进制保存在
[生产验证包](../artifacts/mixed-run-height-fix-2026-09-27/README.md)，103 成员清单
SHA-256 为 `d323f69ab3c6b775b0c9edd69b3423c10eb4c34480c1bb1d403c69cbc449eee6`。

Mac 则从干净的 `24a9f5f` 独立构建旧版，与固定新版执行同参数完整离线回放。
25 份有效采集对应的 356 页、3,834 行及完整 trace 均相同；5 份 VOID 仍不可判。
两轮对 Word 的比较仍是 25 FAIL，保留原有结构/几何偏差和 selfcheck 不可判，
不能称为 Word 对齐通过。该语料中的混排主要为同字体不同字号，未覆盖新反例的
跨 run 字体分量峰值；修复的直接验证仍来自上述新增真实字体用例。
详细前后报告见[Mac 回放包](../artifacts/mixed-run-height-mac-replay-2026-09-27/README.md)。
542 成员清单 SHA-256 为
`8c38d91357f83cb6f2a815aaccb83361d93a94370b9e6ece854d12c23a8399ff`；
临时旧版 worktree 在确认干净后已移除，构建和清理收据均保留。
