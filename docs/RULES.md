# 规则登记表

引擎里每条「照 Word 排」的规则一行：在哪个平台 / 视图生效、凭什么、哪个测试钉住、读数现状。
证据细节在各自的专题文档，这里只做索引；新增或改动规则的提交同时改这一行。

- **平台 / 视图**：`全部` 指 OOXML 语义，桌面与 Android 一样；`Android` 只经 `--platform android`
  （库接口 `with_platform`）生效，`Android 移动` 另需 `--view mobile`。桌面缺省不变。
- **状态**：`已证`（Word 读数唯一确定，采集前写下的预测被验证）、`已证+假设`（主干已证，表中列出的
  分支没有读数，按假设实现）、`缺口`（有读数、未实现或未对上）。
- **读数**：`tools/measure/gate/readings.json` 里的条目，按 `fixture@view` 指；`scripts/verify.sh` 判。

本表从 2026-10-04 的 word_analyse 对齐开始登记；在此之前的桌面纵向 / 分页规则见
[量到的结论](ENGINE-GAPS-MEASURED.md)、[量具清单](MEASUREMENT-BACKLOG.md) 与各 `*-EVIDENCE` 专题，尚未逐条迁入。

## 横向（断行与字宽）

| 规则 | 平台 / 视图 | 依据 | 测试 | 读数 | 状态 |
| --- | --- | --- | --- | --- | --- |
| `w:fitText` 整截占 `w:val`、不可拆 | 全部 | [P0 对齐 §3.1](WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md)，`fittext.md` | `fit_text.rs`（12） | `fittext*@print` 3 | 已证+假设（截内摊法、同 id 多 run 分摊、区内制表符） |
| 字符单位缩进优先于 twips，1 字符 = 首个可见 run 的 1 em | 全部 | §3.2，`indent.md` | `indent_chars.rs` | `ind-chars*` | 已证+假设（写 0 退回 twips；标记与正文字号不同未测） |
| 移动视图缩进 × 视图版心 / 声明版心 | Android 移动 | §3.3 | `indent_chars.rs::mobile_view_scales_indents_by_view_width_over_declared_width` | `ind-left@mobile`；`ind-left-dig@print`（打印视图不缩） | 已证（单元格内段落、制表位不缩） |
| 字宽按每英寸 778 设备像素量，`w:w` 截断 | Android 移动 | §3.4，Q15 / P0-1b | `android_device_pixels.rs` | `px-*@mobile` 5、窄路径 32 | 已证+假设（778 只测一台设备；未执行 hinting） |
| 行尾空格不计宽（挂出） | Android | §3.4 规则二；补测第 7 条 | `android_device_pixels.rs::trailing_spaces_hang_on_android` | `latinscale@mobile`、`hang-tol@print` | 已证 |
| 纸页没有行宽容差 | Android | 补测第 5 条 | —（无规则代码，靠既有断行） | `hang-tol@print` | 已证 |
| 不写 `w:kern` 也调字距；写了照阈值；只差颜色的 run 跨界照调 | Android | §3.5，补测第 6 条 | `android_kerning.rs`（6） | `kern-off@print`、`kern-48@print` | 已证+假设（只测 Latin `AV`；字号 / 字体不同的两侧不补） |
| 禁则表（不挂标点、类读取的闭括号类） | Android | `kinsoku.md`、[kinsoku 专题](PREREG-2026-09-18-kinsoku2.md) | `kinsoku_android.rs`（21，名字带 `assumed_` 的是假设） | 窄路径回放 186 | 已证+假设 |

## 纵向与分页

