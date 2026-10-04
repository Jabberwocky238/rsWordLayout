# word_analyse P0 轮对齐（2026-10-04）

本轮读 `../word_analyse` 自 9 月 27 日接入之后的新材料（首次提交 `aa144a7`，2026-10-02：
P0 静态轮、9 月 28 日动态轮、9 月 30 日设备轮、10 月 1 日源码定位轮），与当前实现逐项对照，
并把有 Word 读数支撑、此前未实现的规则接进引擎。代码基线 `f15ec47`。没有启动 Word、
连接手机或新采集；外部材料只读。

手机字体已不在 `/private/tmp/wordfonts`。本轮用 Mac Word 自带的
`/Applications/Microsoft Word.app/Contents/Resources/DFonts/Calibri.ttf`
（SHA-256 `ea801e1f…3dee53`，与手机那份 `40c0f6bd…` 不同）和
`../docx-layout/corpus/layout/fonts/NotoSansCJKsc-Regular.otf` 代替。换字体后 Android 窄路径
回放仍是 186/186（条件，同 [接入记录](WORD-ANALYSE-INTEGRATION-2026-09-27.md)），但这不证明
两份 Calibri 的每个字宽相同。

## 1. P0 结论能不能变成排版规则

| 项 | word_analyse 的结论 | 对引擎 |
| --- | --- | --- |
| P0-1 字形宽度 | `0x207e4cc` 不在执行路径上（closed） | 无变化：宽度来自字体与整形 |
| P0-1b `dvMultiLineHeight` | `= ascent + descent`，拟合 `13.1442·pt + 17.08 twips` | 拟合式不采用。四组读数是 **778 dpi 设备像素**：`ppem = round(pt×778/72)`、`asc = round(1950·ppem/2048) + 8`、`desc = round(550·ppem/2048) + 8`（Calibri win 度量）逐个复现 137/44、194/60、255/78、316/95；778 dpi 4/4，777 为 3/4，779 为 2/4。这是带 8 像素余量的显示度量，不是 twips 行高。778 即 Q15 视图换算 `w3 = (1440·rect + 389) / 778` 的常数。**作为线索可用**：移动视图按这个像素量字宽，对上了两处此前解释不了的窄路径读数，见 §3.4 |
| P0-2 断点最优选择 | 最优段落路径在设备上不执行（`0x212a54c` 287 次恒跳过，强开即 SIGSEGV） | 已测视图里 Word 不跑全局最优断行，与本引擎的贪心断行一致；无变化 |
| P0-3 表格列宽 | 公式只有静态读法，运行期早退（`w2 = 0`） | `tcw-*` 夹具没有 Word 页/行读数，无可对齐的行为；R07 照原计划 |
| P0-4 表格行步长 350 | 同一次注入内预算逐行减 350（内部单位） | 不是 twips 规则。报告自己指出阶梯页数不可复现（`talltable-052` 两次 3/1、`064` 1/4），只有 `talltable-080` = 4 页稳定；该夹具的行没有 `trHeight`（内容行高），引擎尚不支持 |
| P0-5 / P0-6 图片 | 环绕算式化简为 `0x3FFFFFFF − 上游`；`+0x24` 是占用；`dist42` 恒 0 | 哨兵算术，没有可用的环绕几何；无变化 |

结论：P0 轮的运行期读数都在引擎内部单位或内部状态上，没有一条可以直接写成 twips 排版规则。
可对齐的仍是此前的**行为读数**（断行起点、打印视图页数与页内行数）。P0 的结论只当线索：P0-1b 的
像素单位拿既有窄路径读数核过之后成为规则（§3.4）；P0-2 印证了引擎本来的贪心断行；其余几项要新的
Word 读数才能落地（§5）。

## 2. 行为读数扫描

把 `reports/rsword-diff/*.md`、`findings/pagination-path.md`、`findings/state-machines.md`、
`reports/diff/docgrid-decided.md` 里写明的 161 个 Word 读数逐个转录，用当前引擎重排比较：
打印视图用文档自身几何，窄路径用 `--view mobile --content-width 5329`，都带
`--platform android`。这些是报告级读数，没有逐份原始日志绑定，与验收面板的 report-only
同一等级，**不进严格分子**。脚本与结果在本机 `artifacts/word-analyse-p0-alignment-2026-10-04/`
（不提交）。

本轮之前 70 / 161 一致，fitText 与缩进两片之后 74 / 161，§3.5 之后 75 / 161，§3.6 之后 80 / 161，§3.7 之后 84 / 161：

