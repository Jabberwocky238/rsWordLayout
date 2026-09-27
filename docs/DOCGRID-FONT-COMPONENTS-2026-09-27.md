# docGrid 字体分量到 LS 聚合

本片将宿主字体回调接到 LS 文本节点及行高聚合，补上网格 helper 的上游输入。
已证明的是静态字段传递及有条件的整数变换；默认包装与指标提供者的方法已定位，
更下层字体接口、运行时尺度和部分公开属性仍待确认。本片没有执行 Word、采集新文档
或修改生产排版算法。

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
共享指针。最终虚表 +20 返回记录 M，宿主读取 M+0、M+4、M+10。默认对象的
getter 已由后续小节定位；这些偏移仍不能直接命名为 hhea、OS/2 或
WLM TEXTMETRIC 字段。

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

续查把该 content-info 对象具体绑定到 `LsCreateLineCore` 的局部 I=SP+124，
format context X=SP+e8 的首个指针指向 I。调用 `LsFillLineInfoFromLine` 时 x4
仍来自这个局部地址。`LsFormatMainLine` 的 `0x1ce28..3c` 通过 X[0] 清零
I+0..6b，因此包含 I40；此后 I 随格式化与断行调用继续传递，清零不代表最终值。
普通文本、定位及三条断行路径的已查函数尚未提供有实际 I 别名支持的非零 I40
生产者。候选扫描只按指令寻址生成线索，不证明全部别名或调用树均无写入。

还找到了条件启用位的来源入口：已冻结 `LsSetLineProperties` 在 `0x9c00c`
读取输入 lslinerestr.byte20 的 bit7，置位时在 `0x9c018..1c` 设置
formatprop+58 的 bit12。Core 随后在 `0x1b418..434` 将这个 bit 作为 content
enabled 参数传入填充函数。尚未证明样本文档启用或关闭它。完整对象链与调查边界见
[content-info README](../artifacts/docgrid-content-info-inputs-2026-09-27/README.md)。

宿主输入续查已闭合普通调用的 `Q=SP+280` 身份，并找到两处 `Q.word20|=0x80`
的真实 setter。它们比较宿主日志标为 `cp` 的输入与 paragraph client 的两个 getter：
`max_signed(A(P),cp)==B(P)`。A/B 分别读 P+2d4/+2e4，并按可选 P+238 映射对象
增加偏移；尚未命名为公开段落属性。一般 builder 先把比较结果写入 R128.bit3，
中间调用后重新读取该 bit 才置 Q 的标志，不能省略可能改变状态的调用。另一 builder
直接使用比较结果，但属于外部 formatter 分支，不能认定必到同一直接 LsCreateLine。

初始化路径把 bit7 清零，但 builder 后续可以置位；另一内联初始化写入未 mask 的
源字节，不能把它默认为 Boolean。以上闭合的是条件输入来源，既没有得到探针最终
gate 值，也没有得到 I40 的非零生产者。八个完整函数及边界见
[content-enable README](../artifacts/docgrid-content-enable-input-2026-09-27/README.md)。

## Mac 字体 API 的独立证据

WLM 的 `GetTextMetricsW` 填充阶段可以具名确认：+4 为 ascent、+8 为 descent、
+10 为 leading、+0 为前两项转换后的和。PfmFont 的普通路径分别对 CTFont
返回值舍入，ascent 还可走字体族调整比例及最小值分支；Symbol 另有专用分支。
API 在填充后可能继续偏移、转换输出。

这说明组件舍入和字体特例需要被显式保留，但尚不能证明 LS 使用的 M 就是
该 TEXTMETRIC。现有证据没有闭合实际 CTFont 大小、单位、字体族表项或到 M 的
复制链，因此不据此替换 `RealMetrics`。后续默认对象追踪进一步缩小了这一边界。

## 默认指标对象的后续闭合

共享缓存构造器 `0x100349d38` 是四字节 thunk，实际构造体 `0x100349d3c`
只有 256 字节。令 W 为该缓存、O 为其 owner。构造时 W+18 持有基础指标对象，
W+28 持有默认调整对象，W+38 共享指针为空。后续 W+38 可变化，不能将这个
构造状态当成所有字体回调的选择状态。

