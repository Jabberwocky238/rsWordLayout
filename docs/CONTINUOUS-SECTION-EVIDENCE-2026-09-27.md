# 连续分节与栏平衡：旧证据定向审计

审计日期：2026-09-27。只读取 `../word_analyse` 的现存报告、DOCX ZIP、
原始日志和保存的代码，没有新运行 Word、重新采样或修改该项目。

**结论：这些 Android/PTS 资料不能确定连续后继的栏平衡触发条件。**
不能据此决定“仅改变栏数才平衡”、同栏数连续节是否合并平衡，或硬栏断、
keep 对平衡的具体约束。旧报告确实保存了一些页数读数，但部分因果解释
与当前 ZIP 中的节属性归属不一致，不能直接成为算法验收规则。

## 证据等级

| 等级 | 本文含义 |
| --- | --- |
| 输入核验 | 直接读取现存 ZIP 的 XML；证明当前文件写了什么 |
| 原始动态记录 | 现存运行日志可以复读计数、宽度和事件顺序；不自动证明输入身份或原因 |
| 动态报告 | 报告记录了运行观察，但本次未定位到对应原始采集文件 |
| 静态代码/索引 | 保存的代码或字符串交叉引用；不能代替目标平台的动态行为 |
| 未知 | 现有核查范围不能回答，未用名称相似或经验补全 |

这里的 ZIP 核验并不补上旧日志缺少的同次采集输入哈希绑定。下列文件名
建立了报告关联；本次没有声称它们具备新 Mac 采集那样完整的身份链。

## 连续属性写在了哪一节

段落 `pPr/sectPr` 描述由该段结束的节，body 末尾 `sectPr` 描述最后一节。
因此第一节末尾的 `type=continuous` 不能当作后继节显式声明了 continuous。
这与当前解析投影使用的节归属一致，参见
[真实 DOCX 节归属回归](../crates/core/tests/grid_input.rs:219)。

本次直接读取以下 ZIP 的 `word/document.xml`：

| `word_analyse/fixtures/` 文件 | 第一节的终止 `pPr/sectPr` | body 末尾第二节 `sectPr` | 栏设置 |
| --- | --- | --- | --- |
| `sect-cont.docx` | `type=continuous` | 未写 type | 两节都未写 cols |
| `sect-cont-fit.docx` | `type=continuous` | 未写 type | 两节都未写 cols |
| `sect-cont-cols.docx` | `type=continuous` | 未写 type | 第一节未写 cols；第二节 num=2、space=720 |
| `sect-cont-ladder-10.docx` | `type=continuous` | 未写 type | 两节都未写 cols |
| `sect-cont-ladder-21.docx` | `type=continuous` | 未写 type | 两节都未写 cols |
| `sect-cont-ladder-22.docx` | `type=continuous` | 未写 type | 两节都未写 cols |

生成器也直接证明这个结构：
[make_sectcont_ladder.py:41](../../word_analyse/tools/make_sectcont_ladder.py:41)
的 `SECT_FINAL` 没有 type，
[同文件:52](../../word_analyse/tools/make_sectcont_ladder.py:52)
的 `mid_sect()` 把 continuous 放入前节；
[make_repro.py:72](../../word_analyse/tools/make_repro.py:72)
中的短内容对照使用相同方式。

所以不能用这些文件证明“后继 continuous 本身强制分页”，也不能用
`sect-cont-cols` 证明“连续后继改变栏数仍强制分页”。后继未写 type 在
特定客户端如何处理，仍须与该次真实输入、有效属性及视图状态共同核实；
不能把当前审计改写成未经采集的相反布局结论。

## 阶梯报告的高度问题

[sectcont-threshold-fixtures.md:21](../../word_analyse/findings/rules/sectcont-threshold-fixtures.md:21)
声称每行 exact 480，且 `16838 - 1440 - 1440 = 15398`。
实际 ZIP 的上下边距均为 1440，所以版心高是 **13958**；15398 对应的是
上下各 720。生成器
[make_sectcont_ladder.py:38](../../word_analyse/tools/make_sectcont_ladder.py:38)
也把常量写成了 15398。

此外，`mid_sect()` 的 `mid` 段仅有 sectPr，没有 `w:spacing`。生成器却在
总高度中将它计为额外一个 480 行。相邻正文段声明 exact 480 不等于该段
也声明了 exact 480；因此报告的 10080、15360 等总高度不是完全由输入
锁定的值。报告所称“-21 贴边、-22 越界”不成立，不能据此收敛阈值。

