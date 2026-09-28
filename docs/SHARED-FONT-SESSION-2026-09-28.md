# R02 共享文档会话：SVG CLI 与 layout-trace 走同一条路（2026-09-28）

本片执行 [北极星规划](NORTH-STAR-AND-ROADMAP.md) 的 R02：从 SVG outlines 管线提取共享的
真实字体会话，并立即由入口消费。两个 CLI（`render`、`layout-trace`）都改为经会话装载、排版
与绘制。C ABI、WASM 与 WebGL 仍是旧路径，留给 R03。没有运行 Word、原生注入或新的采集。

## 交付物

`crates/core/src/session.rs`，从 crate 根导出：

| 项 | 作用 |
| --- | --- |
| `PreparedDocument::load(&[u8])` | `load_document` + `layout_document`；可套 `apply_page_overrides`；`has_layout_content()`；`diagnostics()` 给装载与投影阶段的诊断 |
| `layout_approximate(&LayoutOptions)` | 近似模式：`SimpleMetrics`，不带字体，绘制不整形，带 `METRICS_APPROXIMATE` |
| `layout_with_fonts(FontRegistry, VerticalGrid, &LayoutOptions)`（`fontenv`） | 真字体：注册表按值交进会话，量宽、整形与绘制同用它；至少要一个正文字体 |
| `LayoutOptions { platform, view, wrap }` | 默认 Desktop + Print + `WrapPolicy::None`，等于 `Engine::new` |
| `WrapPolicy::{None, Anchors}` | `Anchors` 重开包按 `AnchorScan` 绕排，版心取页面覆盖之后的第 0 节 |
| `DocumentSession` | 只读：`pages`、`document`、`paint()`、`faces`、`font_fingerprint`、`fonts()`、`vertical_grid()`、`anchors()`、`diagnostics()` / `layout_diagnostics()`、`is_approximate()` |
| `SessionDiagnostic { code, message }` | `DiagnosticCode` 有稳定的 `as_str()`（如 `GLYPH_NOMINAL`），文字给人看 |
| `SessionError { kind, diagnostics }` | `NoLayoutContent` / `NoPrimaryFont` / `AnchorScan`；已收集的诊断随错误交出 |

附带：`LayoutDocument::has_layout_content()`（段落或受支持表格非空）、`FontRegistry` 的
crate 内 `char_coverage`（与 `shape_text` 同口径的缺字分类）。

## 会话合同

- **同一字体环境。** 注册表按值交进会话，之后只读（`fonts()` 给 `&FontRegistry`）。
  `paint()` 不接受外面的 shaper，face 表就是排版时的 `face_ids()`。换字体只能建新会话、重排。
- **明确的度量模式。** 近似模式没有字体、字形序列为空、指纹为 `None`，诊断里写明不进精确验收。
  真字体模式只有回退字体时拒绝（`NoPrimaryFont`）：回退链只接 eastAsia 槽的字符，其余字符没有 face、
  被跳过，排出来不是一份连贯的版面。
- **字体快照。** 会话留着注册表本身：`font_fingerprint()`（fontenv 指纹）、`faces()`（注册顺序，
  即整形器下标）、`fonts().fallback_faces()`（回退次序）、`vertical_grid()`。fontenv 指纹不含角色
  与次序，核角色要看后两项。R02 没有另造会话键——还没有第二个调用方需要它。
- **诊断不丢。** 装载（并排 `w:rPr` 合并与失败）、投影原文、跳过的块、解析器告警、环绕近似与
  读不出的锚定、字体覆盖（见下）全部进诊断；建会话失败时随 `SessionError` 交出。
- **有可排版内容**按 `has_layout_content()`：受支持表格至少一行一格一段，所以这与
  layout-trace 原来的「正文段落加单元格段落」判据等价。缺尾段的诊断照留，入口不替 Word 修文档。

### 字体覆盖诊断

真字体模式扫正文与单元格里不隐藏的 run，按**源字符**分类，口径同 `shape_text`：

| 码 | 条件 | 以前 |
| --- | --- | --- |
| `GLYPH_NOMINAL` | 谁都画不出的 CJK，名义 1 em 画 `.notdef` | 只有轨迹顶层的 `notdefGlyphs` 计数 |
| `GLYPH_DROPPED` | 谁都画不出的其他成字形字符，跳过、不占宽度 | 无处计数（回退报告只数 eastAsia 槽） |
| `FONT_FAMILY_SUBSTITUTED` | 文档点名的族没装：字符所在的槽，没写就是 ascii / hAnsi 槽 | 只有 `layout-trace --require` 能发现 |
| `FONT_STYLE_SUBSTITUTED` | 族装了、选中的也是该族的 face，但没有要的粗体 / 斜体，用了字重与斜度最接近的 | 无处可见；引擎不合成粗体 / 斜体，宽度随之不同 |

