# word_analyse 评估与接入（2026-09-27）

后续更新：本文保留接入时点的基线。随后已完成直接声明 `w:vanish` 的共用算法实现，
并复现本文记录的 Word 起点；开发路线与验收见
[共享 PTS/LS 开发记录](SHARED-PTS-LS-DEVELOPMENT-2026-09-27.md)。

结论：`../word_analyse` 能继续推进本引擎开发，最有价值的是带原始读数的最小
DOCX、断点表和判别实验。反汇编笔记适合寻找实验入口；其中的语义解释、单位换算
和汇总状态不能直接当成排版规则。

本轮只读该项目，在 rsWordLayout 中新增 Android 离线回放工具和比较器，保留此前
未提交的 Rust 实现。没有启动 Word、连接手机、修改采集资料或引入新的拟合常量。

## 已有实现与新材料

9 月 25 日的引擎迭代已经使用这个项目的资料，实现了紧急断行、制表符、平台/视图
区分、禁则回退、大小写、字距、CJK 回退与软回车。详见
[迭代记录](ENGINE-ITERATION-2026-09-25.md)。旧报告中“rsword 没有实现”的描述需要
与当前代码核对，不能重复立项。

| 材料 | 可用范围 | 开发价值 |
| --- | --- | --- |
| `reports/diff/*.word.narrow.jsonl` | 11 份窄路径采集，186 行；夹具 SHA 可核验 | 持续回放断行边界 |
| `reports/fixture-matrix.json`、`fixtures/` | 459 份夹具、11 类；类别统计不代表已采集 | 为表格、分栏、脚注等补测提供输入 |
| `reports/rsword-diff/vanish.md` | Android 隐藏文字与两个对照的行起点 | 下一个有明确判据的功能缺口 |
| `reports/diff/docgrid-decided.md` 与 `reports/diff/print/dg-decide-*.print.live.log` | 三档 pitch 的相邻段数与最终页数 | 给 docGrid 建立分页边界测试 |
| `findings/function-cards/`、`findings/verification-queue.md` | 静态入口、候选字段及未覆盖项 | 指导后续动态采集，不代替输出验收 |

## 本轮接入

新增入口 [android_replay.py](../tools/measure/android_replay.py) 与比较器
[android.py](../tools/measure/wordmeasure/android.py)。运行器只读外部资料，用当前
`layout-trace` 重新排版，不把存档中的旧引擎轨迹当成当前结果。

```sh
cargo build --offline --features fontenv --bin layout-trace

tools/measure/.venv/bin/python tools/measure/android_replay.py \
  --analysis-root ../word_analyse \
  --font /tmp/wordfonts/calibri.ttf \
  --fallback-font '/tmp/wordfonts/NotoSansCJK-Regular.ttc#2' \
  --assume-legacy-narrow \
  --output artifacts/android-replay-2026-09-27
```

字体路径是本机已有文件，不随仓库分发。重新运行需换一个不存在的输出目录。
`--fixture kinsoku` 可筛选单份；不传则扫描所有窄路径 JSONL。

- 显式固定 Android、mobile、5329 twips、真实字体、无纵向量化；要求装入 Calibri。
- 记录夹具、采集、引擎二进制、字体文件的 SHA-256、命令和标准输出/错误。
- 夹具哈希不符不启动引擎；执行失败、超时、缺文件、缺测都保留为 `UNDECIDABLE`。
- 按采集顺序比较完整 `(sourceStart, sourceEnd)`；缺行、多行和乱序都参与判定。
  不按行起点重排，也不截取双方共有前缀。分母逐份取两侧行数较大值。
- 每份保存 `engine.json`、`result.json`，汇总为 `summary.json` / `summary.md`。
  退出码 0 表示所选边界均匹配，1 表示有失配，2 表示存在不可判且无失配。

当前 11 份采集都声明 `cpUnit=utf16`，但 `cpSpace=unknown`、`mode=unknown`。
默认严格模式因此返回不可判。`--assume-legacy-narrow` 明确接受历史窄路径的
“文档 CP / 移动视图”假设，报告会标记 `conditional` 和所用假设。该选项不能覆盖
明确不同的 CP 单位、story 坐标、打印视图或不符的夹具哈希。

这个回放是边界回测，不是新的独立 Word 实验。当前字体哈希也不能证明采集时使用了
同一字节版本的字体；Noto 回退是显式选择，未证明等于 Word 每个字符的实际选字。

本轮验证：

- 当前二进制离线构建成功；上述命令生成的 11 份新轨迹为 **186/186 行区间匹配**，
  全部带条件假设。报告在 `artifacts/android-replay-2026-09-27/summary.md`，该目录不提交。
