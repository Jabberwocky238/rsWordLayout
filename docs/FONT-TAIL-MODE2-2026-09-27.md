# 替代字体尾到 mode 2 纵向分量的条件连接

`font_tail.py` 新增 `rsword-font-tail-mode2-input/1` 输入：先运行既有替代尾，
再计算 h2、五字段缩放、c8 补偿与默认 M 的前三项更新量。输出仍为
`ARITHMETIC_REFERENCE`。原来的仅尾部输入保留，生产 Rust 算法尚未消费本接口。

## 输入合同

根对象仍是 `schema` 和 `project`。新 `project` 必填：

| 字段 | 读取点和范围 |
| --- | --- |
| v_prefix_words | 当前 V 的七个 i32 `[V0,V4,V8,Vc,V10,V14,V18]` |
| tail_p0 / tail_p8 / tail_p13 | 替代尾读取的 u32 / u16 / u8 |
| metric_byte38 | 改写后、尾部 gate 读取的 u8 |
| pre_scale_cc | 缩放前 F.cc，独立 i32，不能用 Vc 或 stackResult 代填 |
| input_size | h2 生产者所读原 incoming+6，u16，允许 0 |
| dy | 尾部、h2 生产和缩放读取的同一 F174，i32 |
| scale_y | 缩放与补偿时的 C18c，i32 |
| correction_p0 / metric_byte37 | 后续独立补偿读取点，u32 / u8 |

接口固定 C/F mode 2，不接受额外 `mode`。旧接口的 `incoming_scale` 只在 mode 1
消费，新接口因此不要求它，嵌套 tail 报告注明其未消费的占位值 0。这不是实际 TLS
尺度的读数。所有数值要求严格整数，未知字段、缺失 cc、simple-tail gate 均拒绝。

本接口是有明确前提的算术连接：V 数值前缀到 h2 读取时仍然有效；四个 tail 字段
未被中间调用改写；dy 与 mode 不变；h2 的复制被保留；随后完成 F→C 和默认 wrapper
更新，且没有进一步改变目标分量。它不模拟重试、虚方法的隐式副作用或备用 wrapper。
同样的条件不能从文件名、字体 codepage 或一次独立字体 API 调用中推断。
更具体地，h2 与补偿读取的 C.mode 和缩放读取的 F.mode 均须为 2，整个连接属于
同一次完成的选择流程，不能跨过重选后把不同字体记录的阶段结果拼在一起。

## 后处理指针边的核对

此前未分析的 `1003482c0` 完整原始指令已经存在于冻结包，本次复用它，并补取
`100348c58` 和其标量叶子 `100348f30`，总计三个完整函数、726 条指令。
[后处理证据](../artifacts/font-tail-postprocess-2026-09-27/README.md)确认：

- `3482c0` 本体构造 88 字节辅助记录，复制到 F+110..167，没有直接写五个纵向字段。
- 接收 F 指针的 `348c58` 只读取 F196、Fd5、F191，其写目标为调用者栈上的辅助记录。
  它没有继续把 F 指针传给其它函数；标量叶子没有内存读写或调用。
- SP+1ec 的独立 stackResult 按值传入，平移辅助分量并参与单独的尺度换算。
  它没有写入 F.cc，也没有修改 c0/c4/c8/d0。

这闭合了该明确指针传递边的直接写入范围，不是所有全局别名或虚方法的无副作用证明。
其它续段调用及其条件另见[续段审计](../artifacts/font-tail-continuation-audit-2026-09-27/README.md)。
其中 `100347c34` 的直接 F 写入为模式字和横向字段；条件 `102e4e3e4` 只读 V37/V38，
修改另一份局部记录并返回字体标识，后者可能触发重选。新分配记录发布时的 cc=0
有清零指令依据，但缓存、后续调用与重试仍阻止将通用 `pre_scale_cc` 默认为零。

## 计算顺序

simple 和 alternate 路径现共用 `project_vertical_fields`，只共享已核验的纵向阶段。
simple 路径仍自行产生五个输入字段；alternate 路径产生四个，再接受独立的 cc。
不会为缺失的水平字段、F184 或其它 M word 填零。

```text
h2 = clamp_u32(N(wrap32(V0-Vc), input_size, dy), 1, 3276)
for field in [c0, c8, c4, cc, d0]:
    scaled[field] = N(scale_y, wrap32(field*h2), wrap32(dy*144))
if correction_p0.bit16 && !correction_p0.bit15 && metric_byte37.bit0:
    scaled.c8 = wrap32(scaled.c8 + trunc_signed(scale_y/36))
updatedMetricWords = [scaled.c8, scaled.c4, wrap32(scaled.c8+scaled.c4)]
```

N 是既有原生有符号乘除模型，保留零分母、舍入、饱和和快路径异常。
负的 h2 中间结果按 u32 钳位，因此会到 3276，而非 1。乘积和分母先回绕再调用 N。
cc 独立缩放，不参与三个 M word；stackResult 留在嵌套 tail 报告中，不被误当作 cc。

```sh
python3 tools/measure/font_tail.py \
  --input tools/measure/examples/font-tail-mode2.json \
  --out artifacts/font-tail-mode2-new.json
```

此合成示例的四字段为 c0=800、c4=350、c8=950、d0=0，另显式 cc=17；
input_size=24、dy=1000、scale_y=6000 使缩放结果相等，三个 M 更新量是
`[950,350,1300]`。它不代表某个 Word 文档的实际参数。

另一回归用例的缩放前 c8/c4 为 951/349，分别乘 0.5 后舍入得到 476/175，
总和为 651；将合计 1300 先乘 0.5 则得到 650。逐分量舍入和先求总和不能互换，
两次 15% 加量也不能简化为通用浮点 1.3 倍规则。

## 验证与接入边界

新增 45 项模型检查和 9 项 CLI 检查，完整 Python 套件 **1,019 passed**。
CLI 报告保存输入、各阶段和本次共享纵向模块的源码哈希，已有目标不会被覆盖。
实际 CLI 示例和测试收据保存在[验证包](../artifacts/font-tail-mode2-reference-2026-09-27/README.md)。
另有[独立指令检查](../artifacts/font-tail-mode2-check-2026-09-27/README.md)：
59,862 组替代计算和 15,182 组 gate 拒绝均符合指令期望；旧 simple 模型固定到
提交 `2678d27`，22,926 组有效输入的完整结果与 55,354 组拒绝结果在抽取共享代码
前后相同。检查还覆盖 9 组非法输入、18 组独立 cc 配对，未发现本片算术差异。
本次没有修改 Rust，没有新增 Word 采集，也未把这些条件数值声称为最终 LS 高度、
PDF 原点或分页对齐。实际 Word 请求、字符集、属性位、模式/尺度及后续行聚合仍须绑定。

已审核冻结的 SHA256SUMS 清单：

| 包 | 成员数 | 清单 SHA-256 |
| --- | ---: | --- |
| font-tail-postprocess-2026-09-27 | 15 | `1b85f7959d9f1e3f60dfcce2aa648bba4f03ca800555be281e392f3282309fdc` |
| font-tail-continuation-audit-2026-09-27 | 10 | `df8b6c45b847b21b242107661a9cdf6ed13b36bfb530dcabd32c964a870ad618` |
| font-tail-mode2-reference-2026-09-27 | 19 | `31c4910834e0b6afc6f1a32bcb75621b3f45605526d02b1f9212920cc5f938f7` |
| font-tail-mode2-check-2026-09-27 | 11 | `12e048cb7a71f8a0eb2e6be830fe51ebcd0498dcef2b6daddb5d7dcaa616a20b` |