三张直接安装的虚表和 RTTI 字符串链给出对象身份：

| 对象 | 虚表地址点 | RTTI 名称 |
| --- | --- | --- |
| W | 0x104581a00 | PhysicalFontRtfontWrapper |
| 基础指标对象 | 0x104581730 | BaseFontMetricsRtfontWrapper |
| 默认调整对象 | 0x104581778 | AdjustedFontMetricsRtfontWrapper |

默认调整对象的虚表 +20 解码到 `0x1003712a8`，其八字节实现直接返回
`this+34`。因此默认 M 指针身份已经闭合。其构造器通过基础对象的虚表 +28
取得 `base+0c`，复制完整 24 字节至 M。基础对象自己的 +20 返回一个 flag，
不是 M getter，两个对象的同号虚表槽不能混用。

令 T 为 `0x1003463ac(O,&T)` 写入的临时记录，构造重排为六个 word：

```text
M initial = [T[4], T[8], T[0], T[0x10], T[0x14], 0]
```

下标是字节偏移。它只证明构造初值，不保证回调读取前没有再调整 M。尤其候选
c 所读的 M+10 对应 T+14，而非 T+10，不能凭临近 WLM 填充代码就称其为 leading。

源 helper 本身也是 16 字节透明转发：取 `Q=O.ptr[120]`，尾调用
`Q.vtable[30](Q,&T)`。另一 helper `0x100348b40` 转发到同一 Q 的 +38，
产生的附加字段在这 24 字节之外。O+120 的安装与 +30 实现由下一片继续闭合，
尚未连接到具体平台字体 API。

上述虚表项均按实际 format-6 chained-fixup 页链核验；RTTI 名称的高位标记被
保留，未解释其 ABI 语义。类名证明包装类型，不证明字体文件、CTFont 大小、
单位或运行时实例。原冻结回调报告中的未知 getter 由本后续片限定为默认路径闭合。

## 源指标提供者与局部转换

O 的构造器 `0x100344c20` 将传入的共享指针安装到 +120/+128；其虚表地址点
`0x1045022e8` 的 RTTI 名称为 `RTDWRITEFONT`。两个直接创建路径将该参数
接到工厂 `0x10034473c` 与构造器 `0x1003447ac`，第三个克隆路径复制旧 owner
的共享指针。提供者构造器安装 Q 的虚表地址点
`0x1045020f8`，RTTI 名称为 `CFontFaceTextMetrics`，+30 解码到
`0x1003463bc`。这是默认构造链的具体提供者，不代表所有可能的运行时替换。

该方法先由 `0x1003464d4(Q,&tmp)` 取得包含 16 位分量的临时块，再构造 T。
令 a16、b16 为 tmp+2/+4 的无符号半字，e16 为 tmp+6 的有符号半字：

```text
T[4] = a16 + (Q.i32[0x3c] != 0 ? e16 : 0)
T[8] = b16
T[0] = T[4] + T[8]
T[0x0c] = T[0] + Q.i32[0x48]
T[0x10] = Q.i32[0x3c] == 0 ? e16 : 0
T[0x14] = T[0x18] = max(1, convert(f32(tmp.i16[0x30]) * Q.f32[0x150], 1, 1))
```

此处偏移均为字节，W 寄存器的整数相加与取负保留 32 位回绕，浮点乘法在单精度执行，convert
指 `0x1003469f4`。它再次说明 M+10 的候选 c 来自一个独立缩放且至少为 1 的
分量，不能将其直接替换为 T+10 或普通字体 leading。

上述 T 还可能调整：Q+50 等于 0x384 或 0xa8c 时，`0x10310379c` 计算增量 g，
对 T+0/+4/+8/+c 分别加 `[2g,g,g,2g]`；T+10/+14 不受此 helper 修改。
g 先以单精度依次计算 `u16(tmp[0]) * (f32(-Q[0x48])/f32(u16(Q[0x10])))`
和 `1/64` 的乘积，再调用相同 convert。常量取值尚不足以命名该分支的公开属性。

