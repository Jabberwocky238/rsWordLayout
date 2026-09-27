# SVG 定位字形输出

`9fd8948` 将自动间距接入字形绘制后，SVG 仍忽略 `DrawCmd::DrawGlyphs.glyphs`，
只将原文交给浏览器重新整形。`render` 命令又固定使用 `SimpleMetrics` 和无 shaper
的绘制路径，因此实际导出没有消费自动间距、回退 face 和小型大写的字形坐标。

## 两种输出合同

旧的 `SvgCanvas` / `render_html` 保留可选择文字的 `<text>` 输出；它继续是近似模式。
新增 `fontenv` 特性下的 `render_outlined_html(list, fonts, title)` 返回有错误处理的
定位轮廓 HTML。字体库由调用方传入，须与生成 `PaintList` 时使用的 face 标识一致。
它不重新根据 Unicode 字符选择字形，也不依赖浏览器字体替换。

命令行入口如下：

```sh
cargo run --offline -p rsword-layout-svg --features fontenv --bin render -- \
  --text-mode outlines \
  --font fixtures/fonts/LiberationSans-Regular.ttf \
  --fallback-font fixtures/fonts/DroidSansFallbackFull.ttf \
  fixtures/cjk-plain.docx layout.html
```

主字体全部先注册，回退字体按声明顺序注册，沿用 `FontRegistry` 的选字体规则。
`path[#index]` 显式传递 TTC index，缺省为 0。轮廓模式必须提供至少一个主字体；
文本模式拒绝字体参数，避免参数被悄悄忽略。未启用 `fontenv` 的构建会在读取文档前
报告轮廓模式不可用。参数、字体或渲染失败时不会开始写出结果；输出仍使用原有
`std::fs::write`，没有声称具备原子写入或所有 I/O 失败下的数据保护。

## 字形定位规则

每个 `PositionedGlyph` 提供实际 face、glyph ID、百分之一点字号、精确横坐标和
1/7200 英寸纵坐标。SVG 直接读取该 face 的原字体字节和 TTC index，用锁定的
skrifa 0.44.0 提取默认变体、未提示的字形轮廓。原生二次和三次曲线保留浮点坐标，
不经过只支持整数 twips 的 core `Path`。

若 `sy = size_centipoints / 100 / units_per_em`，`sx` 为 `sy` 乘有效横向比例，
则每个字形使用 `matrix(sx 0 0 -sy x_pt y_fine/100)`。负纵向比例把字体的向上坐标
转成页面的向下坐标。缩放只作用于字形轮廓，不再次缩放绝对字形原点。
字距、自动间距与 kerning 已体现在字形位置中，不再重复施加；caps 和 small-caps
已经体现在 glyph ID 与各字形字号中，不再交给浏览器变换。颜色和透明度来自绘制命令。

