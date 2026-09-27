# 东亚文字与数字的自动间距

`autoSpaceDN` 是段落属性，控制东亚文字与数字之间的自动间距。
此前 bridge 没有消费它，SimpleMetrics 和 RealMetrics 始终添加既有的汉字/ASCII
数字交界间距，因此显式关闭选项不能改变断行或制表位后的词宽。

## 输入与接口

钉住的 parser `399e36a` 已解析 XML `w:autoSpaceDN`，其 native JSON 字段为
`autoSpaceDn`。`LoadedDocument` 的 Resolver 输出保留段落样式、basedOn、docDefaults
和直接格式的合成结果；不能改读 compat_ts 的合并字段 `autoSpace`，也不能把 XML 名
直接当成 native JSON 键。

[Microsoft 的规范摘录](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.autospacedn?view=openxml-3.0.1)
规定该属性按段落文字的 Unicode 区域起作用；段落省略时沿用样式层的设置，全层均未指定
时视为开启。规范没有规定 1/4 em 的数值。

现有 `FontSpec` 是一次度量请求中影响推进量的条件。将有效段落开关传入每个 run 的
请求，可以让 `measure`、`advance_pt` 及派生 fitting 使用同一个值；LinePiece 和最终
TextFragment 继续保留该请求条件。不能在引擎得到第三方 FontMetrics 的返回值后，
一律减去某个间距，因为提供者未承诺已添加它。也不能用可变的引擎全局状态，避免段落
交替、keep 预排和连续栏重放互相影响。

公开 Rust `FontSpec` 现在增加 `auto_space_dn: bool`，`FontSpec::new` 默认 true。
使用完整 struct literal 的调用方需补上该字段。内置 SimpleMetrics/RealMetrics 消费
该条件；第三方提供者负责自己的策略。裸 native JSON 只识别布尔 `autoSpaceDn`，
缺失或非布尔值保持开启，run 属性和 `autoSpaceDe` 不代替这个段落字段。

段落标记仍独立解析字体、字号、颜色和升降，但最终的 `LineTail` 必须携带正文同一
段落的 `auto_space_dn` 条件。否则显式关闭后，仅这个布尔字段的差异就会拆开原本
同款的正文与标记，改变送入 shaper 的字符串；纯拉丁文本也可能因此改变字形。
回归需检查最终绘制请求，以及软换行、分页符与段落标记相邻时的条件传播。

## 证据边界

`word_analyse/reports/rsword-diff/breakme.md` 报告现有 1/4 em 策略使一个 Android
窄路径断点从 596 移到 593。它只支持当前样本的断行约束，不能验证所有字号、混合字号、
字符类别或绘制坐标。本片不扩展字符集合、改变该数值，也不接入 `autoSpaceDE`。

另有两个已定位的独立一致性问题：段内交界拆在不同 run 时没有补间距；字形绘制只应用
缩放与字符间距，没有补测量中已有的自动间距。开关接通不等于这两项已修复，后续必须
分别检查源区间、实际字形原点及断行，不能仅验证 measure 的返回值。

## 回归验证

`auto_space_dn.rs` 的 11 项公开 API 回归覆盖真实 DOCX 的 OnOff 解析、docDefaults /
默认段落样式 / basedOn 继承与直接覆盖，空段落、多 run、相邻段落条件隔离，断行与
分页源区间、普通及紧急 fitting、字符间距和缩放，以及右/中/小数点制表位。
其中 1 项需要 `fontenv`，使用仓库内两份固定字体核对关闭后的 shape advance 总和。
独立度量提供者用自定宽度验证请求传播，引擎不能代替它扣减某个预设间距。

`paragraph_mark_paint.rs` 另加 3 项回归，检查同款 mark 恢复单次 `shape("fi ")`，
异款 mark 的字体、字号、颜色和抬升不丢失，以及软换行/分页符尾部的源区间。
初始开关的 9 项、随后 mark 的 3 项均先得到编译成功后的断言失败，再应用修复。
这些是公开管线的一致性验证，不是 Word 新采集。

开发期日志与副本保存在 `artifacts/auto-space-dn-2026-09-27/`。它们没有自动的
逐进程前后源码采样；事后清单只绑定保留文件。最终验收使用独立的
`artifacts/auto-space-dn-checks-2026-09-27/final-mark-condition/` 命令收据。
同包较早的 `final/` 在 mark 修复前完成，不作为最终验收。

最终 workspace + `fontenv` 为 **706 passed / 12 ignored / 0 failed**，core 默认
配置为 **580 passed / 0 failed**；两组重叠，不能相加为独立测试总数。
workspace/all-targets/fontenv Clippy `-D warnings`、trace 构建及 Android 回放
均退出 0。五次命令收据中的 121 个本地 Rust/manifest/lock 输入在执行前后及最终
审计时一致，覆盖六份 examples；另保留八份改动源码副本。范围不包括全部外部依赖。

Android 11 份、186 行的完整 engine trace 与冻结的 mirror-margins 基线逐值、
逐字节一致，未归一化字段。这仍是 legacy narrow/mobile 条件下的源区间匹配，
不代表 Word 的字体、几何或物理分页全部对齐。

Mac 使用既有采集包离线回放：25 份完整 trace 的 356 页、3,834 行、18,150 个字形
逐字节不变，另 5 包均无 trace；比较结果只排除 `candidate.trace` 输出路径。
覆盖状态仍是 23 FAIL / 7 UNDECIDABLE / 0 OK，25 份可比较样本的页数全部相同。
这证明已有结果未退化，不证明已经解决旧差异，也不是显式 false 的新 Word 证据。
完整比较文件重核了 896 个绑定文件；根复核另检查最终审计与源码绑定，共 1,070 项。

冻结材料：

| 目录（均在 `artifacts/`） | 数据成员 | SHA256SUMS 的 SHA-256 |
| --- | ---: | --- |
| `auto-space-dn-2026-09-27` | 36 | `905eaefb0c8e125ba386620ad447ee4364199bab8c34592d993ba46989cde9af` |
| `auto-space-dn-checks-2026-09-27` | 162 | `1b7bab98e66146be2774542fb415568f8375e59a7e48ff86cea3b231b94b2481` |
| `auto-space-dn-mac-replay-2026-09-27` | 266 | `f3b2cfab110d26fb2b2b631b029d375a54e9e1e1d3f86d0b85302a3f17d6f516` |

最终二进制 SHA-256 为
`a0c6fe8682107132640670c36271c8bf6cc9f4f948cb9de8c21a252027452abd`；
最终 `audit.json` 为
`297cc3a62f5edf0b117227b883731f490f5672508adf6ea665dd3a86503505bb`；
Mac `comparison.json` 为
`57dd32cb629e2ef777095994809b75df09b1f47f1606bf697e4e66081a826316`。
忽略的 artifacts 不加入源码提交。