| 族 | 读数 | 一致 | 不一致的原因 |
| --- | ---: | ---: | --- |
| exact 行距与段距（单侧、max 折叠） | 12 | 12 | — |
| 边距 / 页面尺寸 / 装订线 / 页眉距离 | 6 | 6 | — |
| 分节（含 oddPage / evenPage 不插空白页、continuous 换页宽） | 10 | 10 | — |
| 分栏 / 分栏符 / 段内分页符 | 9 | 9 | — |
| widow / keepNext / keepLines / 段前分页 | 6 | 6 | — |
| 隐藏文字 | 4 | 4 | — |
| **fitText** | 3 | **3** | 本轮实现（§3.1） |
| **缩进，窄路径** | 1 | **1** | 本轮实现（§3.3） |
| 缩进，纸页路径 | 22 | 0 | 这批读数取自移动视图状态（`wm size` 改视图宽后跟着变），乘上视图比例之后 22/22（§3.3）；真正的打印视图不缩（2026-10-04 补测第 9 条），这批不是打印视图读数 |
| **单字 / 字距** | 4 | **4** | `kern-off` 由 §3.5 改对 |
| breakme 纸页 | 1 | 0 | 原因未明（[迭代记录](ENGINE-ITERATION-2026-09-25.md)） |
| auto 行高 | 12 | 2 | 字体身份（§4） |
| atLeast | 4 | 3 | 同上（200 落到 auto 单倍） |
| docGrid | 27 | 6 | 网格未实现；且依赖 §4 的单倍行高 |
| **行单位段距 `beforeLines` / `afterLines`** | 5 | **5** | §3.6 |
| contextualSpacing | 10 | 0 | 未实现；旧夹具有混淆，规范夹具待测（[证据边界](CONTEXTUAL-SPACING-EVIDENCE-2026-09-27.md)） |
| 表格 | 17 | 9 | 6 份内容行高不支持；`table18-h839` 差在补的空段的行高（§4）；`talltable-080` 见 R09。表后空段由 §3.7 改对 4 份 |
| 脚注 | 8 | 4 | 脚注未排（R12） |

新增一致的 14 份全是本轮的改动；没有原本一致的读数变成不一致。

窄路径另有 32 个单行读数写在 `tab.md`、`char-scale.md`、`han-size.md`、`br-soft.md`、`vanish.md`、
`indent.md`、`kinsoku.md`、`mix-cjk.md` 里（另 186 行由 `android_replay.py` 回放）。fitText 与缩进那两片
之后是 29 / 32，§3.4 之后 32 / 32：`i-plain`、`zero-scale`、`latinscale` 这三份由 §3.4 改对。

## 3. 本轮实现

### 3.1 `w:fitText`

规则：带 `w:fitText` 的 run，文字合起来恰好占 `w:val` twips，整截**不可拆**——断行把它当一个
cluster：放得下整截放，有内容的行放不下就在它之前收行（交界可断就断在交界，否则退到更早的断点，
都没有就是紧急断行切在它前面），空行上宽于整行也整截收下。

依据：Android 纸页 10466，Calibri 12pt（`fittext.md`）。三份都由这一条加既有紧急断行规则复现：

| 夹具 | 形状 | Word | 本轮前 | 现在 |
| --- | --- | --- | --- | --- |
| fittext | 40 个 `0` 压到 2000，后接 80 个 `0` | 0、109 | 0、86 | 0、109 |
| fittext-wide | 拉到 8000 | 0、60 | 0、86 | 0、60 |
| fittext-over | 10 个 `0`，40 个 `0` 设成 12000，再 80 个 `0` | 0、10、50 | 0、86 | 0、10、50 |

实现：

- 钉住的解析器只建模单元格的 `w:tcFitText`；run 级的留在 `RunProps::raw_unmodeled`。
  `load.rs` 照读兼容项的办法直接读原 DOM，补进有效属性 JSON 的 `fitText`（只读 run 的直接属性）。
- `FontSpec::fit_text` 携带目标宽。度量提供者照旧量自然宽度；断行侧经
  `Engine::text_metrics` / `text_advance_pt` 把整截推进量换成目标宽，`fit_clusters_atomic`
  把整截当一个 cluster，`last_break_candidate` 与 `word_ahead` 不在截内找断点。
- 绘制把差额均摊在 cluster 之间（末 cluster 之后不摊），字形推进量合计恰为目标宽；
  终止符画的空格不并进这个片段，也不带目标宽。

