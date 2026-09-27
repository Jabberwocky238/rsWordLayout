# 共享 PTS/LS 排版内核开发顺序（2026-09-27）

开发方向：用一套源字符、段落断行和页面编排算法服务 Android 与桌面端。
Android 的 PTS/LS 分析用于定位规则和设计实验，真实输出用于验收；字体、视图、
文档兼容选项和宿主默认值作为输入保留。本计划在现有 Rust 引擎上分步实现。

第一片 `w:vanish` 已实现，第 2 片的有效属性和首个文档兼容开关已经接入；
直接隐藏文字的真实字体专项回归已通过。
后续取得一份新 Mac Word 双栏原始 PDF，确认普通文档末页 32+8 顺序填充，
见 [采集结果与连续分节超时限定](COLUMN-BALANCE-MAC-2026-09-27.md)。
第 3 片的文档几何与跨页续排也已实现，见
[页面几何进展](SECTION-GEOMETRY-2026-09-27.md)。其余未标记实现的条目仍是计划。

## 依据与适用范围

用户提供了 Android 和桌面端共用 PTS/LS 的代码分析结论，本计划据此采用共享内核方向。
目前仓库中可以直接复查的是
[`word_analyse/findings/android-word-layout.md`](../../word_analyse/findings/android-word-layout.md)：
Android 的 `ptls7/ls`、`ptls7/pts` 字符串、回调族，以及 LS 产行、PTS 编排页面的路径。
该文第 5 节仍将 Windows 对应关系列为检索线索。因此，本轮不把“同族源码”扩写成
“各版本二进制相同”，也不把 Android 输出自动计为 Windows 验收通过。

桌面端还有更直接的本机静态证据：
[`macos-word-layout.md`](../../word_analyse/findings/macos-word-layout.md) 记录 Word 16.112.3
链接 `MicrosoftPTLS7.framework`，导入 `FsCreatePageFinite`、`LsCreateLine` 等同族接口；
页尺寸策略类也与 Android 对应。后续按共同的 LS/PTS 职责实现，平台差异先定位到输入、
兼容选项与具体分支，不为两个平台各写一份断行和分页算法。

现有基线见 [接入评估](WORD-ANALYSE-INTEGRATION-2026-09-27.md)：

- Android：11 份既有窄路径采集，186 个完整有序行区间可回放；视图及 CP 空间使用
  显式历史假设，验证范围是行边界。它们不能验收打印分页、字形位置或独立的 run 切分。
- Mac：本仓库 `captures/` 与 `tools/measure/sweep.py` 提供几何和结构回归。
- Windows：尚无本仓库独立实测基线。`Platform::Desktop` 当前实测基础是 Mac。
- 同一套算法不保证同一组输入：字体文件、回退、度量量化、视图宽度、兼容模式和默认
  制表位都可能改变结果。保留 `Platform` / `View`，发现差异先定位实际输入或分支。

## 当前接缝与实施边界

| 层 | 现有代码 | 下一步沿用的接缝 |
| --- | --- | --- |
| DOCX 与属性 | `crates/core/src/load.rs`、`bridge.rs::paras_from_document` | 把声明属性补成有效属性，保存节、源坐标和兼容设置 |
| 源字符与行内对象 | `layout.rs::Run`、`segments`、`cursor_source` | 可见性只改变布局贡献，UTF-16 源位置保持连续 |
| LS 对应职责 | `layout.rs::break_paragraph`、`shortfall`、`PendingLine`、`font/*` | 同一套 cluster 度量、断点、禁则、制表位和跨 run 回退 |
| PTS 对应职责 | `layout.rs::Engine::layout`、`line_height_fine` | 根据节、页面/栏、保留约束和行度量选择落位及续排位置 |
| 输出 | `place_line`、`paint_document`、`oracle.rs`、`oracle_json.rs` | 保留行身份和源区间，分别报告断行、几何与页/栏归属 |

制定路线时有三处接缝限制了后续算法，现已完成基础改造：

