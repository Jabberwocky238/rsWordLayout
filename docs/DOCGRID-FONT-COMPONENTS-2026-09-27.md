# docGrid 字体分量到 LS 聚合

本片将宿主字体回调接到 LS 文本节点及行高聚合，补上网格 helper 的上游输入。
已证明的是静态字段传递及有条件的整数变换；实际字体对象的虚表方法、运行时尺度
和部分公开属性仍待确认。本片没有执行 Word、采集新文档或修改生产排版算法。

证据绑定 Word 16.112.3 / 16.112.26083020。宿主与 PTLS 的完整二进制哈希见
各冻结目录 README；宿主哈希为
`b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c`。
以下地址均为未加 slide 的 arm64 Mach-O 虚拟地址。

## 默认回调及字体提供者

宿主 `0x10005ef98` 构造 `lscontextinfo`，在 info+118 安装 `0x100370740`，
该四字节 thunk 跳到 `0x100370744`。`LsCreateContext` 将 info+80 的 350 字节
复制到 context+10，所以该函数成为 context+a8，正是 `FillTextMetrics` 使用的
回调。创建前有可选 hook，可以修改 info 或中止创建；静态默认值不是运行观测。

令 K、U 为回调 x2、x3 参数，I 为 `0x100370f28(run)` 的布尔结果。已恢复的
条件读取 K/U 低 32 位，虚表转交保留完整寄存器。LS 使用 K=1 和 K=0 分别取得
两套 20 字节指标，但不能据此直接将它们命名为打印/屏幕或物理/逻辑单位。

回调的前置 feature gate 可以改走父对象提供者。旧提供者分支又分为：

| 条件 | 来源 |
| --- | --- |
| I 非零，K 非零/零 | run+148 / run+168 的共享指针 |
| I 为零，K 非零/零 | global slot620 / slot628 的尺度上下文，再取其字体提供者 |

第二条路径经 `0x100349c28`、`0x100349c78` 取得缓存对象，选择其 +38 或 +28
共享指针。最终虚表 +20 返回记录 M，宿主读取 M+0、M+4、M+10。该虚表实现尚未
定位，因此这些偏移仍不能直接命名为 hhea、OS/2 或 WLM TEXTMETRIC 字段。

## 字体回调的五个独立分量

此处用小写 `[a,b,h,c,f]` 表示回调的五个 i32，避免和网格 helper 的
`[H,A,B,C,D,F]` 混淆。普通初始 a/b 来自 M+0/+4；抑制分支可将它们置零。
K=1 时的 run+208/+20c 缓存发生在后续 minimum-b、+10 和 h 建和之前，且
抑制分支跳过此缓存。缓存不是最终结果。

回调先调整 a/b 的最小值及模式补偿，再设置 `h=a+b`。c 有多条提供路径，M+10
只在 `0x100370bd4` 的回退写入中成为 c。成功后续的顺序如下：

| 阶段 | 对五个输出的已证明影响 |
| --- | --- |
| 图形效果 | a、b 加各自增量，h 加两增量之和 |
| f 写入 | 写入提供者结果的低 bit |
| 有符号位移 | a 加 delta、b 减 delta，各自钳到零；h 不变 |
| packed record 调整 | a、b 各加 e，h 加 2e |
| 最后清零 | 特定 run 标记将 a/b/h 清零，保留 c/f |

整数操作按 ARM W 寄存器语义保留 32 位回绕；转换 helper 各有独立的截断、
舍入和越界行为。位移阶段不能用重新求和替代：一侧钳零后，h 可以不等于 a+b。
可选 c 提供者失败会跳过这些后续阶段，失败输出不保证五个 word 全部有效。

`docgrid-textmetric-adjustments-2026-09-27/README.md` 给出完整条件和公式，
包括 K 属于 {0,1} 时 packed helper 的所有 style 分支。图形效果块全部 192
字节为零时该阶段严格跳过；非零时仍有外部 Gfx 虚表 +98 未展开。文档给出的
字段级 no-op 充分条件没有被当作本批探针的实际状态。

## 从字体回调到网格输入