假设与限制（都没有 Word 读数）：截内字形怎么摊；同一 `w:id` 的几个 run 按 UTF-16 长度分摊
总宽、区内 run 交界的断法；区里有制表符或对象占位符时整区不按 fitText 排；字符样式里的
`w:fitText` 看不到。所有平台都生效（OOXML 语义，Mac 采集里没有 fitText）。

### 3.2 字符单位缩进

规则：`startChars` / `endChars` / `firstLineChars` / `hangingChars`（1/100 字符）非零时优先于
同一边的 twips 值；一个字符是 1 em。依据（`indent.md`）：`left=720` + `leftChars=100`、
`firstLine=720` + `firstLineChars=100` 都按字符单位排；`leftChars` / `rightChars` 每行都缩、
`firstLineChars` 只缩首行；八档 `firstLineChars`（100～8400）把每百份收到 122.0–122.6 twips，
等于 12pt 的 240 twips 乘下节的视图比例。

假设：写成 0 退回 twips（Word 用它清掉样式继承来的字符缩进）；悬挂一侧优先于首行一侧（沿用
原来 `hanging` 胜 `firstLine`）；一个字符取**第一个可见 run 的字号**（空段落取按段落标记属性补的
那个 run）。最后一条偏向数据：夹具的段落标记没写字号，若它按 §4 推断的 11pt 缺省排，按标记取字号
就是 220、与读数不符。标记与正文字号不同的夹具没测过。

### 3.3 移动视图的缩进缩放

规则：移动视图（`View::Mobile`）按「视图版心宽 / 文档声明的版心宽」缩正文段落的缩进
（twips 与字符单位都缩）。比例取页面覆盖之前与之后的版心宽（`LayoutDocument::mobile_indent_scale`），
没有覆盖时为 1；打印视图不缩。

依据（`indent.md`）：窄路径 `ind-left`（左 720）每行 40 个 `0`，不缩是 37；同期纸页路径五档左缩进
（720 / 1000 / 1440 / 5000 / 9600）的有效比例共同落在 0.507–0.519，5329 / 10466 = 0.509；
`wm size 800x1600` 后同一份变成每行 84 个。纸页路径的缩进随视图宽变，说明那些读数来自依赖视图的
排版；它们是在移动视图状态下取的（日志没有记视图状态，由下文的打印视图补测反证）。

离线核对：把比例 5329 / 10466 套到纸页路径重排，`indent.md` 的 22 份纸页读数（五档左缩进、
左右各缩、首行、悬挂、字符单位八档、`*Chars` 与 twips 混写）**逐行全部与 Word 相同**；比例换成
3280 / 10466（800 宽视图里重复出现的宽度）时 `ind-left` 给 84，也与 Word 相同。真正的打印视图下
Word 不缩（2026-10-04 补测：按打印视图截图读 `ind-left` 每行 80 个 `0`，照比例缩会是 83），与引擎的打印视图
一致；这 22 份是移动视图状态下的读数，在打印视图比较里仍记为不一致。

限制：表格单元格里的段落不缩（移动视图下表格怎么排没测）；制表位不缩（自定义制表位不随视图
缩放是实测，`tab.md`）。

### 3.4 移动视图按设备像素量字宽；Android 行尾空格不计宽

规则一：Android 移动视图（`--platform android --view mobile`）的字宽按每英寸 778 个设备像素量：
`ppem = round(字号 × 778 / 72)`，每个字形 `px = round(推进量 / em × ppem)`；`w:w` 缩放取
`px × pct / 100` 的整数截断；字符间距取 `round(间距 × 778 / 1440)` 像素加在 cluster 末尾。
断行（`HorizontalGrid::DevicePixels`）与绘制（`PixelShaper`）同一口径。打印视图与桌面不量化。

依据，三条各自独立：

- Q15（动态）：窄路径版心 `w3 = (1440 × rect + 389) / 778`，5329 twips 就是 2879 个每英寸 778 的像素；
- P0-1b（动态）：同一设备上四组 run 度量逐个等于上式的 ppem 与像素取整（另加 8 像素余量），
  换成每英寸 777 或 779 都对不全；
- 窄路径的单字读数：`i-plain` 每行 95 个（精确宽度给 96；`i` 30 px → 95）、`zero-scale`（`w:w=80`）
  每行 55 个（精确比例给 54；`0` 66 px × 80% 截断成 52 → 55，四舍五入成 53 则给 54）。其余单字读数
  （`0` 43、`M` 25、`A` 38、`w:w` 50/55/90、`M` 的 80%、12pt 与 24pt 的「汉」）两种量法给同一个数。

