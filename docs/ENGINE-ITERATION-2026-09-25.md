# 引擎迭代 2026-09-25：对齐 Android Word 的断行缺口

起点：`fbc9108` 加用户未提交的 WIP（`font/linebreak.rs`、`font/real.rs`、`font/simple.rs` 里的
autospaceDN，`bin/layout-trace.rs` 的 `--margin` / `--content-width` / `--page-width`）。
这一轮**什么都没有提交**：改动在一个集成工作区里一步一步落地，每步留一份相对 `fbc9108` 的
累积补丁（含 WIP）作快照。解析器 rsWordParser 仍钉在 `399e36a`，没有动。

Word 一侧的依据全是 word_analyse 里已有的 Android Word 16.0.20513.20014 读数
（`reports/diff/*.word.narrow.jsonl`、`reports/rsword-diff/*.md`、`findings/*.md`，只读），
字体用手机上拉下来的 Calibri。Mac 一侧是本仓库 `captures/` 的既有采集，只当回归护栏用。
这一轮**没有启动 Word，也没有新采集**。下面所有的 PASS 都是对既有读数的回测。多数规则本来就是
照这些读数设计的，属于样本内拟合，不是独立检验。

平台与视图的默认是 `Platform::Desktop` + `View::Print`，库的 `Engine::new` 与不带选项的
`layout-trace` 都一样。只读平台 / 视图的规则有四条：缺省制表位、行末标点挂出、段末分页符、
回退字体，不带选项时都走 Mac 那一边。Android 规则要显式传 `--platform android`，移动视图要传
`--view mobile`。其余规则两个平台共用，桌面上也变了：超长词、制表符本身、caps、字距 / 缩放、
禁则表、跨 run 回退、名义 1 em。Mac 回放里看得见的只有字距 / 缩放那三包，见「回放与检查」。

落地顺序分两条线，最后合并。A 线依次是 G0 → P → G1 → G5 → G6 → R。B 线接在 P 后面，
单独一个工作区，依次是 G3 → G2 → G4。每步过 8 道闸门：构建；两组测试；svg、check 与 clippy；
Calibri 测试；记分器；夹具扫描；Mac 回放；快照。每步还有两道独立审查。
终审分代码、Mac、Word 三路，第 1 轮全部通过。记分器与闸门脚本从头到尾没有改过。

## 缺口一览

记分变化是基线到终态、Android 档（`--platform android`，窄路径加 `--view mobile`）的数。

| 缺口 | 病因 | 落地的规则 | 主要依据 | 记分变化 |
| --- | --- | --- | --- | --- |
| G0 超长词兜底 | 断不开的串一行一个码元；行里已有内容时 run 边界成了断点 | 按 cluster 紧急断行，宽度 ≤ 剩余宽度，空行至少一个；run 边界不是断点 | `tab.md`、`char-scale.md` 等 13 例；`vanish.md` | STARTS +3，EXT 0 → 13 |
| P 平台 / 视图开关 | 同一形状的文档，两个平台实测相反，文档里没有属性能区分 | `Platform` / `View` 作显式输入，默认 Desktop + Print | `findings/pagination-path.md` | 无（设计如此） |
| G1 制表符 | 不读 `w:tabs` / `w:defaultTabStop`；`'\t'` 当字形量；制表符之后可断 | 每个制表符单独落位；前可断、后不可断；缺省档 Desktop 720、Android 221（拟合） | `tab.md` 9 例 | STARTS +9 |
| G5 禁则回退 | Android 上照 Mac 挂出；没有行尾禁则；西文进 CJK 不查禁则 | 挂出只在 Desktop；行首 / 行尾禁则三档表；共用 `kinsoku_ok` | `kinsoku.md`、`brkcls-map.csv` | ALL +2，STARTS +4 |
| G6 br-page 段标记成行 | 段末分页符与段落标记并成一行 | 移动视图拆成两行 | `br-page.word.narrow.jsonl` | ALL +2 |
| R 跨 run 回退 | 一截放不下时退不回前一个 run 里的断点，只能在 run 边界收行 | 段落拍平成片段序列，游标可回退，`Engine::shortfall` 分四种情形 | 推断（`vanish.md` + `tab.md`） | 无（记分夹具都是单 run） |
| G3 w:spacing / w:w | 桥接层写死 0 / 100；两个兄弟 rPr 解析器取后一个；绘制不加字距 | 读 spacing / scale；`load.rs` 先到先得合并兄弟 rPr；度量与绘制按 cluster 同一口径 | latinspace jsonl、`letter-spacing.md`、`char-scale.md` | ALL +24，EXT +4 |
| G2 w:caps / w:smallCaps | 桥接层丢了这两个属性 | 1:1、只 BMP 的大写映射；小型大写 floor(0.8 × 半点) | `caps.md`、caps-on jsonl | ALL +4，STARTS +4 |
| G4 CJK 回退字体 | eastAsia=SimSun 手机上没有，字符零宽跳过，整段排成一行 | 有序回退链（android 默认 Droid）；谁都盖不住的 CJK 画名义 1 em 的 `.notdef` | han22 / mix-cjk jsonl、`han-size.md` | STARTS +4（其中 nofb-kinsoku 要和 G5 一起） |
| 软回车（2026-09-26 追加，缺口表之外） | 解析器把软回车发成 `'\n'` 加 br / cr 段，桥接层只认 U+FFFC；占位符又按非文本段的个数对位 | 按段的字节区间把软回车段的 `'\n'` 换成 U+FFFC（`LineBreak`）；每个 U+FFFC 取包含它的段的种类 | `br-soft.md` 三份；breakme 576 的字面 LF 作反例 | STARTS +3 |

## 记分板

记分器每次重新生成 63 份轨迹再打分，读数的来历见每行的「定义」一栏。

| 指标 | 定义 | 基线 | 终态 |
| --- | --- | ---: | ---: |
| ALL D4 | 11 份 Android 窄路径全量行记录共 186 行，按 cpFirst 配上、cpLim 也相等才算 | 154/186 = 0.828 | **186/186 = 1.000** |
| ALL D5 | 在 D4 之上再要求行内 run 切分相同 | 154/186 = 0.828 | **186/186 = 1.000** |
| PARITY D4 | ALL 去掉 `MISSING_RULE` 里的夹具（caps-on、latinspace 等），剩 158 行 | 154/158 = 0.975 | **158/158 = 1.000** |
| STARTS | `reports/rsword-diff/*.md` 引到的 34 组 Word 行起点，前缀相同算过 | 6/34 | **33/34 = 0.971** |
| EXT | 18 组补充的单词用例（G0x、G3x），不计入 STARTS | 0/18 | **17/18** |

逐步的路径（ALL 的 D4 与 D5 每一步都相等）：

| 步 | 内容 | ALL | PARITY | STARTS | EXT |
| --- | --- | ---: | ---: | ---: | ---: |
| 基线 | `fbc9108` + WIP | 154 | 154 | 6 | 0 |
| G0 | 超长词兜底 | 154 | 154 | 9 | 13 |
| P | 平台 / 视图开关 | 154 | 154 | 9 | 13 |
| G1 | 制表符 | 154 | 154 | 18 | 13 |
| G5 | 禁则回退 | 156 | 156 | 22 | 13 |
| G6 | br-page | 158 | 158 | 22 | 13 |
| R | 跨 run 回退 | 158 | 158 | 22 | 13 |
| G3（B 线，接 P） | w:spacing / w:w | 178 | 154 | 9 | 17 |
| G2（B 线） | caps / smallCaps | 182 | 154 | 13 | 17 |
| G4（B 线） | CJK 回退 | 182 | 154 | 16 | 17 |
| 合并 | A 线 + B 线 | 186 | 158 | 30 | 17 |
| 软回车 | 2026-09-26 追加，即终态 | **186** | **158** | **33** | **17** |

读这张表要注意四件事。另外，word_analyse 自己的 `tools/agreement.py` 在决定 D3 之后（caps-on、
latinspace 移出 `RSWORD_MISSING_RULE`，11 份轨迹按 Android 档重新生成，旧轨迹留在
`reports/archive/rsword-diff-2026-09-26-pre-gapfix/`）读 ALL 186/186、PARITY 186/186。

- **需求里贴过来的 69.8% / 94.9% 是旧数。** 同一段需求里的 br-page「10/16」、「6 of 74」也一样。
  同一个记分器在这一轮的起点（HEAD + WIP）上就是 **82.8% / 97.5%**。br-page 17 行里错 2 行，
  不是 6 行。本轮的增量要从 82.8% / 97.5% 算起。
- **记分用的是 Android 档。** 记分器传 `--platform android`，窄路径、没改页宽的用例再加
  `--view mobile`。同一个终态二进制不带选项（桌面默认）打分是 ALL 182/186、PARITY 154/158、
  STARTS 25/34、EXT 17/18，差的正是读平台 / 视图的那几行：
  - br-page 15/17、kinsoku 0/2；
  - 缺省档 720 下的 tab-zeros、tab-i、tab-after-a、tab-paper ×2；
  - 挂出的 k-kinsoku、k-period、nofb-kinsoku。

  用自定义制表位的 4 组在桌面档下照样过。
- **PARITY 不计 latinspace 与 caps-on。** 记分器的 `MISSING_RULE` 照 word_analyse 的
  `agreement.py` 仍豁免这两份，所以 G3、G2 的 28 行只进 ALL。把它们算进去，PARITY 是 186/186。
  要不要改豁免表是决定 D3，记分器是只读的。latinscale 在 w:w=80 的反常查清之前应当继续豁免。
- **样本内。** G1 的 9 组制表符用例里，只有 4 组用自定义制表位，与拟合出来的 221 无关，
  算独立确认：tab-stop-720、tab-stop-1440、tab-right-1440、tab-right-fit。另外 5 组是 221 的
  样本内拟合：tab-zeros、tab-i、tab-after-a、tab-paper 两个宽度。G0 的 13 个 EXT 用例、
  G2 的 floor(0.8)、G3 的先到先得，也都是照被记分的那几份读数定下来的。

## 各缺口：病因、规则、依据

凡是规则，代码注释里都写明了是实测（引夹具 / 报告）、推断还是假设。本节只摘要点，
假设逐条列在后面「假设清单」里，编号 A0.x … A4.x。

### G0 超长词兜底

