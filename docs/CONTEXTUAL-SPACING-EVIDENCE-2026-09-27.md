# contextualSpacing：源输入与规则边界

`word_analyse/reports/rsword-diff/contextual-spacing.md` 提供了段距排查线索，但还不能
用其中“前段开关消除整个相邻段距”的解释替换生产算法。本轮只静态复核原始文件，
没有重新运行 Word；当前引擎仍未消费 contextualSpacing。

## 旧输入的混淆因素

10 份 `context*.docx` 共 455 段。只有 space、mixed、height 三份含 styles.xml，
它们都将 styles 关系放在包根 `_rels/.rels`，没有建立 document.xml 所属的关系。
这不证明 Word 忽略了样式，但实际加载的样式、修复行为和有效样式 ID 未被记录。
尤其 height 的每段都直接声明 360/480 行距，恰好重复对应样式中的值；报告的
36 行页数因此不能独立证明样式加载成功，也不能证明不同有效样式之间仍消除段距。

其余 7 份没有样式引用或样式部件。全部输入缺少显式字体、标记字体及 settings，
文本重复为同一个标签。报告给出的 cpLim 与段数乘 34 相容，但没有原运行的源 SHA、
字体身份、Word build 或原始输出绑定。当前文件的归档哈希不能追溯替代那些记录。

before-phase 报告以 22 行判断“自身 before”还是“前段控制下一段 before”，
但按其固定行高模型，两种规则均在 22 行可放、23 行超高，页数无法区分。
both-phase 则保留了真实的报告冲突：按规范候选应容纳 30 行，报告称 27 行；
没有相邻字形原点或完整原始日志可以在本片裁决。

## 规范候选与后续测量

ECMA 17.3.1.9 的合同要求比较相邻段的段落样式，并从原本合成的段距中减掉
有资格忽略的自身 before/after，结果不小于零。其同样式 after=200、before=240
示例保留 40 twips，不能等同于“先把自身 after 清零再取 max”，也不能等同于
“前段有开关就丢掉整个 gap”。参见
[Microsoft 的 contextualSpacing 规范摘录](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.contextualspacing?view=openxml-3.0.1)。

新的区分输入应正确关联 styles，先以 style-only 行距验证加载，再比较同/不同
style ID、继承与直接 false、before 和 both 的交替相位。每段使用唯一 ASCII 标签，
明确字体、标记字体、keep/widow、compatibility 和页面参数，测完整相邻原点。
规范候选是实验预测，不能记成 Word 验收结果；原点也不等同于页面 required extent。
即使后续先按规范实现，也需要把有效样式身份和开关传给实际段距、keep 预留与重放，
不能只在 bridge 上读取一个布尔值。

原文件、所有 ZIP 成员、报告、逐段 CP 和复核脚本保存在
`artifacts/contextual-spacing-evidence-2026-09-27/`。
`audit.json` SHA-256：
`e817bf9f509a9f2b89e24b3b1b52d379e5026fd6815532af1361bfe70f5dc55b`。
报告与采集边界的详细核对见该目录 README。
该包已冻结 50 个数据文件并由主代理逐项核验；`SHA256SUMS` SHA-256 为
`89563eff2b58545d91ab5200c8270d707e12d4b1db8f0b202f3f10d22b071a91`。

## 已生成的规范输入

`tools/measure/make_contextual_spacing_fixture.py` 已生成
`fixtures/contextual-spacing-canonical-2026-09-27/` 的 6 份 DOCX。共 23 个单行段、
23 个唯一四字符标签及 115 个 UTF-16 源单位。每份包含正确关联的 6 个部件，
输入合同在 manifest.json，所有未实测候选单独保存为 hypotheses.json。

| 输入 | 要区分的行为 | 规范候选相邻 gap，twips |
| --- | --- | --- |
| style-only-line-control | 仅样式定义 exact360/720，检查 A/A 与 B/B | 0；不同 line 的 A/B 过渡不作简单原点预测 |
| same-style-200-240 | 同样式，开关为 F/F/T/F | 240, 0, 40 |
| different-style-200-240 | 属性相同但显式 style ID 交替 | 240, 240, 240 |
| style-only-contextual-override | 样式继承 T，中段直接 F | 40, 0 |
| before-phase | before600，开关 T/F/T/F | 600, 0, 600 |
| both-phase | before300/after100，开关 T/F/T/F | 200, 0, 200 |

除样式加载控制外，条件原点步长使用 exact480 加候选 gap；它要求对应段的基线偏移
稳定，并须保留原始量化观测。独立复核确认三组候选算术一致且组合有鉴别力；单独的
before-phase 仍不能区分规范与“先清零再 max”。

生成器固定 ZIP 时间、权限与部件顺序，拒绝覆盖已有目录；`--check` 校验确定字节、
XML 顺序、部件关系、样式/直接属性、字体与源 CP，范围是本批输入合同而非完整 XSD。
主代理独立检查通过，manifest SHA-256 为
`652eea51ca227348c3ee41c4fb32fadf3d856a3f692054f0fc189e69b9e94f0b`。
这些输入没有运行引擎或 Word，也没有设置 expectedLayout 验收值。
重新生成的 9 个文件逐字节相同，覆盖已有目录和 4 类 XML 篡改均被拒绝；10 条 CLI
检查符合预期。验证回执保存于 `artifacts/contextual-spacing-canonical-checks-2026-09-27/results.json`，
SHA-256 为 `2b43f3e4830898b01186425ae563647917658049500fa6e775f532fec348e5e5`。

## 具名原生入口的边界

既有 Mac 冻结反汇编中，`0x100334c8c..0x100335d64` 的完整 1078 条指令包含
`DyaFetchCoalesced` 诊断。Android 索引中的同名引用位于
`0x9a986c..0x9aa830` 函数范围内；这提供对应调查入口，不证明两平台二进制等价。
本片只重新阅读已有记录，没有执行原生代码或新增反汇编。

Mac 本体先在条件门控下比较两个邻方向记录，将两个初始候选分别清零。扩展路径又有
链分量的最大值合成，并在 `0x100335908..0x10033590c` 作无符号饱和差；重算结果
在缓存前再次按原比较标志清零。另有旁路、缓存直接返回和诊断返回。它计算一个选定
分量，不能孤立地简化成最终 gap 的“先清零再 max”或“max 后减”。

当前最短的命名缺口是 thread-context 的 byte[a6]、u16[28]、i32[3c/40]，以及
两类独立 fetch 记录的字段。位置邻接和低 12 位比较仍不足以把它们命名为
contextualSpacing、style ID 或 before/after；还须绑定属性来源与调用方合成。
这份分析因此没有在六份探针的候选之间作选择，也没有新增生产近似。

冻结包 `artifacts/contextual-spacing-native-entry-2026-09-27/` 含 4 个数据成员，
主代理核对了清单、8 个来源哈希、原始函数逐行复制及连续指令地址。
`SHA256SUMS` SHA-256：
`c2484779947a2e83b6e487552dd9f81793b0e1accc792ed017648f666e1de981`。