报告
[动态解释:58](../../word_analyse/findings/rules/sectcont-threshold-fixtures.md:58)
从两个 2 页观察推到“continuous 自身产生一个分页”，同时受以上节归属
和高度前提问题影响。本文保留页数观察，拒绝采用该因果解释。

## 现存原始日志实际证明什么

本次逐条提取 PGIDX 的 count，按日志顺序压缩连续相同值，并只统计
`ENTER` 的 w3 宽度，结果如下。

| 原始文件 | 行数 | PGIDX count 序列 | 纸页宽 `w3=2342` 的 ENTER 数 | 可保留的观察 |
| --- | ---: | --- | ---: | --- |
| [print/sect-cont-ladder-10.print.live.log](../../word_analyse/reports/diff/print/sect-cont-ladder-10.print.live.log) | 1927 | 1 连续 26 次，随后 2 连续 54 次 | 221 | 后段页数组读数为 2，与报告计数一致 |
| [print/sect-cont-ladder-21.print.live.log](../../word_analyse/reports/diff/print/sect-cont-ladder-21.print.live.log) | 2469 | 1 连续 56 次，随后 2 连续 24 次 | 283 | 后段页数组读数为 2，与报告计数一致 |
| [sect-cont-fit-word-20260925.log](../../word_analyse/reports/diff/sect-cont-fit-word-20260925.log) | 206 | 1 连续 4 次 | 5 | 存在 1 页读数，但不能由此证明打印布局已稳定 |
| [qb-sectcont-mobile-20260925.log](../../word_analyse/reports/diff/qb-sectcont-mobile-20260925.log) | 10 | 无 PGIDX | 0 | 只有 hook 初始化及一条 QHOOK，不是分页结果 |

两份 ladder 日志均恰好包含 **80 条 PGIDX**，最后序号为 `n=79`，已达到
原探针 `n < 80` 的采样上限。其末尾 `count=2` 只代表最后记录值，不能证明
其后没有继续变化，也不能认证最终页数或分页稳定。上述计数是对原始读数
的保留，不是最终页数验收；这与
[Android 页数证据的采样上限口径](ANDROID-PAGE-EVIDENCE-2026-09-27.md:32)
一致。

`sect-cont-fit` 的四条 PGIDX 在日志第 43、49、122、124 行；首次纸页宽度
CALLER/ENTER 出现在第 156/157 行，后面没有新的 PGIDX。因此
[d7-gaps-repro.md:28](../../word_analyse/reports/diff/d7-gaps-repro.md:28)
的“打印视图内容放得下，所以 continuous 不换页”超出了这份日志单独能
确认的范围。纸页 FormatLine 调用本身也不证明 UI 已完成切换与分页稳定。

上述日志没有提供连续多栏节各栏的源 CP 分配或可见坐标标签，所以即便
接受页数组读数，也不能回答 20+20 与 32+8 等栏内分配问题。

同时存在 `reports/diff/dyn-20260926/sect-cont-ladder-{10,21,22}.live.log`
和 `reports/diff/dyn-20260927/sect-cont-ladder-22.live.log`。它们未在本次
审计中提升为新的打印栏平衡证据；不能仅凭文件名或混合视图 count 使用。

## 分栏、硬栏断与 keep

[pagination-path.md:89](../../word_analyse/findings/pagination-path.md:89)
和 [columns.md:5](../../word_analyse/reports/rsword-diff/columns.md:5)
报告了栏宽 4873、双栏 64 行为 1 页、65/70 行为 2 页。
这些支持双栏容量的历史观察，但 **64 行满两栏的页数无法区分是否执行平衡**。
40 行为 1 页同样不能区分 20+20 和 32+8。

[br-column.md:5](../../word_analyse/reports/rsword-diff/br-column.md:5)
报告双栏硬栏断产生同页两组 x 坐标，
[同文件:9](../../word_analyse/reports/rsword-diff/br-column.md:9)
报告单栏硬栏断翻页。没有连续节平衡与硬栏断组合的对照，故不能推导
“硬栏断禁用平衡”或“分段独立平衡”等规则。

[keep-next.md](../../word_analyse/reports/rsword-diff/keep-next.md) 与
[keep-lines.md](../../word_analyse/reports/rsword-diff/keep-lines.md)
报告的是单栏分页中的 CP1054 行为，没有平衡候选高度、连续节或多栏
keep 链对照。不能据此确定 trial 平衡时 keep 是否退化或回退到哪个高度。

