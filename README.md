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
cargo run --bin render -- fixtures/sample.docx fixtures/sample.html
```

输出每页一个 `<svg>`，坐标全部由布局引擎算出，浏览器只负责画字形、不参与排版决策。

## 状态

已实现：段落断行（西文按词 / CJK 按字 + 行首禁则）、行高（`auto` / `atLeast` / `exact`）、
y 游标分页、`keepNext` / `keepLines` / `pageBreakBefore`、四种对齐、首行与悬挂缩进、SVG 后端。

**已知缺口**（代码注释里逐条标注）：

- `SimpleMetrics` 是**近似度量桩**，按字符类别给固定宽度，不读字体文件、不做 shaping。
  真实排版需实现 `FontMetrics` 接 HarfBuzz。
- `bridge.rs` 按 `styleId` 做最小样式映射；有效属性应走 `rsword::resolve::Resolver`
  （`document()` 的 JSON 给的是声明值）。
- 未实现：表格、浮动与文字环绕、分栏、页眉页脚、编号列表。

## 许可

MIT OR Apache-2.0