**病因。** `FontMetrics::fit` 只在断点处切。字母、数字串里除了串尾没有断点，串尾整个放不下，
`fit` 就返回 None。`break_paragraph` 的兜底分支于是在空行上硬塞一个字符就收行：断不开的串
一行一个码元，直到剩下的尾巴能整段放下。`x`×200 在 5329 上排成 149 行单字加一行 51 个，
caps-on 窄路径基线排成 75 行。行里已有内容时，同一分支什么都不塞就收行，run 边界于是成了
隐含的断点：`webhidden`、`specvanish`（20 + 30 + 80 个 `0` 分在三个 run 里）Word 是 0、86，
rsword 是 0、50。

**规则。** 一行从行首起没有放得下的断点时，紧急断行。行首那个断点不算，与下一片段的交界算。
紧急断行切在最后一个整 cluster 之后，累计宽度要 ≤ 剩余宽度。这由新的、带默认实现的 trait 方法
`FontMetrics::fit_clusters` 完成：`RealMetrics` 只在整形器的 cluster 起点切，`SimpleMetrics`
用 UAX #29 的近似表（`cluster_boundaries`）。

- 空行至少收一个 cluster。
- run 边界不是断点，断不断由两侧的字符定。
- 行首之后有更早的断点，仍断在那里，长串整个挪到下一行。
- 两端对齐的空隙只加在词间：交界是断点，或右片以空格 / 制表符开头。
- 对象占位符两侧的断点改由 `LinePiece.after_object` 显式标记，不再从源区间的缝推断，
  以后的 w:vanish 不会平白造出断点。

**依据（实测，Android Word，Calibri 12pt）。**

- 一串 `0` 窄路径每行 43 个，纸页 86 个（`tab.md`）；caps-plain 纸页 91 个（`caps.md`）。
- caps-on 窄路径 (0,38),(38,76),(76,114),(114,121)，用 `A` 的宽度（jsonl）。
- m-plain 25 @5329；纸页上 n-plain 83、v-plain 76、sz-48 43、sz-36 57、sz-25 82、
  szcs / bold-zero / italic-zero 86、italic-A 75。
- 零容差：第 44 个 `0` 只超 23 twips，第 26 个 `M` 只超 6 twips，都不收（`char-scale.md`）。
- run 边界不是断点：webhidden、specvanish 都是 0、86（`vanish.md`）。
- 「更早的断点胜」只在制表符上量过：tab-right-1440（0、1、45）与 tab-right-fit（0、4、48）。
  tab-after-a 分不出，不作依据。

**记分。** STARTS 6 → 9（zero-plain、zero-paper@paper、caps-plain@paper），EXT 0 → 13，
13 个 G0x 全过。i-plain 仍 FAIL，见「仍然失败的」。
一段 20000 字符的无断点串 @5329，从约 285 s 降到 5.3 s（G0 当时测的）。每行仍是 O(n)，因为 `measure(rest)`
与 `fit(rest)` 仍要量整段余下的文字。

### P 平台 / 视图开关

**病因。** 最小 docx（没有 settings、styles，也没有 `w:lang`）在 Mac 与 Android 上有几条规则
实测相反：缺省制表位、行末挂出、段末分页符，以及回退字体装不装。文档里没有属性能区分这两个平台，
只能由调用方说明在模拟谁。

**规则。**

- 类型：`pub enum Platform { Desktop, Android }`、`pub enum View { Print, Mobile }`。
- 接口：`Engine::with_platform(platform, view)` 两个一起设，`platform()` / `view()` 读。
- 默认：库与 CLI 都是 Desktop + Print（决定 D2）。`tools/measure/sweep.py` 与 Mac 文档的命令没改。
- CLI：`layout-trace --platform mac|android --view print|mobile`。拼错的值在解析参数时就报错，
  两个取值记进轨迹的 `metrics` 栏。
- Desktop 的实测基础只有 Word for Mac 16.112，Windows 沿用同一套规则是假设（AP.1）。
- 窄路径（w3=5329）是移动视图，纸页路径（w3=10466）是打印视图。这是引证，不是本轮测的：
  `findings/pagination-path.md`、`findings/page-frame-selector.md`。

**记分。** 这一步不改任何排版：63 份记分轨迹除 `metrics` 栏外逐字节相同，夹具扫描 0/430。

### G1 制表符

**病因。**

1. 桥接层建 `Para` 时不读 `w:tabs`（段落与样式链）和 settings 的 `w:defaultTabStop`，
   `Para` 也没有制表位字段。
2. 含 `'\t'` 的文字直接交给度量，制表符被当成一个字形量：Calibri 的 U+0009 落到 glyph 0，
   121.6 twips，从不落到停靠点。
3. `break_opportunities` 把制表符之后当断点，Word 断在制表符之前。tab-after-a Word 是 0、1、43，
   基线是 0、2、3、…。
4. 再加上 G0 的一行一字，tab-zeros 基线是 0、1、2、3、…。

**规则。** 每个 `'\t'` 单独成一片。

- **停靠点**：取位置严格大于当前 x 的最近一个自定义制表位，竖线制表位不算；首行悬挂缩进时，
  左缩进处另有一个隐含的。都没有就落到从左页边距起的默认栅格。
- **推进**：左对齐是「停靠点 − x」。右 / 居中 / 小数点对齐是
  「max(0, 停靠点 − x − 该段的对应份额)」，段一直延伸到下一个制表符、占位符或段末，跨 run 累加。
- **断行**：制表符之前可断、之后不可断。后面的字放不下时，制表符跟着字下去，到下一行重新落位；
  要先在下一行试排，确认字上得来才带下去。
- **缺省档**：`Para.default_tab_stop` 是 `Option<Twips>`，没写或 ≤ 0 为 None，由引擎按平台补：
  - Desktop 720，ECMA-376 §17.15.1.25 的缺省，**Mac 上未测**；
  - Android 221，**拟合值**（决定 D9）。
- **绘制**：制表符画成一个空格字形，推进等于制表宽度。两端对齐只作用于最后一个制表符之后。

**221 的来历。** 只用窄路径的数据，Word 这一档稳得住的区间是 **(123.9, 222.04)**：

- 下界 W/43 来自 zero-plain 一行 43 个 `0`、tab-zeros 制表符后 41 个；
- 上界来自 i-plain 放不下 96 个 `i`，而 tab-i 制表符后放得下 92 个。

本引擎的 `0` 是 121.64 twips，要复现 tab-zeros 的 41 个，这一档必须 ≥ 221：220 + 42 × 121.64 =
5328.9，放得下第 42 个。往上一直到 248，五个依赖它的夹具都照样复现，所以**记分器分不出 221 与
248**，把它钉在 221 的只有单元测试。将来若按像素（1440/778 twips）量化窄路径字宽（它能解释
i-plain 的 95），Word 的区间变成 (198.3, 220.5]，这一档必须挪进去。
代码与文档里都标的是「拟合」，不是「测量」。

**依据。** `tab.md` 的 9 组与 tabone 的 jsonl。其中 4 组与 221 无关，算独立确认，另 5 组是
样本内拟合，分组见「记分板」。

**记分。** STARTS 9 → 18，9 组全过；tabone 1/1 不变。Mac 回放不受影响：采集里一个制表符都没有。

### G5 禁则回退

**病因。**

1. **挂出。** 桥接层照 Mac 的缺省，把没写的 `w:overflowPunct` 当作开，版面层再把它直接交给挂出。
   `22汉` 恰好放得下（22 × 240 = 5280 ≤ 5329）时，越界的 `）`、`。` 被挂出版心，给 23；
   Android 退回 21（kinsoku、kinsoku-period）。
2. **没有行尾禁则。** U+FF08 落在 `is_cjk` 的 FF00..FF60 里，所以 `（|汉` 可断；`(|汉` 由
   `enter_cjk` 放行。kinsoku-open 与 kinsoku-open-ascii 于是给 22，Word 是 21。
3. **西文进 CJK 的边界不查行首禁则。** breakme 的 `6|）` 在 594 处可断，只是因为 593 本来就被选中，
   才没出错。

**规则。**

- 挂出只在 `para.overflow_punct && platform == Desktop` 时尝试。Android 从不挂出，退回前一个合法断点。
- 新增行尾禁则集（brkcls 的 lead=0），扩充行首禁则集（foll=1），按来源分三档标注：
  - 输出实测：`（ ( ） ) 。 ））`；
  - 从类表读出、「就此不断」是推断：CSV 里的其余成员；
  - 外推：CSV 里没有的括号另一半 `〔〕〖〗〝〞﹙﹚﹛﹜﹝﹞￠￡￥`。

  `〘〙〚〛` 找不到来源，去掉了。
- CJK 边界与西文进 CJK 的边界共用 `kinsoku_ok`。
- ASCII `?` 仍禁行首（CSV 给 following=3），保持原行为并钉了测试；`%` 跟 CSV。
- CSV 偏 zh-CN。

**依据。**

- Android，`kinsoku.md` 与 kinsoku 的 jsonl：
  - 22 个放得下，23 个放不下；
  - 越界的 `）`、`)`、`。`、`））` 退一个合法断点，到 21，不再往前退（kinsoku-pair）；
  - 放得下的禁则字留在行尾（kinsoku-fit，22）；
  - 放得下的 `（`、`(` 不留在行尾（21）。
- 类表 `reports/brkcls-map.csv` 加 `findings/brkcls.md`：运行时从 `LserrGetBreakingClasses` 读出
  350 个码位，7 个输出级用例全与表相符。
- Mac：`docs/PREREG-2026-09-18-kinsoku.md` K-a..K-d 4/4 挂出；`-kinsoku2.md` 显式关闭时给 36。

**记分。** ALL 154 → 156（kinsoku 0/2 → 2/2），PARITY 154 → 156，
STARTS 18 → 22（k-kinsoku、k-open、k-open-ascii、k-period）。合并 G4 之后 nofb-kinsoku 也过。
k-han22、k-ascii、k-fit、k-pair 与 breakme 13/13 照旧。
Mac 回放的 kinsoku、kinsoku2、cjk-plain 不变，桌面仍照段落属性挂出。
不计分的 hanging@10466（第 5 行起点 206 → 205）与 brkmix 也变了。brkmix 段末的 `（` 独占多出的一行，
这是规则的推论，但没有 Word 的行数据。

**桌面上的未测变化。** 禁则表不看平台，下面这些 Mac 回放与仓库夹具里都没有（A5.11–A5.13）：

- `22汉。〉10汉`、`22汉，…10汉` 由 23 退到 21；
- `21汉％。10汉` 由 23 退到 20；
- `21汉…。10汉`、`21汉°。10汉` 由 22 退到 20。