字体尺度校验沿用 [OpenType `head` 规范](https://learn.microsoft.com/en-us/typography/opentype/spec/head)
规定的 `unitsPerEm` 16..16384 范围。页面坐标使用 SVG 用户单位和局部变换，相关合同见
[SVG 2 坐标系统](https://www.w3.org/TR/SVG2/coords.html)。浏览器的像素化检查与物理打印尺寸
是不同验收范围，本片没有增加打印尺寸对齐结论。

有效空格允许成功产生空轮廓；不存在的 face、无效 glyph ID、不可用轮廓格式、
读取或绘制失败不能当作空格。非空文本却没有定位字形时直接报错，不退回浏览器文本。
非有限字形横坐标或轮廓坐标也会报错。轮廓输出不承诺文字选择或搜索，HTML 提示与此一致。

图形状态也在本片修复：`Save` 记录当前嵌套层级，`Restore` 关闭该保存点以后
叠加的变换和裁剪；无保存点的额外恢复保持无操作。裁剪编号独立递增，并区分页，
避免恢复后复用 ID 或跨页 HTML 引用冲突。旧文本模式也使用这套恢复规则。
外层 `Transform` 的平移保留原始浮点 twips，不先截成整数，也不压到两位小数点值，
避免刚保留的字形坐标又因嵌套变换丢失精度。

## 验证边界

测试使用真实字体进入公开输出接口，检查 fallback、精细字号、缩放、tracking、
自动间距、簇与源位置、空轮廓和错误路径。独立浏览器检查使用本机 Chrome 的无界面
临时实例，核对真实 DOM 变换、字形轮廓范围及截图；它不操作 Word 或现有浏览器窗口。
工作区 `cargo test --offline --workspace --features fontenv` 为 740 通过、12 忽略；
SVG 默认配置另为 12 通过、0 忽略，两者包含重叠测试，不相加。工作区与 SVG 默认配置的
全 target Clippy 均以 `-D warnings` 通过。实际 CLI 分别导出同一 DOCX 的文字与轮廓模式，
执行前后绑定同一二进制；独立审计核对 7 条命令、126 个源码与清单输入、3 个资产，
全部通过。本片 core 相对 `9fd8948` 无变更，没有新增 Android 或 Mac Word 回放结论。

真实字体回归覆盖 TTC 内非零 face index；CLI 成功路径当前使用 TTF index 0。
浏览器样例使用 Liberation Sans、DejaVu Sans 与 Droid Sans Fallback，均为 index 0，
包括 12pt / 7.92pt 的自动间距、80% 横向比例加 tracking、small-caps、阿拉伯语必需
连字及组合标记、隐藏 run 和非 BMP 源位置。阿拉伯语样例不等于完整双向布局验收。

第一轮独立样例生成器解析到了六个不同于生产锁文件的传递依赖版本，保留为历史记录。
最终样例由生产锁文件中的包版本重新构建：69 个非生成器包的名称、版本、来源与校验和
均匹配，不据此声称 Cargo feature graph 完全相同。首轮浏览器检查器还出现三项误报：
两项将浮点 CTM 相减的误差只按差值计算，一项将数值等价的 transform 字符串当作差异。
原 FAIL 收据保留；第二版传播两端原点误差，并继续严格检查实际 CTM、轮廓、颜色和像素，
仅不比较原始 transform 字符串。10 项协议测试包含 0.001pt 真偏移的拒绝反例。

最终 `run-02` 由 Chrome 153 的独立临时实例重新采集，14 个样例全部通过：
57 项字形矩阵检查、4 组 whole/split 实际像素一致性、4 组自动间距位移，共 65 项。
FontTools 独立读取绑定字体并核对 57 个轮廓边界；Pillow 独立解码 14 张 PNG，核对
RGBA 哈希、非白色像素数与范围，全部通过。轮廓边界相同不等于完整曲线逐点相同。
另目视抽检自动间距和 small-caps 两张截图，文字可见且无明显裁切。
临时 Chrome 正常退出，专用 profile 已清理，本片未启动新的 Word 采集。

该路径忠实输出已有绘制指令，仍受上游字体选择、缺字策略、行布局和分页规则限制。
不实现字体可变轴选择、彩色或位图字形绘制，不额外合成粗体或斜体。本文的浏览器检查
不能作为 Word 字形或分页对齐证明；docGrid、混合字号自动间距等仍保留原有未测边界。
当前没有 CFF、可变或彩色字体的独立样例。

## 证据索引

忽略目录 `artifacts/` 保存原始失败、修正后结果、执行收据和独立审计；源码与本文随
提交保存，未将本机二进制、截图和生成文件强制加入 Git。冻结清单为 `SHA256SUMS`，
重放应新建目录，不改写已有证据。作者包、工作区验收和浏览器检查各自限定验证范围。

| 包 | 数据文件数 | SHA256SUMS 的 SHA256 |
| --- | --- | --- |
| `svg-positioning-2026-09-27`：作者 red/green 与聚焦检查 | 147 | `2b0b87fa095781a790b7f1989018701100b730434f3bdccf4bfd7b7c6fde6d85` |
| `svg-positioning-checks-2026-09-27`：工作区、CLI、独立审计 | 48 | `e1c2647c541f678abc39eefafad4bdf8f166072e6bee2e265d6f1e83f8b20421` |
| `svg-positioning-browser-input-2026-09-27`：历史依赖解析 | 27 | `eedc6d87dc93d4ea1c64c55b64305ea1e8f06942587487037d95fc95ee4b569d` |
| `svg-positioning-browser-input-root-lock-2026-09-27`：最终输入与独立复核 | 33 | `0921c624f0e421511cf57763e2e2e295fa810106d3aef9dff7932f15bc929087` |
| `svg-positioning-browser-2026-09-27`：历史与最终采集、独立字体与像素复核 | 97 | `4e4a4f041cf2bb11e95b2a0fcc2aeda3e83d1ff82c3a1664ce906b6d9c4c3609` |

实际 CLI 二进制 SHA256 为
`5c237f61570215d9c141881e2101f9296e7c7d12e27e61f5913aa754ceb6b666`；
最终浏览器输入 manifest 为
`9df3874c31257878e7cd96aebbabe1424d228844d9d07db7aadca74f23eb3c0e`，
`run-02/receipt.json` 为
`f63fbbea26ac41a4ecebe791b80816318fd400a10a2639b53d2858a72b26417e`。
