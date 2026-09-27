# 字体指标到初始记录的可执行参考

`tools/measure/font_record.py` 将已测 API 指标接到恢复出的宿主转换步骤，输出
定义的临时 U 字段、T+0..18 的七个数值 word，以及构造时的 M 六元组。
它补齐可执行的数值链，状态为 `ARITHMETIC_REFERENCE`，不表示运行过宿主 T/M
函数或证明生产 Word 的输入。原始 API 测量仍独立保留 `MEASURED` 状态。

## 输入与真实数据绑定

工具要求同一个成功 DirectWrite 报告中的 GetMetrics 设计单位数 D、GDI@D 和
非复制路径的 GDI@float32(-L)。读取报告原始 stdout 再验证完整调用/释放协议，
并核对当前字体文件 SHA-256。只接受单面 TrueType，读取唯一 OS/2 表，要求
实际表长至少 78 字节，按有符号半字取 xAvgCharWidth。缺少精确请求的 API 结果、
重复请求读数冲突、字体文件变化或未成功的测量都会拒绝，不能拿原始字体表插值。

L（lfHeight）、Q150 的 width scale 和 lfEscapement 显式给定。模型要求负 L 且
取负后为正、D 非零和有限 binary32 width scale；这些是当前参考工具支持的输入
范围，并非声称宿主对所有其它输入都会拒绝。Face1 失败、CFF 分支、无效 OS/2、
其它提供者路径及完整 T 的未知字节均未模拟。

## 转换语义

- 每次整数到 float、除法和乘法都按 binary32 分别舍入，不能合并比例或最后才转换。
- H 按已恢复的 VarR8FromR4 扩宽、double ULP 外推、同号 0.5、ceil/floor、范围
  检查与 INT32_MAX 哨兵执行。这里仅实现指标调用所用 m=d=1；不推成整个 helper。
- 非复制路径对八个扩展有符号字段、宿主附加半字及九个基础字段逐步缩放。
  STRH 保留低 16 位，有符号读回可以改变符号；它不是饱和转换。
- `float32(-L)==float32(D)` 时保留定义的 50 字节内容对应字段；不伪造末尾未写两字节。
- T 的 typographic 分支读初始化 Q 的标志，不能用临时 U 的标志替代。
  平均宽度候选先将附加半字有符号读回，再乘 Q150、执行 H，最后钳到至少 1。
- lfEscapement 为 900 或 2700 时才计算 e，给 T 的前四个 word 加 `[2e,e,e,2e]`。
  W 寄存器加法与移位保留 32 位回绕；其它三个 T word 不因此改变。

代码见 [font_record_reference.py](../tools/measure/wordmeasure/font_record_reference.py)。
原始指令、接口和导入绑定分别来自已冻结的 source-provider、face-inputs 与
float-conversion 证据片，其哈希写入每份结果。模型采用 IEEE binary32/binary64
round-to-nearest ties-to-even 浮点操作；Word 实际 FPCR 尚未观测。

输入还必须在相应宿主读取点成立。未展开 `0x100346900` 的 T37 结果及其可能影响
没有用零或“纯函数”假设填补；构造后的 adjusted-wrapper 修改、字体选择变化和
LS 节点聚合仍未包含。`initial M=[T4,T8,T0,T10,T14,0]` 不等于最终网格 H。
构造后的缩放、条件补偿和默认 M 覆盖现有独立
[字体调整参考工具](FONT-ADJUSTMENT-REFERENCE-2026-09-27.md)。其输入检查点需要明确
提供。后续[提供者身份调查](DOCGRID-FONT-COMPONENTS-2026-09-27.md#原始-v-与提供者-t-的身份)
已将选定路径的 V 输出地址绑定到此 T 构造器；两次调用及后续调整仍须分别处理，
不能把初始 M 当作最终 M。新增的 [font_vertical](FONT-VERTICAL-REFERENCE-2026-09-27.md)
重新使用原始测量，按明确的简单尾 gate 和 mode 2 参数连接纵向路径，输出前三项
更新量；其它分支、水平字段和完整 M 仍不自动推导。

## 已验证数值

使用已冻结 `dwrite-metrics-cli-2026-09-27/tnr-final.json`，当前字体哈希匹配，
OS/2 位于文件偏移 520、长 96，读取 xAvgCharWidth=821。在显式 width scale=1 下：

| L | lfEscapement | 分支 | T 前七个 word |
| --- | --- | --- | --- |
| -16 | 0 | scaledFace1 | `[19,15,4,3,1,6,6]` |
| -50 | 0 | scaledFace1 | `[57,45,12,7,2,20,20]` |
| -2048 | 0 | copy50 | `[2268,1825,443,220,87,821,821]` |
| -50 | 900 | scaledFace1 | `[59,46,13,9,2,20,20]` |

四组通过完整报告解析与真实字体读取，并符合人工核算的固定期望；T/M 是模型
计算结果，不是原生宿主 T/M 采样。API 的来源与限制见
[独立字体指标实测](DWRITE-METRICS-2026-09-27.md)。

新追到的 mode 2 路径会条件性地写入 `L=-D`，但 D 来自临时查询的 face，尚未与
最终对象身份等同。因此表中的 -2048 仍是显式实验输入，不是对真实 Word 分支的
观测，也不能凭它宣布 copy50 或生产网格已对齐。

专项测试覆盖半整数相邻 float、double ULP 边界、32 位哨兵、16 位截断与符号翻转、
两个相反的 typographic 标志、角度分支和输入拒绝；原生报告协议由已有测试单独覆盖。
结果与原始命令回执保存在 `artifacts/font-record-reference-2026-09-27/`。
54 项专项测试通过，完整 Python 套件 **429 passed**；独立代码和证据复核通过。
该目录七文件清单 SHA-256 为
`481e5eda723b4be8ac7a1049d3417be7bd45d058248a9b29b2e33abfd6cb7b40`，逐项核验通过。
本片未修改 Rust 生产算法，没有新增 Cargo 或 Word 排版回归结果。