### G6 br-page：段末分页符与段落标记成行

**病因。** `break_paragraph` 的占位符分支在段末分页符处把段落标记收进分页符那一行，(340,342)。
段末分支于是再也不出单独的段落标记行；`layout()` 在那一行之后翻页，页是 [11,5]。
这对分页视图是对的（Mac 实测），对 Android 的移动视图不对：窄路径正是记分用的那条，Word 排成
(340,341)、(341,342)。桥接层没问题：解析器 JSON 给的就是 U+FFFC 加 `{kind: br, breakKind: page}`。

**规则。**

- `Engine::splits_page_break_and_mark(para)` 定义为
  `view == Mobile && para.terminator == ParagraphMark`。成立时分页符收行，行终点含分页符，
  段落标记另起一行。
- 以分节符结束的段不拆（假设，A6.5）。
- 新的私有字段 `PendingLine::last_content` 让两端对齐不去拉伸拆开后的分页符行，
  所以 x 与打印视图相同。
- 行形状与 OOXML 兼容项 `w:splitPgBreakAndParaMark` 相同，但不是它：br-page.docx 没有 settings.xml。
  以后读到那一项时，在这个函数里与视图取「或」。

**依据。**

- Android 窄路径（实测）：`br-page.word.narrow.jsonl` 是
  `…(306,340),(340,341),(341,342),(342,376)…`，一份夹具，一处。
- 打印视图合并，Mac 实测：`breaks-sections` 3 组、`vmisc2` 1 组，逐字符报同页同行。
- 打印视图合并，Android 是推断：br-page 第一页 11 条裁剪带、第二页 5 条。`br-column-one` 里
  段落标记独占的一行是有带的，拆开会多一条。
- 顺带更正了 Mac 字形证据的读法：Times New Roman 那个空格是分页符，段落标记用 run 的字体，
  在 144pt 之后（`captures/breaks-sections-2026-09-17/glyphs.json` 页下标 5）。
  `docs/PREREG-2026-09-17-vmisc2.md` §5.1 的读法与它自己的采集不符；预注册是记录，没有改。

**记分。** ALL 156 → 158、PARITY 156 → 158（br-page 15/17 → 17/17）。
打印视图的输出与上一步逐字节相同：word_analyse 全部夹具 × 两宽 × 两平台，各 0/430。
`--view mobile` 下的页下标是引擎照样分页排出来的（br-page 是 11 / 6），不对应 Word。

### R 跨 run 回退

**病因。** G0 之后还剩一处：一截放不下、更早的断点又落在前一个 run 的片段里时，引擎退不回去，
只能在 run 边界收行，run 边界又成了断点。症状有四：

- `[hello wor][ld + 60 个 0]` @5329 给 0、9、52，同样的字放在一个 run 里是 0、6、49；
- G5 的拆 run 禁则 `[22汉][）10汉]`、`[22汉][。10汉]`、`[21汉（][10汉]` 给 22，一个 run 是 21；
- G1 的制表符加后面的词拆在两个 run 里时，断法与一个 run 不同；
- 桌面上还有一族：run 边界挨着一处被新禁则字挡住的挂出时，越界的 `。`、`，` 开了下一行
  （`[21汉％][。10汉]` 给 22），与 Mac 实测的行首禁则相反（`PREREG-2026-09-18-kinsoku2.md` J-a 4/4）。

**规则。** 段落先拍平成片段序列（文字 / 制表符 / 占位符），`break_paragraph` 用一个可回退的
（片段, 字节）游标走；`carry_tab`、`next_run_chars`、`line_has_break` 都删了。
一截放不下时，`Engine::shortfall` 依次判断：

- 交界本身是断点：就在那里收行。
- （甲）不粘在制表符上：退到行首之后最后一个合法候选，截断本行，回退游标。
- （乙）没有候选：G0 的紧急断行。
- （丙）粘在制表符上、有候选：把候选到制表符的那一截在下一行试排，制表符落得下、字上得来才退。
- （丁）粘在制表符上、没有候选：出一条只有制表符的行，或紧急填满。

另有几条约束：

- 制表符之前那一处交界按 `chars_break(前一个字, Some('\t'))` 判断，所以 `（<TAB>` 之间不断。
- 行尾空格跨 run 吃掉。
- 回退不越过硬换行、分页符或分栏符，也不留下只有制表符的行。
- 审查后补的一处：左对齐停靠点被行尾推到行尾时也要试排（默认档 720 上的
  `20汉（<TAB>Sincerely`），退到 `汉|（`，第一行止于 20。

**依据：推断。** R 是两条实测合起来的预言：`vanish.md` 的 webhidden / specvanish 说 run 边界不是断点，
`tab.md` 的 tab-right-1440 / tab-right-fit 说更早的断点胜。没有一份记分夹具或 Mac 采集走到这条路径：
记分夹具全是单 run，下面这些都与上一步逐字节相同：

- 434 次夹具扫描（Android 与桌面两档）；
- 31 份仓库夹具 × 4 种模式；
- 30 个 Mac 包。

支撑它的只有单元测试（`tests/cross_run_retreat.rs`，含 3000 × 2 组「随机拆 run 与一个 run
排法相同」的性质测试），以及「拆不拆 run 断法都一样」这条推理。
G0、G1、G5 钉下的 10 个已知偏差测试全部去掉了 `#[ignore]` / `#[should_panic]`，改名 `assumed_*`，
现在正常通过。

**记分。** 无变化，符合预期。

### G3 w:spacing 与 w:w

**病因。**

1. 桥接层给每个 run 写死 `letter_spacing: 0, scale_pct: 100`，而解析器 JSON 其实给了
   `spacing`（w:spacing）和 `scale`（w:w）。
2. latinspace、latinscale 的 run 里有两个兄弟 `w:rPr`（CT_R 只许一个）。rsWordParser 399e36a 的
   `build_run` 逐个读，后一个整个覆盖前一个，而字距 / 缩放恰在前一个里，就丢了。
   没有诊断，`warnings` 为空。word_analyse 的夹具里只有这两份是这种形状。
3. 度量按字形加了字距，绘制（`position_glyphs`）用的却是原始推进量，既不缩放也不加字距。

**规则。**

- 桥接层读 spacing 与 scale。
- 兄弟 rPr：新的 `crates/core/src/load.rs` 提供 `load_document`，这是决定 D4 的权宜之计，解析器不动。
  - 它在会话已解析的主部件源文字里扫兄弟 rPr，扫到就按属性先到先得合并成一个，再交给解析器会话。
  - 合并失败时退回未合并的 JSON，并记在 `LoadedDocument.merge_error`。
  - layout-trace、cffi、wasm、`examples/probe_oracle` 都走它。
  - 它不链入解析器的编辑引擎：webgl wasm32 比 P 只大 36 KB。审查前用 `SessionTable::apply` 的写法
    要大 4.4 MB。
- 字距按整形 cluster 计一次（`spacing_slots` / `apply_char_spacing`），度量与绘制同一口径；
  空格和断行前最后一个可见字形也加。
- w:w 按精确比例乘推进量。
- SVG 用 letter-spacing 加 transform 表达。

**依据。**

- latinspace 窄路径 jsonl 24 行（实测）：
  - 每个字符加 val/20 pt，空格也加；
  - 「空格不加」「断行前最后一个字形不加」两个模型在第 4 行起点被否：Word 124，模型 130；
  - 另见 `letter-spacing.md` 与 `findings/state-machines.md` 的字宽表；
  - 文字每 124 码元重复一次，24 行里只有 3 个独立约束；
  - 「最后一个字形也加」只在「行尾空格不计」的前提下被证，rsword 计行尾空格，所以这一条标为假设（A3.1）。
- 兄弟 rPr：Word 用第一个 rPr 的属性（实测：latinspace 的 +20、latinscale 的约 80%）。
  第二个用不用、同一属性冲突时谁赢，未测。
- w:w：50 / 55 / 90% 的 `0` 与 80% 的 `M` 都与精确比例相容，80% 的 `0` 不相容（`char-scale.md`）。
- Mac 旁证（`PREREG-2026-09-17-hbox2.md` M2）只到 sp/20 这个量级。

**记分。** ALL +24（latinspace 0/24 → 24/24，D4 与 D5 都是）。EXT +4：zero-scale-50 87、zero-scale-55 79、
zero-scale-90 48、m-scale-80 32。zero-scale（w:w=80）仍 FAIL。PARITY 不计 latinspace，见上文。
Mac 回放里 hbox、hbox2、vmisc3 三包的字距 / 缩放行变了，全都更靠近 Word，见「Mac 回放」。

### G2 w:caps 与 w:smallCaps

**病因。** 桥接层建 FontSpec 时不读 caps / smallCaps（解析器 JSON 里有），FontSpec 也没有这个字段。
度量与绘制都看不到大写形：caps-on 按 `a`（114.96 twips）量，而不是按 `A`（138.87）。
再加上 G0 的一行一字，基线 caps-on 窄路径排成 75 行。样式链上的 caps 不解析，与 bold / italic 同缺，
这一轮没补。

**规则。**

- `FontSpec.caps: Caps { None, All, Small }`。caps 与 smallCaps 都开时 caps 胜。
- **映射**：`font::caps::display_chars_with` 把源字符映射成显示字符，带着源区间。决定 D8：
  - 只有大写恰是一个字符、且两者都是单个 UTF-16 码元（BMP）时才映射；
  - 所以 ß、ﬁ、ŉ 不变，德瑟雷特、阿德拉姆等星芒面字母也不变；
  - 映射表是 rustc 的 Unicode 数据；
  - span 机制保留，以后换成完整映射只是局部改动。
- **小型大写**：只缩「有不同大写且画得出」的字符，字号取 floor(0.8 × 半点)。
  其余（大写、数字、标点、CJK、空格、TAB）全尺寸，组合符跟 cluster 的基字。
- **画不出**：大写哪个字体都画不出、源字符画得出时，画源字符，全尺寸。
- **口径**：断点、禁则、挂出的上下文用源文字，只有宽度与字形用显示字符。一条整形路径同时供
  度量、绘制与字形记录，`ShapedRun.size_centipoints` 带每个字形的字号。

**依据（实测：`caps.md`、caps-on 窄路径 jsonl、`findings/state-machines.md` 字宽表）。**

- caps 下每个字符取其大写字形在 run 字号下的推进：
  - 120 个 `a` 纸页 75，与 `A` 相同；
  - 窄路径 (0,38),(38,76),(76,114),(114,121)。