纸页路径不按这个量：`zero-paper` 每行 86 个 `0`、无缩进 190 个 `i`，按 778 dpi 的像素宽都放不下
（85、188）。

规则二：Android 上行尾空格不计入放不放得下（`FontMetrics::fit_hanging_spaces`），挂在行宽之外。
依据：`latinscale`（窄路径，`w:w=80`）第三行 [130, 197) 按像素去掉行尾空格是 2874 px、带上是 2897 px，
行宽 2879；Word 收到 197。精确宽度下这一行不论空格算不算都是 5350 twips，放不下。所以这一行同时
要两条规则。纸页路径没有能区分的读数（`breakme` 纸页改前改后都不对，§5），断行代码两条路径共用，
所以按平台生效。桌面没有能区分的 Mac 读数：试验时全平台开启，25 份 Mac 轨迹逐字节不变，桌面照旧计入空格。

假设与限制：

- 每英寸 778 是 word_analyse 那一台设备（b0e3d198）的常数，别的设备未测；
- 像素宽按 `round(推进量 × ppem / upem)` 取，没有执行字体的 hinting 程序（Calibri 带 hinting、没有
  `hdmx` 表），hinting 之后的宽度有没有差别未测；
- `w:w` 截断只有 `zero-scale` 一份能分开截断与四舍五入；
- 中西文自动间距（1/4 em）与制表位不落到像素上。制表符的 8 个读数照样一致，但 `tab-i` 恰好卡在
  行宽上（221 + 92 个 `i` 的 5108 = 5329）；默认档 221 仍是拟合值。

### 3.5 Android 不看 `w:kern`，总做字距调整

规则：Android 上整形总开字体的字距调整（GPOS `kern`），不论 `w:kern` 写没写、阈值多少；断行与绘制
同一口径（`RealMetrics::with_kerning_always`、`PaintShaper`）。桌面照旧按 `w:kern` 的阈值。

依据：`kern-off`（Calibri 12pt 的 `AVAV…`，不写 `w:kern`，纸页 10466）与写了 `w:kern w:val="2"` 的
`kern-on` 同为每行 82 个字母。Calibri 的 GPOS 在 A→V（−89 字体单位）与 V→A（−96，类对表）两个方向
都调：82 个字母宽 10399 twips、84 个宽 10652，正好给 82；不调是 76。`kern.md` 说读到的 PairPos 对不上，
是只看了一个方向。Mac Word 不写 `w:kern` 不调（`B11` 的两个 `1`，[量到的结论](ENGINE-GAPS-MEASURED.md)
W-3），所以这是平台差异。

范围：186 行窄路径与 32 个窄路径单行读数在开启后都不变（以 Latin 文字为主的 `latinwrap`、`justify`、
`latinspace` 的窄路径断点没有一处被字距改动）；打印视图 161 个读数里只有 `kern-off` 变成一致。

假设：写了大于字号的 `w:kern` 阈值时 Android 调不调未测，这里一律调；只量过 Latin 的 `AV`。

### 3.6 `w:beforeLines` / `w:afterLines`

规则：`beforeLines` / `afterLines`（1/100 行）非零时优先于同侧的 `before` / `after`；一行在节的
`w:docGrid` 是 `lines` / `linesAndChars` 且 `linePitch > 0` 时取 `linePitch`，否则取 240 twips，**不随字号**。
换算 `round(值 × 一行 / 100)`，在 `Engine::layout_document` 里按段落所在节做，之后与 twips 段距同一套
（页顶段前照算、页末段后不计、相邻段 max 折叠）。

依据（Android 打印视图，精确行距，版心 15398）：

| 夹具 | 写法 | Word | 本轮前 | 现在 |
| --- | --- | ---: | ---: | ---: |
| before-lines | `beforeLines=100`，行距 480 | 21 | 不一致 | 21 |
| before-lines-50 | `beforeLines=50` | 25 | 不一致 | 25 |
| before-both | `before=240` + `beforeLines=50`，行距 360 | 32 | 不一致 | 32 |
| after-both | `after=240` + `afterLines=50` | 32 | 不一致 | 32 |
| after-lines | `afterLines=100`，行距 360 | 26 | 不一致 | 26 |
| before-lines-sz48-n（2026-10-04 补测） | 同 before-lines，`w:sz=48` | 21 | — | 21 |
| before-lines-grid312-n（2026-10-04 补测） | 同 before-lines，`docGrid lines 312` | 19 | — | 19 |

两份补测是采集前写下的判定（补测第 4 条）：按字号会给 24pt 那份更少的行、按固定 240 给 21；有网格时按
240 给 21、按 `linePitch` 给 19。Word 是 21 与 19。

