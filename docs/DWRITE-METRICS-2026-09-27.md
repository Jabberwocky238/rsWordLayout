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
