# docGrid 输入、继承边界与夹具核查

日期：2026-09-27。本文记录本地源码、解析器执行结果和 DOCX 包内 XML，
输入已接入 `DocumentGrid` 与 `Para.snap_to_grid`，见下文当前实现。
本片保留文档网格输入，不应用网格行距，不拟合行高常数，
也不把解析器行为当成新的 Word 实测。

解析器钉住版本为 `399e36a3c645e9b9001531fb06969f8ce5347e1f`，见
[Cargo.toml](../Cargo.toml:20)。下文外部源码链接指向本机 Cargo 对该版本的只读检出，
根目录为 `/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a`。
没有依据同名的其他检出或在线规范补推默认值。

## 已有投影

| XML 位置 | 解析器 Rust 字段 | native JSON 路径 | 当前布局状态 |
| --- | --- | --- | --- |
| `w:sectPr/w:docGrid` | `SectionProps.doc_grid: Option<DocGrid>` | `sections[i].props.docGrid` | `LayoutSection.grid` 保留原声明 |
| `w:docGrid/@w:type` | `DocGrid.kind: Option<Val<DocGridType>>` | `docGrid.kind` | `DocumentGrid::kind()`；键名不是 `type` |
| `w:docGrid/@w:linePitch` | `DocGrid.line_pitch: Option<Val<i32>>` | `docGrid.linePitch` | `DocumentGrid::line_pitch()`，不补默认值 |
| `w:docGrid/@w:charSpace` | `DocGrid.char_space: Option<Val<i32>>` | `docGrid.charSpace` | `DocumentGrid::char_space()`，保持原整数单位 |
| `w:pPr/w:snapToGrid` | `ParaProps.snap_to_grid: Option<bool>` | 段落 `props.snapToGrid`，以及有效段落属性中的同名键 | 有效属性投影到 `Para.snap_to_grid`，保留 None/false/true |
| `w:rPr/w:snapToGrid` | 未建模，留在 `RunProps.raw_unmodeled` | 不投影 | 不能通过现有 run Resolver 取得 |

字段来源为
[section.toml:130](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/schema/props/section.toml:130)、
[types.toml:661](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/schema/props/types.toml:661) 和
[para.toml:95](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/schema/props/para.toml:95)。
run 的同名元素只出现在
[run.toml:11](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/schema/props/run.toml:11)
的元素顺序表，没有对应 `table.field`；实际含 `<w:rPr><w:snapToGrid w:val="0"/></w:rPr>`
的最小探针，run `props` 投影为 `{}`。不要把段级开关当作 run 级开关的替代输入。

`DocGridType` 只有 `Default`、`Lines`、`LinesAndChars`、`SnapToChars`，对应字符串
`default`、`lines`、`linesAndChars`、`snapToChars`。`chars` 不是该解析器接受的枚举值，见
[types.toml:597](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/schema/props/types.toml:597)。
外部报告中把未测类型略写成 `chars` 的地方，不能直接变成布局 API 的合法值。

## 继承边界

段落 `snapToGrid` 可以直接复用现有有效属性通道。
[Resolver::para](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/src/resolve/mod.rs:405)
按 `docDefaults → 段落样式 basedOn 链（根到叶）→ 直接段落属性` 合成该开关。
编号级别在此实现中只贡献缩进，不贡献 `snapToGrid`；字符样式不参与段落属性合成。
它是普通的覆盖布尔值，不按 run toggle 的异或规则合成。

本仓库 [effective_properties](../crates/core/src/load.rs:301) 已选择默认段落样式并调用
`resolver.para(...).props.to_json(...)`。当段落未指定样式时使用默认段落样式；
当它显式指定了不存在的样式时，现有代码不会再退回默认样式。
[project_paragraphs](../crates/core/src/bridge.rs:728) 已优先读取这份有效属性。
当前已在这里投影 `snapToGrid`，没有重写样式合成或重新读取 styles XML。
裸 `paras_from_document` 入口没有完整 Resolver，仍只具备它现有的声明值/近似能力。

