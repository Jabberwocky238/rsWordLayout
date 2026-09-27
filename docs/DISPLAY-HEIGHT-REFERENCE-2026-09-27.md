# 显示行高与端点量化参考

[display_height.py](../tools/measure/display_height.py) 已把宿主的双端点换算和行高调整
连接成可执行参考。它接收显式原生整数，保存中间值、直接写入和 LS 转发参数，输出
`ARITHMETIC_REFERENCE`。这补上网格计算之后的一段真实宿主处理，不代表已求出
文档的运行尺度、最终基线或 PDF 坐标；生产 Rust 的网格默认行为未改。

## 已闭合的路径

固定 Word 16.112.3 的 `1034dd6c0` 调用已知产行入口 `100350374`，从输出槽取得
同一行记录 R。此前条件处理可能改变 R.b8/R.b4/R.c4，因此输入必须是后续读取点的
当前值。`1034de750..784` 实际执行：

```text
endpoint = wrap32(origin + R.b8)
requested = wrap32(N(endpoint, targetScale, sourceScale)
                  - N(origin, targetScale, sourceScale))
adjust_1034941c0(R, requested)
```

`sourceScale` 是已选轴的 slot620 字，`targetScale` 是 slot628 字。N 复用已恢复的
`100064708`，保留最近整数、半值远离零、零除数以及快慢路径的回绕/饱和特例。
R.b8 是源高度；旧目标高度 R.b4 是独立输入，两者不能合并。

这解释了一个具体的整数相位来源。例如源比例 2、目标比例 1、源高度 1：起点 0
得到目标高度 1，起点 1 得到 0，连续六段得到 `[1,0,1,0,1,0]`。逐段单独换算
高度会累计为 6，端点差累计为 3。这些是合成整数，不是 Word 探针的实际尺度。

后续主显示入口在各次方向读取稳定为 0 时，传给 `LsDisplayLineNew` 的点是
`(input4-R.19c, input5-R.bc)`，两项均为 32 位回绕。静态路径已经把格式化行与显示行
通过实际指针键绑定。因此初始网格 tuple 的 R.bc 不能无条件代入显示坐标：它可能
已经经过上述调整。两个未展开的宿主调用、实际方向与尺度选择、LSDEVRES 换算和
最终绘制回调仍是后续边界，工具不跨越它们推导 PDF 原点。

## 调整合同

`adjust_line_height` 复现 `1034941c0` 的直接操作：

1. 令 `delta=wrap32(requested-old_b4)`，先把
   `wrap32(old_bc+max_signed(delta,wrap32(-old_d4)))` 写入 R.bc。
2. 将 requested 钳到至少 0 得到 target。若格式化行存在，且
   `wrap32(target-old_b4)` 非零，计算四项高度并调用转发器。
3. 最后直接写 R.b4=target、R.byte1c0=1。

缩减分支使用 `SUBS` 结果的符号位，即 `B.PL`，不是有符号 `target>=old_b4`。
例如 target=INT_MAX、old_b4=-1 时，差回绕为 INT_MIN，仍进入缩减分支。
该分支依次处理 d4 和 c4，保留每次负号、加减和有符号比较的原生行为；负 requested
也必须先参与 bc 更新，再钳为零。没有行对象时仍执行上述直接写入。

输出 `directStores` 标注转发前后的位置；`forwardedHeights` 是给 `100379ef4` 的
四个有序整数，未调用时为 null。该转发器调用 `LsModifyLineHeight`，并可能调用
`LsModifyDisplayLineHeight`。模型不模拟其内部效果，尤其不把转发的 c4/d4 当作
宿主对应字段的直接写入，也不声称转发后的 R.bc 最终值仍等于之前的 store。

## 回放与验证

```sh
tools/measure/.venv/bin/python tools/measure/display_height.py \
  --input tools/measure/examples/display-height-explicit.json \
  --out artifacts/display-height-new.json
```

输入必须包含 `conversion`、`record` 和显式 Boolean `hasLine`。所有数值均为
有符号 32 位整数；浮点数、重复键、未知字段和用 bool 冒充整数都被拒绝。
报告绑定原始输入、CLI、模型、换算 helper 和发布器的 SHA-256，并保存静态证据
清单哈希。输出使用已有原子发布器，拒绝覆盖文件或悬空符号链接。

93 项专项测试通过，完整量具测试 801 项通过。测试覆盖起点相位、独立有理数对照、
端点/差值回绕、两阶段缩减、负请求、无行对象、B.PL 溢出和严格 JSON 入口。
命令、退出码、代码哈希和真实 CLI 输出保存于
[回放目录](../artifacts/display-height-reference-2026-09-27/README.md)。
该包的 12 成员清单 SHA-256 为
`f75a9ed882dbb9a0ec3eb3aac0a2be6cfcc8512602f747ff46ceb90dfe5b089a`。
本片没有运行 Word、操作桌面、附加进程或重新执行 Cargo。

新增静态证据保存于
[显示点来源](../artifacts/docgrid-display-point-source-2026-09-27/README.md)，
含 17 个完整函数，已独立复核并冻结，42 成员清单 SHA-256：
`c5bd14d9a1c3656dbd2095a4043a78ef0613a300bf306025aff20d89f58c57f1`。
调整体与转发器复用此前冻结证据，清单哈希也写入每份报告。