所有平台都生效：这是 OOXML 语义，桌面 Word 的「行」也是同一定义；Mac 采集里没有行单位段距，25 份轨迹不变。

假设：写成 0 退回 twips（与字符单位缩进同一口径，没有读数）；样式里继承来的行单位照样读（经有效属性，
没有单独读数）；`snapToChars` / `default` 网格按无网格算。

### 3.7 正文以表格结尾时补的空段

规则：正文最后一个块是表格（之后只有节属性）时，排版在最后一节末尾接一个空段：缺省段落样式、没有
直接属性，段落标记照缺省字符属性。源文档不改——`LayoutDocument::paras` 不含它，放在
`implied_final_para`，只在 `Engine::layout_document` 里接上；装载诊断照留（措辞改为说明排版补了一段）。
它在结果里是一行空行，源区间紧接表格之后的一个单位，终止符是分节符。

依据（Android 打印视图，版心 15398，无边框单列表，精确行高；`reports/rsword-diff/table*.md`）：

| 夹具 | 表格高 | 剩余 | Word | 本轮前 | 现在 |
| --- | ---: | ---: | ---: | ---: | ---: |
| table30-h500 / h502 / h503 | 15000 / 15060 / 15090 | 398 / 338 / 308 | 1 | 1 | 1 |
| table34-h444 | 15096 | 302 | 1 | 1 | 1 |
| table30-h504 / h506 / h510 | 15120 / 15180 / 15300 | 278 / 218 / 98 | 2 | 1 | 2 |
| table32-rowh | — | — | 2 | 1 | 2 |
| table18-h839 | 15102 | 296 | 2 | 1 | 1 |
| table32-tail（自带尾段） | — | — | 1 | 1 | 1 |

1 页里最小的剩余 302、2 页里最大的剩余 296，夹出空段高在 (296, 302] 里。等线 11pt 的 auto 单倍行高
1.3546 × 220 = 298（§4）落在区间里；引擎现在按 Calibri 12pt 量是 293，所以 `h839` 还差这一点，等缺省字体那一片。
2026-10-04 决定在排版层模拟（北极星规划原来是「不在入口修复」，入口仍不改）。

所有平台都生效：这是 Word 打开文档时的修复，不是 Android 的排版规则；Mac 采集里没有以表格结尾的文档。

假设：中间节不会以表格结尾（节属性在段落里），所以只看正文末尾；表格后面紧跟另一种不支持的块时
不补（诊断照旧）。补的这一段的样式取缺省段落样式，没有用带样式的文档核过。

### 3.8 尚未落地的 P0 线索

| 线索 | 要变成规则还缺什么 |
| --- | --- |
| P0-1b 的 +8 像素余量 | 移动视图的行距按像素带排（`pagination-path.md` 的 105/106 像素带），但移动视图没有页，没有可比的行高读数；不影响断行 |
| P0-3 余量均分的列宽公式 | `tcw-*` 夹具在打印视图的页数或窄路径行起点；先有多格表格（R07）才用得上 |
| P0-4 每行 350 | 读数单位不明；需要 `talltable-*` 稳定的打印视图页数（报告说阶梯页数不可复现）与内容行高的表格实现（R09） |
| P0-5 / P0-6 环绕 | 图片夹具（`iw*`）的行起点或页数；目前只有探针日志 |

## 4. 纵向：缺的是字体身份，不是 Calibri 的公式

> **2026-10-04 补测结论**（手机 Word，打印视图截图读数）：缺省字体是 **等线（DengXian，云字体，
> `DengXian-54497409372.ttf`）11pt**。`longpage-auto12pt` 页 0 是 **47 行**（预测：缺省 11pt 47、跳档 51）；
> 进程映射里无字体夹具打开时加载的是等线；无字体的一串数字窄路径每行 45、打印视图 90，与等线 11pt
> 的字宽相符。等线 hhea (asc+desc+gap)/upem = 1.0420，乘 1.3 = 1.3546 em，正落在第 1、2 条的区间里：
> Android 的 auto 单倍行高对 CJK 代码页字体（OS/2 `ulCodePageRange` 第 17–21 位）用的是 Mac 已知的
> 「N7」×1.3 规则。只有等线（GB 代码页）一个字体的读数，其他代码页未测。下文是采集前的推理，保留原样。