通过当前解析器执行最小 DOCX 探针，确认以下结果；这验证的是输入合成，不是 Word 布局：

| 输入 | 有效值 | `EffectiveParaProps::source(ParaPropsField::SnapToGrid)` |
| --- | --- | --- |
| docDefaults 为 false，Base 样式为 true，段落直接 false | false | `Direct` |
| docDefaults 为 false，Leaf 基于 Base，Base 为 true，Leaf 和段落均未声明 | true | `ParaStyle("Base")` |
| docDefaults 为 false，段落没有可用样式或直接声明 | false | `DocDefaults` |

来源查询 API 见
[EffectiveParaProps:119](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/src/resolve/mod.rs:119)。
所有层都缺省时，当前合成算法保留 `None`，不补出一个布尔默认值。
布局输入也应保留 `None` 与显式 false 的区别；未来算法如何解释缺省，本次尚未确定。

节网格走另一条路径。[SectionInfo:7412](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/src/model/mod.rs:7412)
只保留本节声明，分节段落是该节的最后一块。
[Resolver::section](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/src/resolve/section.rs:81)
只处理页眉页脚槽继承、变体与节几何，`EffectiveSection` 没有 `docGrid`。
实际两节探针中，第一节有 `docGrid`、第二节 `<w:sectPr/>` 时，第二节 JSON 不含 `docGrid`。
这证明 parser 没替布局做网格继承，不能据此断言 Word 跨节一定继承或一定不继承。

当前从 [document_from_json](../crates/core/src/document.rs)
遍历的 `sections[i].props` 保存节网格，使用既有 `blockRange/paraRange` 归属。
不要从某个段落有效 `props.sectPr` 外推全节网格，也不要因为上一节有网格就自动向后复制。
连续分节的网格切换时机尚无这里可以证明的规则，应单独记录而不是混入页面尺寸切换。

## 缺省与非法值

`DocGrid` 的三个属性均可缺省。JSON 不含某个键，与显式 `kind="default"`、
显式数值 0 是三种不同输入。本次不补 `linePitch`、不推 `charSpace` 的单位换算，
也不把 `charSpace` 的原始整数直接转成 twips。

解析失败的 `Val<T>` 在 native JSON 中保留为 `{"raw":"原文"}`，见
[ToJson for Val:271](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/src/bind/native/json.rs:271)。
实际探针：

```json
{
  "docGrid": {
    "kind": {"raw": "chars"},
    "linePitch": {"raw": "oops"},
    "charSpace": {"raw": "2147483648"}
  }
}
```

上述三项产生 `PROP_BAD_VALUE`。而 `linePitch="0"` 和 `linePitch="-1"`
都作为整数通过 parser，没有 warning；`charSpace="-17"` 也原样保留为整数。
因此仅检查 parser 是否成功，不能证明网格可以参与行距算法。

输入接入时应保留原声明并区分以下诊断，不能统一 `unwrap_or` 成某个可用网格：

- 非法枚举或 `{"raw": ...}` 数值：记录原值与所在节，报告无法解释。
- 活跃行网格缺少 `linePitch`，或 pitch 非正、超出布局可表示范围：报告不能计算。
- `linesAndChars`、`snapToChars`：保留类型，报告尚未支持，不降格成 `lines`。
- 显式 `charSpace`：保留原整数；字符网格语义尚未实现时报告限制，不转成普通字符间距。
- 缺省 `kind`、缺省段落 `snapToGrid`：保持缺省状态，不能伪称已确认默认行为。
- `lines` 输入在真正的网格算法接入前：元数据应说明“已读取但尚未用于网格排版”。

