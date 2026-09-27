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

## 默认字符集改写的来源

后续[静态调查](../artifacts/font-charset-rewrite-2026-09-27/README.md)已经闭合
`100341634..650` 的局部 gate 和被调转换函数：

```text
if current V.byte38 == 1 and current P0.bit22 == 0:
    V.byte38 = low8(MsoChsFromCpg(MsoGetACP()))
```

gate 内没有字体族或 mode 判断。它读的是当时的 P/V，不能用独立 LOGFONT 返回值
替代。P0.bit22 的一个初始来源是上游属性字 bits24/25 的 OR；后面有接收可写 P
的调用，因此这个初值也不能自动当作最终读取值，更未将其命名为某个 OOXML 属性。

固定 mso30 中的 `MsoChsFromCpg` 是无调用的整数比较树，相关转换为：

| 明确输入的码页 | charset 输出 | 是否在 SPECIAL 集合 |
| ---: | ---: | --- |
| 932 | 128 | 是 |
| 936 | 134 | 是 |
| 949 | 129 | 是 |
| 950 | 136 | 是 |
| 1361 | 130 | 否 |
| 10000 | 77 | 否 |
| 1252 或未列在完整映射表中的 i32 | 0 | 否 |

`MsoGetACP` 经 mso20 转到 WLMKernel 的进程缓存。其 `dispatch_once` 初始化器
读取 mbulocale 的首选文本编码并转成码页；编码来源链明确使用
`CFBundleGetMainBundle` 与 bundle 的首选语言。码页转换返回 -1 时缓存 10000。
完整映射和缓存初始化指令均保存在新包中，没有执行这些 API。

因此 ACP 属于主应用 bundle、语言与初始化状态的输入，不是某个字体文件的属性。
以后即便在独立 Python 中成功读取 ACP，也不能直接当作 Word 进程的 ACP。
目前缺的是实际 Word 缓存值、当前 P/V 与选中路径；改写后还有可接收 P/V 并触发
重试的调用，不能直接推定 tail 最终 V38。六字体全部返回 DEFAULT_CHARSET 的
实测与这条改写规则并不矛盾，但它们仍未选定 page-start 的实际补偿分支。

本片核验 7 个完整函数、133 个旧证据成员；离线指令解释覆盖全部 u16 码页及
三个 i32 边界，共 65,539 例，与完整常量表一致。未宣称穷举全部 i32，更没有新增
Word 或原生 API 测量。经审核冻结的新包 38 成员，SHA256SUMS 清单哈希为
`3d0b144d6a0ec4b6d621d23301605d7bb974420d1cdfd8db7b2da7895fd5eee9`。