auto 行高、atLeast、docGrid、表后空段、脚注这批打印视图夹具**都没有声明字体**：没有 `w:rFonts`、
没有 `styles.xml`，字号阶梯只写 `w:sz`（`longpage-auto125` 等）。手机 Word 用自己的缺省字体排它们；
引擎把没写的字号补成 12pt，又只装了 Calibri，于是按 Calibri 12pt 量。此前的报告与
[docGrid 页容量](DOCGRID-PAGE-FIT-2026-09-27.md)都把这批读数当成 Calibri 12pt 的行高去拟合。

三组互相独立的读数指向同一个缺省：**11pt、单倍行高约 1.351–1.354 em 的字体**（不是 Calibri 的
1.2207 em）：

1. 字号阶梯（12.5 / 13 / 13.5 / 14 / 15 / 16 / 18 / 24pt，同一缺省字体）的页内行数要求单倍行高
   在 1.3507–1.3578 em 之间，各档共用一个比例；
2. 没写字号的那份配 docGrid 时，pitch 297 加倍、298 不加倍，单倍行高落在 (297, 298] twips：
   按 11pt 是 (1.3500, 1.3545] em，与第 1 条重叠；按 12pt 是 1.24 em，与第 1 条对不上，
   就得假设 12pt 与 12.5pt 之间换了字体（word_analyse 原来的「12.5pt 起跳档」）；
3. P0-1b 在这批夹具上读到的那组不随字号变的度量（asc 123 / desc 55，即缺省字号的段落标记），
   照 §1 的像素公式去掉余量，按 11pt（ppem 119）是 1.353–1.370 em，按 12pt 是 1.24–1.25 em。

照这个假设，`lines = 1 + floor((15398 − B) / 步长)`（末行要 B，不是整步长，见既有 docGrid 分析）
同时解释单倍 51、1.5 倍 34、2 倍 26（单纯 `n × 步长` 给 25）、3 倍 17、atLeast 200 = 51、
表后空段 (296, 302] 与 docGrid 21 点。它仍是**假设**：字体是哪一个不知道（本机核过的候选都不在
区间里：Calibri 与 Aptos 的 hhea 1.2207 em、Aptos win 1.2847、Microsoft YaHei 1.3198、Noto Sans
1.3620、Noto Sans CJK 1.4480，手机系统字体 MiSans 没有文件可核）。在拿到字体之前不往 Calibri 或
任何字体里加系数，与 R04 的停止规则一致。

能一次判定的采集：word_analyse 已有的 `fixtures/longpage-auto12pt.docx`（显式 `w:sz=24`，
不写字体）读打印视图页 0 行数——缺省 11pt 的假设预测 47 行，「12pt 起点、12.5pt 跳档」预测 51 行。
另需从手机进程映射或 Word 安装包确认缺省字体文件，再用它重放这批夹具。

## 5. 仍不一致、需要决定的

| 项 | 读数 | 现状与建议 |
| --- | --- | --- |
| 表后补的空段 | `table32-rowh`、`table30-h504/506/510`、`table18-h839` | Word 在正文以表格结尾时补一个缺省空段；北极星规划决定「保留缺尾段诊断，不在入口偷偷修复」。在排版层模拟这一段可以改对其中 4 份（`h839` 只剩 296 twips，还要 §4 的行高）。2026-10-04 决定在排版层模拟，已实现（§3.7），改对 4 份；`h839` 等 §4 |
| contextualSpacing | 10 份 | 规范夹具已生成，待 Word 采集 |
| 内容行高表格 | 6 份 + `talltable-080` | R09 |
| 脚注 | 4 份 | R12 |
| docGrid | 21 份 | 依赖 §4（缺省字体已确认为等线 11pt，待实现） |
| breakme 纸页 | 1 份 | 第一行 Word 收 102 个字。去掉行尾空格、开字距也要 10489.5 twips，比 10466 宽 23.5 twips，引擎怎么量都放不下；加一个行宽容差就得拟合常数。需要纸页读数分开「行宽有容差」与「纸页字宽更窄」：例如末词分别超出 5、15、30 twips 的三份纸页夹具（已知容差小于 54：`i` 每行 190 个） |

`d7-gaps-repro.md` 记的「`sect-cont-fit` 短内容加 continuous 是 1 页」不成立：那份日志的视图宽全是
5329（移动视图，页数恒为 1），`android_pages.py` 判为 `TERMINAL_WIDTH_CONFLICTS_WITH_LEGACY_PRINT_RECIPE`。
打印视图的有效读数（`sect-cont`、两份阶梯）都是 2 页：第一节写 continuous、末节省略类型即
nextPage，换页的是第二节自己的类型。引擎对 `sect-cont-fit` 排 2 页，与这一解释一致。

### 补测队列

