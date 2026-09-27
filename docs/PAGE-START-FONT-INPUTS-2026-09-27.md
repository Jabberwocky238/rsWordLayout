# Page-start 六字体的独立 API 输入

已对 page-start 的三组异常字体和三组对照字体读取 Word 随附字体库的实际返回值。
结果补齐了明确字体文件下的指标与 LOGFONT 转换观测，也排除了两条直接替换规则：
不能把所有 hhea 指标换成这组 API 指标，不能用本次转换的 charset 区分异常字体。

调用只在独立 Python 子进程中加载固定哈希的 dwrite10，没有启动、操作或附加
Word。每个字体均明确使用隔离工厂 type=1、face index=0、simulations=0；这些
条件与 Word 实际的字体请求仍需分别核对。

## 指标观测

每字体取得 `GetMetrics1`，以及 emSize 为自身 upem、29、33、35 DIP 的四个
`GetGdiCompatibleMetrics1` 返回值；固定 pixelsPerDip=1、transform=null。
29/33/35 是探针参数，不能据数字相同就当作 Word 文档的点数或 lfHeight。

| 字体 | upem | hhea ascent / abs descent / gap | API 在 upem 请求下的三项 |
| --- | ---: | --- | --- |
| Brush Script MT | 2048 | 1820 / 692 / 0 | 1820 / 692 / 0 |
| AppleMyungjo | 1025 | 891 / 326 / 0 | 891 / 326 / 0 |
| Arial Unicode MS | 2048 | 2189 / 555 / 0 | 2189 / 555 / 0 |
| Khmer Sangam MN | 2048 | 2294 / 1393 / 380 | 1120 / 863 / 2084 |
| Lao Sangam MN | 2048 | 2052 / 646 / 0 | 2040 / 1088 / 0 |
| Silom | 1000 | 959 / 316 / 0 | 1000 / 250 / 25 |

表中 API 分量均为设计单位。六字体的完整 `GetMetrics1` 记录各自与 upem 请求
逐字段相等，全部 30 条指标记录的 `hasTypographicMetrics` 均为 0。较小 emSize
的返回值会变化，原始数据全部保留，不从某个请求插值得到其它请求的指标。

Khmer 和 Silom 的分量变化但总和不变；Lao 的总和从 2698 变为 3128，而它在
归档 Word 中本来就是分页一致的对照。这说明当前独立 API 路径不足以成为引擎
统一的字体度量来源，仍需验证宿主选中对象与后续调整。分量不同也会影响首行
位置，不能仅凭总和一致就认定两个输入等价。

## 字符集观测

另一个独立探针通过 `IDWriteFactory::GetGdiInterop` 和
`IDWriteGdiInterop::ConvertFontFaceToLOGFONT` 读取完整 92 字节 LOGFONTW。
API 槽、结构宽度和 UTF-16 布局同时有保存的官方头文件与本机宿主调用指令依据。
所有调用均成功；只有成功转换后才接纳输出，失败的清零缓冲区不能作为 charset 0。

六个字体的 `lfCharSet` **全部为 1，即 DEFAULT_CHARSET**，返回 faceName 与
各字体族标签一致。因此本次转换值不能区分 Brush Script MT、AppleMyungjo、
Arial Unicode MS 与三个对照，也不能证明其中任何一个进入 `{128,129,134,136}`
的高度补偿分支。

宿主已知路径还可能在 `100341634..650` 重写 DEFAULT_CHARSET，且存在不同的
请求来源、属性 gate、字体对象取得方式和后续变化。实验使用隔离工厂，已保存的
宿主转换路径使用共享工厂；不能把本次返回值当作 Word 运行现场。

## 证据和后续边界

[指标包](../artifacts/page-start-font-api-2026-09-27/README.md)保留六份完整协议、
输入前后哈希、30 组指标、离线核验脚本及与当前字体表的对照。
[LOGFONT 包](../artifacts/page-start-logfont-api-2026-09-27/README.md)保留独立探针、
六份完整字节输出、UTC/命令/退出记录及协议审计。每字体只执行一次，没有重试
筛选结果。两组共 12 个原生子进程均 exit 0、无超时，接口引用均按逆序释放。

当前字体字节与归档引擎回放使用的文件相同，但旧 Word 采集没有固定每个字体
文件的字节。这些是新的字体 API 输入观测，没有新增 Word 排版采集或验收通过。
下一步仍需绑定实际宿主请求及补偿分支；不能由字体名称或 OS/2 代码页相关性
直接启用 1.3 倍率。

两份经审核冻结的 SHA256SUMS 清单：指标包 10 成员，
`6754c6d6d2fec4b5e0bda4f4676542ae02d7ef2e345f47044664c7ee71630f1f`；
LOGFONT 包 30 成员，
`d226c0b7d5c742aaff04945d75c09a15a595611e2299353a2ccf8955579db6d7`。