- smallCaps 12pt 的 `a`：纸页 95，14560 版心 132，10560 版心 96。若按 Calibri 的 `A` 线性缩放，
  绘制字号落在 (9.460, 9.505] pt。
- floor(0.8 × 半点) 在 12pt 给 9.5pt，落在带内。被数据否掉的候选：
  - ECMA-376 的「小两磅」：10pt，给 90；
  - 不取整的 80%：给 94；
  - 取整到整点：9pt，给 100；
  - 0.785：smallcaps 给 96；
  - 0.794：smallcaps-12000 给 95。
- 未测：其他字号与上下标（792 → 600 cp），都是假设。1:1 映射的依据只有 libwlibandroid.so 的
  静态导入名。

**记分。** ALL +4（caps-on 0/4 → 4/4），STARTS +4：caps-on@paper 75、smallcaps@paper 95、
smallcaps-wide 132、smallcaps-12000 96。caps-plain@paper 91 不变。
其余不含 caps 的记分轨迹逐字节相同。

### G4 CJK 回退字体

**病因。** CJK 夹具的 eastAsia 槽都写 SimSun，手机上没有。fontenv 按字体哈希序查覆盖，Calibri 没有
U+6C49，选字返回 None。整形时没有 face 的段被直接跳过，也不出 `.notdef`，度量加 0。于是 `fit`
永远放得下，整段落在一行：nofb-han22 rsword 排 1 行，Word 是 22 + 8。轨迹的 `unassignedGlyphs`
仍是 0，丢字是无声的。
次生问题：CJK 字体作普通 `--font` 时，会按哈希序与西文字体抢槽族缺席的字符。breakme 用 Noto 作
`--font` 时西文落到 Noto，一行 45 个，而不是 51 个。

**规则。**

- **回退层**：一个有序的回退层，在 fontenv 的哈希序搜索之外。
- **顺序**：eastAsia 槽的字体画不出的字符，先查回退链、再查其他 `--font`；ascii / hAnsi / cs 照旧。
- **不参与的字符**：控制字符、Cf 与默认可忽略字符既不取回退，也不算缺字。
- **注册**：链上的字体只在盖得住还缺着的字符时才注册。
- **名义字形**：回退链也盖不住的 `is_cjk` 字符画 `.notdef`（glyph 0，run 的 ascii/hAnsi 字体），
  推进恰为有效字号的 1 em。轨迹顶层新键 `notdefGlyphs` 计数，不复用 `unassignedGlyphs`。
  这一条在库里，两个平台都一样。
- **谁装回退链**：由 `layout-trace` 装（决定 D7）：
  - `--no-fallback`：不装；
  - `--fallback-font PATH[#INDEX]`：可重复，优先；
  - `--platform android`：读 `RSWORD_FALLBACK_FONT`，没设就用仓库的
    `fixtures/fonts/DroidSansFallbackFull.ttf`，编译时按 `CARGO_MANIFEST_DIR` 定位、运行时检查，
    不用 `include_bytes!`；
  - `mac`：不装。
- **装的时机**：显式链在「槽里画不出」时装；默认的 Droid 只在有字符「哪个 `--font` 都画不出」时才装。

**依据（Android 实测）。**

- 手机 Word 的进程映射里只有 NotoSansCJK-Regular.ttc、Noto Serif CJK、MiSans 等，没有 SimSun，
  也没有 Droid（`reports/maps-word-13336.txt`，那份快照不全）。
- 表意字按 1 em 排，各夹具给出的推进量 a 的区间：
  - han22，12pt，窄路径每行 22：231.7 < a ≤ 242.2 twips；
  - han-paper，每行 43：237.9 < a ≤ 243.4；
  - mix-cjk：233.4 < a ≤ 245.7；
  - han-24，24pt，每行 11：0.925–1.009 em；
  - 12pt 的交集是 0.991–1.009 em。
- 候选字体的 `汉` 与全角标点都是 1 em，所以断行上分不出 Word 换成了哪个字体：**未测**。

**记分。** STARTS +4：nofb-han22 [0,22]、nofb-mix-cjk [0,20,45]、nofb-han-24 [0,11,22]、
nofb-kinsoku [0,21]，最后一个要和 G5 一起才过。A 组以及带 Noto 的记分轨迹不变。
Mac 回放不变，因为 mac 不装回退。

### 软回车：`w:br`（缺省或 textWrapping）与 `w:cr`（2026-09-26 追加）

**病因。** 解析器（rsWordParser 399e36a `segment()`）把 textWrapping 的 `w:br`、不写 `w:type` 的
`w:br` 和 `w:cr` 都推成 run 文本里的 `'\n'`，只有分页、分栏推 U+FFFC。桥接层只认 U+FFFC，
于是软回车一路当普通字符量过去，`br-soft` 整段排成一行（0–11）。
第二个毛病藏在对位上：旧的 `run_placeholders` 按「非文本段的个数」对 U+FFFC，而 `w:tab`
（`'\t'`）、`w:lastRenderedPageBreak`（零长）、`w:fldChar` 这些不落成 U+FFFC 的段也算进个数，
一对不上就整体退回 `Object`。软回车改成 U+FFFC 以后，同一 run 里只要还有一个 `w:tab`，
软回车又会丢。

**规则。**

- 段的 `text` 是它在 run 文本里的**字节区间**。按区间走一遍 run 文本：软回车段（`br` 且
  `breakKind` 不是 page / column，或 `cr`）的那个 `'\n'` 换成 U+FFFC，记成 `LineBreak`；
  其余每个 U+FFFC 取包含它的那个段的种类（page → `PageBreak`，column → `ColumnBreak`，
  drawing / pict / object / 脚注引用 / 未知 → `Object`），段外的按 `Object`。
- `'\n'` 与 U+FFFC 都是 1 个 UTF-16 码元，源偏移不变。
- **按段换，不按字符换**：文本段里字面的 LF 不动。
- 段不带区间（手写 JSON）时退回旧的按次序对位，不改文本；区间越界、不在字符边界或重叠时，
  文本不改、U+FFFC 全按 `Object`——保守方向，同旧注释。
- 排版层原有的 `LineBreak` 处理不变：收行，终止符 `SOFT_RETURN`，画 1 个字形。
- 两个平台一样：软回车断行不是平台差异。
- `w:vanish` 的 run 里的软回车不转换、不断行（审查发现的回退：隐藏文字还没实现、照样排，
  转换后会凭空多一行）。只看 run 自己声明的 `vanish`。修订删除的 run 照常转换，与删除的分页符同口径。
- 软回车紧跟一条刚按宽度收下的行、中间什么都没有时（Desktop 挂出 `。`、`）` 后当场收行，
  或行末空格溢出），它收进那一行，不自成一条空行——同一位置的段落标记本来就收进那一行
  （审查发现：否则 `37 汉 + 。 + 软回车 + 10 汉` 排出 3 行）。

**依据。**

- Android 窄路径 `br-soft`、`br-bare`、`br-cr` 的起点都是 0、5（`br-soft.md`，实测）：
  软回车那个码元收在上一行，与 `(0,5)`、`(5,11)` 相同。
- 反例：breakme 的 UTF-16 576 是 `xml:space="preserve"` 的 `w:t` 里的字面 LF，Android 窄路径
  那一行是 561–593，没在 576 断（实测）。
- 画 1 个字形：方法 §4 的计数约定（Windows 03d–03h 30/30），引擎原有。

**记分。** STARTS 30 → 33（br-soft、br-bare、br-cr），其余用例不变。夹具扫描（Android 与桌面各
502 份）只有这三份夹具变了；Mac 回放 30 包 0 包变化——采集里没有软回车，仓库的 `breaks.docx`
有一处但不在采集里。测试 `tests/soft_breaks.rs` 10 个，含 `fixtures/breaks.docx` 经真解析器的端到端一条；
挂出那一条做过变异检查（关掉并行规则即失败）。

**假设（Android 未测）。** 段末软回车之后段落标记另起一条空行（引擎原有、桌面 Word 行为）；
挂出标点之后的软回车收进挂出那行；`w:br w:clear` 当普通软回车；软回车所在 run 的字号不参与行高
（行高只看文字片段，字形按该 run 的字号画）；两端对齐照拉伸软回车结束的行（OOXML 缺省），
`w:doNotExpandShiftReturn` 没读；段落级的裸 `w:br`（解析器发成 atom）照旧被丢。

## 回放与检查

### 记分器

记分器是这一轮会话里固定下来的脚本，不在仓库里；本轮没有改过它。它对每个用例调用
`layout-trace`：

- 固定参数：`--platform android`、`--font <手机 calibri.ttf>`，CJK 夹具再加 NotoSansCJK
  （breakme 加仓库的 Droid），最后 `--content-width 5329|10466`；
- 窄路径、没改页宽的用例再加 `--view mobile`；
- 改了页宽的用例加 `--page-width`。

然后拿行的 (sourceStart, sourceEnd) 对 Word 的读数。单独复现一行的写法见
[`tools/measure/README.md`](../tools/measure/README.md)。

### Mac 回放

`tools/measure/sweep.py` 不带平台选项，走桌面默认：

| 指标 | 基线 | 终态 |
| --- | ---: | ---: |
| 采集包 | 30 | 30 |
| VOID，不比较 | 5 | 5 |
| 比较的轨迹 | 25 | 25 |
| 结构一致 | 22/25 | 22/25 |
| 页数一致 | 25/25 | 25/25 |
| 比较状态 | 25 FAIL | 25 FAIL |
| selfcheck | 25 UNDECIDABLE | 25 UNDECIDABLE |
| 与基线字节不同的包 | — | 3 |

零容差下，所有包在基线上就是 FAIL，这一轮不改变这一点。变了的 3 个包都是 G3 的结果：
hbox、hbox2 的 `w:spacing` 行（15 / 20 / 30 / 40 / 60 / 80），vmisc3 的 `w:w` 行（66 / 120 / 180）。
改动过的字形没有一个离 Word 更远：

| 包 | 改动的字形 | 更近 / 更远 | 这些字形的平均 \|d\| | 最大 \|d\| | 整包 maxAbs |
| --- | ---: | ---: | --- | --- | --- |
| hbox | 36 / 638 | 36 / 0 | 15.1512 → 0.0155 pt | 47.9987 → 0.0323 pt | 47.998680 → 28.668909 pt |
| hbox2 | 36 / 1147 | 36 / 0 | 11.3384 → 0.0368 pt | 35.8949 → 0.1316 pt | 40.998467 → 40.998467 pt |
| vmisc3 | 27 / 111 | 27 / 0 | 17.4477 → 0.0742 pt | 54.0398 → 0.2252 pt | 54.039840 → 11.520000 pt |