1. `layout-trace` 已读取 DOCX 页宽、页高和边距，并单独记录 CLI 覆盖值；分节保留独立输入。
2. 页面或环绕变化后，`LineCursor` 从原源流续排；保留规则搬移整段时也重新断行。
3. `PendingLine.vertical` 已区分游标推进与最大占高，分页和保留约束共用组合规则。
   现有无网格行仍产生相等数值，尚未引入网格阈值，见 [纵向度量](LINE-VERTICAL-METRICS-2026-09-27.md)。
   段内环绕查询已统一使用实际精细推进；连续栏组预排保留精细顶部，避免逐行或栏顶舍入改变断行。
   最终行顶、推进、占高和量化前后基线也已保留到诊断轨迹；exact 的旧 ascent 定位仍有明确反例，
   见 [证据边界与后续探针](EXACT-VERTICAL-EVIDENCE-2026-09-27.md)。
   字体 ascent 已可通过默认兼容的精细出口传到最终落位，回退和四种收行路径均保留其精度；
   新规则可以在此接入，不必先压回整数 twips。12 份规范 exact 探针也已取得独立 Word 采集。

先修改现有模块。到第 3 片确实需要续排状态时，再将相关私有实现提取到
`crates/core/src/layout/line.rs` 和 `layout/flow.rs`；保留 `Engine` 及 `lib.rs` 的公开出口。
不根据反汇编对象偏移复制类层次，也不预建完整的 PTS 对象树。

## 1. 隐藏文字贯穿源流、断行和绘制

**状态：直接声明与有效样式属性已实现。** 显示隐藏文字选项和段落标记完整规则待续。

修改 `bridge.rs::collect_runs`、`layout.rs::Run`、`segments` 和必要的制表符后文本度量。
隐藏 run 仍消费原始 UTF-16 长度，但不贡献宽度、可见字形、行高或控制符动作。
隐藏区域两侧不因源坐标缺口凭空成为断点。`webHidden` / `specVanish` 沿用已测对照行为。

验收：

- `../word_analyse/fixtures/vanish.docx`，Calibri 12pt，Android mobile，版心 5329：
  行起点 `[0, 73, 116]`；print，版心 10466：`[0, 116]`。
- `webhidden.docx`、`specvanish.docx` 的 print 对照仍为 `[0, 86]`。
- 合成回归覆盖前导/尾随隐藏文本、全隐藏内容、补充平面字符、隐藏 tab/换行/分页符、
  跨 run 回退，以及右/中/小数点制表位后含隐藏内容。隐藏文本不能泄漏为绘制命令，
  后续可见字形源坐标不能前移；全隐藏段落的段落标记规则单独测试。
- Android 186 行回放无退化；已有 `source_offsets`、`cross_run_retreat`、`soft_breaks`、
  `tab_stops` 和绘制测试通过。

这些 Word 起点是已有报告的回测，不计入那 186 行，也不声称获得了新 Word 读数。

