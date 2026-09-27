# Mac 分栏平衡采集（部分完成）

新的 Word 原始 PDF 已确认：本次普通双栏、40 段短行文档的末页按 **32+8** 顺序填充，
没有平衡成 20+20。连续分节样本在导出 PDF 时超时，尚无该样本的页栏分配证据。

## 输入与实测

夹具生成器是 `tools/measure/make_column_balance_fixture.py`。六个基础输入位于
`fixtures/column-balance-2026-09-27/`；一个同栏连续分节对照位于
`fixtures/column-balance-same-columns-2026-09-27/`。清单逐一保存 DOCX、document.xml、
settings.xml 的哈希和标签的源区间，没有写入 Word 期望结果。

审阅发现首版借用的 `make_probe_fixture.para()` 把 `widowControl` 写在 `spacing/jc`
之后，与 pinned parser schema 的 CT_PPrBase 强制序列不符；七份原始输入均有此问题。
本次 PDF 结论仍是这些确切字节的观察，但不能用它证明规范顺序输入或该开关被 Word
正确接受。也没有证据表明属性乱序导致 PDF 超时。
原 DOCX、清单和采集回执保留不改；复现它们须传 `--legacy-property-order`。

生成器默认改用 `widowControl → spacing → jc → rPr → sectPr` 的相对顺序。
新的六份规范输入在 `fixtures/column-balance-canonical-2026-09-27/`，同栏对照在
`fixtures/column-balance-canonical-same-columns-2026-09-27/`；清单 schema 为 `/2`。
这些是不同哈希的待采样输入，没有继承首版的 Word 期望结果。

所有正文和段落标记显式指定 Times New Roman 12pt、exact 480 twips 行距、段前后 0、
widowControl 关闭。纸张为 11906×16838 twips，四边距 720，两栏间距 720，等宽栏各
4873 twips；兼容模式为 15，`noColumnBalance` 显式写 0 或 1。

| 输入 | 采集结果 |
| --- | --- |
| terminal40-noBalance0 | 1 页；C000–C031 左栏 32 行，C032–C039 右栏 8 行 |
| continuous40-noBalance0 | 成功打开并取得正文、41 个段落区间；PDF 导出超时，页栏分配未知 |
| terminal40-noBalance1 | 已生成，未打开 |
| continuous40-noBalance1 | 已生成，未打开 |
| continuous40-break10-noBalance0/1 | 已生成，未打开；C009 之后有显式栏断，随后仍有连续单栏后继 |
| continuous40-sameCols-noBalance0 | 已生成，未打开；连续分节后继仍为双栏 |

成功样本的 PDF 标签首字形 x 为 36.0 和 315.6 pt；两栏首字形 y 均为 55.2 pt，
末字形 y 分别为 799.2 和 223.2 pt。这里的列分配来自 PDF 可见标签及坐标，
没有将原生 Ln 序号猜作栏号，也没有使用本引擎结果充当 Word 期望。

## 身份与限定

Word 为本机 16.112.3 / 16.112.26083020，既有进程 PID 39057。采前原生读回打开文档数
为 0，并能列出 Times New Roman；成功 PDF 使用 `TimesNewRomanPSMT`，字体替换核查通过。
只打开新建副本，没有操作用户已有文档、安装字体或重启 Word。

成功 DOCX SHA256：
`b686f844a87811d8781d21b8b8ca9c13ec967e2f7a1b5b254a86be4115463409`。

原始 PDF SHA256：
`aac9cbdb0691764de992131ffeb2b3323bd1f5a1e894f5a01cc410145bb2ecd4`。

成功样本的连续两次扫描均覆盖 200 个 UTF-16 位置且原始回执一致；40 个标签的源位置
与实际取回正文相符，段落区间与源 XML 相符。独立重新读取 PDF 得到的完整 glyph 数组
与存档一致。扫描稳定不等于单独证明了几何行身份。

采集对自有文档设置 print view 并调用 repaginate；原生视图读回为 `page view`。
pagination 属性读回 `missing value`，因此没有宣称背景分页状态已确认。
栏断控制字形数量仍未实测，本次没有增加对应的计数验收规则。

## 失败保留与开发边界

第一份成功采集在关闭自有文档后遇到离线校验缺少 `rawReceipt` 元数据的错误。
原始脚本和状态快照保留；修复仅补齐已保存扫描的哈希，没有重采或覆盖首份读数。

第二份的 PDF AppleEvent 于 `2026-09-27T01:46:47.889106Z` 发出，子进程 PID 17095；
180 秒后返回原生错误 `-1712`。保留状态为 **UNCONFIRMED**，没有 PDF、行号扫描或
页栏坐标。现存正文和段落边界只能确认源结构，不能证明平衡或顺序填充。
控制器已退出；Word 未被关闭、杀死或重启，其自有采集副本可能仍打开。

后续 1 秒 OS 采样显示主线程在 `MbuOpenSecurityScopedResourceSessionForWrite`、
`MbuObtainSecurityBookmarksForFileURLs` 和 `NSApplication runModalForWindow` 路径等待。
该窗口内没有排版 CPU 循环证据，说明此次阻塞涉及写入路径安全授权；采样本身不能确认
授权目标路径或 PDF 将在何时完成。独立诊断保存在
`artifacts/column-balance-recovery-diagnostic-2026-09-27/`，未覆盖原故障回执。

原始 PDF、两次扫描、逐事件脚本/回执、环境指纹、故障状态、离线审计和哈希清单均在
`artifacts/column-balance-2026-09-27/`。该目录按项目约定被 Git 忽略；其中 README
记录实际命令和恢复经过，`observations.json` 给出逐标签可回读的页、字形索引、坐标与源位置。

当前证据支持保留本例普通文档末页的顺序填充；连续分节平衡的触发位置、
`noColumnBalance` 效果、同栏连续后继和栏断约束仍需新的成功采集，不能由这次超时补全。

## 引擎回归

`column_flow::reported_mac_times_new_roman_terminal_columns_and_source_anchors`
用实际 Times New Roman 字体回放首版 terminal40-noBalance0；断言 1 页、32+8 行、
每行栏归属及 UTF-16 区间从 0 到 200 连续，已通过。该测试保留为需要本机字体的
ignored 测试，以 `RSWORD_TEST_TIMES_NEW_ROMAN` 指定文件运行。
它不比较字形坐标，也不为尚未成功采集的其他夹具写期望。
独立 CLI 轨迹与原生段落区间核对也为 40/40，页栏分配相同；产物在
`artifacts/column-balance-engine-2026-09-27/`。几何仍未对齐：本次未启用纵向量化的
轨迹首字形相对 Word 为 x 差 0 至 0.05pt、y 差 -8.5pt，不能把页栏匹配写成完整布局通过。

本片全工作区 `cargo test --offline --workspace --features rsword-layout-core/fontenv`
为 537 通过、10 ignored、0 失败；上述本机 TNR 专项另行执行 1 项通过。
新生成器还逐字节复现了旧版 7 个 DOCX 和 2 份清单，规范版 285 个段落的属性顺序核查通过。