hbox2 的整包最大差不在这些行上，所以没动。剩下的 0.02–0.23 pt 仍大于零容差。
其余 22 个包的轨迹逐字节相同，G0 的「两端对齐只加在词间」也没碰到任何一份 Mac 采集。

下面这些桌面上的变化，Mac 回放看不见，因为采集里没有对应构造：

- 超长无断点串；
- 制表符（缺省档 720）；
- 扩充的禁则表；
- caps；
- 跨 run 回退；
- 谁都画不出的 CJK 按名义 1 em。

仓库的 31 份夹具 × 4 种模式（real、simple、不装 CJK 字体、real + Mac 纵向栅格），基线与终态之间
124 份轨迹里变了 20 份，全都对得上：

- `complex` 的一长串 `X`（G0）：4 种模式都变，原来一行一个字，现在一行 62 个；
- hbox、hbox2、vmisc3（G3）：只有字形变，3 种模式；
- 不装 CJK 字体时，breaks、cjk-plain、kinsoku、kinsoku2、plain、sample、wrap、complex 的 CJK
  从零宽变成名义 1 em（G4）。

### 夹具扫描

word_analyse 的全部夹具 × {5329, 10466}，共 434 次运行，只比页与行区间。用基线二进制在同一批夹具上
重跑作对照（基线当时的存档只有 428 次）：

| | `--platform android` | 桌面默认 |
| --- | ---: | ---: |
| 与基线不同的运行 | 129 / 434 | 128 / 434 |

逐步的变动数：G0 117、G1 10、G5 9、G6 0、R 0、G3 14、G2 8、G4 0。同一次运行可能在几步里都变
（比如 tab-* 在 G0 与 G1、caps-on 在 G0 与 G2），所以加起来多于 129。扫描只传 `--platform android`、
不传 `--view mobile`，所以看不到 br-page。两档之间的差别：

- Android 多了 kinsoku@5329 与 kinsoku-period@5329：21，桌面挂出，仍是 23；
- 桌面多了 spacing@5329：那一段是 `pre<TAB>tab1<TAB>…`、没写 defaultTabStop，720 与 221 的差别。

36 次报错（table* 夹具，「没有可排版的段落」）是原有的，每一步都一样。

### 测试与静态检查

| 检查 | 基线（HEAD + WIP） | 终态 |
| --- | --- | --- |
| `cargo test -p rsword-layout-core --features fontenv` | 177 通过 | 347 通过 / 0 失败 / 6 忽略（40 组） |
| 同上，不带 features | 未记 | 252 / 0 / 0（33 组） |
| 加 `RSWORD_TEST_CALIBRI`、`--include-ignored` | 未记 | 353 / 0 / 0 |
| `cargo test -p rsword-layout-svg` | 未记 | 4 / 0 / 0 |
| `cargo check --workspace --all-targets` | — | 0 警告 |
| `cargo clippy -p rsword-layout-core --features fontenv --all-targets` | 1（WIP 的 `items_after_test_module`） | 0 警告 |

6 个被忽略的测试都要手机 Calibri（`#[ignore = "needs RSWORD_TEST_CALIBRI"]`），其余只用
SimpleMetrics、合成 run 或仓库字体。已经没有带 `#[should_panic]` 的已知偏差测试。WIP 那条 clippy
警告不再出现：G0 把 `cluster_tests` 挪到了 linebreak.rs 的末尾，后面有 `mod` 时 clippy 不报这一条。

## 假设清单

每条后面是能定案它的夹具；括号里是本引擎的预测，或「这里给 / 硬推给」的对照。
run 的拆法写成 `[…][…]`，`@5329` 指窄路径版心宽。

### G0

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A0.1 | 紧急切口「恰好放满」算放得下（≤，不是 <） | zero-plain，版心恰为 43 个 `0` 的宽度（到 twip），再来一份少 1 twip |
| A0.2 | 空行至少收一个 cluster，哪怕它自己就宽于一行；fittext-over 只部分支持 | 单个字形宽于一行：40 个 `0` 的词配 leftIndent 5000 @5329，或特大字号 |
| A0.3 | 行首之后更早的空格 / CJK 断点胜过紧急填满；只量过制表符的版本 | `hello ` + 100 个 `0`、`汉汉` + 100 个 `0` @5329 |
| A0.4 | 切口零容差。实测上界只有「理想字宽 < 6 twips，按像素取整 < 13」；像素取整（1440/778）是假说 | 越界量在 1–6 twips 之间的字形 @5329 |
| A0.5 | 「字符」是 cluster：RealMetrics 用整形器的 cluster 起点，SimpleMetrics 用 UAX #29 近似表；代理对、组合符、ZWJ、RI 对都不拆 | e + U+0301 ×100、泰文 `กำ` ×60 @5329 |
| A0.6 | 紧急切口不看禁则与挂出 | `0`×42 + `）` + `0`×30 @5329 |
| A0.7 | 切口之后的空格归本行 | `0`×43 + 三个空格 + `0`×100，版心恰好放满 43 个 |
| A0.8 | 对象占位符两侧都可断，含行首对象之后（对象独占一行） | 行内图片 + 100 个 `0`；`0`×30 + 图片 + `0`×100 @5329 |
| A0.9 | 两端对齐的空隙只加在词间；这是叠在原有「按片段交界分配」近似之上的，Word 按空格分 | jc=both 的 20、30、80 个 `0` 三个 run @5329；run 在空格之前拆开的两端对齐行 |
| A0.10 | run 边界恰在 `-`、`/` 之后时不再断（UAX #14 的 BA/HY 没实现） | `[self-][employed…]` 拆 run 与不拆各一份 |
| A0.11 | 只有前导空格的行允许（原行为） | ` ` + 100 个 `0` @5329 |
| A0.12 | 下一行更宽（首行缩进、环绕）时照样在本行切 | firstLine=2000 + 40 个 `0` 的词 @5329 |
| A0.13 | 仓库字体没有泰文，real-metrics 测试用 DejaVu 的老挝文 AM（U+0EB3）代替泰文 SARA AM，走同一条 HarfBuzz 泰文整形路径 | 能画泰文的仓库字体，或泰文夹具 |

### P

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| AP.1 | Desktop 同时代表 Mac 与 Windows Word，只量过 Word for Mac 16.112 | 任一个 Mac 采集包在 Windows 上再采一次 |
| AP.2 | Desktop + Mobile 可以组合，没有依据：只见过 Android 的移动视图 | — |
| AP.3 | Mobile 下的页下标是合成的；实测的移动视图只有一条页记录 | 量不了 |

### G1

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A1.1 | Android 缺 `w:defaultTabStop` 时补 221，这是拟合本引擎当前窄路径字宽的结果，不是测量 | tab-zeros 写 defaultTabStop 720 与 1440（照用给 38 / 32，按视图缩放给 41 / 38，不认给 42）；`0<TAB>` + 90 个 `i`、不带尾巴（栅格给 1 行，固定约 221 宽给 0、1） |
| A1.2 | Desktop 缺省 720 是规范值，Mac 未测 | Mac 采一份行首制表符 + 一串 `0`、没有 settings.xml 的 |
| A1.3 | 两条路径用同一个缺省档，而且是固定 twips，不随字号 | tab-zeros 改成 24pt 放纸页路径（固定给 43，随字号给 42） |
| A1.4 | Android 认写了的 defaultTabStop；0 或负数当没写；settings.xml 里没有这一项等于没有 settings.xml | 同 A1.1 |
| A1.5 | 停靠点「严格大于」笔位 | 笔位恰好落在某个制表位上的夹具 |
| A1.6 | 默认栅格锚在左页边距；只量过 x = 0 | `0<TAB>` + 90 个 `i` |
| A1.7 | 悬挂缩进的首行有隐含制表位；制表位相对页边距，不是相对缩进 | 悬挂缩进的列表段 + 制表符；有缩进的段 + 行首制表符 |
| A1.8 | 居中、小数点对齐的公式；只认 `.` 作小数点 | 居中与小数点制表位夹具 |
| A1.9 | 右 / 居中 / 小数点停靠点夹到行尾；居中、小数点段不许越过行尾 | 右对齐 10466 的 `Title<TAB>12` @5329（1 行）；居中 5233、右对齐 10466 的 `Left<TAB>Center<TAB>Right` @5329 |
| A1.10 | 左停靠点在行尾或之外：制表符留在本行，字换行、前面不带制表符 | 左对齐 6480 的 `A<TAB>000` @5329（0、2） |
| A1.11 | 制表符后的词若空行上放得下，就不切，也不推出行尾，而照实测规则硬推会切；「放得下」按到第一个断点的前缀、去掉尾空格算 | tab-stop-1440.docx 纸页路径（这里 0、2，硬推 0、1、76）；`<TAB>` + 94 个 `i` @5329（0、1 对 0、93）；左对齐 5040 的 `Name:<TAB>Date: today` @5329（0、6 对 0、5、8）；`<TAB>` + 43 个 `0` + `i` + ` x` @5329（0、1、46 对 0、42） |
| A1.12 | 行尾空格不挂出：制表符后的词连同空格放不下就整词换行。这是引擎的一贯规则；Word 大概让空格挂出，两边都未测 | `<TAB>Sincerely, x`，制表符后的余量落在 `Sincerely,` 与 `Sincerely, ` 的宽度之间；同一份把制表符换成 `A ` |
| A1.13 | 粘着的制表符只在下一行试排确认字上得来时才带下去，否则留在本行 | 左对齐 5300 的 `A<TAB>B C` @5329（0、2 对 0、1、4） |
| A1.14 | 只有制表符的行，只在行首制表符之后一个字都上不来时出现 | 左对齐 5760 的 `<TAB>Sincerely,` @5329（0、1）；左对齐 9000 的 `<TAB>` + 80 个 `0` @5329（0、1、45） |
| A1.15 | 连续制表符之间可断；制表符与后接的 CJK 之间不断 | `A<TAB><TAB>` + 一长串 `0`；越界的 `A<TAB>汉…` |
| A1.16 | 右 / 居中 / 小数点的段延伸到下一个制表符、占位符或段末，跨 run 累加，行尾不回填 | 段在停靠点之前折行的右对齐制表符 |
| A1.17 | 两端对齐只作用于最后一个制表符之后 | 带制表符的多 run 两端对齐行 |
| A1.18 | 制表符画成一个空格字形，推进等于停靠宽度；前导符与竖线不画；推进不计 w:spacing 与 w:w | Mac 采一行带制表符的 |
| A1.19 | tab-right-fit 的 `00` 恰好在 1440 结束 | tab-right-fit 的行内字形位置 |

