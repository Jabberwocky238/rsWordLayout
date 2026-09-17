# 测试字体

按 Word 的四个字体槽（`w:rFonts`）覆盖，供 `probe_fonts` 与将来的渲染回归使用。

| 文件 | 槽 | 许可 |
| --- | --- | --- |
| `LiberationSerif-Regular.ttf` / `-Bold.ttf` | `ascii` / `hAnsi` | SIL-OFL-1.1 |
| `LiberationSans-Regular.ttf` | `ascii` / `hAnsi` | SIL-OFL-1.1 |
| `LiberationMono-Regular.ttf` | 等宽 | SIL-OFL-1.1 |
| `DejaVuSans.ttf` | `hAnsi`（拉丁/希腊/西里尔） | Bitstream Vera |
| `DroidSansFallbackFull.ttf` | `eastAsia`（CJK） | Apache-2.0 |

三种许可都允许再分发且不传染。

## 为什么是 Liberation

它们是 Word 常用字体的**度量兼容克隆**（Serif↔Times New Roman、Sans↔Arial、
Mono↔Courier New）：advance width 与原字体逐字相同。

这一点有实测依据：度量兼容替换在几何上**完全不可见**——2618 条字形记录的
`glyphOrigin` 与 `advanceVector` 逐位相同（max |Δ| = 0.000000pt），只有字体名不同。

所以它们能验证**布局几何**（断行与分页结果应与真实 Word 一致），
但**不能**验证渲染像素——字形外观与原字体不同。

## 为什么不是 WOFF

skrifa 只认原始 SFNT（TTF/OTF/TTC）。WOFF/WOFF2 外面套了压缩层（WOFF2 用 Brotli），
要先解包才能读，而解出来的与直接放 TTF 一样。WOFF 的体积优势是为网络传输设计的：
原生侧从磁盘读不需要，wasm 侧走 HTTP 已有 gzip/brotli。

## 缺一个 OTF

CFF 轮廓（`.otf`）与 TrueType 轮廓（`.ttf`）在 skrifa 里走**不同的解析路径**，
本应各有样本。探针验证过 CFF 路径可用（同一个 `A` 在 OTF 里是 gid 34、TTF 里是 36，
说明字符映射与轮廓解析都是独立的），但本机唯一的 OTF 来源包是 AGPL-3，
与本仓库的 MIT/Apache 双许可不相容，故未收录。

需要时可自行放一个许可允许再分发的 OTF 进来，`probe_fonts` 会自动带上它。