按能一次判定的顺序排。预测在采集之前写下，不在采集后改。2026-10-04 用手机（b0e3d198）做了第 1、2、3、4、9 条，
结果写在表后。

| # | 采集 | 判定的问题 | 预测 |
| --- | --- | --- | --- |
| 1 | `longpage-auto12pt.docx` 打印视图页 0 行数 | 无字体夹具的缺省字号（§4） | 缺省 11pt 的假设 47 行；「12pt 起点、12.5pt 跳档」51 行 |
| 2 | 打开无字体夹具时 Word 进程映射里的字体文件 | 缺省字体是哪一个（§4） | 单倍行高 1.351–1.3545 em 的字体 |
| 3 | 窄路径一串 `i`：9.5pt、11.5pt、13.5pt；一串 `0`：14.5pt；一串 `M`：12.5pt（同 `i-plain` 写法，Calibri，`w:sz` 各异） | 移动视图的像素字宽（§3.4） | 像素：119、102、84、35、25；精确宽度：122、100、86、36、24 |
| 4 | `before-lines` 改写 `w:sz=48`（24pt）一份、加 `w:docGrid linePitch=312` 一份 | 行单位段距的「一行」 | 固定 240：两份都按 240；按字号：前一份 480；按网格：后一份 312 |
| 5 | 纸页三份：一行末词分别超出行宽 5、15、30 twips | breakme 纸页那 23.5 twips（§5） | 没有容差：三份都换行 |
| 6 | `kern-on` 改成 `w:kern w:val="48"`（阈值 24pt，大于 12pt） | Android 是否完全不看 `w:kern`（§3.5） | 照本实现：仍每行 82 个；照阈值：76 个 |
| 7 | 纸页一行末词只在去掉行尾空格时放得下的夹具 | 行尾空格在打印视图是否同样挂出（§3.4） | 照本实现：末词留在本行 |
| 8 | `fixtures/contextual-spacing-canonical-2026-09-27/` 六份 | contextualSpacing 的规则（[证据边界](CONTEXTUAL-SPACING-EVIDENCE-2026-09-27.md)） | 见该目录的 `hypotheses.json` |
| 9 | 按打印视图配方（切到打印视图并滚动，`w3=0x28e2`）读 `ind-left` 纸页行起点 | 真正的打印视图缩不缩缩进（§3.3） | 照本实现不缩：每行 80 个 `0`；照视图比例缩：83 个 |

2026-10-04 结果（读法：注入探针会在切视图时让 Word 崩溃，改为打印视图截图逐行数；对照
`longpage-grid297` 读到 26 行，与既有读数一致）：

| # | 结果 | 判定 |
| --- | --- | --- |
| 1 | 47 行 | 缺省 11pt 成立（§4） |
| 2 | 等线 `DengXian-54497409372.ttf`（云字体） | 缺省字体是等线（§4） |
| 3 | 119、102、84、35、25 | 五份全按像素（§3.4），精确宽度一份也不对 |
| 4 | 24pt 那份 21 行，网格那份 19 行 | 无网格固定 240、有网格取 `linePitch`（§3.6） |
| 9 | 每行 80 个 `0` | 真正的打印视图不缩缩进；`indent.md` 的纸页读数取自移动视图状态（§3.3） |

第 5–8 条待测。

## 6. 验证

- 新增 `crates/core/tests/fit_text.rs`（12 项）与 `crates/core/tests/indent_chars.rs`（5 项）：
  桩度量钉规则，真实 DOCX 字节钉装载，fontenv 下用仓库 Liberation Sans 钉绘制。逐条撤掉新规则
  （fitText 不原子、退回落进 fitText、移动视图不缩、字符单位不优先）各有测试变红。
- 工作区 `cargo test --workspace --features rsword-layout-core/fontenv`：792 passed / 0 failed /
  14 ignored；`cargo clippy --workspace --all-targets --features rsword-layout-core/fontenv
  -- -D warnings` 通过。
- Android 窄路径回放：186 / 186（条件）。
- Mac 采集回放（`tools/measure/sweep.py`）：新旧二进制的 25 份轨迹逐字节相同，状态仍是
  23 FAIL / 7 UNDECIDABLE / 0 OK。
- word_analyse 全部 515 份夹具 × 三种模式（Android 打印、Android 移动 5329、桌面打印）新旧对照：
  1545 次里 61 次变化，全是 fitText 三份、字符单位缩进夹具，以及移动视图里带缩进的段落
  （含 `spacing` 夹具里一段 `left=1417`），没有别的输入变化。
