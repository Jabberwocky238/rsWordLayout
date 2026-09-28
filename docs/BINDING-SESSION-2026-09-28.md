# R03 C ABI、WASM 与 WebGL 改用共享文档会话（2026-09-28）

本片执行 [北极星规划](NORTH-STAR-AND-ROADMAP.md) 的 R03：C ABI、WASM（以及基于它的 WebGL）
逐个采用 R02 的构建合同（[共享文档会话](SHARED-FONT-SESSION-2026-09-28.md)）。此前三者固定
`SimpleMetrics`、丢弃全部诊断、用 `paras.is_empty()` 判空；WebGL 还用近似度量排版、用真字体整形绘制。
没有运行 Word、原生注入或新的采集。

## 核心新增（`crates/core/src/session.rs`）

| 项 | 作用 | 调用方 |
| --- | --- | --- |
| `FontSources`（`fontenv`） | 按声明顺序记下字体与角色；`build()` 重放出一份相同的注册表交给会话；`rasterizer()` 只交出 `&mut dyn Rasterizer` 给图集 | C ABI 的 `RslFonts`、WebGL 的 `FontSet`、WASM 会话 |
| `DocumentSession::paint_page` | 一页的绘制指令，与 `paint()` 的那一页相同 | C ABI 帧、WASM / WebGL |
| `DocumentSession::same_fonts` | face 集合与顺序、回退链成员与次序、指纹都相同。指纹不含角色，只比指纹会放过「同一组 face 换了角色」 | WebGL `render_page` |
| `DocumentSession::layout_json` | 规范化布局结果 `rsword-layout-result/1`：模式与选项、字体快照、轨迹的全部页 / 行 / 源区间 / 字形、`layoutInput`、表格行盒、全部诊断；不含路径与 DPI | C ABI、WASM、跨入口测试 |
| `DocumentSession::table_layout` | 表格行盒记账，由 `layout-trace` 搬来，它现在也调这个 | layout-trace、`layout_json` |
| `SessionError::report` | 错误原因加随错误交出的诊断，一条一行 `CODE: 原文` | C ABI、WASM |

`FontRegistry` 不能克隆，会话又要按值收走注册表，所以宿主字体集记下字节与角色、每建一个会话重放一次。
注册是确定的：同样的字节、序号与角色按同样顺序装，face 表、回退链与指纹都相同（有测试）。
代价是字节多存一份。

## 各入口

**WASM**（`crates/wasm`，新特性 `fontenv`）：`LayoutSession` 持有 `DocumentSession`。
`build(docx, dpi)` 仍是默认选项的近似会话；新增 `build_approximate(docx, dpi, &options)` 与
`build_with_fonts(docx, dpi, &FontSources, grid, &options)`。`paint(index)` / `paint_all()` 不再接收外面的
shaper 与 face 表（那正是「布局后换一套 shaper」的入口）。新增 `diagnostics()`、`diagnostics_json()`、
`layout_json()`、`is_approximate()`。建会话失败的字符串带上诊断。

**WebGL**（`crates/webgl`，wasm32 下开 `rsword-layout-wasm/fontenv`）：

- `FontSet` 改存 `FontSources`，新增 `add_fallback`；
- 新增 `LayoutSession.withFonts(docx, dpi, fonts)`：量宽、整形、绘制都用字体集当时的快照。
  两参数构造保留，是近似会话；
- `render_page` 拒绝近似会话（没有字形，画不了文字），也拒绝字体集在排版之后变了的情形
  （`same_fonts` 不成立），错误信息提示重排；绘制用会话自己的字形，`FontSet` 只用来栅格化；
- `web/src/main.js`：Droid 改以回退字体注册（原来当正文字体，会按内容哈希抢走拉丁字母），
  有字体时用 `withFonts` 建会话，诊断打到控制台。

**C ABI**（`crates/cffi`，新特性 `fontenv`）：

- `rsl_session_new` 签名不变，是默认选项的近似会话；
- 新增 `rsl_session_new_ex(data, len, dpi, fonts, options)`，`fonts` 为 NULL 时近似；
- `RslOptions { platform, view, wrap, vertical_grid }`（全零即默认）与 `RSL_PLATFORM_*` / `RSL_VIEW_*` /
  `RSL_WRAP_*` / `RSL_GRID_*`。取值无效、近似模式给了纵向栅格都报错；
- `fontenv` 下有 `rsl_fonts_new` / `rsl_fonts_add(fonts, data, len, index, fallback)` / `rsl_fonts_free`，
  装载错误码（如 `FONT_INVALID`）经 `rsl_last_error` 取；
- 诊断：`rsl_diagnostic_count` / `_code` / `_message`，指针在会话释放前有效；
  `rsl_session_is_approximate`；`rsl_layout_json`（用 `rsl_string_free` 释放）；
- 建会话失败时 `rsl_last_error` 的文字带上诊断，一条一行；
- 不开 `fontenv` 时 C 侧造不出字体集，传进非空的坏指针会报错，不会 panic 越过边界。

