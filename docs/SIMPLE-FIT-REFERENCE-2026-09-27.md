# PTS Simple 容纳与裁减的可执行参考

[simple_fit.py](../tools/measure/simple_fit.py) 把已恢复的 Simple 局部决策做成 JSON
回放工具。输入是显式的原生整数，输出状态固定为 `ARITHMETIC_REFERENCE`。它不选取
Word 实际运行分支，不推导字体量、S 或页面单位，也不把单行判定当作完整分页结果。
静态来源和条件见 [Simple 容纳证据](DOCGRID-SIMPLE-FIT-2026-09-27.md)。

## 输入与结果

`fit` 必须给出 `u/v/s/limit` 四个有符号 32 位整数。U/V 是此前可选修改已完成后的
两个分量，S 是成功查询的可抑制底部空间，或已知不查询路径显式提供的零；limit 是
同单位的剩余量。S 还须通过已证
范围 `[-0x3fffffff,0x3fffffff]`；不能把负 S 钳为零，也不能默认令它等于网格尾部量。

模型先计算 `N=wrap32(U+V)` 和 `required_i32=wrap32(N-S)`，按有符号数比较：

| 条件 | 分类 |
| --- | --- |
| required_i32 > limit | REJECT |
| required_i32 <= limit 且 N <= limit | KEEP |
| required_i32 <= limit 且 N > limit | NEEDS_OVERHANG |

`required_i32` 仅为此处比较量，不代表物理占高。`excess_i32=wrap32(N-limit)` 保留
SUBS 的结果，但不能用它的正负代替 N 与 limit 的有符号比较，回绕时两者不同。
只有 NEEDS_OVERHANG 才消费可选的 Boolean `allowOverhang`：true 保留 U/V，false
保留 U 并令 `V=wrap32(limit-U)`，得到 CLIP。省略时仍为 NEEDS_OVERHANG，最终
U/V 和 advance 为 null；REJECT 同样不伪造可提交的推进量。

查询失败、错误清理、分支分派、宿主 overhang 回调和外层重试没有模拟。显式 bool
只表示回调结果已知；工具不会调用回调或猜测未给出的结果。拒绝分支在原生查询后
重新读取 U，本接口要求查询没有额外改变所给元素值，不据此证明真实回调无副作用。
模型输入和决策使用
不可变记录；resolve 会核验其输入确实对应尚未决的分类，拒绝错误或重复求决。

## 合成样例

仓库的 [合成输入](../tools/measure/examples/simple-fit-synthetic.json) 为
U=80、V=40、S=20、limit=110。N=120，比较量=100，因此需要 overhang 决策。
省略 bool 时没有最终推进；true 得到 KEEP、推进 120；false 得到 CLIP、V=30、
推进 110。这些整数没有绑定任何字体、页面或 Word 文档。

另一个关键反例是 U=80、V=20、S=-20、limit=110：虽然 N=100 小于剩余量，
比较量 120 仍要求 REJECT。它防止把“原推进能放下”提前当成接受结果。

每份报告保存原输入、初步分类、是否消费 overhang、最终结果、原始输入 SHA-256、
CLI/模型/发布器源码 SHA-256，以及固定 PTLS 和静态清单哈希。入口拒绝浮点、
非有限数、重复 JSON 键、多余字段、以 bool 冒充整数和非法 schema。输出使用已有
原子发布器，拒绝覆盖文件或悬空符号链接。

模型和 CLI 的 92 项定向测试通过，完整量具套件 597 项通过；其中独立条件码对照
覆盖 1,715 组边界组合。独立复核核对了冻结指令、范围检查、入口及原子发布，未见
阻断问题。四次真实 CLI 执行分别保留待决、允许、拒绝和负 S 反例，均正常退出。
[回放目录](../artifacts/simple-fit-reference-2026-09-27/README.md) 的 11 个成员已冻结，
清单 SHA-256 为 `9d93bea3d4eaba4c425c46e8928cfd0bdaae7b13791fba945bdc8054f385598e`。
本片未改生产 Rust，也没有重新运行 Cargo 或导出 Word 文档。

## 生产接入约束

现有 `page_line_quota` 只返回行数，随后 `format_flow_paragraph` 又取原来的
`line.vertical` 更新游标。将裁减仅加入预检会使判定与提交不一致。未来接入需要把
每条接受行的已决推进与源游标一并传递给放置、required 检查和 cursor 更新。

`line_vertical` 没有剩余容量，不能在这里完成容量相关裁减。keep、widow、换栏
重排和 balance trial 都需要针对新容量重新判定；提前按固有推进累加或缓存裁减值
都不充分。balance 的候选容量事件还须重新验证完整性。裁减本身也不应移动基线、
改变 CP，或把空页强制前进当成正常接受。

因此本片提供独立的算法校验接口，生产 core 尚未接入新的默认行为。S、宿主 gate、
实际分支与单位来源的缺口继续保留，没有用合成样例或分页阈值拟合填补。
