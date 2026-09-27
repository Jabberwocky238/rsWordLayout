# 普通区间的 PTS 容纳与末行裁减

本片将已知普通矩形生产路径接到 `FsFormatLineSimpleW`，并继续闭合其底部空间和
overhang 宿主回调。核心结论是：普通区间在进入 Story 分派后选择 SimpleW；它的
容纳检查和保留后的行高修改不能用 Chain 比较替代。这仍是有条件的静态路径证据，
不是已采集 Word 文档的运行分支观测，也没有启用生产网格算法。

绑定 Word 16.112.3 / 16.112.26083020，宿主整体 SHA-256 为
`b569d257c22eb75b990ff8212b75c734ae20f51e9dacc9ccc509c988a6e0d52c`，
PTLS 为 `cf7699ab47748bdf16ad1a5e8ba06803d99f7e4c4533bd57974ba9bbf2c89492`。
地址均为 arm64 未加 slide 的虚拟地址，字段偏移为十六进制字节。

## 分派与区域来源

`FsFormatStoryW` 在 `0x19ab74..19ab9c` 计算：

```text
use_chain = L.i32[0x10] > 1 || L.ptr[0x18].i32[8] != 0
```

这里是 empty-space 区间数和首区间的第三个 word，不是页面栏数。重试分支可以
回到此处重算；两个下游调用点使用保存的分类。前片已证明的普通矩形成功路径
返回 count=1、第三 word=0，因此选择 SimpleW。更上游还可能走文本复用缓存、
`FsFormatTextSimple` 或 StoryG，不能仅凭“单栏文档”认定实际执行 SimpleW。

textfi+20 的矩形也已追到上游 `0x143f8` 的入参 Q。方向匹配时：

| 条件 | textfi 的四个矩形分量 |
| --- | --- |
| 几何对象为空 | Q0、Q1、Q2、Q3 |
| 几何对象有效且列几何 getter 成功 | Q0、shaft[1]、Q2、Q1+Q3−shaft[1] |

shaft 来自具名 `FsGetCurrentColumnDimensionCore`，继续转到几何提供者的虚表 +40。
附近 `FsGetPageRectangleCore` 的两个输出另存 textfi+30/+40，并不是这个 +20 矩形。
方向不匹配有独立矩形转换；错误输出没有视为正常几何。Q 的更上游来源和物理单位
仍待确认，当前不能令它等于配置页高减边距。

SimpleW 的剩余量为 `limit=end−q`，其中
`end=L[0]+(wordspec.bit0 ? L[8] : L[4])`。普通矩形分支还通过 Core 输出、
`FsAssignLrW` 和 Story 反相条件启用底部空间查询；不能把该分支的 S 默认为零。

## 容纳之后还会修改推进

令 U/V 为 PTS 元素最终参与检查的两个分量，N 为 32 位相加结果 U+V，S 为
`GetDvrSuppressibleBottomSpace` 返回值。此前可选的 ChangeSplat 回调可能已修改 U/V。
`0x1920c4..192138` 的行为是：

| 条件 | 结果 |
| --- | --- |
| N<=limit 且 N−S<=limit | 保留 U/V，构造行 |
| N−S>limit | 返回不能容纳的结果并清理元素 |
| N>limit 且 N−S<=limit，允许 overhang | 保留 U/V，构造行 |
| N>limit 且 N−S<=limit，拒绝 overhang | 保留 U，改 V=limit−U，再构造行 |

这些加减按 W 寄存器回绕，比较有符号；第一行不能省掉第二个条件，因为 S 未被
限定为非负。查询失败走错误清理。修改器保存旧 U/V 并标记改动。该路径构造单元素
行，后续 `FsGetLineDvr` 因而返回修改后的 U+V：正常保留时为 N，裁减时为 limit。
外层仍有附加对象、停止、保留和重试逻辑，不据此推定整个页面状态机。

因此固有 advance/required 分离仍有必要，但还不充分：保留后的最终 advance
可能依赖可用空间和 overhang 决策。不能先永久固定行推进，再只用 required 决定移页。

## S 的真实宿主来源

已注册的 context+1b8 回调解码到宿主 `0x1034d47fc`。它读取 line client 的
ptr+38 指向记录的首 word；指针为空则返回零。该 client 与宿主行记录 R 是不同
对象。已闭合的新建 packer 路径将 client 的 128 字节清零，附加记录也从零初始化。

packer 的底部空间候选为：

```text
R.byte[0x8d].bit1 clear: s = Rd4 - Rf8
otherwise:              s = U_at_writer + V_at_writer
```

后一式使用扣 delta 后、加 alpha 前的 U/V，不等于无条件的最终 N。候选还受门控：
cookie 对象 c6==1 时，一个策略 helper 可以跳过整个写入；否则该路径先钳到非负。
非零候选才分配并写入记录。其它 c6 路径不做这个非负钳制。

格式化回调随后还会在入参 w6 非零、cookie 对象 c5.bit0 置位，或具名 client 条件
与未展开 helper 组合成立时清零 S。alpha 可以改变 U，但不会在这里重新计算 S。
提前成功的其它 producer 和未展开调用的额外作用仍未全部覆盖。

