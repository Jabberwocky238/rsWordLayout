# As-built 记录

**只追加，不改旧条目。** 每一片排版改动提交时在末尾加一条：采集前的预测、结果、闸门数字、
`layout-trace` 的 SHA-256（`scripts/verify.sh` 第一行打印）。后来发现某条结论错了，就追加一条更正并指回去，
不回头改原文——这份记录要能回答「那一天、那个二进制，我们以为什么是对的」。

条目格式：

```
## 日期 提交 标题
- 规则：一句话（登记表的哪一行）
- 预测（采集前）：……
- 结果：Word ……；引擎 ……
- 闸门：打印视图 x/161（phone）· y/161（standin）；窄路径回放 186/186 ×2；Mac 25 份 不变/变了哪些
- binary：sha256 前 12 位（2026-10-05 之前的条目没有记录，写「未记」）
```

2026-10-04 之前的片记在各 `ENGINE-ITERATION-*` 与专题文档里，不补录。

---

## 2026-10-04 ac6420c / 440b2ce fitText；字符单位缩进与移动视图缩进缩放
- 规则：登记表「`w:fitText`」「字符单位缩进」「移动视图缩进」
- 预测：无独立采集前预测（依据是 word_analyse 已有读数）
- 结果：`fittext*` 3 份、窄路径 `ind-*` 一致；纸页 22 份缩进读数按移动视图比例逐行一致
- 闸门：打印视图 70 → 74/161；窄路径 29/32
- binary：未记

## 2026-10-04 1fd8887 / 4ca8daa 移动视图设备像素字宽；行尾空格挂出；Android 不写 kern 也调
- 规则：登记表「设备像素」「行尾空格」「字距」
- 结果：`i-plain` 95、`zero-scale` 55、`latinscale` 197 一致；`kern-off` 一致
- 闸门：打印视图 75/161；窄路径 32/32
- binary：未记

## 2026-10-04 cd64266 `beforeLines` / `afterLines`
- 预测（补测第 4 条）：固定 240 → sz48 21、grid312 21；按网格 → grid312 19；按字号 → sz48 更少
- 结果：Word 21、19 → 无网格 240、有网格 `linePitch`
- 闸门：打印视图 80/161
- binary：未记

## 2026-10-04 36d30b5 表后补的空段
- 结果：`table30-h504/506/510`、`table32-rowh` 一致；`h839` 等缺省字体
- 闸门：打印视图 84/161
- binary：未记

## 2026-10-04 c59dd6c `w:kern` 写了照阈值；跨 run 照调
- 预测（补测第 6 条）：「Android 完全不看 `w:kern`」→ 第 1 段一行
- 结果：Word 两行，末行 6 个 → **预测被推翻**，规则改为照阈值；第 3 段一行 → 跨 run 补调
- 闸门：打印视图 84/161 不变；`kern-48` 一致
- binary：未记

## 2026-10-04 68d18c7 contextualSpacing
- 预测（补测第 8 条）：「本段管本段」32/31/25/25/21；「任一段管两侧」32/31/32/32/21；both-phase「清零再 max」25 vs「整个去掉」27
- 结果：Word 32/31/25/25/21、27/27/27；`sr-doc` 21 / `sr-root` 30（样式 part 不由主文档关系引到时不加载）
- 闸门：打印视图 94/161
- binary：未记

## 2026-10-04 8811317 Android 缺省等线 11pt；东亚 ×1.3；多倍行距页末单倍高
- 预测（补测第 1 条）：缺省 11pt → `longpage-auto12pt` 47；12.5pt 跳档 → 51
- 结果：Word 47；进程映射里是 `DengXian-54497409372.ttf`
- 闸门：打印视图 113/161（phone）· 93/161（standin，`longpage-auto-triple` 替身恰好对上的那份变不一致，见 P0 对齐 §3.9）
- binary：未记

## 2026-10-04 b306c29 Android 打印视图行网格
- 结果：docGrid 27 份全部一致（phone）；α ∈ (0.071, 0.531]，取 ½ 为假设
- 闸门：打印视图 128/161 · 109/161
- binary：未记

## 2026-10-04 4fe9a41 没写行高的表格行随内容高
- 闸门：打印视图 134/161 · 115/161
- binary：未记

## 2026-10-04 60f0c3e Android 缺省段后 160；可见边框、缺省单元格边距、`tblW auto`
- 预测（补测第 10、11 条）：段后 160 → `sp-default` 24（段后 0 → 32）；「段后 160 + 边框 10」→ tt 21/28/29/29
- 结果：Word 24；21/28/29/29 → 预测全中
- 闸门：打印视图 135/161 · 116/161；窄路径回放 186/186 ×2；Mac 25 份不变
- binary：`773cab7debb4`（2026-10-05 补记：之后 afd2db8 未改引擎，闸门在同一二进制上建立基线）

## 2026-10-05 afd2db8 闸门入库
- 不改引擎。读数转录为 225 条（word_analyse 打印视图 159、窄路径 32、手机补测 34），基线按字体哈希记录
- 闸门：phone 193/225 · standin 171/225；窄路径回放 186/186 ×2；Mac 25 份与 `mac-traces.sha256` 相同
- binary：`773cab7debb4`

## 2026-10-05 整形缓存与性能闸门
- 不改规则。`layout-trace` 加 `--timing` / `--no-trace`；`tools/measure/gate/perf.py` 生成正文 / 表格 / 混排 1、10、100 页文档，量墙钟时间与峰值内存
- 采样（macOS `sample`，body-100）：约四成时间在 `Face::from_slice` 逐段重建 GSUB/GPOS 覆盖表，其次是 `slot_face` 逐字符两遍 `normalize_family`
- 改法：整形器用 `self_cell` 持有字节与首次解析后的 `rustybuzz::Face`（不写 unsafe）；`FontRegistry` 按（族名, 字重, 斜体）缓存候选 face 顺序，注册字体时清空
- 预测：只改耗时，输出逐字节不变
- 结果（standin 字体，release，`--no-trace`）：body-100（144 页）排版 14.4 s → 2.9 s，全程 14.9 s → 3.2 s；mixed-100 6.8 s → 1.5 s；table-100 0.48 s → 0.17 s；峰值内存 291 → 304 MiB（缓存的 face）
- 闸门：phone 193/225 · standin 171/225（不变）；窄路径回放 186/186 ×2；Mac 25 份逐字节不变；`perf-baseline.json` 写在本机
- binary：`3bd433b29928`

## 2026-10-05 拆 `layout.rs`：缺省值、段距、行网格
- 不改规则。`layout_document` 开头的预处理与相关常量、函数移到 `layout/defaults.rs`（Android 缺省字体、字号、段后）、`layout/spacing.rs`（行单位段距、contextualSpacing、`paragraph_gap_fine`）、`layout/grid.rs`（节步距、段落对齐网格、对齐行的推进与所需高度）；`layout.rs` 4028 → 3864 行
- 节步距原先在 `section_line_unit` 与行网格的 match 里各写一遍，条件相同，合成 `grid::line_pitch`
- 预测：输出逐字节不变
- 闸门：phone 193/225 · standin 171/225（不变）；窄路径回放 186/186 ×2；Mac 25 份逐字节不变
- binary：`128a7457a899`