### G5

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A5.1 | Android 不挂 `，`、`、`；只量了 `）`、`。` | `22汉，10汉`、`22汉、10汉` @5329 |
| A5.2 | Android 纸页路径也不挂；挂不挂只看平台，不看视图 | kinsoku.docx、kinsoku-period.docx 在 10466 |
| A5.3 | 显式写 `<w:overflowPunct w:val="1"/>` 在 Android 上也不挂 | kinsoku.docx 加显式 overflowPunct=1 @5329 |
| A5.4 | 第二档：类表读出的 lead=0 / foll=1 成员。类值是实测，「lead=0 或 foll=1 就不断」是推断 | `21汉「10汉`、`22汉」10汉`、`22汉…10汉`、`汉·汉` 落在行边 @5329 |
| A5.5 | 第三档：外推的括号另一半 `〔〕〖〗〝〞﹙﹚﹛﹜﹝﹞￠￡￥`，CSV 里没有 | 对这些码位跑一次 brkcls，或 `21汉〔10汉`、`22汉〕10汉` @5329 |
| A5.6 | ASCII `?` 仍禁行首（CSV 给 2/3），`%` 跟 CSV | `22汉?10汉`、`22汉%10汉` @5329 |
| A5.7 | 行尾禁则字之后、制表符之前不断（`（<TAB>`） | `20汉（<TAB>Sincerely` @5329（第一行止于 20）；`（<TAB>` + 60 个 `0` @5329（没有单独的 `（` 行） |
| A5.8 | 西文进 CJK 的边界也查禁则，`6` 与 `）` 之间不断；这只与 breakme 相容 | `21汉` + `A` + `）` + `10汉`，落在行边 @5329 |
| A5.9 | 空格之后不查禁则，`汉 ）` 可断 | `21汉` + 空格 + `）` + `10汉` @5329 |
| A5.10 | 紧急断行无视禁则：`（`×30 + `汉`×5 给 [0,22] | 这一段 @5329 |
| A5.11 | 桌面未测变化，后一个字那边：`22汉。〉10汉`、`22汉，…10汉` 由 23 变 21 | Mac 采这两段 |
| A5.12 | 桌面未测变化，前一个字那边：`。`、`，` 跟在 13 个新增 CJK 禁则字 `〉〕〗〞＂％＇．］｀｜｝～` 之后不再挂出，行退三个字，`21汉％。10汉` 由 23 变 20。给前一个字保留旧表也说得通，那是 `overflow_punctuation_candidates` 里的局部改动 | Mac 采 `21汉％。10汉`（与 `21汉～。10汉`） |
| A5.13 | 桌面上西文进 CJK 也查禁则：`21汉…。10汉`、`21汉°。10汉` 由 22 变 20 | Mac 采 `21汉…。10汉` |
| A5.14 | 偏 zh-CN 的表也用于 ja / ko | 同样的禁则夹具写 w:lang eastAsia=ja-JP、ko-KR |

### G6

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A6.1 | 分页符之前有字时也拆：`ab` + 分页符 + 段落标记给 (0,3) MidParagraph、(3,4) ParagraphMark | 窄路径 `hy` + 分页符 + 段落标记的行记录 |
| A6.2 | 连续几个段末分页符各成一行：(0,1),(1,2),(2,3) | 窄路径上一段两个分页符 |
| A6.3 | 拆开后段落标记那行落到下一页顶，页是 [11,6]，第二页下移一行（480 twips）。照 OOXML 兼容项的定义 | 量不了：Android 移动视图没有页 |
| A6.4 | 移动视图的字形数：分页符行 0 个，段落标记行 1 个空格 | 移动视图 br-page 的字形级采集 |
| A6.5 | 以分节符结束的段不拆，分节符留在分页符那一行 | 窄路径：带 pPr/sectPr 的段里 `ab` + 分页符，后接 nextPage 分节 |
| A6.6 | 移动视图下，分页符之前有文字的两端对齐行不拉伸，x 与打印视图相同 | 窄路径：两端对齐的多词行 + 分页符 + 段落标记的 x 位置 |
| A6.7 | Android 打印视图把分页符与段落标记并成一行 (340,342)，这是从裁剪带 11 / 5 推断的 | 10466 上打印视图 br-page 的 FormatLine 行记录（预期 w2 340 后接 342） |
| A6.8 | 拆行是视图行为，不是 `w:splitPgBreakAndParaMark`；打印视图认不认那一项，未测 | settings.xml 写上该项的 br-page，在打印视图采 |
| A6.9 | Desktop + Mobile 同样拆，没有依据 | — |

### R

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| AR.1 | （甲）退回前一个 run 里更早的断点，由 webhidden / specvanish 与 tab-right-1440 / tab-right-fit 合推 | `[hello wor][ld + 60 个 0]` @5329（0、6、49）；`[22汉][）10汉]`、`[22汉][。10汉]`、`[21汉（][10汉]` @5329（第一行止于 21） |
| AR.2 | 行尾空格跨 run 吃掉 | `[43 个 0][ hello world]` @5260（(0,44),(44,56)）；`[aaaa ][  bbbb]` 在行边 |
| AR.3 | 行内对象之后的空格在断行处不吃。这是原行为；UAX #14 说该吃 | 文字 + 行内图片 + ` word`，恰到行尾 |
| AR.4 | 停靠点够得着时 `（<TAB>` 不断，退到 `汉` 与 `（` 之间（丙）；没有候选就紧急填满（丁） | `20汉（<TAB>Sincerely` @5329、缺省 221（20）；`（<TAB>` + 60 个 `0` @5329（0、42） |
| AR.5 | 左对齐停靠点被行尾推到行尾（默认档 720）时，`（<TAB>` 照样试排，退到 `汉` 与 `（` 之间；`41×A (<TAB>Sincerely` 同理 | `20汉（<TAB>Sincerely` @5329、写 defaultTabStop 720（Android 与 Mac 都预测 20）；`41×A (<TAB>Sincerely` @5329（0、42） |
| AR.6 | 已知偏差：制表符到了下一行也够不着停靠点（左对齐 6480）时，`（` 与制表符留在行尾，字从页边距起。退下去的话，`（` 照样停在下一行行尾，还多出一行 | 左对齐 6480 的 `20汉（<TAB>Sincerely` @5329（0、22） |
| AR.7 | 断点紧挨在行外停靠点的制表符之前时，制表符留在本行，字从页边距起（G1 规则，未变） | 缺省 720 的 `21汉<TAB>Sincerely` @5329（0、22，Sincerely 在 x = 0）；左对齐 6480 的 `A<TAB>000` |
| AR.8 | 粘在制表符上的词跨 run 量宽；只有制表符的行，只在制表符后什么都放不下时出现 | 左对齐 4900 的 `[<TAB>Sin][cerely,]` @5329（`<TAB>`、`Sincerely,` 各一行） |
| AR.9 | 推广的试排：候选到粘着制表符的那一截在下一行试排，字上不来就把制表符留在行尾 | 左停靠点靠近行尾的 `A 汉（<TAB>Sincerely` |
| AR.10 | 桌面上拆 run 的「挂出被挡」序列与一个 run 相同：`[21汉％][。10汉]` 20、`[21汉～][，10汉]` 20、`[22汉][。〉10汉]` 21、`[22汉][，…10汉]` 21、`[21汉][％。10汉]` 20 | Mac 采这些两 run 段 |
| AR.11 | fit 的前缀含词后的空格，跨 run 也是：`[…词][ 下一词]` 余量不足一个空格时，`词` 挪下去（R 之前 `词` 留下、下一行以空格开头）。Word 大概让空格挂出 | 一行的末词结束一个 run、下一个 run 以空格开头，余量不足一个空格宽 |

### G3

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A3.1 | 断行前最后一个可见字形也加字距。latinspace 只在「行尾空格不计」时证明这一条，rsword 计行尾空格 | han22 文本 w:spacing=146 @5329（每行 13 对 14），或 zero-plain 文本 w:spacing=180 @5329（17 对 18） |
| A3.2 | 行尾空格（宽度加字距）计不计入未定，rsword 计 | latinspace 文本 w:spacing=21 @5329：1.05pt 只落在「不计」的区间 0.992–1.11，「计」的区间是 0.911–1.023 |
| A3.3 | 每个整形 cluster 计一次字距（SimpleMetrics 按源 cluster），不是按字形或 UTF-16 码元 | x + U+0301、代理对 U+20000、CJK 上的 w:spacing |
| A3.4 | 字距不随 w:w 缩放 | zero-plain 文本 w:w=50 + w:spacing=20 @5329 |
| A3.5 | 负字距不夹到零推进 | 一串 `0`，w:spacing ≤ −200 |
| A3.6 | 段落标记字形、CJK、制表符也加字距；制表符那条归 G1 | han22 加字距；带字距的制表符夹具 |
| A3.7 | 兄弟 rPr 按属性先到先得合并 | 第一个 rPr w:spacing=20、第二个 w:spacing=−10 |
| A3.8 | 第二个兄弟 rPr 到底用不用 | 第一个 rPr w:spacing=20、第二个 w:sz=36，latinwrap 文本 @5329 |
| A3.9 | Android 不把非整点的 w:spacing 截成整点（TruncDxaExpand 候选） | latinwrap 文本 w:spacing=15 或 30 @5329 |
| A3.10 | w:w 按名义比例精确计算：50 / 55 / 90 相容，80% 反常（行尾空格计入时为 0.787–0.790） | 更多 w:w（60 / 70 / 75 / 85），别的字形与字号 |
| A3.11 | SVG 的 letter-spacing / transform 是近似，用的是浏览器字体度量；这一条不需要 Word 依据 | — |