哪个槽都没写时 `family` 是桥接层的宿主缺省 `Times New Roman, SimSun, serif`，不是文档要的，
不报（按码位选 face）。`w:caps` 下显示字符换了 face 的情形与段落标记不在扫描内。

## 入口差异：统一了什么，什么留作显式策略

| 项 | 现在 | 说明 |
| --- | --- | --- |
| 装载、投影、页面覆盖 | 统一 | 两个 CLI 都用 `PreparedDocument` |
| 引擎构造、度量、绘制 | 统一 | `Engine::with_wrap(m, sections[0].setup, wrap).with_platform(p, v)`；绘制用会话的注册表 |
| 可排版内容判据 | 统一 | `has_layout_content()`；错误文字仍是 `没有可排版的段落` |
| 诊断 | 统一 | 两个 CLI 先打 `PreparedDocument::diagnostics()`，排完再打 `layout_diagnostics()` |
| 环绕 | 显式 `WrapPolicy` | render `Anchors`，layout-trace `None`（60 例验收文档都没有锚定对象） |
| 平台、视图、纵向栅格 | 显式选项 | render 用默认 Desktop / Print / 无栅格；layout-trace 照命令行 |
| TTC | 入口策略 | render `path#i` 装一个 face（缺省 0）；layout-trace 裸路径装到装不进为止 |
| 回退字体 | 入口策略 | render 按声明全装；layout-trace 只装盖得住缺字的 face（`Trigger` 不变） |
| 字体装载失败 | 入口策略 | render 停；layout-trace 0 号 face 装不进只告警，全都装不进就走近似模式并在诊断里说 |

回退字体的两种装法**确实排得不同**，所以没有硬统一。`fixtures/table.docx` 以
`--font LiberationSans --fallback-font Droid` 为例：段落标记的空格带 eastAsia 提示，
render 全装时它落到 Droid（SVG `matrix(0.03515625 …`，9pt / 256 upem）；layout-trace 的缺字扫描
不看段落标记，候选为 0，Droid 不装，标记用 Liberation Sans（face `4659bc0c…`）。
layout-trace 注释里「盖不住的不装——装了也不改结果」对段落标记不成立。哪种与 Word 一致**未测**；
R03 的「与 Rust 一致」比的是同一套显式输入下的会话结果，不是 layout-trace 的历史轨迹。

## 有意的输出变化

- render 接受只有表格的文档（`table32-rowh` 现在两种模式都出图），缺尾段诊断照打；
- render 的 stderr 多出会话诊断（并排 `w:rPr` 合并计数、跳过的块、解析器告警、近似模式、
  环绕近似、字体覆盖与字形替换）；stdout 摘要改为 `段落 N（表格 M）`，跳过块的旧说明
  「表格 / 绘图等，本版未实现」改为「不支持的表格 / 绘图等，见 layout 诊断」；
- layout-trace 拒绝 `--metrics simple` 与 `--font` 同用（原来是桩度量排行、真字体整形出字形，
  两边对不上），拒绝 `--metrics` 的未知取值（原来悄悄当 real）；没有工具这样调用；
- layout-trace 的 `--font` 读不到时报错带上路径；页面覆盖出错时先打完装载诊断再报错；
- 两个 CLI 的 stderr 多出会话诊断；出错时最后一行仍是原来的 `Error: "…"`。

轨迹 JSON、SVG 字节均不变（见下）。

## 验证

改动前的二进制：render（default / fontenv）与 layout-trace
`d0593c5c82e63dd9570d85bef2e47074ebde80799b6369bf781a2d05fec90239`（与 `story-diagnostics-*`
基线同一个）。改动后的 layout-trace 为
`706e3864119ae42357791247f1e5145c43168e2b27e8ac3d2bc6700b618c809f`，三组回放与面板用它
（`artifacts/r02-session-2026-09-28/final2/`）；render 逐字节与测试矩阵在同一份代码上跑
（`final/`，之后只改了注释）。`artifacts/` 在本地，不入库。

- **render 逐字节**：`fixtures/*.docx` 与 60 例验收的全部 DOCX 共 63 份，text 模式加 outlines 三组
  字体（Liberation Sans + Droid；Serif + Sans + Droid；DejaVu + [Liberation Sans, Droid]）。
  fontenv 构建 252 次运行，原来成功的 240 份 HTML 全部相同；default 构建 63 次 text 运行，
  原来成功的 60 份全部相同。唯一新增成功的是 `table32-rowh`（4 + 1 次）；原来失败的其余运行仍失败。