三者判空都改为 `has_layout_content()`（段落或受支持表格），缺尾段的诊断照留。
页面几何原来传 `PageSetup::a4()`，排版实际用的是第 0 节的几何（`layout_sections`），所以不变。

## 行为变化

- C ABI、WASM 接受只有表格的文档；原来被丢掉的诊断现在交出。
- WebGL 两参数构造的会话不能再 `render_page`，原来是近似排版配真字体画字，现在报错要求 `withFonts`。
  JS 演示已改；外部调用方需要改成 `withFonts`。
- WASM 的 `paint` / `paint_all` 去掉 shaper 与 face 参数（仓库内唯一的调用方 WebGL 已改）。
- C ABI 的帧仍只有矩形类几何：本库还不给 C 侧字形图集，真字体会话的字形批次同样为空。

## 验证

**跨入口一致**：同一份 DOCX、同一套字体与选项，比整份 `layout_json`，不只比页数。测试文档包含
西文、隐藏的 run、eastAsia 槽点名 SimSun 而走回退链的汉字、一张受支持表格和尾段。

- `crates/wasm/tests/cross_entry.rs`（default 3 项，fontenv 6 项）、`crates/cffi/tests/cross_entry.rs`
  （default 3 项，fontenv 5 项，经导出的 `extern "C"` 函数调用）覆盖以下内容：
  - 近似会话（Android + 移动视图）与 Rust 逐字节相同；
  - 真字体会话（Liberation Sans + Droid 回退，Mac 栅格）与 Rust 逐字节相同，并核对：
    - 汉字由回退 face 画；
    - 隐藏区间不出字形、源位置仍保留（汉字从 15 起）；
    - 表格行盒在结果里；
  - 字体集之后再变，已建会话的结果不变，`same_fonts` 转为假；
  - 错误路径：无内容时错误带 `BLOCKS_SKIPPED` 等诊断；坏字体 `FONT_INVALID`；只有回退字体被拒；
    无效选项被拒；只有表格的文档能排。
- `crates/core/tests/session.rs` 新增 4 项：
  - `paint_page` 逐页等于 `paint`；
  - 同一组 face 换角色时指纹相同而 `same_fonts` 为假；
  - `FontSources` 重放得到相同的注册表，装不进的字体不改字体集；
  - `layout_json` 的内容，以及重建后逐字节相同。
- **浏览器**（Chrome，最小测试页加载 `wasm-bindgen --target web` 产物，没有经过 Vite 演示页）：
  - DejaVu + Droid 回退用 `withFonts` 排 `fixtures/cjk-plain.docx`：7 页，第一页 WebGL 画出文字
    （459 个暗像素）；
  - 浏览器的 `layout_json` 与原生会话同输入逐字节相同（174614 字节，301 个字形）；
  - 近似会话、Droid 升为正文字体后的旧会话都被 `render_page` 拒绝；再加同一份字节不改变字体集，照常绘制。

**不变的部分**：

- `layout-trace` 表格输出改调核心之后：
  - Mac 25 份 trace、Android 11 份 `engine.json`（186/186）、验收 17 份 trace
    （其中 2 份带 `tableLayout`）与 `story-diagnostics-*` 基线逐字节相同；
  - 面板严格 0/60、新增退化 0；
  - 二进制 SHA-256 为 `99e03c9889fa71d925f99179193477726573c7cd3261310e7b17e6d803d60ee9`。
- render 与 `c567d61` 相比，`fixtures/*.docx` 34 份 × text / 两组轮廓字体，102/102 的 HTML 与 stderr 相同。

测试数（均为 `RSWORD_TEST_CALIBRI=/tmp/wordfonts/calibri.ttf`；「之前」是 `c567d61`，见 R02 文档）：

| 命令 | 之前 | 之后 |
| --- | --- | --- |
| `cargo test --offline --workspace` | 645 / 0 / 2 | 652 / 0 / 2 |
| `cargo test --offline --workspace --features fontenv` | 776 / 0 / 14 | 791 / 0 / 14 |
| `cargo test --offline --workspace --features rsword-layout-core/fontenv` | 765 / 0 / 14 | 775 / 0 / 14 |

增量逐项对得上：default +7（session 1、wasm 3、cffi 3）；fontenv +15（session 4、wasm 6、cffi 5）；
第三行 +10（这个组合不开 cffi / wasm 自己的 `fontenv`，两者各 3 项）。
`cargo clippy --offline --workspace --all-targets -- -D warnings` 在 default、`--features fontenv`、
`--features rsword-layout-core/fontenv`、`--all-features` 下通过；`cargo check --offline --workspace
--target wasm32-unknown-unknown --locked` 在 default 与 `--all-features` 下通过。

## 未做

- WebGL 演示页（Vite）本身没有在浏览器里跑：本机没有 `node_modules`，没有装。
  上述浏览器验证用的是同一份 wasm 产物和同样的调用顺序。
- WebGL 与 C ABI 的会话选项：WebGL 只用默认选项（桌面、分页、不绕排、不量化）；C ABI 全部可选。
- C 侧字形图集与 GPU 的缩放 / 状态 / 裁剪属于 R14。
- `../docx-layout` 仍按本机路径引用、未钉版本。
