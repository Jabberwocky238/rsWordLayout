# rsWordLayout

DOCX **布局层**（Rust）。接在 [rsword](https://github.com/LilLeapo/rsWordParser) 之后，
回答它明确不回答的问题：**一行放得下几个字、一页放得下几行、每个字形落在哪一页的哪个坐标。**

rsword 的冻结架构（其 `docs/03` §1.2）把「分页、行布局、字体度量、渲染」划在范围之外，
并把这些留给「调用方的渲染层」。本项目就是那个消费者。

```
rsword::resolve              rsWordLayout                后端
  EffectiveParaProps  ──┐
  EffectiveRunProps   ──┤
  EffectiveSection    ──┼──►  布局引擎  ──►  LaidOutDocument  ──►  impl Canvas
  ColumnView          ──┤     (y 游标)      (已定位 Fragment)      SVG / PDF / GPU
  Numbering           ──┘
                           ▲
                           └── impl FontMetrics（HarfBuzz 等，由调用方提供）
```

## 两条设计约束

1. **度量与后端解耦**：度量走 `FontMetrics` trait，所有后端共用同一份。否则同一文档
   在 SVG 后端分 10 页、PDF 后端分 11 页，预览就失去意义。
2. **布局产物是数据**：`LaidOutDocument` 是纯数据，能直接做快照回归，不必靠截图比对。

几何与颜色类型标了 `#[repr(C)]`，内存布局有测试钉死（`tests/repr_c.rs`），
可直接作为顶点数据喂给 Vulkan / OpenGL 后端。

## 跑一下

```sh
cargo run -p rsword-layout-svg --bin render -- fixtures/plain.docx layout.html
```

默认模式使用近似度量，输出可选择的 SVG `<text>`；浏览器会重新整形文字，片段内部
的字形位置不保证与引擎一致。需要使用排版后的字形位置时，显式提供字体并选择轮廓模式：

```sh
cargo run -p rsword-layout-svg --features fontenv --bin render -- \
  --text-mode outlines \
  --font fixtures/fonts/LiberationSans-Regular.ttf \
  --fallback-font fixtures/fonts/DroidSansFallbackFull.ttf \
  fixtures/cjk-plain.docx layout.html
```

此模式用同一字体库完成度量、整形和矢量轮廓导出，保留每个字形的实际字体、字号和
精确位置，正文字形不依赖浏览器安装的字体；轮廓没有可选择文字。字体参数可重复，
`path.ttc#1` 选择 TTC 内序号 1 的字体，省略序号时为 0。
这保证的是引擎绘制指令的输出一致性，不代表已有 Word 排版差异已消除，见
[SVG 字形定位](docs/SVG-POSITIONING-2026-09-27.md)。

## 状态

共享 PTS/LS 内核的后续开发顺序、证据和验收条件见
[开发路线](docs/SHARED-PTS-LS-DEVELOPMENT-2026-09-27.md)。正式 DOCX 入口使用
`load_document(...).layout_document()`：样式继承后的属性与文档节几何一起传给共用排版器。
`w:vanish` 隐藏内容不排版，但保留 UTF-16 源位置。

已实现：段落断行（西文按词 / CJK 按字 + 行首禁则）、行高（`auto` / `atLeast` / `exact`）、
y 游标分页、`widowControl`、`keepNext` 链 / `keepLines` / `pageBreakBefore`、四种对齐、首行与悬挂缩进、
节页面尺寸与边距、换页后重新断行和环绕查询、SVG 后端。
等宽与显式不等宽栏共用断行器，支持自动换栏、独立栏断，以及相同页面几何下的
连续分节末页栏平衡和同页多个栏组，
详见 [多栏进展](docs/COLUMN-FLOW-2026-09-27.md)。
文档兼容项 `splitPgBreakAndParaMark` 独立于平台和视图输入；
实现与验收范围见 [段落保留进展](docs/PARAGRAPH-FLOW-2026-09-27.md)。

**已知缺口**（代码注释里逐条标注）：

- `SimpleMetrics` 是**近似度量桩**，按字符类别给固定宽度，不读字体文件、不做 shaping。
  真实字体路径使用 `fontenv` 特性下的 `RealMetrics` 与 rustybuzz。
- 裸 JSON 的 `paras_from_document` 保留历史样式近似；`LoadedDocument` 已通过
  钉住版本的 `rsword::resolve::Resolver` 合成有效属性。Android 的复杂 toggle 继承仍需实测。
- 未实现：docGrid、表格、跨多栏组 keepNext、连续节页面几何切换、完整浮动锚定、页眉页脚占位、编号标记绘制、完整连续分节规则。

## 许可

MIT OR Apache-2.0
