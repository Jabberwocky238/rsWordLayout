# 字体记录后续调整的可执行参考

[font_adjustment.py](../tools/measure/font_adjustment.py) 已把构造后的 mode 2 字体
缩放与默认 wrapper 更新做成可重复回放工具。状态固定为 `ARITHMETIC_REFERENCE`，
不执行 Word 或原生函数，也不自动选择文档的字号、模式、字体或 flags。

## 精确输入边界

`project.pre_scale` 是进入 `0x100348f8c` 时的十一个有符号 32 位字段。
此前的 leading/上下部重分配必须已经反映在输入中。`scale_x/scale_y` 是该次读取的
C188/C18c；`dx/dy` 是 F168/F174；h2 是 F196 的无符号半字。
`initial_m` 表示更新前的六个 word，允许它已经经历过其它修改，不限定为首次构造值。
`c8_correction` 必须显式提供 bool，表示已确定后补偿分支是否执行。

模型分别对五个纵向字段、五个横向字段做 N 换算。每个 `field*h2` 与 `dimension*144`
先按 W 寄存器回绕到有符号 32 位，再传入 N。N 保留原 helper 的零分母哨兵、
最近整数舍入、范围饱和和 fast SDIV 的 INT_MIN/-1 例外。补偿开启时，在舍入后给
c8 加 `trunc_signed(scale_y/36)`，保留加法回绕。

随后模拟明确的 F→C 前 0x188 字节复制与默认 wrapper 投影：

```text
M = [C.c8, C.c4, wrap32(C.c8+C.c4), old_M.c, C.184, old_M.14]
```

F184 不参与十项缩放，M+c/+14 由更新前记录保留。输出只表示列明的检查点，不声称
重建完整 C 的所有字节。还要求这些值在已识别的完成选择、复制和默认 wrapper 更新
路径上成立；未展开的中间副作用、可选 wrapper 以及回调后处理没有模拟。

可选 `h2Source` 独立模拟模式 2 因子生产：
`n=N(wrap32(raw0-raw_c),input_size,denominator)`，然后将 n 当作无符号数钳到
1..3276，负数因而钳到 3276。其 h2 必须与 `project.h2` 相等，不能静默替换输入。
raw0/raw_c 来自 V，尚未绑定为此前 [font_record](FONT-RECORD-REFERENCE-2026-09-27.md)
模型的 T。工具不自动连接这两份记录。

## 合成样例与验证

仓库的 [合成输入](../tools/measure/examples/font-adjustment-synthetic.json) 使用
Sx=1440、Sy=2880、dx=dy=2000、h2=24，缩放前 c8=1900、c4=400、184=7。
因此 c8=456、c4=96，总和=552；原 M+c=11、M+14=13 保留，结果为
`[456,96,552,11,7,13]`。仅将补偿改为 true 后增量为 80，结果变为
`[536,96,632,11,7,13]`。这两个例子不是 TNR12 实测，也没有使用分页边界拟合输入。

纯模型测试包含独立有理数舍入对照、符号边界、回绕、两个尺度、保留字段和负 h2
中间值的无符号钳位；CLI 测试核对原始输入/源码哈希、因子一致性、非法输入拒绝和
已有输出保留。定向测试 76 项、完整量具套件 505 项均通过，独立指令/源码复核
未发现阻断问题。输入/schema/输出的入口说明见
[量具 README](../tools/measure/README.md#字体记录后续调整参考)。

两次真实 CLI 执行均正常退出，得到上述合成结果。原始输入、报告、命令和 UTC 回执
保存在 [回放目录](../artifacts/font-adjustment-reference-2026-09-27/README.md)，七个成员
的清单 SHA-256 为 `488a654916a26032c79267ea95e4d136f575b8e09c4b72361f15163e5dd60307`。
本片没有生产 Rust 修改，也没有运行 Cargo 或重新导出 Word 文档。

静态依据见[字体分量证据](DOCGRID-FONT-COMPONENTS-2026-09-27.md#构造后的字体调整与缩放)，
其 adjusted-font 片 33 个成员的清单 SHA-256 为
`3fd1611a399f9a2e4fda5f3167b56d1e1c0e437b21c1f30c1fc10bddaed90ed2`。
每份报告记录该绑定、原始输入 SHA-256 和 CLI/模型/输出发布器源码 SHA-256。