- 对照用的二进制（debug，fontenv）：新 `9e7c630d…e6dda03bea`，基线 `590e79d8…88817ca5`。

§3.4 一片另行核对（二进制 `fa654c4e…7920ee3cb`）：

- 新增 `crates/core/tests/android_device_pixels.rs`（6 项：行尾空格挂出、拆 run 不变、像素宽、
  缩放截断与间距、只有 Android 移动视图按像素断行、绘制与断行同一像素）。逐条撤掉（空格不挂、
  移动视图不量化、绘制不量化、缩放改四舍五入）各有测试变红。`tab_stops.rs` 的制表符不变式在
  Android 上只量片段去掉行尾空格的部分（挂出的空格本来就在行尾之外）。
- 工作区 798 passed / 0 failed / 14 ignored；core 不带 fontenv 634 passed；Clippy 通过。
- Android 窄路径 186 / 186；窄路径单行读数 32 / 32；打印视图 161 个读数仍 74，判定一个不变；
  Mac 25 份轨迹逐字节相同（桌面的轨迹 `metrics` 栏也不变）。
- 全部夹具三种模式对照（不计 `metrics` 栏）：桌面 0 变化；Android 打印 6 份断行变化（行尾空格，
  只有 breakme 有纸页读数，改前改后都不对）；Android 移动 475 份变化，其中 440 份只有字形位置，
  35 份断行变化——有窄路径读数的都变得与 Word 一致。

§3.5 一片另行核对（二进制 `bace1b59…ed1a6f45fb`）：

- 新增 `crates/core/tests/android_kerning.rs`（3 项：Android 不写 `w:kern` 也调、桌面照 `w:kern`、
  绘制与断行同一字距）；撤掉 Android 的字距、或只在绘制里撤掉，各有测试变红。
- 工作区 801 passed / 0 failed / 14 ignored；core 不带 fontenv 634 passed；Clippy 通过。
- Android 窄路径 186 / 186；窄路径单行读数 32 / 32；打印视图 75 / 161（只有 `kern-off` 变）；
  Mac 25 份轨迹逐字节相同。
- 全部夹具对照（相对上一片，不计 `metrics` 栏）：桌面 0 变化；Android 打印 269 份只变字形位置、
  4 份断行变化（`kern-off` 与 Word 一致；`latinwrap`、`justify`、`latinscale` 的纸页没有读数）；
  Android 移动 266 份只变字形位置、2 份断行变化（`kern-off`、`wrap_mixed`，都没有窄路径读数）。
  之后只改了注释与文档，最终二进制 `93353c65…37cd30e86` 与前者在上述 1545 次运行上逐字节相同。

§3.6 一片另行核对（二进制 `27ce0b34…d5d80adf`）：

- 新增 `crates/core/tests/spacing_lines.rs`（4 项，真实 DOCX 字节、精确行距，结果不依赖字体度量）。
- 工作区测试全部通过；Clippy（带与不带 fontenv）通过。
- Android 窄路径 186 / 186；窄路径单行读数 32 / 32；打印视图 80 / 161（只有行单位的 5 份变）；
  Mac 25 份轨迹逐字节相同。
- 全部夹具三种模式对照：1545 次里 15 次变化，正是这 5 份夹具 × 三种模式（只有纵向位置变）。

§3.7 一片另行核对（二进制 `ad1e7e41…1a7960f`）：

- 新增 `crates/core/tests/table_tail.rs`（3 项：503 一页、504 两页，自带尾段不再补，源段落不改）；撤掉接段
  测试变红。`session.rs` 里钉住「不补尾段」的两项按新决定改写（表后多一行，源区间 [20, 21)）。
- 工作区 808 passed / 0 failed / 14 ignored；Clippy（带与不带 fontenv）通过。
- Android 窄路径 186 / 186；窄路径单行读数 32 / 32；打印视图 84 / 161（只有 4 份表格变）；
  Mac 25 份轨迹逐字节相同。
- 全部夹具三种模式对照：1545 次里 27 次变化，正是 9 份以表格结尾的夹具 × 三种模式。

## 7. 复现

```sh
cargo build --offline --features fontenv --bin layout-trace
python3 artifacts/word-analyse-p0-alignment-2026-10-04/sweep.py target/debug/layout-trace /tmp/sweep-out
```

`sweep.py` 的字体路径写死在脚本开头，换机器要改。窄路径的 32 个单行读数用同目录的
`sweep_narrow.py`（参数：二进制、输出目录）。全量夹具对照用 `scan.py`
（参数：旧二进制、新二进制、输出目录）。