- 严格元数据检查对这 11 份旧采集均返回 `UNDECIDABLE`。
- `tools/measure/.venv/bin/python -m pytest -q tools/measure/tests`：**192 passed**，
  其中新增比较器21项、运行器8项。
- `git diff --check` 通过。Rust 排版算法未改动。

## 旧评分的边界

本轮复跑 `word_analyse/tools/agreement.py`，得到其报告的 ALL / PARITY
均为 D4=186/186、D5=186/186。但不能据此宣称 run 切分、字形位置或分页都对齐：

1. `agreement.py` 在两侧 CP 区间相等后，用同一份 DOCX `cp_kinds` 重建两侧 run。
   因而 D5 没有独立的引擎 run 观测，实质重复 D4。新工具将 run 切分保留为不可判。
2. 原工具忽略坐标系与视图的 unknown 元数据；新工具默认不把它们补成已知值。
3. 原工具按 `cpFirst` 查表，可忽略多出的引擎行或重复起点。新工具比较完整有序序列。
4. Word 的几何单位尚未与引擎对齐；mobile 页容器不是打印页。新工具不计算这两类一致率。

## 后续实现顺序

### 1. 保留 UTF-16 坐标的 w:vanish

`reports/rsword-diff/vanish.md` 的夹具包含 20 个可见 `0`、30 个隐藏 `0`、80 个
可见 `0`。Word 纸页路径起点为 `[0,116]`，移动路径为 `[0,73,116]`；
`webHidden` / `specVanish` 的纸页对照为 `[0,86]`。

当前桥接层只在软回车处理时考虑 vanish，未从度量和绘制中移除隐藏文本。
实现应保留隐藏 run 的源长度，过滤其布局贡献，并同时处理制表符后文本宽度、
跨 run 断点和隐藏控制符。直接删除字符串会使后续源坐标错位。

本轮已用当前二进制重新验证：移动视图起点是 **`[0,43,86,129]`**，打印视图是
**`[0,86]`**，均与上述 Word 读数不符。外部报告中的旧引擎起点50已不适用。
两份诊断轨迹保存在本次回放目录的 `vanish-gap-mobile.json` / `vanish-gap-print.json`。
这是另两次缺口探测，不混入11份有完整 JSONL 的186行评分。

```sh
target/debug/layout-trace --platform android --view mobile \
  --font /tmp/wordfonts/calibri.ttf --content-width 5329 \
  ../word_analyse/fixtures/vanish.docx /tmp/vanish-mobile.json
```

打印路径改为 `--view print --content-width 10466`，预期起点 `[0,116]`。

### 2. docGrid 解析与分页

9 月 27 日原始日志支持以下 Android Word 页容量边界：

| pitch | 装得下 | 装不下 |
| --- | --- | --- |
| 139 | 37 段 / 1 页 | 38 段 / 2 页 |
| 188 | 41 段 / 1 页 | 42 段 / 2 页 |
| 220 | 35 段 / 1 页 | 36 段 / 2 页 |

这些夹具每段一行，必须使用 auto 行距；exact 版本不是同一个实验。
结果支持区分“相邻行推进量”和“首末行占高”。但 `298` 及 `LL∈[319,350]`
只是当前字体/字号/页面下的候选参数，LL 的物理意义和精确值仍未确定。
本轮不把它们写成通用常量，也不声称已实现 docGrid。

页数读取还需要加强：`tools/read_pgcount.py` 无 PGIDX 时返回三项，调用方解包四项，
会崩溃；纸页宽度只列了两个常量，且“日志某处曾出现纸页”不能证明最后一段读数
对应打印视图。页数回放应保留全部读数段，并绑定目标夹具及最终布局通道。

### 3. 先修正证据，再研究 continuous / auto / justify

- `tools/make_sectcont_ladder.py` 的 `COLUMN=15398` 与其上下边距 1440 不符：
  `16838-1440-1440=13958`。中间段未指定 exact480，也被按480计入预测。
  continuous 写在第一节属性，末节省略 type；当前桥接规则将末节视为 nextPage。
  因此这组两页读数不能证明 continuous 本身强制换页。
- `findings/rules/auto-lineheight-code.md` 从两个区间重叠推导两种高度相等，依据不足；
  静态返回值的打包字段语义也尚未验证。
- `findings/rules/tabstop-rule-table.md` 未验证31680的单位和换算，不采用其比率推测。
- `findings/rules/justify-slot-array.md` 未读出可伸缩槽和增量算法；现有 justify
  JSONL 只能验证断点，不能支撑新的两端对齐几何算法。

这些问题已记录在本仓库，外部原始材料保持不变。