在前述 K=Rd4 的条件下，普通候选是 **K−Rf8**。Rf8 的来源尚未闭合，不能直接令
S=K。PTS 还验证 S 属于 `[-0x3fffffff,0x3fffffff]`，不是通过钳零代替验证。

后续对 Rf8 的有界追踪确认了两个清零位置。新分配路径
`0x100352eb8 -> 0x100060f80` 在 R+f0..10f 写零，覆盖 f8；池复用分支跳过
这个构造器。另一个真实生产调用 `0x10035094c -> 0x100353110` 条件性地清零
同一 R 的 f4..fb，仍不是每行必经操作。

这些位置只证明当时的值为零。随后 `LsCreateLine` 执行回调，R 也已发布到全局
别名；非零写入者尚未绑定。旧高度路径确实读取 Rf8 并将其并入下部量。
新旧网格函数体未见直接写入，也不能证明其全记录 hook、间接调用及后续调整
保持字段不变。因此继续保留 Rf8 为未知输入，没有以 fresh-zero 推导 S=K。

对全局别名的续查又区分了两个槽。`0x1048a5658`（池 +38）的已证读者仅在失效
遍历中跳过相等记录；它不是已证明的格式化回调 getter。分配/取回函数另把同一 R
存到 `0x1048a5650`（池 +30）。另一条 LsCreateLine 路径返回后读取 +30 的当前值，
将它传给 `0x1034efe84`，或进入前述 `0x100353110` 条件 reset。槽本身可变，
没有假定跨嵌套格式化仍指向原来的记录。

`0x1034efe84` 完整 1016 字节内，对已证明的 R、R+14、R+128 别名没有覆盖
f4..fb 的写入；其它指针参数与未展开 callee 仍可能有别名或副作用。pair/vector
与动态索引的候选中又排除了栈副本和 OpenType feature 数组，未把同偏移当作同对象。
非零写入者仍未找到；下一条证据必须来自真实 callback/参数别名，不能据这些局部
负查推导最终 Rf8 恒零。

## overhang 的宿主决策

context+250 解码到 `0x1034d7510`。它先在 name client+18 为正或 cookie 对象
c5.bit0 置位时拒绝；通过这些条件后，元素 kind!=3 时允许。

kind==3 时计算另一个换算量 v。其上游仍是 `DyaFetchCoalesced`，但 selector=0，
与 delta 路径的 selector=1 不同；随后使用 cookie 对应尺度和 1440 做原生整数换算。
令 e=N−limit，最终为：

```text
allow = e > v || (e == v && paraclient.byte[0x10].bit0 != 0)
```

这段比较与真正的调用参数相连。它并未证明 v 的公开段落属性身份，也没有绑定探针
的实际 flags。不能把 overhang 设成一个全局常量，或将同一个 coalesced 结果同时
套入 delta 与此处。

## 验证与接入边界

冻结目录均位于 `artifacts/`，表中是各自 `SHA256SUMS` 的 SHA-256：

| 目录 | 文件数 | 清单哈希 |
| --- | --- | --- |
| docgrid-pts-region-source-2026-09-27/ | 24 | `5a720498d3c8639e842f00be73907e6569bbf17249b0fe4b9776fe3d175d46ca` |
| docgrid-simple-bottom-space-2026-09-27/ | 19 | `40d0418bc292bebf12676f481be3e1221beda28647895f3972a3257a097d68c5` |
| docgrid-bottom-retained-component-2026-09-27/ | 16 | `b8f814a9db27ecbc6422c1a885299590e401a1d5f5812786f684043b41a8086e` |
| docgrid-bottom-retained-writer-2026-09-27/ | 27 | `bff8c428fdf29e87bad114d03b0fbe35bb8f1d64b38128f86783d6cfc297a7d5` |

主代理逐项校验通过。区域分派、矩形存储、S 查询参数和 Simple 比较/修改已独立
复核；宿主片的五个完整函数、两个实际 fixup、client 身份、候选写入/清零和
overhang 公式也经独立审查。冻结前纠正了“分类只计算一次”、特殊分支跳过整个 S
写入，以及候选 U/V 在 alpha 之前这三项限定。旧冻结材料没有改写。
Rf8 续查的实际 R 身份、新分配/复用区别和条件 reset 也通过独立复核；
补齐了后段调用边的旧原文与哈希绑定，16 文件逐项校验通过。
别名续查的两个槽、实际参数传递、基址重定位和候选排除通过独立复核；
27 文件清单逐项校验通过，没有将函数体的负查扩大成调用树不变性结论。

仍需确认实际运行分支、Q 与字体尺度、Rf8、coalesced 属性/缓存和宿主修正。
[原有 U/V/J/K 与 delta/alpha](DOCGRID-PTS-HEIGHTS-2026-09-27.md) 的条件继续有效。
本片仅有静态验证，没有 Word/Cargo 回归，也不宣称已修复已采集的分页反例。