- **Mac 回放**：25 份 trace 与 `story-diagnostics-mac-replay-2026-09-27/after` 逐字节相同，
  仍为 30 bundle、0 OK / 23 FAIL / 7 UNDECIDABLE。
- **Android 回放**：11 份 `engine.json` 与 `android-replay-story-diagnostics-2026-09-27` 逐字节相同，
  OK=11，186/186 行区间。
- **验收报告**：17 份 trace 与 `acceptance-reports-story-diagnostics-2026-09-27` 逐字节相同；
  面板以 `--previous` 对 `acceptance-panel-story-diagnostics-2026-09-27/panel.json`：
  严格 0/60，新增退化 0。
- **新测试**，全部走真实 DOCX 字节：
  - `crates/core/tests/session.rs`（default 7 项，fontenv 15 项）：会话等于手拼的 Engine / paint
    路径（近似与真字体）；平台与视图各自传到引擎（缺省制表位看平台、段末分页符看视图）；
    页面覆盖先于排版；装载诊断与排版诊断拼回全体且不重叠；只有表格的文档；无内容错误带诊断；
    环绕只在要时扫；只有回退字体被拒；缺字计数含单元格、不含隐藏 run，与 `notdef_glyph_count`
    对上；未装的点名族、缺粗体 face 会报，宿主缺省族不报。
  - `crates/core/tests/layout_trace_cli.rs`（fontenv 4 项）：CLI 轨迹等于会话 API 结果；
    `--platform android --view mobile` 进了会话；无内容时最后一行仍是 `Error: "没有可排版的段落"`；
    `--metrics simple --font` 被拒。
  - `crates/svg/tests/render_cli.rs` 新增 4 项：只有表格的文档两种模式都出图、诊断只打一次；
    `wrap.docx` 仍绕排并说明是近似；轮廓模式报缺字与未装族名。
  - 评审时对 11 处关键行做了变异（平台 / 视图被忽略、诊断切分错位、缺字漏单元格或数隐藏 run、
    宿主缺省族被报、CLI 不打排版诊断、render 不绕排或重复打装载诊断、绘制不用会话字体），
    每一处都有测试失败。

测试数（均为 `RSWORD_TEST_CALIBRI=/tmp/wordfonts/calibri.ttf`；改动前在 `db0b01c` 的干净工作树上跑）：

| 命令 | 改动前 | 改动后 |
| --- | --- | --- |
| `cargo test --offline --workspace` | 636 passed / 0 failed / 2 ignored | 645 / 0 / 2 |
| `cargo test --offline --workspace --features fontenv` | 752 / 0 / 14 | 776 / 0 / 14 |
| `cargo test --offline --workspace --features rsword-layout-core/fontenv` | 743 / 0 / 14 | 765 / 0 / 14 |

增量与新增测试逐项对得上：default +9（session 7、render_cli 2）；fontenv +24（session 15、
layout_trace_cli 4、render_cli 4、layout-trace 内测 1）；第三行 +22（这个组合不开 SVG 的 fontenv，
render_cli 只多 2 项）。第三行就是 [验收面板](ACCEPTANCE-PANEL-2026-09-27.md) 记 743 时用的命令；
它**不跑** SVG 的轮廓测试，那些要看第二行。
`cargo clippy --offline --workspace --all-targets -- -D warnings` 在 default、`--features fontenv`、
`--features rsword-layout-core/fontenv` 三种配置下通过；
`cargo check --offline --workspace --all-features --target wasm32-unknown-unknown --locked` 通过。

## 未做与交给 R03 的

> 以下 R03 条目已由 [绑定层接入](BINDING-SESSION-2026-09-28.md) 完成（字形图集与未钉版本的路径依赖除外）。

- C ABI、WASM 仍固定 `SimpleMetrics`、丢弃全部诊断、用 `paras.is_empty()` 判空；WebGL 仍是
  近似排版配真字体整形。三者改用会话时一起补诊断通道与判空，不先单独放宽判空
  （放宽后缺尾段的诊断会被它们吞掉）。
- WebGL 的字体集比文档活得长，栅格化要 `&mut` 注册表。会话把注册表按值收走、只读交出；
  R03 要为图集另给一个不能改字体的栅格化入口，不能直接露出 `rasterizer_mut`
  （`SkrifaRasterizer::add_face` 可以按键覆盖已注册的字节）。
- 跨入口比较用的规范化结果、会话键（角色 + 次序 + 选项）、按页绘制 `paint_page`：
  R02 没有调用方，R03 随 C ABI / WASM 一起加。
- 构建清单：`../docx-layout` 仍按本机路径引用、未钉版本（本片验证时为 `72d3ada`，已跟踪文件无改动）。
