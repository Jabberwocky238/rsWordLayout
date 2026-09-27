# 自动间距的断行与绘制一致性

`7d3c472` 接通了有效段落 `autoSpaceDN` 开关，但既有开启路径还有两处不一致：
交界被拆在不同 run 时没有补间距；绘制时也没有消费度量已经计入的间距。
例如 SimpleMetrics 的 12pt `中0中`，一个 run 为 720 twips，拆为三个同款 run
却只有 600 twips，在 660 twips 宽度下会得到不同断行。

## 证据与算法选择

[autoSpaceDN 规范摘录](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.autospacedn?view=openxml-3.0.1)
和 [Word 对象模型](https://learn.microsoft.com/en-us/office/vba/api/word.paragraph.addspacebetweenfareastanddigit)
定义段落开关，没有定义间距量或混合字号时取哪一侧。
`word_analyse/reports/rsword-diff/breakme.md` 的 12pt 单样本断点实验支持保留现有
1/4 em 策略，不能据此声称直接量得每处 60 twips，也不能推导混合字号规则。
原 DOCX 只有一个没有 rPr 的 run；回放中的拉丁字和汉字可以使用不同实际 face。
因此不能要求两侧 FontSpec 完全相等才补间距。

本次统一现有表意字/ASCII 数字交界策略。混合字号按表意字一侧的有效字号计算，
这是明确的引擎外推，尚不是 Word 实测结论。字符分类、1/4 em 数值、间距不随横向
缩放改变均沿用旧策略；不因重构扩展为完整 Unicode 分类或 `autoSpaceDE`。
隐藏文字两侧按可见相邻字符处理，但这个组合的自动间距仍待专门 Word 验证。

本轮静态证据复核绑定八份原文件及 `breakme.docx` 的三个 ZIP 成员，见
`artifacts/autospace-consistency-evidence-2026-09-27/`。两成员冻结清单的 SHA-256 为
`451b63a141b8ef3a1be35bddca198b5d464cc142cd1c3a206f222f231d3ac0b3`。

## 一致性要求

`FontMetrics::boundary_spacing` 同时接收两侧字符及 FontSpec，返回独立的
`SpacingAdvance { fit_twips, paint_pt }`。默认返回零；选择提供间距的实现须在
自身 `measure` / `advance_pt` 中包含相同的片内间距，shaper 则返回尚未添加它的
原始 glyph advance。内置 SimpleMetrics 与 RealMetrics 共用分类和金额计算。

`TextFragment.spacing` 保留 `SpacingEvent { source_end, advance_pt }`，其中
`source_end` 是左侧字符的全篇 UTF-16 终点。完整 struct literal 的 Rust 调用方
需补此字段。LinePiece 的 leading 间距属于接纳预算，转成绘制事件时归左片尾部；
回退后只对保留内容生成事件，paint 不修改 Page 内保存的事件。

- 同一度量提供者决定片内和跨片交界的推进量；第三方提供者默认不额外添加间距。
- 断行整数推进量与绘制精确推进量分别保留。每个交界单独截断的旧规则不改成
  整行最后一次取整，例如 7.92pt 的两处交界仍是 39 + 39 twips，精确值为 3.96pt。
- 交界间距参与文字接纳前的预算；收行或回退丢弃右片时，也必须丢弃那处间距。
- 负字符间距允许前缀抵消交界带来的负预算。预算原样交给 fitter；接纳其非空
  结果，不能再次要求预算非负或总宽不超预算，否则会拒绝合法负宽前缀或标点挂出。
- 制表位后宽度、跨 run 词宽和回退试排使用同一计算入口。
- 绘制消费最终保留内容的 UTF-16 源位置事件，不再根据字形序号或片段字体重新猜测。
  隐藏文字、代理对和控制符不能把事件移到另一个源字符上。

混合字号与隐藏文字的 Word 探针应包含双向的数字/表意字对、交换两侧字号、单 run
和拆 run 对照、显式开启/关闭对照，并同时核对源区间、断行和字形原点。

字形坐标的保证针对能够提供有效源簇范围的 shaper。`source=None` 或某个簇跨过
间距交界时，不能按字形序号猜测插入点。没有 shaper 的 PaintList 不产生字形坐标；
现有 SVG 后端仍按 text 字符串输出，尚不消费定位后的 glyphs，其一致性需要另行接通。
这两个输出域不能用本轮字形回归冒充已经验证。

## 验证范围

新增 17 项公开管线测试，包括实际 glyph advance/origin、同款和异款 run、显式
关闭、39 twips 与 1.98pt 的双精度口径、整片回退、零宽前缀前的间距欠额、隐藏
非 BMP 源间隔、制表位与对象/控制符屏障，以及非零全篇源偏移和重复绘制。
240 组同串/逐字符拆 run 组合检查实际绘制输出；真实字体测试同时覆盖 7.92pt、
字符间距和 150% 横向缩放。测试没有把旧的 measured-prefix 推算当作字形坐标。

首次 8 项运行得到 5 个行为失败、3 个保护性通过，再应用修复；早期编译预检与扩展
测试的编译失败单独保存。第一轮定向运行 151 passed / 3 ignored / 0 failed，
122 个本地输入的执行前后采样一致。随后修正一处 Clippy 嵌套 if。

完整复核又发现负字符间距反例：12pt、字距 -180 twips、宽 100 twips 时，
`汉0汉` 的拆 run 版本错误地拒绝了可抵消欠额的 `0`；字距 -220、宽 60 时，
`汉0汉。汉` 的合法标点挂出也受影响。两项新增测试先产生运行时失败，再移除
多余的 `remain >= 0` 条件，定向结果为 71 passed / 1 ignored / 0 failed。
两次执行各自的 122 个源码输入前后相同，实际源区间和字形坐标均受检查。

原作者证据 `artifacts/autospace-consistency-2026-09-27/` 及根验证的 `final-lint/`
均是负字距修复前的历史记录，其冻结说明不会回写。负字距补充证据独立保存在
`artifacts/autospace-consistency-negative-prefix-2026-09-27/`；最终源码验收使用
`artifacts/autospace-consistency-checks-2026-09-27/final-negative-prefix/`。

本片没有新增 continuous-section replay 的专项测试。静态复核确认每次续排从绝对
源偏移重建行内 pieces，间距不存于跨次可变状态；现有分栏/续排测试仍参加完整回归。
专项组合实测和测试覆盖仍可加强，不能把静态检查当作已新增的动态覆盖。

## 最终验收

最终源码分别通过 `cargo clippy --offline --workspace --all-targets --features fontenv
-- -D warnings`、workspace 的 fontenv 测试（723 passed / 12 ignored）及 core 默认
配置测试（596 passed / 0 ignored），均无失败。五步根验证包括构建与 Android
回放，执行前后的 122 项本地源码、manifest 和 lockfile 哈希均一致；九个改动源码
副本绑定到这一轮采样。该清单不声称覆盖全部外部依赖或进程环境。

最终 `layout-trace` 的 SHA-256 为
`3ad92ba604dc775f26b48199c8a78e398a0f5979492997c59e7491fc6c2923b9`。
Android 11 份样本的条件源行匹配保持 186/186。唯一含 DN 的 `breakme` 保留十处
交界；跨行的另一处不计间距。其 trace 恰好符合事先独立列出的 27 处变化：
十个横向 advance 和十七个横向 origin，其余字段均相同。另十份 trace 逐字节相同。
这批样本中的原 `breakme.docx` 只有一个 run，跨 run 行为由新增管线测试覆盖，
不能把此回放称为 Word 跨 run 或混合字号行为的实测验证。

根审计结果保存在 `artifacts/autospace-consistency-checks-2026-09-27/audit.json`，
SHA-256 为 `d82fa6a29babe892c8d9113f91922e5018d915f9ebf0eb5700fb0aced943183b`。
预先登记的回放预期清单位于 `artifacts/autospace-consistency-replay-audit-2026-09-27/`，
清单 SHA-256 为 `4d717dd99aaf2f100ecd4edc59ea92aa1e40a54498d7880c7102093d4dc7317b`。
负字距补充证据的 32 成员清单 SHA-256 为
`efc67dc9df67d10c6ff9d4ba5fff0357c40e869e7543b412df7683bc3e2a1d70`。

Mac 历史样本离线回放的 25 份 trace 均逐字节相同，共 356 页、3,834 行和
18,150 个字形。该批源文档均不含 DN 交界，故验证的是非目标文本未受影响，
不是 DN 的正例。30 个 bundle 的历史对照仍为 0 OK / 23 FAIL / 7 UNDECIDABLE，
其中五个没有可执行 trace；既有 Word 差异没有被本轮结果消除。回放命令退出 1
反映这些既有失败，版本间比较通过。没有执行新的 Word/native 采集。

Mac 比较记录位于 `artifacts/autospace-consistency-mac-replay-2026-09-27/comparison.json`，
SHA-256 为 `14d06253d74e60f5d794019be695fbd7137fd666d403af5c747fb5fdfb1c17dc`；
其中 1,163 项文件绑定经根代理独立复核。比较器仅忽略 comparison/selfcheck 中
候选 trace 的输出路径，完整 trace 不作任何字段排除或归一化。

最终根验证包的 166 成员清单 SHA-256 为
`4f5e2e45553d3ecd95e7d3374c91e816d2a2b60044ada3382a8994afa9e50212`；
Mac 回放包的 266 成员清单 SHA-256 为
`4891ecf461b5884aa4ba1a898e95af1bcba2ab116143eba870cc85534d9c4a5b`。
