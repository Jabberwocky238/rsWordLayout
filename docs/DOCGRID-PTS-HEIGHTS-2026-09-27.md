# PTS 行高返回与容纳判断

本片把 Word 宿主行记录接到具名 PTS 回调、ascent getter 和实际高度比较。
已核对回调注册、指针转交和字段读写，尚未把比较边界绑定为物理页面底部，也未
证明本批探针的实际执行分支。全部来自本机 Word 16.112.3 的离线 arm64 文件分析，
没有启动或附加 Word，没有采集、原生执行或生产代码修改。

## 回调注册

宿主 `0x1000644bc` 将静态表 `0x10457eed0` 的 2c8 字节复制到
`fscontextinfo+40`；`FsCreateContextCore` 再复制到 `fscontext+30`。
磁盘 chained fixup 的实际链成员解码得到：

| 接口 | 表偏移 | context 偏移 | 宿主目标 |
| --- | --- | --- | --- |
| FscbkFormatLineWord | +228 | +258 | 0x1034d7618 |
| FscbkGetDvrAdvanceWord | +238 | +268 | 0x1034d835c |

两个 PTS 包装器确实读取并调用这两个槽。这是完整的静态注册边，不是按相同偏移
猜测对象身份；也不是运行时指针读取，不能排除之后的替换。它们与 LS 的字体
回调 context+a8 属于不同接口。

## 四项输出的传递

令 E 为 PTS element，T=E.ptr[0] 为 line bubble，C=T.ptr[50] 为另一数据记录，
R 为宿主行记录。参数序号从零开始。PTS 包装器在调用宿主时先插入一个临时 int*，
因此后四个保留输出的参数序号整体加一。

| 中性名 | PTS 参数 | 宿主参数 | 最终字段 | 宿主初始写入 |
| --- | --- | --- | --- | --- |
| U | 29 | 30 | C+18 | R.b4−R.bc |
| V | 30 | 31 | C+1c | R.bc |
| J | 31 | 32 | T+88 | R.b4−R.d4−R.cc |
| K | 32 | 33 | T+8c | R.d4 |

三层宿主调用的栈槽映射和 bubble 构造参数逐项保存在冻结材料 `mapping.json`。
`FsGetLineElementAscent` 在 `0x18ef14..18ef20` 返回 C+18，所以 U 已有具名
ascent 消费者。但这不等于 PDF 字形原点、原始字体表 ascent 或引擎最终基线。

这些是进入宿主 `0x1034d7bf0` packer 路径后的初始写入。此前 `0x1034d7a98`
调用 `0x10034eed0` 时也传入四个输出指针，若其返回 bit0 为 1，宿主直接成功
退出，绕过该 packer。这个 callee 尚未展开，不受表中四项公式覆盖。

在 packer 路径，宿主随后可按条件调用 `0x1034b3ca8` 并从 U 减去返回的 delta；
delta 的公开语义尚未知。packer 返回后还会在下列全部条件成立时给 U 加 alpha：
宿主参数 6 为零，cookie.ptr[10] 的 byte[c5] bit0 清零，R.e4 非零，且
`alpha=signed32(R.e4−宿主参数8)` 至少为 1。加法发生在
`0x1034d7c6c..7c78`，不能遗漏在 packer 之外的这一层修正。

另有 R+128 的 bit6 同时将 U、V、K 清零，而不清 J。独立 SimpleW 路径还能
通过 ChangeSplatLineHeight 改写 element 高度，不能省略。

## 实际比较与 advance 接口

`FsFormatLineChainW` 生成 `tagLineHeightsWord` 的两个 word：

```text
heights[0] = J
heights[1] = U + V - J - K
```

随后按输入 `fsemptyspacewordspec` 的 bit0，执行有符号比较：

```text
bit0 != 0: reject if U + V > input argument 9
bit0 == 0: reject if U + V - K > input argument 8
```

拒绝路径写入数值为 2 的 `_klinesuccess` 并清理。函数还包括区间高度检查；
列表层在一个额外条件下比较 `max(U)+max(V)` 与参数 8，并可清除整个 element
列表。因此上面两个逐元素比较不是完整接受条件。参数 8/9 的坐标原点与公开含义
仍待追踪，不能仅凭函数名将它们称为页面剩余高度。

独立的 GetDvrAdvanceWord 回调返回：

```text
out6 = R.b4 - R.d4 - R.c4
out7 = R.c4
```

PTS 的特定空列表分支传入 `out6=&heights[1]`、`out7=&heights[0]`，顺序相反。
该调用还受另一个输入 flag 控制；它不是普通每一行都会执行的接口。
上述加减与比较应按原始 32 位指令语义解释，不能忽略溢出后再作有符号比较。

## 与网格 helper 的有条件组合

只有满足此前普通适配器、同尺度指针、无清零、无改变结果的 hook 等全部条件，
且该适配器确实生产了当前 R，再加上本片进入 packer、delta=alpha=0 和无输出抑制，才可代入
[原生数据流](DOCGRID-NATIVE-DATAFLOW-2026-09-27.md) 的中性元组：

```text
R.b4=H+B+C    R.bc=A+C    R.c4=B    R.cc=H    R.d4=C
U=H+B-A      V=A+C       J=B       K=C
tagLineHeightsWord=[B,H]
two per-element fit quantities: H+B+C or H+B
```