### G2

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A2.1 | 1:1、只 BMP 的大小写映射：ß、ﬁ、ŉ 与星芒面字母不变，ı→I、ſ→S、µ→Μ 映射。依据只有 libwlibandroid.so 的静态导入名 | 纸页 10466 上，w:caps 与 w:smallCaps 下各一段 ß（与 ﬁ）。Calibri 下 ß 与 SS 的下一行起点差得开 |
| A2.2 | 映射表是 rustc 的 Unicode 数据（`char::UNICODE_VERSION` 17.0），不是 Word 的；含格鲁吉亚 Mkhedruli→Mtavruli（Unicode 11）、ɪ→U+A7AE（9）、ʂ→U+A7C5（12）、ƛ→U+A7DC（16） | w:caps 下的格鲁吉亚文段落与 `ɪʂƛ`，字形级对 Word |
| A2.3 | 大写哪个字体都画不出、源字符画得出时画源字符（smallCaps 下全尺寸），组合符跟着基字 | 手机 Calibri 没有 Ɪ U+A7AE 与 Ɑ U+2C6D：Calibri 的 `ɡɪɑ` 在 w:caps / w:smallCaps 下的字形级采集，看 Word 画源字、回退字体的大写，还是 notdef |
| A2.4 | 小型大写 = floor(0.8 × 半点)，只量过 12pt → 9.5pt | smallCaps sz=22 @10466（floor 给 106，round 与 ECMA −2pt 给 100）；sz=36（64 / 62 / 63 / 58 / 56）。不依赖线性的对照：w:caps sz=19 在 10466 / 14560（页宽 16000）/ 10560（页宽 12000），若给 95 / 132 / 96，就说明 12pt 小型大写等于 19 半点的大写 |
| A2.5 | 上下标 + smallCaps 对精确字号取 floor：12pt 上标 792 → 600 cp。另一种算法 floor(0.8 × 声明半点) × 0.66 约为 627 | 上标的 smallCaps 一段 |
| A2.6 | caps 与 smallCaps 都开时 caps 胜 | 120 个 `a` 两者都开 @10466（caps 给 75，small 给 95） |
| A2.7 | 小型大写只缩「有不同大写且画得出」的字符；大写、数字、标点、CJK、空格、TAB 全尺寸 | 混大小写 / 数字 / 标点的 smallCaps 行 @10466 |
| A2.8 | 组合符、ZWJ、变体选择符取其 cluster 基字的字号；只影响字形几何，不影响宽度与断行 | NFD `e` + U+0301 的 smallCaps，字形级 |
| A2.9 | 槽与字体按源字符选（ı→I 仍从 hAnsi 字体画），缺大写时再按显示字符查覆盖 | ascii ≠ hAnsi 字体的 run 里，w:caps 下的 ı / ſ |
| A2.10 | 全小型大写段的行高按 run 的全尺寸 | 约 60 段小写 smallCaps 文本与同文的 caps 比分页 |
| A2.11 | 小型大写段的 kerning 开关，按全尺寸对 w:kern 阈值算 | 无夹具 |
| A2.12 | SimpleMetrics 看不到字体覆盖，照映射后的字符与小字号走；这是桩度量的固有限制 | — |
| A2.13 | 样式链上的 caps / smallCaps 不解析，只读声明的属性，与 bold / italic 同缺 | — |
| A2.14 | w:mwSmallCaps（settings.xml 兼容项）未实现 | — |

### G4

| # | 假设 | 能定案的夹具 |
| --- | --- | --- |
| A4.1 | 槽感知顺序：eastAsia 槽画不出的字符，先查回退链、再查其他 `--font`。显式链能复现：`--fallback-font NotoSansCJK-Regular.ttc#2` 下，审查做的 quote-hint.docx 给 Noto 12pt 的引号。默认的 Droid 复现不了，它没有 U+201C | 手机上 SimSun 缺失、hint=eastAsia 的 U+201C（与 U+00B7）：推进 Noto SC 1000、Noto JP 474、MiSans 378、Calibri 418（/1000 em） |
| A4.2 | 显式链在「槽里画不出」时装，默认链在「哪个 `--font` 都画不出」时装：这是策略，不是测量。默认链不局部：有一个缺字就装上 Droid，并接走全文 hint=eastAsia 的空格等字符，这也没有 Word 数据 | 同一份 quote / 空格 hint=eastAsia 夹具，在手机上有、没有一个额外缺字各一份；`RSWORD_FALLBACK_FONT` 指到拉下来的 Noto#2 |
| A4.3 | 「槽字体缺」指槽族画不出该字符，不另查装没装 | eastAsia 槽写已装的西文字体（eastAsia=Calibri）、含 hint=eastAsia U+201C 与 `汉` 的夹具，对照 SimSun 版 |
| A4.4 | 控制字符、Cf、默认可忽略字符、U+2028/2029 不取回退与名义字形，也不算缺字 | 审查做的 latin-cf.docx（Calibri 段落含 U+202A/U+202C）及其 hint=eastAsia 变体，在手机上比行距与字形数 |
| A4.5 | 名义字形的字体与纵向量取 run 的 ascii/hAnsi 字体（占位）；Mac N7 的东亚行距 ×1.3 在 Android 上是否成立未知 | 窄路径上 80 行 12pt `汉` 的一页，看每页行数；再做对应的 N7 检查 |
| A4.6 | 所有 is_cjk 字符名义 1 em，但只有表意字与全角标点在 12 / 24pt 受行数约束（han22 jsonl、`han-size.md`）。谚文按 1 em 是已知偏差（Noto 是 0.92 em） | 窄路径 30 个 `한`；`〈〉` 行 |
| A4.7 | 名义字形下，Mn 标记（U+302A–302D、U+3099/309A）零宽、并入前一个 cluster。依据是 Noto 的字体表，不是 Word | 没有 CJK 字体覆盖时的 か + U+3099 |
| A4.8 | 谁都盖不住的非 CJK 字符仍零宽、跳过；is_cjk 之外的 eastAsia 字符（注音、彝文、半角形）没有名义宽 | 用手机没有字体的文字写的夹具；注音一行 |
| A4.9 | 默认回退是仓库的 Droid，不是手机上的字体。记分夹具上断行相同，纵向量不同：Droid 1.309 em，Noto 1.448 em。审查的合成长文档每页名义 47 行、Droid 44、Noto 39–40，都没有 Word 的数 | 同 A4.5；或把 `RSWORD_FALLBACK_FONT` 指到 Noto#2 |
| A4.10 | 回退链按槽（eastAsia）选，不按语言 | w:lang 写 zh-CN 与 ja-JP 的 U+201C / U+00B7 |
| A4.11 | android 默认开回退、mac 默认关：这是决定 D7，不是测量 | — |

## 仍然失败的

后续更新（2026-10-04）：下表的 fittext 三份与 `ind-*` 已接入（`w:fitText`、字符单位缩进、
移动视图缩进缩放）；纸页路径的缩进读数取自移动视图状态，乘上视图比例后逐行复现。`i-plain`、
`zero-scale` 与 `latinscale` 由移动视图的设备像素字宽加 Android 行尾空格不计宽解释（「已知的亚 twip
字宽问题」与「80% 反常」都是像素取整）。见 [P0 轮对齐](WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md)。

记分器里还有 2 个用例没过：STARTS 1 个（i-plain），EXT 1 个（zero-scale）。加上不计分、但有 Word 起点的用例，都在下表里。

| 用例 | Word | rsword 终态 | 原因 | 状态 |
| --- | --- | --- | --- | --- |
| i-plain @5329（STARTS） | 0、95 | 0、96、192 | Word 窄路径的 `i` 是 55.51–56.09 twips，本引擎用理想字宽 55.08。按像素（1440/778 twips/px，约 55.53）量化能解释 95，但这只是假说，没实现。实现的话，221 必须挪进 (198.3, 220.5] | 已知的亚 twip 字宽问题，按决定不追 |
| zero-scale，w:w=80（EXT） | 0、55 | 0、54、108、162 | Word 80% 的有效比例，`0` 是 0.782–0.797，latinscale 是 0.788–0.797；50 / 55 / 90% 以及 `M` 的 80% 都符合精确比例。原因不明，没有去调 | 已知反常 |
| latinscale @5329（不计分） | 0、64、130、197、259 | 0、64、130、194、259 | 同上：16 个起点对上 15 个 | 已知反常 |
| kern-off @10466 | 0、82 | 0、76 | Word 写不写 w:kern 都是 82。rsword 只在写了 w:kern 时做对字距，kern-on 给 82 PASS。看来 Android 不看 w:kern 就做对字距，但 `kern.md` 用它读到的 PairPos 表对不上 82，机制未明 | 候选的 Android 平台项，未做 |
| vanish @10466 / @5329 | 0、116 / 0、73、116 | 0、86 / 0、43、86、129 | w:vanish 没实现，隐藏文字照样算宽度。G0 与 R 已经提供了跨 run 的填满，只要隐藏文字连成一段、不变成断点缝 | 未做 |
| ind-left @10466 / @5329 | 83 / 40 | 80 / 37 | Android 的缩进按当时的视图宽度缩放：比例在 0.507–0.519，与 5329/10466 = 0.509 相容，`wm size` 改了视图宽就变（`indent.md`）。firstLineChars 每 100 约 122 twips，不是 1 em。rsword 按写下的 twips 扣，leftChars / rightChars 也没实现 | 未做 |
| ind-left-1440、ind-both、ind-right、ind-first @10466 | 80、80、83、83 / 169 | 74、74、80、80 / 166 | 同上 | 未做 |
| ind-hang、ind-chars、ind-first-mix、ind-leftchars-long @10466 | 86 / 169、85 / 171、85 / 171、187 / 374 | 86 / 166、86 / 172、80 / 166、190 / 380 | 同上 | 未做 |
| fittext、fittext-wide、fittext-over @10466 | 0、109 / 0、60 / 0、10、50 | 三份都是 0、86 | w:fitText 没实现。在 G0 之下，fitText 的 run 应当作一个不可切的 cluster，这样 G0 的规则就能复现 fittext-over | 未做 |
| br-column / br-column-one | 1 页两栏 11 + 6 行 / 2 页 11 + 6 行 | 都是 1 页 17 行 | 分栏与分栏符都没实现。Android 的分栏符总把段落标记带下去，应复用 G6 的拆行路径 | 未做 |
| breakme @10466（不计分） | 0、102、201、303、405、504、591 | 0、99、198、297、396、495、585 | Word 在纸页路径上有时一行多收一个词：有效行宽大约宽一个词，或者 Calibri 整形窄零点几 pt（`breakme.md`）。未明 | 未做 |
| italic-n @10466 | 0、84 | 0、83 | 需要斜体字面，手里只有正体 Calibri | 缺字体 |
| italic-a @10466 | 0、84 | 0、75 | 在大小写不敏感的文件系统上，这份夹具与 italic-A 撞名，评不了 | 要重新生成 |