临时块生成有两类来源。`f32(-Q[0x48]) == f32(u16(Q[0x10]))` 时直接复制
Q+10 起的 50 字节；否则先清零，再通过 Q.ptr[8] 查询固定标识的接口。
查询成功调用返回对象的虚表 +98，失败走原对象的 +80，均将缩放尺寸和输出地址
显式传入。随后若干有符号半字另行缩放，`strh` 只保存转换结果的低 16 位。
两条路径取半字的位置不同，不能统一为原始字体表乘同一比例。Q 的初始化还包含
字体表读取与条件修正；Q+10 的复制路径也不证明这些字节未经处理。

convert 的分母为零时返回 INT32_MAX；否则先调用导入的 `VarR8FromR4`。
该源提供者片保留了导入边界，后续小节补齐其实际实现。其后按参数执行乘法、可选的
加半分母再除法，并调用 `0x100346a94` 的参数零分支。该分支从 double 指数 E
得到 `ldexp(1,E−1075)`，按符号推离零，再加同号 0.5，最后选择 ceil/floor。
运算次序不能压成语言内建 round；最终范围与无序比较可返回 INT32_MAX。
局部指标路径使用的乘数和分母均为 1，仍需保留这些浮点、半字和越界语义。

源提供者片留下 Q.ptr[8] 的具体字体接口、原始分量与缩放参数的运行值，
以及初始化与后续变更；接口合同由下文后续片补齐。已知 `MsoGetTextMetricsW` 包装器的地址引用没有证明
它属于这里的虚表 +30，故没有用同布局结构或邻近符号跨接这条边。
这些静态字段尚不提供将 `RealMetrics` 直接投影为原生网格 H 的依据。

## 浮点导入的后续闭合

Word 的 arm64 符号表明确将 `VarR8FromR4` 绑定到 OLEAutomation 导入；同一应用
内的该 framework 在 `0x632c` 导出它。`LC_FUNCTION_STARTS` 界定其完整 16 字节
实现，逐条为 `FCVT D0,S0`、`STR D0,[X0]`、`MOV W0,0`、`RET`。
因此这份二进制在该导入内只做 binary32 到 binary64 的扩宽并返回成功，没有额外
十进制转换或软件舍入。后续 epsilon、同号 0.5、ceil/floor 及范围哨兵仍照前述执行。

该判断来自磁盘导入/导出与实际指令；没有测量运行时替换、FP 控制状态、NaN 载荷或
次正规数处理，也不据此把具体字体输入尺度视为已知。OLEAutomation 整体哈希为
`dfd1ee8efae232194129434f0d977661a1bb0562ff65ef9036aac98af528d75a`。

## 字体接口与请求记录的后续闭合

