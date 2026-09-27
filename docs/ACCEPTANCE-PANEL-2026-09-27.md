# R01 验收用例登记与首个面板（2026-09-27）

本片执行 [北极星规划](NORTH-STAR-AND-ROADMAP.md) 的 R01：登记现有验收用例，
建立支持/证据矩阵，把面板绑定到当前基线，并给每项失败归因。没有运行 Word、原生
注入或新的采集；面板只读取已有回放产物，以及引擎对已登记文档的离线运行结果。

## 交付物

| 文件 | 作用 |
| --- | --- |
| `tools/measure/acceptance/cases-v1.json` | 版本化用例清单 v1，共 60 例；代码基线 `2a192d3` |
| `tools/measure/acceptance.py` | `register` 生成清单，`run-reports` 离线运行报告级用例，`panel` 生成面板 |
| `tools/measure/tests/test_acceptance.py` | 7 项：严格判据、绑定、分层、未排内容、条件/报告不入分子、退化检测、清单覆盖 |

每例保存 case ID、分组（同构变体归同组）、功能族、配置（平台、视图、Word 版本、
字体环境哈希或所需族、坐标域）、文档路径与 SHA-256、参考类型与哈希、比较器及证据状态。
清单中的 `baseline` 记录历史状态，面板逐项核对能否复现。

证据状态分为五类，按清单登记，不在出结果后改动：

| 状态 | 含义 | 本版数量 |
| --- | --- | --- |
| strict | 原始 Word 参考 + 严格比较器（Mac PDF 字形原点，0pt 容差） | 25 |
| conditional | 原始参考，但需历史假设（Android 窄路径 CP 空间与视图） | 11 |
| report-only | 只有报告级约束，没有严格的页/行日志 | 19 |
| reference-invalid | 参考存在但已作废（VOID） | 5 |
| binding-missing | 参考绑不到当前文档 | 0 |

report-only 共 19 例，都来自 word_analyse 的 Android print 约束：
- 表格 4 例：table32、table32-tail、table32-noborder、table32-rowh；
- 脚注 1 例：footnote32；双栏 1 例：twocol64；
- 保留约束 4 例：widow-split、widow-on、keep-next、keep-lines；
- docGrid 判别夹具 9 例：dg-decide 三档 × 三份。
其中 dg-decide 的原始 PGIDX 日志在严格模式下不可判，见 [页数证据](ANDROID-PAGE-EVIDENCE-2026-09-27.md)。

严格通过必须同时满足五项：严格参考、有效绑定、完整支持、比较 OK、自检 OK。
条件结果和报告约束另行计数，永远不进入严格分子。

**支持状态由面板自行判定，不只看轨迹。** 面板会直接读 DOCX 包，清点引擎目前不排的
内容：脚注/尾注正文、页眉页脚引用、drawing/pict、文本框。有任何一项就记为 partial。
这样做是因为首轮面板发现 footnote32 的脚注被引擎**静默丢弃**，轨迹里既没有跳过计数，
也没有诊断。

## 当前面板

命令（输入均为本轮新目录，冻结的旧包没有改写）：

```sh
cd tools/measure
.venv/bin/python acceptance.py run-reports --cases acceptance/cases-v1.json \
  --font /tmp/wordfonts/calibri.ttf --output ../../artifacts/acceptance-reports-2026-09-27
.venv/bin/python acceptance.py panel --cases acceptance/cases-v1.json \
  --mac-sweep ../../artifacts/table-flow-mac-replay-2026-09-27/after \
  --android-replay ../../artifacts/android-replay-table-flow-2026-09-27 \
  --report-run ../../artifacts/acceptance-reports-2026-09-27 \
  --output ../../artifacts/acceptance-panel-2026-09-27
```

回放输入对应的引擎二进制 SHA-256 为
`3c12168d3d922a3d448896639e15585f4d98b8de5824e38537c733646651474c`（`1a3c6ba` 的 core；
`2a192d3` 只改了文档）。

**北极星指标：严格整文档通过 0 / 60。**

| 功能族 | 用例 | PASS | FAIL | UNDECIDABLE |
| --- | --- | --- | --- | --- |
| 正文断行 line-breaking | 19 | 0 | 6 | 13 |
| 纵向/分页 vertical-pagination | 35 | 0 | 17 | 18 |
| 分节多栏 sections-columns | 2 | 0 | 1 | 1 |
| 表格 tables | 4 | 0 | 0 | 4 |
| 脚注 footnotes | 1 | 0 | 0 | 1 |
| 页眉页脚/编号、对象 | 0 | — | — | — |

| 配置 | 用例 | FAIL | UNDECIDABLE |
| --- | --- | --- | --- |
| mac-word-16.112.3-print | 30 | 23 | 7 |
| android-word-mobile-5329-legacy | 11 | 0 | 11（条件 OK 11，共 186/186 行） |
| android-word-print-report | 19 | 0 | 19（报告 MATCH 12、MISMATCH 5、NO_LAYOUT 2） |

护栏：

- 能力：full 42、partial 11、unsupported 2（table32 / table32-noborder 的行高不是 exact，
  没有可排内容）、not-evaluated 5（参考作废，没有运行）。
- 基线复现：41/41。Mac 30 包的状态和原因与 `autospace-consistency-mac-replay` 相同；
  Android 11 例的条件状态和匹配行数与 `android-replay-2026-09-27` 相同。