段落开关使用 parser `OnOff`，不是 `Val<bool>`。非法 XML 值可能已经变成 parser
回退布尔值并伴随 warning，不能期待 `snapToGrid` 的 JSON 自带 `raw`。
其读取规则见
[OnOff:90](/Users/lilleap/.cargo/git/checkouts/rswordparser-deb0c68799df1823/399e36a/crates/rsword/src/semantic/props/codec.rs:90)。
`LayoutDocument.source_warnings` 和轨迹 `sourceWarnings` 原样保留解析器警告对象，
包括来源 part、字节区间、代码和消息；不会因页面覆盖丢失。非法 snap 的回退布尔值
因此与原始错误一起可见，原 `LoadedDocument.json` 不改写。

## 实际夹具

对当前 `../word_analyse/fixtures/*.docx` 共 459 份逐个读取 ZIP 中的
`word/document.xml` 和存在的 `word/styles.xml`，没有解析错误。
55 份有 `docGrid`，每份一个，全部 `type="lines"`，全部未声明 `charSpace`。
这批文件的两个 part 内没有任何段级或 run 级 `snapToGrid` 声明。
因此没有显式 snap 开关、样式继承开关、字符网格或跨节网格继承的实测依据。

实际 pitch 集合为：
`60, 65, 70, 75, 85, 96, 99, 100, 125, 139, 150, 151, 175, 188, 200, 209, 220, 240, 250, 280, 290, 293, 296, 297, 298, 299, 300, 312, 315`。
这些是夹具输入集合，不是全部已有可靠页数测量的集合。

`dg-decide-139-n37.docx` 确有 37 段，每段 `lineRule="auto"`、`line="240"`、
段前后 0，节 pitch 139。`longpage-grid297.docx` 是相同段落设置的 60 段，pitch 297。
二者均只有主 XML 与必要的包元数据，没有 styles/settings part，没有显式字体。
构造器依据见 [make_docgrid_deciders.py](../../word_analyse/tools/make_docgrid_deciders.py:75)。
默认字体/字号与最终分页容量仍须由独立证据解释，不能从文件缺省倒推出常数。

六份名称与包内 XML 不符，不能按文件名当成 pitch 175 或 220 的对照：

| 文件 | 名称暗示 pitch | XML `linePitch` | XML 上边距 |
| --- | --- | --- | --- |
| `longpage-h1-grid175.docx` | 175 | 300 | 820 |
| `longpage-h1-grid220.docx` | 220 | 300 | 820 |
| `longpage-h2-grid175.docx` | 175 | 300 | 620 |
| `longpage-h2-grid220.docx` | 220 | 300 | 620 |
| `longpage-h3-grid175.docx` | 175 | 300 | 1020 |
| `longpage-h3-grid220.docx` | 220 | 300 | 1020 |

六份均为页宽 11906、页高 16838，其余正文页边距 720；上表数值单位 twips。
本次只读取原文件，没有修改外部夹具或重写报告。

## 与纵向度量分离的接口

`DocumentGrid` 以原始声明为事实来源；访问器返回 `Result<Option<T>, String>`，
区分缺省、合法声明和不能解释的输入。节轨迹保存原声明与各字段解析状态，
顶层 `grid.paragraphSnapToGrid` 按段落索引保留有效三态值。
两处都标记 `applied: false`。缺省、不支持类型、非法或非正 pitch、字符间距限制
进入诊断，不会静默套用行网格。页面几何覆盖不会更改网格声明。

`grid_input.rs` 验证原生 JSON 与真实 DOCX、节范围、样式继承/直接 false、非法值和
四个平台/视图组合下当前布局不变。这里验证输入合同，尚不验收 docGrid 的行间推进。
本片网格 18 项、文档兼容 16 项及有效属性/段落保留/节几何 60 项测试通过；
验证日志保存在 `artifacts/grid-input-2026-09-27/`。

本片 [VerticalExtent](../crates/core/src/layout/vertical.rs:7) 分开记录
`advance_fine` 和 `required_fine`。两者均为 1/7200 英寸，分别用于推进后续行原点和判断
当前内容在页内需要的最远下边界。组合必须保留此前每行的最远边界，不能只看末行。