实现使用 `Run.hidden`；原有 Rust 调用方的 `Run` 字面量需补 `hidden: false`。
CLI 的缺字扫描也跳过隐藏 run，避免它改变可见文字的回退字体选择。
段落标记独立声明与有效属性已通过 `Para.mark` 接入；解析出完整拉丁字体和字号时，
标记绘制使用自身字体及位移，控制符与 mark 保留各自源区间。全隐藏段落的行高仍沿用
末 run 度量近似；独立 mark 的行高、颜色及可见性规则尚未实现，见
[标记绘制](EFFECTIVE-PROPERTIES-2026-09-27.md#后续独立标记绘制)。显示/打印隐藏文字的选项未加入。
可见分页符后只剩隐藏文本时，仍保留按源位置判断分页符与段落标记是否紧邻的旧策略；
这个组合未用 Word 验证，不能用普通 vanish 夹具的通过推导它也对齐。

## 2. 有效属性与兼容输入贯穿桥接

**状态：有效属性与 `splitPgBreakAndParaMark` 已实现，`noColumnBalance` 三态输入已保留。**

实际入口为 `LoadedDocument::layout_document()` / `paragraphs()`；原始 `.json` 保持
声明值合同，裸 JSON 桥接保留旧行为。run/para 属性使用钉住版本的 `Resolver`，
主题字体解析成四槽，制表位按位置合并。解析器把样式隐藏整段折叠成 invisible 块的情况，
通过同 DOM、同节点号重建文本投影，实际属性仍由原样式合成，隐藏源 CP 不丢失。
详见 [有效属性进展](EFFECTIVE-PROPERTIES-2026-09-27.md)。

兼容项从原 settings DOM 读取，因为钉住版本的原生 JSON 不投影它；
见 [文档兼容项与段落保留](PARAGRAPH-FLOW-2026-09-27.md)。原实施要求如下：

修改 `load.rs` / `bridge.rs` 的投影接缝；优先使用当前钉住的 rsWordParser
`399e36a` 的 `Resolver` 能力。它已有 `resolve::Resolver::run` / `para` / `section`，
但当前 `LoadedDocument` 只保存声明值 JSON，不能直接假定这些 JSON 已完成继承。
先核对钉住版本的可用 API，再决定将有效属性追加到投影，或在装载期间生成布局输入。
不要根据同机另一个 parser 工作区的最新 API 改动依赖。

第一批仅接布局已消费的属性：vanish、字体/字号、段距/行距、keep 系列、制表位。
保存“未指定”和显式 false 的差别，按字段使用解析器的合成规则，避免统一做布尔 OR。
文档 `splitPgBreakAndParaMark` 已独立接入
`Engine::splits_page_break_and_mark`，与现有视图策略组合，不借平台开关表达文档设置。

验收：

- 新增真实 DOCX 回归 `effective_layout_props`：直接格式、段落样式、字符样式、
  `basedOn`、docDefaults，以及直接关闭继承属性；同时检查解析结果和最终布局。
- 相同有效属性的直接声明版与样式版应有相同行区间和几何，源节点标识可不同。
- 新增兼容选项用例，覆盖 Print/Mobile 与开/关组合；未声明设置的旧样本保持原结果。
- `cargo test --offline -p rsword-layout-core --features fontenv`，随后运行两平台回放。

Android 的无 styles 最小夹具不能证明样式合成正确；新样式用例先验证输入一致性，
需要 Word 采集的特殊覆盖语义单列未测。

## 3. 节页面几何与可续排的段落格式化

**状态：文档几何与续排已实现，复杂连续节/栏模型待续。** 这是 PTS 分页开发的第一个
可独立验收切片，先于 docGrid 调参。实际入口为 `document_from_json`、
`LayoutDocument` 与 `Engine::layout_document`，续排由私有 `LineCursor` 记录。
实现和验证边界见 [页面几何进展](SECTION-GEOMETRY-2026-09-27.md)。

在 `bridge.rs` 增加携带段落和节范围的文档级布局投影，保留 `paras_from_document`
给现有调用方；在 `layout.rs` 增加使用节信息的布局入口，原 `Engine::layout(&[Para])`
作为单节入口继续可用。节信息按原 `main` 块范围绑定，不能用过滤后的段落序号替代。
新增入口先支持文本块；被跳过的表格、脚注等必须继续报告，不能给出全文 CP 已完整覆盖的结论。

读入 `pgSz`、四边 `pgMar`、gutter 和分节类型，记录有效值来源。CLI 默认采用文档几何，
显式 `--margin` / `--page-width` / `--content-width` 保留诊断覆盖能力并写入轨迹。
先支持单节与 nextPage；continuous、奇偶页保持独立行为条目，不能全部压成 bool。

在同一切片把断行接缝改成“从源游标在给定区域/y 格式化下一行或一段候选行”。
`PendingLine` 保留字体、宽度、源范围、终止符和行度量；页面/栏切换由编排层负责。
段落被保留约束移到新位置后重新查询 `WrapContext`。快照至少含段内源游标、首行状态、
待处理控制符与行内断行状态，不能只缓存已经摆好 x 的 `Vec<PendingLine>`。

验收夹具与可观测值来自
[`margin.md`](../../word_analyse/reports/rsword-diff/margin.md)：

| 夹具 | Android print 目标 |
| --- | --- |
| `margin-top1440` / `margin-bottom1440` | 第一页 30 行，top 版本首尾源边界为报告所列 `cpLim=1020` |
| `margin-left1440` / `margin-right1440` | 版心宽 9746 twips |
| `margin-left-gutter` | 版心宽 9026 twips |
| `page-10000-long` | 第一页 22 行 |
| `page-swapped-long` | 第一页 21 行 |
| `sect-pagesize` | 两节正文宽依次 10466、8560 twips |

新增 `section_geometry` 集成测试与“段落因 pageBreakBefore/keepLines 移页后重新环绕”
回归测试；原 `section_breaks`、`fine_pagination`、`exact_pagination` 应保持通过。
同宽无环绕场景在重组前后逐项比较轨迹，保证重构本身不改变已有算法。

## 4. 文档网格与行间推进、页面占高

**状态：页数证据审计、推进/占高分离、docGrid 与 snapToGrid 输入已接入，网格算法待实现。**
21 份规范网格输入现已取得 Mac Word 原始 PDF 与双 CP 扫描，见
[网格实测与候选反例](DOCGRID-WORD-EVIDENCE-2026-09-27.md)。步长与首原点仍分别检验，
不能用单页标签间距补造页面所需占高。
新增的 16 份页面容纳输入与 6 份 pitch 阈值输入也已采集并独立复核。TNR12、
auto240、pitch360 的 12 段文本，单页容纳变化收窄至配置正文高度 `(4277,4278]`
twips；固定引擎 `--vertical-grid none` 在六个网格高度上仍错误地把末段留在第一页。
无网格四份对照页归属一致，但原点尚未对齐。PDF 页高取整不足以保留这组分页差异，
见 [页面容纳边界与引擎比较](DOCGRID-PAGE-FIT-2026-09-27.md)。阈值批次则观察到
pitch275 到 276 从约双步长变为约单步长，276 的完整原点向量与本批无网格对照相同。
这些结果还没有识别通用基线或 required 公式。

原生代码已恢复旧分支和 FeatureGate 备用路径中的整数网格算术，包括单位换算、
向上取 pitch 倍数、差量拆分及额外尾部分量；纯模型与边界指令检查已冻结。
调用者会把多个输出分量重新组合，最终占高不能简单等同于取整后的步长。
仍需映射实际字体分量 H、运行时尺度 S、分支选择和后处理到字形原点/required，见
[原生算法证据](DOCGRID-ALGORITHM-EVIDENCE-2026-09-27.md)。这一片没有启用网格生产算法。
已新增 `tools/measure/docgrid_native.py`，可用显式整数输入重放冻结的 helper 模型，
输出每步结果、诊断与输入/代码哈希；合成样例和 32 项专项测试随代码入库。
该工具的 `ARITHMETIC_REFERENCE` 状态只验证算术，不代表真实 Word 输入或分页对齐。

上游已把默认字体提供者接到 LS 节点聚合，H 是聚合分量与条件 content-info 的组合，
不能直接取现有 `RealMetrics` 的自然高度，见
[字体分量与提供者](DOCGRID-FONT-COMPONENTS-2026-09-27.md)。下游已确认普通保留行
实际推进为独立 `max(U)+max(V)`；两项容纳上限来自区间记录，后续可以分离，见
[PTS 高度与区间输入](DOCGRID-PTS-HEIGHTS-2026-09-27.md)。下一步仍需闭合实际字体
请求与运行尺度、条件 content-info、宿主高度修正，以及区间矩形的页面/栏来源。
这些边界尚不足以将静态 helper 直接接入生产网格算法。
字体接口现已绑定 `IDWriteFontFace1`，新增 `tools/measure/dwrite_metrics.py` 可在
独立子进程中读取固定库、显式字体文件和字号的指标；TNR 三字号已实测，完整量具
测试 375 项通过，见[原生字体指标测量](DWRITE-METRICS-2026-09-27.md)。这些显式
请求尚未绑定 Word 文档的实际 lfHeight，不能据此省掉宿主缩放和后处理。
字号链现已定位初始高度 store 及 mode 2 的条件设计单位覆写；临时与最终 face
身份、活动模式和后构造调整仍未等同。新增 `tools/measure/font_record.py`，可复用
成功 API 测量，在显式参数下计算定义的 U 字段、数值 T 前缀和初始 M，完整量具
测试 429 项通过，见[字体记录参考模型](FONT-RECORD-REFERENCE-2026-09-27.md)。
后构造链又接到实际 F→C 复制与默认 M 覆盖。新增 `font_adjustment.py` 可用明确
检查点输入重放 mode 2 的横纵缩放、字号因子与条件补偿，见
[字体调整参考模型](FONT-ADJUSTMENT-REFERENCE-2026-09-27.md)。它保留初始/更新后 M
的区别。随后已将选定 RTARC/RTDWRITEFONT 路径的 V 输出地址绑定到此前 T 构造器；
后处理和实际 flags 仍独立保留，工具不自动选择生产文档的分支。LS 内容高度的
启用位也已追到宿主 CP 与 paragraph client getter 的比较和实际 setter；尚未得到
探针最终 gate 值，见[字体分量证据](DOCGRID-FONT-COMPONENTS-2026-09-27.md)。
续查确认普通矩形的 Story 分派选择 SimpleW：可抑制底部空间独立查询，拒绝
overhang 时会将最终推进裁到剩余量。生产接入还需表示该条件修改，不能只保存
两个固定的固有高度，见 [Simple 容纳与末行裁减](DOCGRID-SIMPLE-FIT-2026-09-27.md)。
新增 `simple_fit.py` 可用显式 U/V/S/limit 重放该局部规则；overhang 未知时保留
待决，显式拒绝时输出裁减后的 V 与推进，见
[Simple 可执行参考](SIMPLE-FIT-REFERENCE-2026-09-27.md)。生产接入仍须让预检和
提交消费同一份已决推进，并在容量变化时重新求决。

`tools/measure/android_pages.py` 保存全部 PGIDX 与上下文，九份旧日志严格模式全部不可判；
条件比较六份相同、三份因 80 条采样耗尽不可判。三组容量继续保留为报告级约束，
不能宣称原日志已认证最终页数。见 [页数证据进展](ANDROID-PAGE-EVIDENCE-2026-09-27.md)。

修改节投影读取 `docGrid`，段落投影读取 `snapToGrid`。
输入已由 `LayoutSection.grid` / `Para.snap_to_grid` 保留，轨迹明确记录尚未应用。
见 [输入接入与证据](DOCGRID-INPUTS-2026-09-27.md)。
修改 `PendingLine` / `line_height_fine` 与编排层，把“下一行基线/游标如何推进”与
“该行放到本页所需的上下边界”分开；使用现有 fine 单位，所有保留约束读同一套量。
`VerticalGrid::MacWordThreeHundredthsInch` 是度量量化策略，与 DOCX 的 docGrid 是两个输入。

先只实现已测的 `docGrid type="lines"`，auto 行距；exact 对照必须保留。
`298` 和 `LL in [319,350]` 是当前 Calibri、字号及页面下的拟合约束，不能写成通用常量。
初末行边界量如何由字体和段落属性产生，需由改变字号/字体/段落行数的判别用例确认。

页数接入已在本仓库 `tools/measure` 新增独立比较路径，保持外部原始材料只读：
保存全部连续 PGIDX 读数段、目标夹具哈希、最终布局通道与宽度；没有足够通道/身份信息时
报不可判。不能仅凭“日志中曾出现纸页宽度”给最终计数标为 print。外部
`tools/read_pgcount.py` 的无 PGIDX 三项返回值和调用处四项解包问题，也不能原样复用。

验收：`dg-decide-139-n36/n37/n38`、`188-n40/n41/n42`、`220-n34/n35/n36`，
三组页数均为 `[1,1,2]`。另设 auto/exact、snap 开关、首末行、字号变化、
`keepLines` / `keepNext` 的边界测试。对未采集字号只验证内部约束，不报 Word 对齐。
报告依据见 [docgrid-decided.md](../../word_analyse/reports/diff/docgrid-decided.md)。

## 5. 孤行与段落保留约束

**状态：有效 widowControl、keepNext 链和硬分页优先已实现。**
实现与证据范围见 [段落保留进展](PARAGRAPH-FLOW-2026-09-27.md)。
原 DOCX 配本机 Calibri 复现报告 CP，但缺少原始 widow/keep 采集日志；
复杂链和环绕组合按合成回归标注，不能宣称所有 Word 保留约束已对齐。

在 `bridge.rs` / `Para` 接入有效 `widowControl`，在编排层基于候选行和续排快照决定断页。
保留现有 `keepLines` / `keepNext`，补充链式 keepNext、段前后距、硬分页以及段落自身
超过一页时的退化路径；确保每次仍前进，不能在空页上反复搬移整段。

验收来自 [widow.md](../../word_analyse/reports/rsword-diff/widow.md)：
`widow-split.docx` 下一页从 CP 1933 开始，`widow-on.docx` 从 CP 1834 开始。
未写 widowControl 的该最小夹具表现，不推成所有文档的默认值；先由第 2 片解释
docDefaults 与直接属性。`keep-next.docx` 和 `keep-lines.docx` 下一页仍从 CP 1054 开始。
新增 `paragraph_keep` 回归，保留 `fine_pagination` 的“恰好装下/少一 twip”测试。

## 6. 同页多栏，之后再扩到表格与脚注

**状态：顺序多栏流、独立栏断、相同页面几何的连续节栏平衡与同页栏组已实现。**
支持等宽和显式不等宽栏，接在原有区域与源游标续排接口上，
见 [多栏进展](COLUMN-FLOW-2026-09-27.md)。表格和脚注仍未实现。

节投影读取 `w:cols`，编排层将每页正文拆成栏区域，按读序将同一个段落断行器续排到
下一栏；栏满才换页。`w:br type="column"` 增加独立控制动作，不能作为普通软回车。
等宽与不等宽栏使用同一源续排器；连续节末页通过严格试排平衡，普通文档末页仍顺序填充。
规范 Mac 7 输入已经覆盖普通/连续/同栏数/硬栏断及 noColumnBalance 两值；
Windows 与 Android 的连续分节行为尚未独立实测。跨多栏组 keepNext、连续节页面几何切换、
RTL 顺序与分隔线仍保留明确限制。
轨迹应携带栏号/栏框，保持页内行身份唯一，不能把第二栏的行记录与第一栏合并。

`twocol64.docx` 的已有 Android print 目标为 1 页、2 栏各 32 行、每栏宽 4873 twips，
见 [columns.md](../../word_analyse/reports/rsword-diff/columns.md)。
`twocol65` / `twocol70` 可作增长边界输入，只有核过原始读数后才列 Word 预期。
新增 `column_flow`，验证跨栏源区间连续、栏断、下一页续排和不同节宽重新断行。

表格和脚注接在栏模型稳定之后，分别做有正文的最小垂直切片：
`table32.docx` 当前无可排段落，`footnote32.docx` 当前不预留脚注高度，两者目标都是已有
print 报告的 2 页。它们还要求正文块与多 story 源坐标模型，不能只在页高里扣一个拟合值。

## 每片使用同一组验收入口

下列命令现在已存在。新切片须先写自己的边界回归，再跑对应护栏；不要求每次重跑
所有字体探针或重新采集 Word。输出目录必须使用本轮新的路径。

```sh
cargo test --offline -p rsword-layout-core --features fontenv
cargo test --offline -p rsword-layout-svg
cargo check --offline --workspace
cargo build --offline --features fontenv --bin layout-trace
tools/measure/.venv/bin/python -m pytest -q tools/measure/tests

tools/measure/.venv/bin/python tools/measure/android_replay.py \
  --analysis-root ../word_analyse \
  --font /tmp/wordfonts/calibri.ttf \
  --fallback-font '/tmp/wordfonts/NotoSansCJK-Regular.ttc#2' \
  --assume-legacy-narrow \
  --output artifacts/android-replay-shared-core-step-N

tools/measure/.venv/bin/python tools/measure/sweep.py \
  --trace-bin target/debug/layout-trace \
  --font-dir /System/Library/Fonts \
  --font-dir fixtures/fonts \
  --output artifacts/mac-replay-shared-core-step-N
```

`/tmp/wordfonts` 是本机已有字体路径，不是仓库资产。缺字体不能用桩度量冒充验收。
Mac 既有误差与不可判项按回放报告保留，切片的条件是相对于改动前基线没有新增退化，
不是把已有包都宣称为 PASS。打印页数审计命令见
[量具说明](../tools/measure/README.md#android-页容器计数审计)，不运行 Word 或引擎。

页面几何、跨页续排、有效属性、首个文档兼容项与段落保留已完成首轮接入。
打印页数证据路径、行间推进/占高分离与顺序多栏流已经落地；
docGrid 与 snapToGrid 的真实输入已保留；连续节栏平衡与同页栏组已接入。
下一步推进网格度量和仍有诊断的分节/保留规则。
不要把错误输入吸收到行高公式里。

## 第一片验收记录

- 修改前，真实 `vanish.docx` 的移动视图断言失败：引擎起点 `[0,43,86,129]`，
  Word 记录 `[0,73,116]`。新加的10项普通回归中9项失败，可见性对照1项通过。
- 修改后，直接加载原始 DOCX：移动视图 `[0,73,116]`、打印视图 `[0,116]`；
  两者都是101个字形（100个可见数字与段落标记），隐藏源区间 `[20,50)` 没有字形。
- `RSWORD_TEST_CALIBRI=/tmp/wordfonts/calibri.ttf cargo test --offline
  -p rsword-layout-core --features fontenv --test vanish -- --include-ignored`：
  **14 passed**。包含隐藏间隙两侧的标点上下文、回退与恰好填满行的对照。
- 核心与 SVG 的完整测试通过；workspace `cargo check`、core/SVG Clippy
  `-D warnings` 通过。量具 Python 测试192项通过。
- Android 既有11份窄路径回放保持 **186/186**，仍为带历史假设的边界回测。
- Mac 修改前后25份有效轨迹逐字节一致，覆盖356页、3834行、18150个字形。
  自检结果一致，比较结果仅输出路径不同；既有25包失败、5包VOID不可判保持原状，
  没有把原有误差计为通过。明细在 `mac-regression.txt`。
- 输入和结果留在 `artifacts/shared-core-2026-09-27/`；该目录不提交。