**引擎自己知道的偏差与欠账**（代码注释或测试里都写了）：

- **禁则与制表符。** AR.6：左对齐停靠点在行外时，`（<TAB>` 让禁则让步。
- **行尾空格与连字符。**
  - A1.12 / AR.11：行尾空格不挂出。
  - A0.10：`-`、`/` 之后不断。
- **制表符试排的近似。**
  - `fits_alone` 不模拟 CJK 收尾标点的挂出，只影响窄于两个 CJK 字的行。
  - 有环绕时，试排按本行的区间估下一行。
- **回退字体。** 默认 Droid 链不局部（A4.2）。
- **`--view mobile` 的分页副作用是合成的，只写进文档、没有压掉：**
  - 后面 `w:pageBreakBefore` 的段会多出一页；
  - keepLines 会把多出的标记行也数进去；
  - 段后距落在标记行之后。
- **绘制端。**
  - GPU / WebGL 的绘制没有 x 缩放：w:w 的字形按全宽画在缩放后的步距上，会叠。SVG 用 transform 压。
  - 整数度量路径对 `advance × pct / 100` 截断，绘制按累积取整，两者最多差 1 twip。
  - WebGL 把没有字体覆盖的 CJK 画成 1 em 的方块（D7 接受）。
- **其他端与加载。**
  - cffi、wasm 没有警告通道，拿不到 `merge_error`；合并失败时会无声地退回未合并的 JSON。
  - 没有二进制大小的守卫：库代码将来调用 `SessionTable::apply` 或 `part_bytes`，就会把编辑引擎链回来。
  - svg / wasm / cffi 不暴露 `Platform` / `View`，固定 Desktop + Print。
- **原有问题，未改。** 谚文字母 U+1160–U+11FF 在 cluster 表里是 Extend，按 `is_cjk` 却可断，
  普通断行与紧急断行的切法不同。

## 手机测量队列

这是 integration.md 的 D10，按实际落地的规则调整过。按优先级排，括号里是本引擎的预测。
这批测完，大部分「假设」能改成「实测」或被推翻。

1. **R（跨 run）。** 记分器里没有一份夹具走到它，所以排第一。
   - 拆 run 的禁则：`[22汉][）10汉]`、`[22汉][。10汉]`、`[21汉（][10汉]` @5329，第一行止于 21（AR.1）。
   - `[hello wor][ld + 60 个 0]` @5329：0、6、49（AR.1）。
   - `20汉（<TAB>Sincerely` @5329：写 defaultTabStop 720 给 20（AR.5）；左对齐 6480 给 22（AR.6）。
   - `[43 个 0][ hello world]` @5260：(0,44)、(44,56)（AR.2）。
2. **G1（制表符）。**
   - tab-zeros 写 defaultTabStop 720 与 1440：照用给 38 / 32，按视图缩放给 41 / 38，不认给 42（A1.1、A1.4）。
   - `0<TAB>` + 90 个 `i`、不带尾巴：栅格给 1 行，固定约 221 给 0、1（A1.1、A1.6）。
   - tab-zeros 改 24pt 放纸页路径：固定给 43，随字号给 42（A1.3）。
   - 右对齐 10466 的 `Title<TAB>12` @5329：1 行（A1.9）；左对齐 6480 的 `A<TAB>000` @5329：0、2（A1.10）。
   - `<TAB>` + 94 个 `i`、左对齐 5040 的 `Name:<TAB>Date: today`、`<TAB>` + 43 个 `0` + `i` + ` x`，
     都 @5329（A1.11）；左对齐 5300 的 `A<TAB>B C` @5329（A1.13）。
3. **G2（caps）。**
   - smallCaps sz=22 与 sz=36，外加 w:caps sz=19 在三种宽度上作对照（A2.4）。
   - ß（与 ﬁ）在 w:caps 与 w:smallCaps 下各一段（A2.1）。
   - caps 与 smallCaps 都开（A2.6）。
   - Calibri 的 `ɡɪɑ`，字形级采集（A2.3）。
4. **G5（禁则）。**
   - `22汉，10汉`、`22汉、10汉` @5329（A5.1）。
   - kinsoku / kinsoku-period 在 10466（A5.2）；显式 overflowPunct=1（A5.3）。
   - 第二、三档的类读用例（A5.4、A5.5）；`22汉?10汉`、`22汉%10汉`（A5.6）。
   - `（<TAB>` + 60 个 `0`（A5.7）；w:lang ja-JP / ko-KR（A5.14）。
5. **G6（br-page）。**
   - br-page 在 10466 的打印视图 FormatLine 行记录，预期 340 后接 342（A6.7）。
   - 窄路径 `hy` + 分页符 + 段落标记（A6.1）；一段两个分页符（A6.2）；带 sectPr 的变体（A6.5）。
6. **G3（字距）。**
   - han22 文本 w:spacing=146，或 zero-plain 文本 w:spacing=180（A3.1）；latinspace 文本 w:spacing=21（A3.2）。
   - w:spacing=15 / 30（A3.9）。
   - 兄弟 rPr：第二个写 sz=36；第一个 20、第二个 −10（A3.7、A3.8）。
   - w:w 60 / 70 / 75 / 85（A3.10）。
7. **G4（回退）。**
   - hint=eastAsia 的 U+201C / U+00B7（A4.1、A4.2）。
   - 80 行 12pt `汉` 的一页，看行距与分页（A4.5、A4.9）。
   - 30 个 `한`（A4.6）；latin-cf.docx（A4.4）。
8. **G0（超长词）。**
   - ` ` + 100 个 `0`（A0.11）。
   - run 内与 run 边界上的 `-`（A0.10）。
   - firstLine=2000 + 40 个 `0` 的词（A0.12）。
   - 版心恰为 43 个 `0` 宽及少 1 twip（A0.1）。
   - `hello ` / `汉汉` + 100 个 `0`（A0.3）。
9. **重新生成夹具。** 把 italic-a 重新生成为 italic-lower-a.docx，避开大小写撞名。

仍然失败的那几项（缩进的视图缩放、kern-off、fitText、分栏）要的是实现，不是新读数：
`indent.md`、`kern.md`、`fittext.md`、`br-column.md` 已经给了足够的 Word 数据。
例外是 i-plain：要决定窄路径字宽量不量化，还缺一组不同字形数的窄路径读数。

**Mac 采集队列**，与手机那批分开：

- 单 run：`21汉％。10汉`、`22汉。〉10汉`、`21汉…。10汉`（A5.11–A5.13）；
- 两 run 段：`[21汉％][。10汉]`、`[22汉][。〉10汉]`（AR.10）；
- 一份行首制表符 + 一串 `0`、没有 settings.xml 的，定桌面的 720（A1.2）；
- 任一个既有采集包的 Windows 版，看 Desktop 能不能代表 Windows（AP.1）。

## API 与用法

公开 API 的增补都已接受（决定 D6），工作区内的结构体字面量都已更新：

- `FontMetrics::fit_clusters`，带默认实现；
- `Platform`、`View`，`Engine::with_platform`、`Engine::platform()`、`Engine::view()`；
- `FontSpec.caps`（`Caps`）；桥接层现在也填 `letter_spacing` 与 `scale_pct`；
- `ShapedRun.size_centipoints`；
- `Para.tabs`、`Para.default_tab_stop: Option<Twips>`，`TabStop`、`TabAlign`、`TabLeader`，
  `TextFragment.tab_advance_pt`；
- `load_document`、`LoadedDocument`（含 `merge_error`）、`merge_sibling_run_props`；
- 字体注册表：`FontRegistry::add_fallback`、`fallback_faces`、`face_for_char`、`nominal_face`、
  `uncovered_chars`、`fallback_candidates`；整形器：`RustybuzzShaper::shape_clusters_with_face_centipoints`；
- 轨迹的顶层键 `notdefGlyphs`。

外部的 `ShapedRun`、`FontSpec`、`Para`、`TextFragment` 结构体字面量需要补字段，
或改用 `..Default::default()` / `FontSpec::new`。

`layout-trace` 的新选项是 `--platform`、`--view`、`--fallback-font`、`--no-fallback`，另有环境变量
`RSWORD_FALLBACK_FONT`；WIP 带来的是 `--margin`、`--content-width`、`--page-width`。
它们写在二进制的文档注释与 [`tools/measure/README.md`](../tools/measure/README.md) 里。
`position_glyphs` 里几件事的顺序是合并两条线时定下的，改动时要守住：

1. `'\t'` 换成 `' '`；
2. 整形；
3. `apply_char_spacing`；
4. 用 `tab_advance_pt` 覆盖 glyph 0 的推进，制表符因此恰为停靠宽度；
5. 每个字形的字号。

## 没做的与留给用户的

- **提交。** 什么都没提交，没 stash，也没建分支（决定 D1）。最终快照是相对 `fbc9108` 的累积补丁，
  包含用户的 WIP。
- **word_analyse 与记分口径（决定 D3，2026-09-26 已执行）。**
  - `reports/rsword-diff/` 的 11 份记分轨迹已用 `--platform android --view mobile` 重新生成，
    旧轨迹与旧的 `agreement.py`、`batch_cplim.py` 备份在 `reports/archive/rsword-diff-2026-09-26-pre-gapfix/`。
  - `tools/agreement.py` 的 `RSWORD_MISSING_RULE` 去掉了 caps-on、latinspace（软回车实现后也去掉了
    br-soft，它本来就没有窄路径 jsonl，不影响记分）。`agreement.py` 读 ALL 186/186、PARITY 186/186。
  - CJK 夹具可以改用 `--font calibri --fallback-font NotoSansCJK-Regular.ttc#2`，这样 Noto 作
    主字体时落到 JP face 的坑就没有了。注意：同时把 Noto 作 `--font` 的话，现在 metrics 栏会注明
    「已是 --font，不进回退链」。
- **rsWordParser（决定 D4）。** 真正的修法在 `build_run`：兄弟 rPr 按先到先得合并，并给诊断；
  然后升级 rev、删掉 `load.rs`。到那时 `parser_alone_keeps_only_the_last_run_props` 会失败，
  那就是提醒。
- **缺口表之外。** br-soft 已在 2026-09-26 补上，见「软回车」一节。
- **没有配套的 `.json`。** 此前各轮的 `ENGINE-ITERATION-*.json` 记的是 Mac 采集回放的逐包比较结果，
  而且每轮 schema 都不同。本轮的主要读数是 Android 记分器的行起点，形状对不上，所以不另出一份。