当前 [Engine::line_vertical](../crates/core/src/layout.rs:3081) 从既有行高创建
`VerticalExtent::uniform`，并未根据 DOCX 网格改变任一个量。未来 docGrid 输入应影响
这层明确的度量计算，不能只替换分页判断中的某个高度常数。
段落保留、孤行检查、段前后距与跨页续排也要消费同一套推进/占高组合。

`FontMetrics::quantize_baseline_fine` 与 `VerticalGrid::MacWordThreeHundredthsInch`
是已有字体度量/基线量化通道，与节 `docGrid` 是不同输入，不共用一个开关。
本次不解释 exact/atLeast/auto 与文档网格的所有组合，也不引入 298、319 或 350 等拟合常数。

页数证据仍按 [ANDROID-PAGE-EVIDENCE-2026-09-27.md](ANDROID-PAGE-EVIDENCE-2026-09-27.md)
的限定使用。九份旧日志严格模式全部不可判；其中三份达到 PGIDX 80 条采样上限，
连条件比较也不能证明最终分页容量，不能靠输入接入或数学重构将其升级成通过。

## 规范网格探针

后续新增 `tools/measure/make_docgrid_fixture.py`，基础输入位于
`fixtures/docgrid-canonical-2026-09-27/`，鉴别输入在独立 `append-discriminators/`。
两份 manifest 记录每个 DOCX、正文 XML、settings XML 的哈希、原文、UTF-16 源区间、
实际 grid/snap/spacing XML 与字体声明，没有预填 Word 排版结果。

基础 18 份显式使用 Times New Roman 12pt，正文与段落标记均声明四槽字体和字号，
段前后距为零，widowControl 关闭，兼容模式 15。包括无网格、pitch 240/300/360/480
各配 snap 缺省/true/false、exact480 与 atLeast240 的 pitch360 对照，以及同段内
12 行的 soft-return 对照和前三段 snap=false、后九段 true 的相位探针。
auto 的 line=240 是 240 分之一行单位，不称作 240 twips。

追加 3 份分别为 12pt/pitch270/snap=true、18pt/无网格、18pt/pitch300/snap=true。
本机 TNR `hhea` 为 upem=2048、ascent=1825、descent=443、lineGap=87；12pt 下字面占高
为 265.78125 twips，自然高度为 275.9765625 twips。pitch270 可区分这两种候选输入，
但这些字体表计算本身不能证明 Word 采用哪一种。

21 份 DOCX 及两份 manifest 均已逐字节重生成复核；252 标签、230 段、22 个 soft return
及字体/属性顺序通过离线核查。当前原生 `layout-trace` 实际读取全部 21 份，解析警告为零；
每份源长 60、排出 12 行。后两项仅是实现前基线，不能用来验收网格算法。
引擎基线、字体表与哈希记录在 `artifacts/docgrid-engine-2026-09-27/`。

新的静态证据复核见 [DOCGRID-ALGORITHM-EVIDENCE](DOCGRID-ALGORITHM-EVIDENCE-2026-09-27.md)。
其中纠正了旧 `LineGapMutator` 报告的三段度量求和解释；当前保存的原生和 Web 材料
仍未给出可直接移植的 docGrid 公式。

首次 Mac 实测停在首份文件的授权窗口，没有产生 PDF、源扫描或布局读数。
打开事件在 2026-09-27 03:03:23 至 03:06:23 UTC 后返回 `-1712`；只读窗口检查确认
前台为 `com.apple.loginwindow`，Word 授权窗口被锁屏遮挡。没有重复打开、点击授权、
操作登录窗口或改写旧采集。解锁后应先识别既有窗口/文档，再用独立恢复记录继续。
失败历史保存在 `artifacts/docgrid-canonical-capture-2026-09-27/`，17 个文件的清单
`durable-hashes.json` SHA-256 为
`80efaa7a184bc4f9812ad9d82a9492c5870f65c35c376fb0f422cc9ab6303ada`。
这次尝试不提供网格公式或默认值证据。
