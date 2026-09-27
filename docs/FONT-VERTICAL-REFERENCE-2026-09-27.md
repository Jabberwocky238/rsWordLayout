# 从实测字体记录到纵向调整的连接

[font_vertical.py](../tools/measure/font_vertical.py) 将已验证的 Face1 原始测量、T/V
提供者关系和 mode 2 简单调整路径连接起来。工具重新检查测量协议与字体字节，计算
T，再计算五个纵向字段、h2、缩放和条件补偿，最后输出默认 M 的前三个 word。
调用方无需再手工复制 T 或填入 preScale；输出仍是 `ARITHMETIC_REFERENCE`。

这个接口为已选定的原生路径提供可重复的数值校验。它不自动将 DOCX 字号、公开
属性或某次 Word 排版映射到内部参数，也不将更新后 M 的三项等同于 LS 最终高度。
实际对象与调用证据见[提供者身份](DOCGRID-FONT-COMPONENTS-2026-09-27.md#原始-v-与提供者-t-的身份)。

## 精确输入

`--measurements` 指向成功的 `rsword-dwrite-metrics/1` 报告；`--input` 使用
`rsword-font-vertical-input/1`，必须包含 `font` 与 `vertical`。

`font` 的 `lf_height/width_scale/escapement` 交给已有 font_record 校验器。它重新解析
原始 stdout 协议，核对采前/采后绑定、固定库、字体 SHA 和 OS/2，要求准确请求尺寸的
GDI 记录，不插值，也不信任报告中缓存的派生数值。当前仅支持已定义的 Face1/non-CFF
单字体 TrueType 路径；此处的 T 与初始 M 都保留在嵌套报告中。

`vertical` 显式提供以下检查点：

| 字段 | 含义 |
| --- | --- |
| tail_p0 | 简单尾读取的 P 前四字节，u32 |
| tail_p13 | `0x100347770` 读取的 P+13 字节，u8 |
| metric_byte38 | 该 gate 读取时 V+38 的字节，含此前可能改写，u8 |
| input_size | h2 生产者读取的 incoming record+6 半字，u16 |
| dy | h2 生产和缩放时的同一个 F174，i32 |
| scale_y | 缩放时的 C18c，i32 |
| correction_p0 | `0x100341b44` 保存、后用于补偿判定的 P0，u32 |
| metric_byte37 | 补偿判定读取的 V+37 字节，u8 |

偏移均为十六进制。两个 P0 是不同读取点，工具不自动令其相等。C/F mode 2、
数值 T 前缀在相关读取点未变、dy 在两个读取点一致、完成 F→C/default-M 更新，
均为明确条件。恢复、字体替换、其它 V/F 改写、备用 wrapper 和 LS 聚合未包含。
不会为水平字段、F184 或另外三个 M word 填零，也不消费初始 M 代替更新结果。

并行的[字号来源调查](DOCGRID-FONT-COMPONENTS-2026-09-27.md#字号请求的实际写入)
已把初始请求半字追到 run L+98 的实际复制。具名诊断还区分了请求前缀 +6 与
选中字体记录 F 的 +196；它们没有证明原始字段直接来自 `w:sz` 或实际使用半点单位。
因此本接口的 input_size 继续保留为显式内部半字，未自动读 DOCX 字号填入。

[请求身份续查](../artifacts/docgrid-font-request-identity-2026-09-27/README.md)
进一步确认 input_size 在 `0x100341be4` 来自原 incoming 对象 I+6。LOGFONT builder
及 F 请求前缀使用的局部 P6 已经过另一次至少为 1 的钳制，不能拿 P6 替代该读取。
本接口允许 input_size=0，由现有 h2 算术执行自己的钳制，没有提前把输入改成 1。

## gate 与算术

工具逐步重放 `0x100347770` 的 gate：初始 selector 为 `(P13>>2)&7`；selector=4
时还看 P13 的符号位和 bit0。进入 V38 检查后，`{0x80,0x81,0x86,0x88}` 会走
替代路径。凡是选择 `0x102e4dcac` 的输入都明确拒绝，不默认为简单尾。

在已选中的 mode 2 简单尾中，两个相关条件必须从同一 tail_p0 计算：

```text
c0 = wrap32(T4-Tc) if P0.bit16 else T4
c8 = c0 if (P0.bit15 or P0.bit16) else wrap32(c0+T10)
c4 = T8; cc = Tc; d0 = 0
```

原生代码的非对齐 `u16[P+1]&0x180` 正好读取 P0.bit15/bit16，所以“减 Tc”与
“仍加 T10”不能独立指定为同时发生。h2 则复用已恢复的有符号乘除和无符号钳位：
`N(wrap32(T0-Tc),input_size,dy)` 的结果按 u32 钳到 1..3276，保留负值的特殊结果。
dy 不自动等于 T0-Tc；普通创建成功可以跳过恢复路径对它的覆写。

五项按 c0/c8/c4/cc/d0 顺序，用 `N(scale_y,wrap32(field*h2),wrap32(dy*144))`
换算。补偿取独立 correction_p0 的 bit16 置位、bit15 清零和 V37.bit0 置位；成立时
给 c8 加 `trunc_signed(scale_y/36)`。最终只投影 `[c8,c4,wrap32(c8+c4)]`。
每步保留中间量和回绕，既不钳负值，也不把算术和物理占高混为一谈。

## 明确参数下的数值

[示例配置](../tools/measure/examples/font-vertical-explicit.json) 显式采用
lf_height=-2048、width_scale=1、escapement=0、input_size=24、dy=2048、scale_y=294912，
tail_p13=8、V38=0，两个 P0 和 V37 均为零。它不是从 DOCX 自动提取的运行配置。

对既有 TNR API 测量，T 前缀为 `[2268,1825,443,220,87,821,821]`，得到：

| 阶段 | 数值 |
| --- | --- |
| preScale c0/c8/c4/cc/d0 | 1825 / 1912 / 443 / 220 / 0 |
| h2 | 24 |
| 缩放后 c0/c8/c4/cc/d0 | 43800 / 45888 / 10632 / 5280 / 0 |
| 更新后的 M0/M4/M8 | 45888 / 10632 / 56520 |

只把 tail_p0 改为 bit15 置位时，T10 不再加入 c8，结果成为
`[43800,10632,54432]`。只把 tail_p0 改为 bit16 时，先减 Tc 且不加 T10，结果为
`[38520,10632,49152]`。在后一配置上另令 correction_p0.bit16 与 V37.bit0 置位，
补偿增加 8192，结果为 `[46712,10632,57344]`。
这些是同一实测 API 输入在不同显式 flags 下的算术结果，不证明某份文档的活动 flags，
也没有将这些值换成 PDF 原点或宣称分页反例已修复。

报告保存原始配置 SHA、测量来源与字体 SHA、各层模型及 CLI 源码 SHA、固定静态
证据清单和逐阶段结果。缺项、重复键、未知字段、不可用分支或失效测量会失败，
输出继续使用原子发布器并拒绝覆盖，包括悬空符号链接。

模型 83 项、CLI 28 项定向测试通过，完整量具套件 708 项通过。独立复核核对了
冻结指令、共享属性位、逐项整数换算及来源校验，未发现阻断问题。上述四种配置均
已通过真实 CLI 回放并正常退出；[回放目录](../artifacts/font-vertical-reference-2026-09-27/README.md)
包含输入、完整报告和命令回执。11 个成员的清单 SHA-256 为
`80e17b33c2c59c186b8d3534cf134612ce85209adc73a2ef4a1bec218d4a9e4d`。
本片未调用新的原生 API、未操作 Word，也没有修改生产 Rust 或运行 Cargo。