普通文本路径的数据流已经逐边核对：

```text
K=1 callback -> cache+a4 -> lsfetchtextrun+44 -> text node+80/+84/+88
K=0 callback -> cache+90 -> lsfetchtextrun+30 -> text node+8c/+90/+94
text nodes -> participating/shift-aware list aggregate -> two heights triples
heights + conditional content-info -> lslinfo -> host R+48 -> grid input
```

其中一个 flag 可以将 K=1 的缓存复制到另一套。节点构造只取各回调结果的前三个
word。普通列表聚合先判断节点是否参与，再按累计位移分别取第一分量加位移、
第二分量减位移的最大值；第三分量独立取最大值。第三分量为 INT_MAX 的节点
先保存为回退，只在普通前两项最大值均为零时使用。CSS 路径另有实现。
因此不能把该流程简化为段内所有 run 的字体高度取最大，也尚未证明段落标记
在所有路径中的参与规则。

令第一套聚合三元组为 V。`LsFillLineInfo` 将 V[0]、V[1] 分别写到 lslinfo+4、
+c。lslinfo+1c 平时为零，条件启用时来自 `LSLINECONTENTINFO+40`。这些字段
复制到宿主 R+4c/+54/+64。宿主网格初始高度为：

```text
grid H = V[1] + max(V[0], conditional content-info+40)
```

这里的 H 不是字体回调的 h，也不是未经变换的字体表自然高度。content-info+40
的生产者仍未闭合。`LsCreateLine` 直接填充 R+48 的调用以及两处
`LsGetLineBreaks` 后复制完整 88 字节的路径，独立支持这一记录身份。

## Mac 字体 API 的独立证据

WLM 的 `GetTextMetricsW` 填充阶段可以具名确认：+4 为 ascent、+8 为 descent、
+10 为 leading、+0 为前两项转换后的和。PfmFont 的普通路径分别对 CTFont
返回值舍入，ascent 还可走字体族调整比例及最小值分支；Symbol 另有专用分支。
API 在填充后可能继续偏移、转换输出。

这说明组件舍入和字体特例需要被显式保留，但尚不能证明 LS 使用的 M 就是
该 TEXTMETRIC。现有证据没有闭合实际 CTFont 大小、单位、字体族表项或到 M 的
复制链，因此不据此替换 `RealMetrics`。下一条精确入口是共享缓存构造器
`0x100349d38` 及它安装的指标对象虚表 +20。

## 冻结与验证

所有目录位于 `artifacts/`。表中哈希是各自 `SHA256SUMS` 文件的 SHA-256。

| 目录 | 文件数 | 清单哈希 |
| --- | --- | --- |
| docgrid-font-components-2026-09-27/ | 66 | `66f9ef0cc5f84a336fcc0e4201685f174a0c290289a1f9879cdb40c4bd8c6bd8` |
| docgrid-textmetric-adjustments-2026-09-27/ | 24 | `8c1953a88dafec79ac6be68063c369acd309310b7dc588b9dba848bb47115b7c` |
| docgrid-font-callback-2026-09-27/ | 29 | `e2c093e4191604fe3a6b19bc849ab4452ddc1a4a10a481af21662bf5049a2ff8` |
| docgrid-font-api-2026-09-27/ | 16 | `f49a3ce70d868a83d097978719076b172ee8ce890e094a985a08a2cbc9af338c` |

主代理逐项核验前两组 90 文件。后两组审计核对 41 个反汇编窗口的完整指令地址
覆盖、原有三份退出状态收据、源材料哈希及 Python 语法；早期窗口仅有原始 stdout，
没有补造历史退出状态。README 指明两个跨边界初始窗口及其后补完整版本。
回调安装位置、选择条件、字段偏移和后处理时序已独立复核。

本片没有 Cargo 或 Word 回归结果，生产算法没有变更。网格输入尺度与下游 LS
四参数见[原生数据流](DOCGRID-NATIVE-DATAFLOW-2026-09-27.md)，数学 helper 见
[算法证据](DOCGRID-ALGORITHM-EVIDENCE-2026-09-27.md)。