以上 columns/br-column/keep 报告未指向可核对的原始日志路径；本次在
项目内的定向文件名、报告引用检索未定位到对应采集日志。这里保留为
动态报告级证据，不宣称整个项目绝对不存在匿名或另名日志。

## 静态线索与边界

原生 Android 字符串索引确实存在：

| 来源 | 内容 | 能证明的范围 |
| --- | --- | --- |
| [string-xrefs.tsv:2800](../../word_analyse/reports/decomp-index/string-xrefs.tsv:2800) | `CF_OnOpen_NoColumnBalance`，xref `0xe20170`；下一行 OnClose | 兼容开关名字和引用位置存在 |
| [string-xrefs.tsv:2900](../../word_analyse/reports/decomp-index/string-xrefs.tsv:2900) | `CF_OnOpen_CachedColBalance`，xref `0xe2100c`；2902 行 OnClose | 同上 |
| [compat-flags.md:3](../../word_analyse/findings/rules/compat-flags.md:3) | 自述为静态名字表，未验证；69 行将其列为 continuous 候选 | 不证明布局读取或开关因果 |
| [macos-word-layout.md:154](../../word_analyse/findings/macos-word-layout.md:154) | `FsGetSubpageColumnBalancingInfo` 等名字；明确未知栏平衡数值规则 | PTS 接口线索，未给出触发分支实现 |

另发现本地保存的 **Word Web JavaScript** 中有实际平衡实现，属于独立的
静态参考，不能当作 Android/Mac PTS 已验证行为：

- [原脚本:6072](../../word_analyse/tools/cdp/out/word-scripts/39__wordeditordsclosurebundle.js:6072)
  的 `HLb.I9` 先用完整高度排，再在 `clientMode != 1`、本节栏数大于 1、
  本节排完、`!lastSection`、无指定 exitReason 时执行候选高度重排。
- [原脚本:2771](../../word_analyse/tools/cdp/out/word-scripts/39__wordeditordsclosurebundle.js:2771)
  把 `coc` 导出为 `lastSection`；
  [原脚本:2862](../../word_analyse/tools/cdp/out/word-scripts/39__wordeditordsclosurebundle.js:2862)
  从后继节 `F.the` 计算它，无后继时为 true。已读分支没有比较前后栏数。
  但这不证明模型构建之前未合并节，也不确认原生端使用相同策略。
- [原脚本:2848](../../word_analyse/tools/cdp/out/word-scripts/39__wordeditordsclosurebundle.js:2848)
  的 `wNb` 检查子栏 `exitReason === 2`；本次没有独立完成该枚举与原生
  硬栏断的映射，所以不将它写成硬栏断结论。
- [原脚本:6073](../../word_analyse/tools/cdp/out/word-scripts/39__wordeditordsclosurebundle.js:6073)
  使用 16 个内部单位的步长、倍增找界和二分候选高度。
  单位及该版本的适用平台未经本次动态验证，不能直接作为 twips 常数。

对应可读化文件在
`tools/cdp/out/pretty/39__wordeditordsclosurebundle.js`：平衡入口 144927，
`wNb` 50970，后继 `lastSection` 51163，节属性读取 146342。
本次未启动浏览器、CDP 或任何 Word 工具，也未将此 Web 代码的行为外推至 PTS。

## 算法仍需独立裁决的问题

下表只总结本次旧 Android/原生静态资料的证据能力；同日新增的 Mac 规范输入对照
另见 [分栏采集](COLUMN-BALANCE-MAC-2026-09-27.md#规范版本对照)，不能混作 Android 已测。

| 问题 | 本次结论 |
| --- | --- |
| 只有 continuous 后继改变栏数才触发平衡吗？ | 原生端未知；旧 `sect-cont-cols` 输入不能裁决 |
| 相同栏数、相同宽度的 continuous 节是否为同一平衡组？ | 未知；未找到可靠原生组合对照 |
| 硬栏断禁止整组平衡，还是划分平衡子组？ | 未知；现存动态报告仅证明普通换栏 |
| keep/widow 如何约束平衡试排与不可满足时的退化？ | 未知；现存单栏分页证据不能裁决 |
| noColumnBalance 的缺省、false、true 是否改变上述行为？ | 原生端未知；名字索引不能代替开关对照 |

后续应使用明确属于后继节的 continuous 声明、规范属性顺序、明确行距
及逐标签源 CP 的新输入，保留完整采集身份与页栏结果。
本次审计不改变当前算法，也没有把未经验证的条件固化为验收期望。
