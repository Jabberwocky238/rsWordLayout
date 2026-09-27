# Word 随附字体库的独立指标测量

已在独立 Python 子进程中成功调用 Word 随附 `dwrite10` 的字体指标 API。
这为宿主字体转换算法提供了可重复取得的输入，范围是明确字体文件与明确 emSize
的 API 输出；没有启动或附加 Word，也没有测得 Word 文档中实际的 lfHeight。

## 首次实测

输入字体是 `/System/Library/Fonts/Supplemental/Times New Roman.ttf`，SHA-256 为
`f3b4ffff71c2a0c7227d37497683b2498fb2d0a4e8beae26f022e3ccfcaabfa3`。
库路径为 Word 应用下 `Contents/Frameworks/dwrite10.framework/Versions/A/dwrite10`，
SHA-256 为 `0c4f1ce675dc4809018481292f48702f8b700439012871ce5943837518fbaa2d`。

调用使用隔离工厂 type=1、字体 index=0、simulations=0。文件分析返回一个受支持的
TrueType face；查询 `IDWriteFontFace1` 成功，所有 HRESULT 调用均返回 0。
GDI 兼容请求固定 pixelsPerDip=1、transform=null。

| 方法 | 显式 emSize，DIP | ascent | descent | lineGap |
| --- | --- | --- | --- | --- |
| GetMetrics1 | 无 | 1825 | 443 | 87 |
| GetGdiCompatibleMetrics1 | 16 | 1920 | 512 | 128 |
| GetGdiCompatibleMetrics1 | 50 | 1843 | 492 | 82 |
| GetGdiCompatibleMetrics1 | 2048 | 1825 | 443 | 87 |

表中三个结果分量全部是字体设计单位，designUnitsPerEm=2048；四次调用的
hasTypographicMetrics 均为 0。原始记录保留全部 23 个字段，其中 emSize=2048
时的整份结果与 GetMetrics1 相等。这是此字体和此请求的观测，不推广到其它输入。

同一字体文件的结果会随 GDI 请求字号变化，因此不能把一份 hhea/OS/2 原始指标
替代所有请求。已恢复的宿主缩放、舍入与半字写入仍应接在这些返回值之后，
接口合同与字段来源见[字体分量证据](DOCGRID-FONT-COMPONENTS-2026-09-27.md)。

## 与实际排版的边界

这次实验绕过了 Word 的字体家族匹配、fallback 和 feature 替换。静态分析所见
Word 文件创建路径使用共享工厂 type=0，本次使用 type=1，也不能把二者的调用
上下文视作相同。16、50、2048 均由探针显式给定，不是 TNR12 的原生字号观测。

实际 run 点数到 lfHeight/原生尺度、选中字体、宿主后续调整仍未闭合。此次读数
不能单独推出网格 H、可抑制底部空间 S，也没有修复已采集的 4277/4278 twips
分页边界反例。其用途是让后续转换模型可以在固定字体和显式请求下对照真实库输出。

## 证据与核验

首轮目录 `artifacts/docgrid-native-face-probe-2026-09-27/` 保留固定输入脚本、
调用脚本、UTC/命令/退出状态回执、逐阶段 stdout、空 stderr 和范围说明，共六文件。
清单 SHA-256 为 `85acce214e2af7076ac176b85a5de538ee8d5c33e4be2e0e31afde2d510253bc`。
子进程设置 45 秒超时并禁用 core dump；本次退出码 0，没有超时。

独立复核将 GUID、UTF-16 字符宽度、48 字节结构、字段符号、虚表槽和函数签名
逐项对照已冻结的官方头文件，未发现 ABI 错误。引用按获取顺序的逆序释放；
Release 返回非零计数不表示探针仍持有自身引用。清单校验通过。

## 可重复运行的工具

已接入 [dwrite_metrics.py](../tools/measure/dwrite_metrics.py)，参数与失败处理见
[量具说明](../tools/measure/README.md#原生-directwrite-字体指标)。调用方显式提供字体
和一个或多个 emSize；工具固定已核对的库哈希，在子进程执行原生调用，核对工具与
输入前后哈希，记录完整指标、逐阶段输出与退出状态，并原子拒绝覆盖已有结果。
崩溃、超时、失败 HRESULT、损坏协议和输入变化都不能生成 MEASURED 状态。
后续已新增[字体记录转换参考](FONT-RECORD-REFERENCE-2026-09-27.md)，可在显式宿主
参数下复用这些 API 结果计算初始记录；模型输出与原生测量保持不同状态。

最终源码 SHA-256 为 `f952f4ce292e72d56230a787451f2d461a5ba0fc8d655eb425e8c3fb264b5e5e`。
新增 53 项测试通过，完整量具 Python 套件 **375 passed**。独立审查发现的非对象
JSON 记录、浮点溢出和深层 JSON 异常已补正并纳入失败收据回归。

最终工具真实重测三个字号，全部 23 字段与首轮一致。另一次真实调用成功读取含日文
文件名的 Hiragino W3 TTC，Analyze 返回四个 face，仍只选 index=0；这验证了该
非 ASCII 路径和集合输入，没有宣称 CJK 排版对齐。非字体、缺失字体、非法字号、
已有输出拒绝且内容不变也已实测。

结果在 `artifacts/dwrite-metrics-cli-2026-09-27/`，八文件清单 SHA-256 为
`b8362e782402ac723fd4e953e8f8286f0851bb4723234f7cea5bac00d7779a1f`。
各次结果保留自己的源码哈希，早期实测没有改写成最终版本。本片没有 Rust 生产
修改或 Cargo 回归；库文件固定也不表示全部系统动态依赖均已固定。

## Word 进程访问能力核对

另在 2026-09-27 08:22 UTC 做了一次只申请 task port 的能力核对。工具确认当时
唯一 Word 进程的可执行路径后，`task_for_pid` 返回 5（KERN_FAILURE），未取得
端口。没有暂停进程、读取内存或修改系统安全配置，因此没有获得现场字体选择值。
此结果只描述本次调用，不证明永久不可访问或具体拒绝原因。

[原始回执与源码](../artifacts/word-runtime-access-2026-09-27/README.md)共五个成员，
清单 SHA-256 为 `31212357e0562f02c4b947ab3bae07f6dbf46ded24db07bd93aec2164e1764a2`。
构建、进程查找和请求均正常退出且未超时；请求失败是返回码事实，不能作为排版观测。
收据固定的是请求前磁盘文件的哈希，未对正在运行的已载入镜像做哈希。