- 稳定性：这是首个面板，没有先前面板，新增退化 0。之后用 `--previous` 传入上一份
  `panel.json`；只要有已通过用例退化，命令就以 1 退出。

页眉页脚/编号和对象两族的分母是 0，这是真实的空缺，说明需要补登记这类参考，
不代表这两族已经通过。

## 失败归因

旧 sweep 给 25 个可比 Mac 包的顶层原因都是 `TRACE_SELFCHECK_UNDECIDABLE`，掩盖了
比较器的真实结论。面板把两层分开报告：

**几何层（22 FAIL，结构完好，页数与字形数一致）**

- 纵向：25 个可比包中 25 个存在 dy 误差，最大 50.88pt（cjk-plain）。
  其中 21 个包的行首 dy 等于全体最大 dy，说明误差出在行基线的位置上，
  不是行内累积。这与 R04/R06 的 exact、自然高度和首原点问题对应。
  例外 4 个：kinsoku / kinsoku2（配对不可判）、page-start（行结构不符）的行首 dy
  都是 0；vertical-precision 行首 17.04pt、全体 17.53pt，还有行内垂直对齐分量。
- 横向：24 个包存在 dx 误差，其中 22 个的行首 dx 为 0，误差来自行内推进；
  hbox 行首 0.11pt，indent-align 行首 0.51pt。
  大量级的例外有：breaks-sections 和 vmisc2 为 141.05pt，hbox 为 28.67pt，
  hbox2 为 40.99pt，cjk-plain / kinsoku 为 7.33pt。indent-align 的行首误差
  指向缩进或对齐的起点。
  141pt 这一项在历史报告中没有单列，建议作为下一次横向诊断的起点。

**结构层**

- page-start：18 页行数不符（line-structure），与 [结构差异审计](STRUCTURAL-REPLAY-AUDIT-2026-09-27.md)
  中三组字体下 35 段分页提前的结论一致。
- kinsoku / kinsoku2：引擎 252 个字形，参考 63 个，3 页配对不可判
  （glyph-count + pairing-undecidable），维持 UNDECIDABLE，不计为失败或通过。

**报告约束层（只作为附加约束）**

| 用例 | 引擎 | Word 报告 | 归因 |
| --- | --- | --- | --- |
| dg-decide-139-n38、188-n42、220-n36 | 1 页 | 2 页 | docGrid 未应用（轨迹有诊断）；三组 n、n−1 为 MATCH，但无网格引擎本就排成 1 页，不能当作网格已对 |
| footnote32 | 1 页 | 2 页 | 脚注被静默丢弃，面板现记为 partial（`unlaid footnotes 1`） |
| table32-rowh | 1 页 | 2 页 | Word 会补表后段落，引擎不模拟，有诊断 |
| table32、table32-noborder | 无布局 | 2 页 | 非 exact 行高，表格被拒 |

table32-tail、twocol64、widow-split、widow-on、keep-next、keep-lines 与报告一致。
这些都是报告级约束，没有逐行归属，不计入严格通过。

## 需要决策的证据缺口

**自检“终止符字形数”阻塞全部严格判定。** 25 个可比包都因这一项得到 UNDECIDABLE：
这些文档没有一行只由终止符构成，检查无物可查。即使几何误差清零，这 25 例也不能
严格通过。可选处理：

1. 保持现状：这一项继续阻塞严格通过。
2. 事先按文档内容把它标为“不适用”：参考侧也没有终止符独占行时，才可以豁免。
   这需要比较器的行结构层先确认两侧一致，并且要升版清单与比较器规则，不能只对候选结果放宽。

建议取 2，但它改变的是验收规则，需要单独确认后另片实施。当前面板仍按 1 计算。
因为这 22 例同时有几何 FAIL，取 2 也不会改变今天的严格计数。

## 由面板得出的下一步

1. 引擎诊断（**已完成**）：用户脚注/尾注（不含分隔符）和节的页眉页脚引用现在进入
   `layoutInput.diagnostics`，明确报告为未排；引用标记仍保留其源单位。
   drawing 已经走锚定/占位路径，本片没有另加诊断。
   回归见 `crates/core/tests/story_diagnostics.rs`，共 3 项，其中真实 footnote32 一项默认 ignored，已运行。
   护栏结果：
   - workspace fontenv 测试 743 passed / 0 failed / 14 ignored；Clippy `-D warnings` 通过；
   - Mac 25 份 trace 与基线逐字节相同（二进制
     `d0593c5c82e63dd9570d85bef2e47074ebde80799b6369bf781a2d05fec90239`）；
   - Android 186/186。
   以 `--previous` 重建的面板（`artifacts/acceptance-panel-story-diagnostics-2026-09-27`）
   没有新增退化，严格计数不变；footnote32 的引擎诊断与包清点现在一致。
2. R04：纵向误差主要是基线位置，继续 exact 候选合同的单一缺口。
3. 横向 141pt：先在 breaks-sections / vmisc2 的最差字形上做诊断，确认它来自制表、
   分节还是配对。
4. 补登记页眉页脚/编号与对象两族的参考，否则这两族的分母一直为 0。

清单升版时保留 v1 与旧分母的比较结果；不能通过移除失败用例来提高分数。