| 规则 | 平台 / 视图 | 依据 | 测试 | 读数 | 状态 |
| --- | --- | --- | --- | --- | --- |
| `beforeLines` / `afterLines` 优先；一行 = 网格 `linePitch` 或 240 | 全部 | §3.6，补测第 4 条 | `spacing_lines.rs`（4） | `before-lines*`、`after-*` 7 | 已证+假设（写 0 退回；`snapToChars` 按无网格） |
| 正文以表格结尾时补一个缺省空段（只在排版层） | 全部 | §3.7，`table*.md` | `table_tail.rs` | `table30-*`、`table32-*`、`table18-h839` | 已证 |
| contextualSpacing：同样式时只去本段一侧，去掉的段后仍抵下一段段前 | 全部 | §3.8，补测第 8 条 | `contextual_spacing.rs`（6） | `cs-*`、`bp-*`、`context-*` | 已证+假设（页顶段的段前调小未测） |
| 样式 part 不由主文档关系引到时不加载 | 全部 | §3.8 `sr-doc` / `sr-root` | `contextual_spacing.rs::a_styles_part_outside_the_document_relationships_is_not_loaded` | `sr-doc`、`sr-root` | 已证（`sr-root` 的段高 505 未解释，见[待决问题](OPEN-QUESTIONS.md)） |
| 缺省字体等线 11pt（装了等线时） | Android | §3.9、§4，补测第 1、2 条 | `android_default_font.rs` | `longpage-auto12pt` 等 | 已证+假设（只测 Latin 与数字） |
| 东亚代码页字体单倍行高 × 1.3 | Android | §3.9 | `android_default_font.rs::android_scales_east_asian_code_page_fonts_by_one_point_three` | auto 行高 12、docGrid | 已证+假设（升降部怎么分未测；桌面未启用） |
| auto 多倍行距的页末一行只要单倍高 | Android | §3.9 | `android_default_font.rs::android_fits_the_last_auto_multiple_line_by_its_single_height` | `longpage-auto-double/triple` | 已证 |
| 打印视图行网格：最小整数倍步距；页末一行单倍高 + ½ 余量 | Android 打印 | §3.10 | `grid_lines.rs`（3） | docGrid 27 | 已证+假设（½ 只知在 (0.071, 0.531]） |
| 没写 `w:trHeight` 的行随内容高 | 全部 | §3.11 | `table_tail.rs::rows_without_a_declared_height_grow_to_their_content` | `table24/28/30/31/32-*` 6 | 已证 |
| 缺省段后 160（无样式 part 且没写段后） | Android | §3.12，补测第 10 条 | `android_default_spacing.rs` | `sp-default` | 已证（有样式 part 而不写段后未测） |
| 可见 `single` 边框把行顶边框宽计入行高；缺省单元格边距 108 / 0；`tblW auto` 取 `tcW` | 全部 | §3.12，补测第 11 条 | `android_default_spacing.rs::bordered_auto_width_rows_take_the_default_space_after_and_the_border`、`table_flow.rs::undeclared_cell_margins_take_the_table_defaults` | `tt-*`、`talltable-080` | 已证+假设（精确行高配边框未测） |
| 脚注区 = 各条脚注行高之和 + 每页一份分隔线区 (293, 308] | Android 打印 | 补测第 12 条 | — | `fn-*` 6、`footnote*` 4 | **缺口**（未实现） |

## 不做的（范围与非目标）

承诺范围以 [北极星规划 §4](NORTH-STAR-AND-ROADMAP.md) 为准。对齐工作另外遵守：

- **不拟合常数。** 规则里的数都来自 Word 读数或字体文件；读数只夹出区间时取的值标成假设（如网格页末的 ½）。
- **平台规则不外溢。** Android 读数推出的规则只经平台 / 视图开关生效；只有证明是 OOXML 语义（两边 Word 都一样）
  或 Mac 采集逐字节不变时才对全部平台开。
- **不把手机字体放进仓库。** 依赖等线的读数在没有字体的机器上是 UNDECIDABLE。
- **不修改源文档来迁就 Word。** 打开时的修复（表后空段）只在排版层模拟，装载诊断照留。
- **反汇编与 P0 结论只当线索。** 进本表的规则必须有 Word 的排版输出作依据。
- **暂不做**：移动视图的行高 / 页（没有页，没有可比读数）、图片环绕、多格表的列宽公式、
  `atLeast` / `auto` 带值的表格行（都列在[待决问题](OPEN-QUESTIONS.md)里，写明缺什么读数）。