宿主查询使用的 16 字节 GUID 与 `IDWriteFontFace1` 完全一致；创建路径的另一 GUID
对应 `IDWriteFactory`，且实际调用具名 `DWriteCreateFactory` 导入。依据固定版本的
[官方 dwrite.h](https://github.com/microsoft/win32metadata/blob/5c5efbc01d4c87f6830ec304d42777991d533154/generation/WinSDK/RecompiledIdlHeaders/um/dwrite.h)
和 [dwrite_1.h](https://github.com/microsoft/win32metadata/blob/5c5efbc01d4c87f6830ec304d42777991d533154/generation/WinSDK/RecompiledIdlHeaders/um/dwrite_1.h)，
原 face 的 +80 与查询所得 face1 的 +98 分别对应两版 `GetGdiCompatibleMetrics`。
这里命名的是接口合同，尚未跟踪返回对象的具体 Mac 方法实现。

基本指标记录为 20 字节，Face1 扩展后为 48 字节。Q+10 对应 designUnitsPerEm，
Q+12/+14/+16 对应 ascent/descent/lineGap，Q+3c 对应 hasTypographicMetrics。
Q+40 已在这份记录之外：成功 Face1 初始化路径从 OS/2 表 +2 读取它，公开字段为
xAvgCharWidth。该值经宿主缩放进入 T+14 和初始 M+10，与 lineGap 不同；后续
回调如何使用它仍不能由字体表字段名替代证明。

GDI 兼容接口接受 DIP 单位的 emSize，返回字体设计单位的指标。宿主初始化时传入
designUnitsPerEm，临时指标构建时传入 `float32(neg32(Q[0x48]))`，两处均为
pixelsPerDip=1、transform=null。宿主之后仍执行自己的单精度比例、整数舍入和
半字写入，不能将 API 的输出直接当作布局坐标。

264 字节请求记录的前 92 字节已绑定 LOGFONTW：具名 GetGdiInterop /
ConvertFontFaceToLOGFONT 调用、实际 92 字节复制和 UTF-16 半字操作相互印证。
Q+48 是 lfHeight，Q+50 是 lfEscapement，因此先前 900/2700 分支有十分之一度
的角度字段依据。公共 lfHeight 的逻辑单位仍不证明 Word 的点数映射。
其中一条按字体家族匹配的路径复用 request+4 作为 stretch，不能把该位置在所有
路径上一概解释成 lfWidth。

`0x1005c4104` 的高度赋值受门控约束：`0x1005c4720` 返回零时跳过转换与高度
写入，把先前清零的 264 字节记录传给构造器。另一分支也未检查转换调用的 HRESULT，
故只能确认复制预清零输出缓冲区，不能假定转换成功。资源管理器选脸、Arial 重试、
feature 替换、初始化 fallback 和 adjusted-wrapper 后续变更仍须保留。

这一步把缺口缩小为实际点数到 lfHeight/坐标尺度、实际选中字体与指标返回值，
没有证明 TNR12 的 lfHeight 是 -16 或 -50，也没有新增生产常量。
随后已用显式文件和显式 emSize 完成[独立字体指标实测](DWRITE-METRICS-2026-09-27.md)，
证明可以读取这份库的数值输出；该实验没有观测 Word 的实际字号或字体选择状态。

## 字号请求的实际写入

普通字体选择分派的一条路径已接到 LOGFONT 高度的实际 store：
`0x10033ef48 -> 0x1003403f8 -> 0x10034049c -> 0x10034326c`。
最初从 run-property 记录 C+4 复制半字到选择记录 R+6；后续可以更换这个值，
进入 builder 前的局部副本也先钳到至少 1。因此下式以真正到达 builder 的 P 为输入，
并未证明 C+4 的 OOXML 字号身份或每条分支都保持它不变。

令 h=P.u16[6]、p=P.u16[8]，N 为已恢复的最近整数乘除 helper，Z 为截断/饱和
helper。在普通正数且不溢出的范围内，初始垂直尺度为：

| mode | 初始尺度 |
| --- | --- |
| 0 | N(p,72,100) |
| 1 | TLS 提供者的 +90 word，另一方向另读 +8c |
| 2，调用者第五参数为零 | 294912 |
| 3 | N(1440,p,100)，p=100 有直接 1440 分支 |

P.byte[2].bit1 选择 N 或 Z，计算 `a=convert(10*h,scale,1440)`，随后写入
`request[0]=min(-1,-a)`。该写入在 `0x1003433e4`，仍需保留 W 寄存器边界语义。
乘十与半点转 twips 一致，但单凭这个系数不能证明原始字段就是 `w:sz`。

模式 2 会再次改写高度。`0x100343b3c -> 0x100343bf4` 的输出依次可能来自缓存、
特殊 bit4 分支的常数 1000、临时字体查询，或失败回退。临时查询先把请求高度写成
-2048，再通过真实 owner face getter 调用 `IDWriteFontFace::GetMetrics`，把首个
u16（designUnitsPerEm）写入两个输出。随后 `0x10034131c` 将第二个输出取负写回
最终请求的首 word，覆盖初始 294912 换算结果。

成功临时查询路径因此写入 `lfHeight=-D`，但 D 属于临时查询得到的 face。之后的
实际创建可能选择另一 manager 或 face，尚未证明最终 Q 的 D 与此值相同，也不能
单凭这次 store 宣称一定走 copy50。缓存、1000 和失败回退仍分别保留。
同一函数后段还会改写 fontObject+168/+174，不能把该字段全程命名为固定 D。

manager 的真实 fixup 已闭合：选择值 2/3/4 进入 `0x100343e50 -> 0x100343e60`
的 DirectWrite 创建链，其它选择有 CreateFontIndirectW 与独立 feature 路径。
另一条曾考虑的 `0x1005c4104` 被具名诊断确认是 stock-font 文件缓存路径，调用者
传入的是 +16；它不能用来证明普通 LS 的 TNR12 请求是 -16。

这次闭合了实际 store 与模式 2 覆写，尚待确认活动模式、内部字号的源 setter、
临时/最终 face 身份。构造后的 adjusted-wrapper 缩放见下节，没有选一个 DPI 去拟合分页。

## 构造后的字体调整与缩放

默认 wrapper 的 M 会在构造之后更新，初始 M 不能直接作为 LS 的最终输入。
本次固定了三个不同对象：选中的字体记录 F、尺度上下文 C、wrapper 内 M。
`0x10034110c..118` 真实执行 `memcpy(C,F,0x188)`，覆盖指标区域而保留
C+188/+18c 的横纵尺度。随后 `0x100349a90 -> 0x10034a04c -> 0x10034a054`
更新默认 wrapper，直接得到：

```text
M = [C.c8, C.c4, wrap32(C.c8+C.c4), old_M.c, C.184, old_M.14]
```

这是明确的复制与覆盖链，不是根据相同偏移假定 F 与 C 是同一对象。可选的 alternate
wrapper 仍可能被回调优先选择，当前样本文档走哪个分支尚未观测。

在 F.mode=2 的缩放分支，`0x100348f8c` 逐项调用已恢复的有符号最近整数乘除 N：

```text
h2 = F.u16[196]; dx = F.i32[168]; dy = F.i32[174]
for k in [c0,c8,c4,cc,d0]: F[k] = N(C.18c, wrap32(F[k]*h2), wrap32(dy*144))
for k in [294,28c,290,bc,180]: F[k] = N(C.188, wrap32(F[k]*h2), wrap32(dx*144))
```

F.184 不参与这十项缩放。因子 h2 有独立生产者：模式 2 先计算
`n=N(wrap32(V.0-V.c),incoming.u16[6],F.174)`，再按无符号数钳到 1..3276。
负 n 因而落到 3276。V 是选中字体对象的 vslot4f0 输出，尚未绑定为此前的 T。

缩放之前，已捕获的一条调整路径可以从 ascent 减 V.c，再加 V.10；另一路径会重新
分配上下部指标。缩放之后还可能给 F.c8 加 `trunc_signed(C.18c/36)`：门控来自
在 `0x100341b44` 保存的 P.0 的 bit16/bit15、当前 C mode 和 V.byte37.bit0，
不能误用早先装入同一寄存器的 C.d4。必须保留先调整、逐项舍入、再补偿的顺序。

原始证据与分支限定见
[adjusted-font README](../artifacts/docgrid-adjusted-font-metrics-2026-09-27/README.md)。
内部字号与 OOXML 的映射、V 的实际提供者、F.184 生产者、现场尺度、可选 wrapper
和后续回调修正仍未全部闭合，因此这里没有启用生产网格算法。

## 冻结与验证

所有目录位于 `artifacts/`。表中哈希是各自 `SHA256SUMS` 文件的 SHA-256。

| 目录 | 文件数 | 清单哈希 |
| --- | --- | --- |
| docgrid-font-components-2026-09-27/ | 66 | `66f9ef0cc5f84a336fcc0e4201685f174a0c290289a1f9879cdb40c4bd8c6bd8` |
| docgrid-textmetric-adjustments-2026-09-27/ | 24 | `8c1953a88dafec79ac6be68063c369acd309310b7dc588b9dba848bb47115b7c` |
| docgrid-font-callback-2026-09-27/ | 29 | `e2c093e4191604fe3a6b19bc849ab4452ddc1a4a10a481af21662bf5049a2ff8` |
| docgrid-font-api-2026-09-27/ | 16 | `f49a3ce70d868a83d097978719076b172ee8ce890e094a985a08a2cbc9af338c` |
| docgrid-font-provider-object-2026-09-27/ | 33 | `2acbaa3d0a58d19d8324bd4d63d462baad34d51500e8152fa083a5672fbee6c0` |
| docgrid-font-source-provider-2026-09-27/ | 61 | `03f6e799d02ecf84f153f1167a6c0df3d24595bd50bc73f4ce71f70aa18a439d` |
| docgrid-float-conversion-2026-09-27/ | 12 | `3ef120400f674a9ab7a19ebca399b981f4f1b34e0ff6c658ab60ce50ab6dc9d7` |
| docgrid-font-face-inputs-2026-09-27/ | 37 | `3b3c4a2e298786e456c640edb31818a2acf13216c1d4bf9cd916fb11b800a3b5` |
| docgrid-font-request-size-2026-09-27/ | 31 | `78a5e6eec51a8875e50ae64280c9eeccae68421424eaa167785694f037a5b3da` |
| docgrid-adjusted-font-metrics-2026-09-27/ | 33 | `3fd1611a399f9a2e4fda5f3167b56d1e1c0e437b21c1f30c1fc10bddaed90ed2` |
| docgrid-content-info-inputs-2026-09-27/ | 50 | `973a180874eb1a527adad68e94617b7ec1aba0ebc0c0e5b473f42fbf5968b724` |
| docgrid-content-enable-input-2026-09-27/ | 13 | `0ca964f079a180be6af64b43ea0992ea158078550d26d61e9c66a6098203d910` |

主代理逐项核验前两组 90 文件。后两组审计核对 41 个反汇编窗口的完整指令地址
覆盖、原有三份退出状态收据、源材料哈希及 Python 语法；早期窗口仅有原始 stdout，
没有补造历史退出状态。README 指明两个跨边界初始窗口及其后补完整版本。
回调安装位置、选择条件、字段偏移和后处理时序已独立复核。
默认对象续查另核对 12 个完整小函数、24 个虚表记录和三条 RTTI 名称链，主代理
逐项复核 33 文件哈希；构造复制及字段重排也通过独立审查。原冻结材料未改写。
源提供者续查保留 9 份成功回执、22 个完整函数、35 个指针记录、两条 RTTI 名称链
及四组常数。主代理逐项核验 61 文件哈希；安装、缩放、T 投影、特殊增量和转换
边界均通过独立复核。早期一次文件名冲突的采集没有退出回执，原始输出保留，
同组函数已完整重采并核验指令一致；没有补造旧回执或改写此前冻结材料。
浮点导入片另核对导入/导出、函数起止、四个原始指令 word、完整反汇编覆盖与源码
语法；12 文件清单校验和独立复核均通过。
接口与请求片另核对 13 个完整函数、两组 GUID、六个诊断字符串、三个官方头文件
与 OS/2 来源。官方 GitHub commit API 的 403 回执保留，随后通过公开 Git ref
解析固定提交，并重新下载核对三个头文件逐字节相等。37 文件清单校验通过；
独立复核指出的门控旁路和未检查 HRESULT 两项限定已在冻结前补正。
字号 store 片核对 15 个完整函数、五个真实 fixup，31 文件清单逐项通过。
独立复核确认初始换算、manager 分派和模式 2 分支；临时 face 与最终 face 身份
尚未等同的限定已在冻结前补齐。旧 176 个证据文件保持原哈希。
构造后调整片核对 20 个完整函数、21 个既有 fixup 与 172 个旧证据文件；33 文件
清单通过。独立复核纠正了 F/C 对象身份和补偿 gate 的寄存器来源，冻结前均已补齐。
content-info 续查核对十份采集、16 个完整函数、八条直接引用及两类有界候选查询；
旧两包 112 个成员未改变，50 文件清单通过。对象地址、初始清零、读取 gate 和
后续待查边已经复核；候选数量不作为最终值为零或完整调用树无写入的证据。
宿主启用位续查核对两份采集、八个完整函数和四个旧包的 178 个成员；13 文件清单
逐项通过。独立复核保留了中间调用重读、外部 formatter 路径和未 mask 初始化的限定。

本片没有 Cargo 或 Word 回归结果，生产算法没有变更。网格输入尺度与下游 LS
四参数见[原生数据流](DOCGRID-NATIVE-DATAFLOW-2026-09-27.md)，数学 helper 见
[算法证据](DOCGRID-ALGORITHM-EVIDENCE-2026-09-27.md)。