GetDvrAdvanceWord 在相同假设下也得到结构 `[B,H]`，独立印证了输出顺序。
若保留已知的两项 U 修正而不清零，普通 packer 路径的 U 为
`H+B−A−delta+alpha`，结构第二项为 `H−delta+alpha`。因此仅排除 delta
与清零仍不足以得到 `[B,H]`；需要同时排除宿主返回层的 alpha。
此处 H/A/B/C 保持原来中性含义；本片没有把 B 命名为行距或把 C 命名为某个
公开段落属性。共同生产入口 `0x100350374` 到网格适配器的调用树尚未展开，
所以这组代入是条件式，不是探针实际运行证明。

这为后续区分推进与占高提供了具体接口。当前生产引擎仍保留网格声明但未应用；
已测得的 `(4277,4278]` twips 页面容纳区间仍是
[采集约束](DOCGRID-PAGE-FIT-2026-09-27.md)，没有被当作这些字段的单位证明。

参数 8/9 和 bit0 的生产者由后续输入片进一步追到区间记录；物理页面/栏边界、
宿主 U 的 delta 调整及运行分支仍未全部闭合。

## 区间输入与实际推进

`FsFormatLineChainW` 的唯一直接调用者位于 `0x19d504..0x19e7c0`，该函数
又被具名 `FsFormatStoryW` 的两处调用。Story 通过 `FsAssignLrW` 填充记录 L，
调用者在选择 Chain 路径后，初始化：

```text
remaining8 = L.i32[4]
q = L.i32[0]
end = q + L.i32[8]
Chain argument 8 = remaining8
Chain argument 9 = end - q
```

普通保留行的实际推进由 `FsGetLineDvr` 计算：非空元素列表返回独立的
`max(U)+max(V)`，空列表返回零。两个最大值可能来自不同元素。调用者用该值 d
执行 `q+=d` 和 `remaining8-=d`；推进不是 `tagLineHeightsWord[1]` 单项。
单元素且满足前述元组与宿主修正条件时，d 为 `H+B+C−delta+alpha`。

两限值仍不能合并：附加对象路径另给 q 增加一个返回量，却不增加用于扣减
remaining8 的 d；另一个结果分支把 q 钳到 end，却从 remaining8 扣去完整请求量。
其它结束、清理和错误分支另行处理，这些递推式不是整个 Story 状态机。

L 的两组数组分别提供 empty-space 和 wordspec。其生产者用 textfi+20 的
`tagFSRECT` 构造当前位置 v 之后的剩余矩形：

```text
rect = [textfi[0x20], v, textfi[0x28], textfi[0x24]+textfi[0x2c]-v]
```

`FsWordGetEmptySpacesCore` 在几何签名/方向匹配、矩形高度非负且满足已记录的
简化障碍物状态条件时，将 L[4]、L[8] 都设为 rect[3]，wordspec 的低三位清零。
这条成功路径的第一次 Chain 调用有相同的两个上限且 s=0。障碍物路径则会独立
缩减两项，并由局部状态决定 bit0；相等不是 L 的通用不变量。

两个具名 getter 也补齐了字段的消费方向：`FsGetLineElementSpaceBefore`
读取 T+88=J，`FsGetLineElementSpaceAfter` 读取 T+8c=K。在前述有条件的组合中
对应 B/C，但这些名字不证明原始 OOXML 段距、折叠规则或物理单位。

这里已接到实际矩形运算与游标更新，仍未证明 textfi 矩形属于页面、栏还是其它
文本区域。下一条精确入口是 textfi+20..2f 的写入者、Story 选择 Chain/Simple
的条件，以及障碍物局部状态的生产者。前述 U 的修正、提前成功、清零和列表级
检查全部保留，不能由这条简单矩形路径消去。

## 冻结与修正记录

原始材料 `artifacts/docgrid-pts-fit-consumers-2026-09-27/` 共 42 文件，清单
`SHA256SUMS` 的 SHA-256 为
`bf98e9c74f442fccb3316def0eb4c455da98d088ab120cb63ede10473e87e638`。
主代理逐项校验通过。四组栈槽、回调存储顺序、初始写入及 U 修正已交叉复核；
回调注册按实际磁盘 chained-fixup 链验证，未把原始编码 qword 当作地址。

**该冻结集必须与修正补充共同阅读。** 冻结后发现其条件组合漏列了 packer 返回层
的 alpha 增量；初版独立复核也未覆盖这一项。没有改写旧材料。补充文件
`artifacts/docgrid-pts-fit-consumers-2026-09-27-post-packer-correction.md`
的 SHA-256 为 `5c95b68e9d27a8a400310a65236ae3a67b7282c5df4b7310b9f23aff07d87295`，
绑定旧 README 与原始指令，记录全部触发条件。本入库报告已采用修正后的条件式。

后续输入材料 `artifacts/docgrid-pts-fit-inputs-2026-09-27/` 共 26 文件，清单
`SHA256SUMS` 的 SHA-256 为
`f9b49ec33232ebc136eeb32cc4d209cd579ef3b3660e1f9aa60aabb2cf48042d`。
主代理逐项校验通过；区间限值、Dvr 推进、条件矩形输出和 J/K getter 已独立复核。
未命名障碍物 helper 的参数只记录已证明的栈位置，没有推定参数序号或公开含义。
旧 42 文件及修正补充均未改写。

本片只验证静态证据与文档，没有运行 Cargo 或 Word 回归；不宣称已修复生产引擎
的网格分页差异。
