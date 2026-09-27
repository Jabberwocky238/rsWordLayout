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

下一步精确入口是 ChainW 参数 8/9、bit0 的生产者，U 的 delta 调整，以及
返回高度到页面/栏/区间布局的后续消费者。

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

本片只验证静态证据与文档，没有运行 Cargo 或 Word 回归；不宣称已修复生产引擎
的网格分页差异。
