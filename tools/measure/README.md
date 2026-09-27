# `tools/measure` — 用真实 Word 验收 rsWordLayout

方法原文：[`docs/WORD-LAYOUT-MEASUREMENT-METHOD.md`](../../docs/WORD-LAYOUT-MEASUREMENT-METHOD.md)。
本文只写**这个仓库怎么落地它**，以及**在这台机器上哪些做得了、哪些做不了**。
方法里的数字与限定不在这里重复，实现里逐条引了节号。

## 一句话

不测 Word 的内部量（行基线、行盒——那条路已被证死，§1），
只测**每个字形被画在哪里**：把引擎输出的字形原点，与 Word 导出 PDF 里同一字形的原点逐个相比。

噪声底是 **0.0000pt**（§5），所以**默认容差就是 0**。引擎对 Word 出现任何非零差，
要么是引擎错，要么是配对错，**不可能是「量得不准」**。

## 装

```sh
uv venv tools/measure/.venv --python 3.13
VIRTUAL_ENV=tools/measure/.venv uv pip install 'pdfminer.six==20231228' pytest
```

`pdfminer.six` **钉死版本**：换版本就可能换读数，而读数是要拿来跟 0 比的。

## 用

```sh
cd tools/measure

./wm rules                              # §4 字形计数约定表（带分母与范围）
./wm preflight --font "Liberation Serif" # §9.2 采前核查：唯一防线
./wm capture case.docx bundle/          # §9.1 驱动 Word 采一次
./wm adopt <既有 run> bundle/            # 折算既有采集，不驱动 Word
./wm model bundle/                      # §3 折成页/行/字形，逐行报三态
./wm control repeat|positive|negative A B   # §9.3 / §9.4 对照
./wm selfcheck trace.json               # 只查轨迹与契约，**不需要 Word 采集**
./wm compare bundle/ trace.json         # §9.6 引擎 vs Word
```

### 引擎侧轨迹

```sh
cargo run --features fontenv --bin layout-trace -- \
    --require "Liberation Serif" \
    --font fixtures/fonts/LiberationSerif-Regular.ttf \
    case.docx trace.json
```

`--font` **不给就没有字形级记录**：没有整形器时 `paint_document` 不产字形序列，
轨迹里只有行，过不了比较器的字形层。

`--require` 不是可有可无的讲究：度量兼容克隆的字体替换**在几何上完全不可见，
只有字体名能发现**（§6.2）。核不过就退出——不核就跑，跑出来的是废数据，
而且废得看不出来。注意它核的是**实际装进来的族名**（`FontRegistry::families`），
不能拿 `select_face` 代替：那带 fallback，族没装也会返回一个能盖住码位的 face，
于是核查永远通过。

页面几何有三个选项，都不给就是 A4（11906 twips 宽）、四边 1 英寸，即上面与下面离线回放的命令：
`--margin <twips>` 设四边边距（默认 1440）；`--page-width <twips>` 改页宽（默认 11906），
先于下一个生效；`--content-width <twips>` 按（改过的）页宽把左右边距平分，凑出这个版心宽
（余量是奇数时右边多 1 twip，上下边距不动），版心比页宽还宽直接报错退出。word_analyse 的
Android 读数用 `--content-width 5329`（窄路径）与 `--content-width 10466`（纸页路径）；
改了页宽的夹具（`smallcaps-wide` 页宽 16000、版心 14560，`smallcaps-12000` 页宽 12000、
版心 10560）再加 `--page-width`。实际用的版心、页宽与左右边距印在 stderr。

`--platform mac|android` 与 `--view print|mobile` 说明在模拟哪个平台的 Word、哪种视图。
默认 `mac` + `print`，就是本目录一切采集所在的 Word for Mac 分页视图；上面的命令与下面的
离线回放都不传这两个选项，靠的就是这个默认。`android` 只给 Android Word 的读数
（word_analyse）用：它的窄路径（`w3=5329`）是移动视图，配 `--view mobile`；纸页路径
（`w3=10466`）仍是 `print`。两个取值都记进轨迹的 `metrics` 栏，拼错的取值直接报错退出。
目前读平台的断行规则有两条（CJK 回退字体装不装也看平台，见下文）：
- 文档没写 `w:defaultTabStop` 时的默认制表位：`mac` 照规范 720，`android` 221（拟合值，
  见 `crates/core/src/layout.rs`）；本目录的采集里没有制表符，Mac 回放不受影响。
- 行末标点挂出：`mac` 照段落的 `w:overflowPunct`（没写即开）把单个越界的 `。，）、` 挂出版心
  （本目录 `kinsoku` 采集实测），`android` 从不挂出（word_analyse `kinsoku.md` 实测 `）`、`。`）。
  没有单独的开关，由平台定，`metrics` 栏里写明。Mac 回放用默认的 `mac`，`kinsoku`、`kinsoku2`、
  `cjk-plain` 照旧。
  行首 / 行尾禁则两个平台共用一套；它扩充之后 `mac` 下也有几处未测的变化（`％。`、`～。`、`。〉`
  这样挨着新增禁则字符的标点不再挂出）。同样的序列拆在两个 run 里时，跨 run 回退退回行里更早的
  断点，与一个 run 里相同（`[21汉％][。10汉]` 20、`[22汉][。〉10汉]` 21，越界的 `。`、`，` 不开
  下一行；假设，Word 未测）。本目录的采集里没有这些序列，Mac 回放看不见它们；见
  `crates/core/src/layout.rs` 里 `Platform` 的说明。

读视图的规则有一条：段末手动分页符之后段内再无内容、段落不带分节符时，`print` 把段落标记收进分页符那一行
（本目录 `breaks-sections`、`vmisc2` 实测），`mobile` 让它另起一行（word_analyse `br-page` 窄路径
实测）。Mac 回放用默认的 `print`，照旧。移动视图没有页，`--view mobile` 轨迹里的页下标是引擎
照样分页排出来的，不对应 Word。

`--fallback-font <path>[#index]`（可重复，按给出的顺序）与 `--no-fallback` 管 CJK 回退链：
`--font` 盖不住的 eastAsia 字符（CJK 夹具的 eastAsia 槽写 SimSun，手机上没有）由回退链补，
回退链也盖不住的 CJK 字符画 `.notdef`、占 1 em，轨迹顶层多一个 `notdefGlyphs` 计数。
**默认只在 `--platform android` 下开**，用仓库里的 `fixtures/fonts/DroidSansFallbackFull.ttf`，
环境变量 `RSWORD_FALLBACK_FONT` 可换成手机上拉下来的 `NotoSansCJK-Regular.ttc#2`；`mac`（默认）
不装，所以本目录的采集与回放不会被悄悄补字。显式给的路径当场核，读不到就报错。
链上的字体只在盖得住「缺着」的字符时才装，「缺着」按链的来路分两种：

- 显式给的链（`--fallback-font`、`RSWORD_FALLBACK_FONT`）与选字体同一个判据：eastAsia 槽里的
  字体画不出就算，哪怕别的 `--font` 画得出（`w:hint="eastAsia"` 下的 `“`、空格），
  一个字符用哪个字体只看它自己；
- 默认的 Droid 只在有字符**哪个 `--font` 都画不出**时才装，所以全文都盖住的文档
  （如 `--font calibri --font NotoSansCJK` 排 CJK 夹具）轨迹除多出 `notdefGlyphs: 0` 外不变
  （字体指纹也不变）。代价是不局部：文档别处只要有一个缺字，Droid 就装上，
  并接走全文槽里画不出、它又盖得住的字符（包括 `--font` 画得出的）。

装了什么、各落到哪写进 stderr 与 `metrics` 栏。细节见 `layout-trace` 的文档注释。

照 word_analyse 的一条 Android 读数排（手机的 Calibri 不在仓库里，路径自备）：

```sh
cargo run --release --features fontenv --bin layout-trace -- \
    --platform android --view mobile \
    --font <手机的 calibri.ttf> --content-width 5329 \
    word_analyse/fixtures/br-page.docx trace.json
```

纸页路径去掉 `--view mobile`、`--content-width` 换成 10466。不传 `--platform android` 排出来的是
Mac 规则（缺省制表位 720、行末标点挂出、不装回退字体），对不上 Android 的读数。

## 离线全量回放

### Android word_analyse 边界回放

`android_replay.py` 只读兄弟项目的 `reports/diff/*.word.narrow.jsonl`，核对 DOCX
哈希后用当前引擎生成轨迹，逐行比较完整 UTF-16 区间。运行示例：

```sh
tools/measure/.venv/bin/python tools/measure/android_replay.py \
    --font /tmp/wordfonts/calibri.ttf \
    --fallback-font '/tmp/wordfonts/NotoSansCJK-Regular.ttc#2' \
    --assume-legacy-narrow --output artifacts/android-replay-new
```

从仓库根目录运行，字体路径按本机实际位置填写，输出目录必须不存在。
默认资料目录是 `../word_analyse`，可用 `--analysis-root` 改；`--fixture` 可重复筛选。
旧采集的视图与 CP 空间为 unknown，默认严格模式判 `UNDECIDABLE`；显式添加
`--assume-legacy-narrow` 后的匹配带条件假设，不代表新测证据。多行、缺行不会略过，
run 切分、几何和分页不在此评分范围内。命令、输入哈希和结果写入输出目录。
详见 [接入评估与证据限制](../../docs/WORD-ANALYSE-INTEGRATION-2026-09-27.md)。

### Android 页容器计数审计

`android_pages.py` 读取单份原始 PGIDX 日志，保留全部计数段和末次读数前后的宽度。
它可与 `--trace` 提供的既有引擎轨迹或 `--engine-page-count` 提供的数量比较，
自身不运行引擎。`--fixture` 必填，输入均记录哈希，`--output` 必须为新文件。

```sh
tools/measure/.venv/bin/python tools/measure/android_pages.py \
    ../word_analyse/reports/diff/print/dg-decide-139-n37.print.live.log \
    --fixture ../word_analyse/fixtures/dg-decide-139-n37.docx \
    --trace /path/to/existing-engine-trace.json \
    --assume-legacy-print --output artifacts/android-page-audit.json
```

旧 PGIDX 缺少文档、线程、通道和完成标记，严格模式始终不可判；
`--assume-legacy-print` 仅启用带显式历史假设的条件比较，不能覆盖哈希/模式冲突、
截断或达到 80 条采样上限。退出码为 OK=0、FAIL=1、UNDECIDABLE=2。
详见 [页数证据与 docGrid 边界](../../docs/ANDROID-PAGE-EVIDENCE-2026-09-27.md)。

### 原生 docGrid 整数参考回放

`docgrid_native.py` 为已恢复的原生整数 helper 提供 JSON 入口。输入必须显式给出
六个分量、tag、两个 u16 参数、u32 尺度、i32 周期及转换旁路；工具不推断字号、
DPI、DOCX 属性或运行分支。仓库样例全部为合成整数，不是采集到的 Word 内存。

```sh
python3 tools/measure/docgrid_native.py \
  tools/measure/cases/docgrid-native-synthetic.json \
  --output artifacts/docgrid-helper-replay.json

PYTHONPATH=tools/measure tools/measure/.venv/bin/python -m pytest -q \
  tools/measure/tests/test_docgrid_native.py
```

输出文件必须全新，完整写入后才发布。报告包含输入、模型与入口脚本哈希、中间值
及诊断，状态为 `ARITHMETIC_REFERENCE`。模型逐字节复用冻结的 `helper_model.py`，
现保存在 `wordmeasure/docgrid_reference.py`，运行时核对固定哈希；没有未提交的
artifact 运行依赖。报告中的 Word 哈希用于标识推导来源，不代表本次读取或执行 Word。

`components` 前五项为有符号 i32，末项为原样保留的 u32 位模式。tag 只接受 0/2，
所有字段必填；重复键、重复 ID、浮点整数、bool 冒充整数与越界值均拒绝。
结果的 `convertedPeriod` 在零尺度/零周期旁路保留输入寄存器值，不代表发生了转换。
负高度路径按原始 SDIV 加非零余数处理，不可概括为数学 ceiling。

工具只回放 helper，不组合宿主后处理、LS/PTS 返回值或页面占高，也不判断是否与
Word 对齐。其用途是复现整数步骤并为后续实现提供对照，证据边界见
[原生算法分析](../../docs/DOCGRID-ALGORITHM-EVIDENCE-2026-09-27.md)。

### 原生 DirectWrite 字体指标

`dwrite_metrics.py` 在独立子进程中调用本机 Word 随附的 `dwrite10`，测量指定字体文件的
`IDWriteFontFace1::GetMetrics` 和 `GetGdiCompatibleMetrics`。只支持 Darwin arm64，
并核对已验证的整个库文件 SHA256；父进程不加载原生库。

```sh
python3 tools/measure/dwrite_metrics.py \
  --font "/System/Library/Fonts/Supplemental/Times New Roman.ttf" \
  --em-size 16 --em-size 50 --em-size 2048 \
  --out artifacts/dwrite-metrics-new.json

tools/measure/.venv/bin/python -m pytest -q tools/measure/tests/test_dwrite_metrics.py
```

`--em-size` 可重复，必须能转换为正有限 binary32。参数单位为 DIP，输出指标保留 API
定义的设计单位；报告保存实际 binary32 数值。固定 `pixelsPerDip=1`、空 transform、
face index 0、无 simulations；先由 `Analyze` 获取 face type，再查询 Face1。
工具使用 isolated factory（类型 1），已有宿主静态调用证据使用 shared factory（类型 0）。
这些测量不等于 Word 排版，也不建立 DOCX 字号、Word `lfHeight` 与 `emSize` 的映射。

`--out` 必须是新文件，已有文件、目录及符号链接均拒绝覆盖；报告完整写好后原子发布。
报告含 UTC、平台、工具和输入前后哈希、原生命令、逐阶段 stdout/stderr（另附原始字节的
Base64）。子进程默认限时 45 秒；超时会终止并回收该子进程。HRESULT 失败、崩溃、超时、
协议缺失或哈希变化均输出 `FAILED` 收据并返回 1；只有完整指标和 Release 序列才输出
`MEASURED` 并返回 0。参数或输出路径错误返回 2。固定库哈希不代表所有动态依赖都已固定。

### 字体记录转换参考

`font_record.py` 读取上述成功测量，在显式宿主参数下重放 Face1 非 CFF 路径的数值转换。
当前要求单面 TrueType；它重新验证原始 stdout 协议与字体文件哈希，并从有效 OS/2 表
读取 signed16 xAvgCharWidth。初始化需要精确 designUnitsPerEm 请求的 GDI 指标，非复制
路径还需要精确 `float32(-lfHeight)` 请求的指标；缺少或重复结果冲突时拒绝，不能插值。
示例里的 2048 是已测 TNR 文件的设计单位数，并非所有字体的固定值。

```sh
python3 tools/measure/font_record.py \
  --measurements artifacts/dwrite-metrics-new.json \
  --lf-height -16 --width-scale 1 --escapement 0 \
  --out artifacts/font-record-new.json
```

三个宿主参数必须显式给定，示例不表示 Word 的 12pt 对应 -16。输出状态始终为
`ARITHMETIC_REFERENCE`，保存逐步 binary32 乘法、H 转换、半字截断、定义的 U 字段、
T 的前七个数值 word 与初始 M 六元组；不生成完整 T，不计算后续字体调整或 LS 聚合。
模型采用 IEEE RNE 浮点假设，尚未观测 Word FPCR。`--out` 同样原子拒绝覆盖。
公式、数值例与边界见[字体记录参考模型](../../docs/FONT-RECORD-REFERENCE-2026-09-27.md)。

### 字体记录后续调整参考

`font_adjustment.py` 重放 mode 2 的十项横纵缩放、显式选择的 ascent 补偿，以及默认
wrapper 的四个字段覆盖。输入是缩放入口的 F 字段和上下文尺度；不能直接把上一工具
的初始 M 作为这些字段。示例完全是合成整数，不代表任何字体或 Word 文档状态。

```sh
python3 tools/measure/font_adjustment.py \
  --input tools/measure/examples/font-adjustment-synthetic.json \
  --out artifacts/font-adjustment-new.json
```

输出 `ARITHMETIC_REFERENCE`，保留每次乘除前的回绕值、舍入结果、补偿后记录、
复制后的上下文字段和更新后的 M。可选 `h2Source` 会计算字号因子，并要求它与显式
`project.h2` 一致；省略时不推断来源。所有整数和补偿 bool 都严格检查，重复 JSON
键和多余字段被拒绝，已有输出（包括悬空符号链接）不会覆盖。输入与源码哈希随报告保存。
合同与手算样例见[字体调整参考模型](../../docs/FONT-ADJUSTMENT-REFERENCE-2026-09-27.md)。

### Mac 采集回放

已有采集包可重复回放，不启动 Word：

```sh
tools/measure/.venv/bin/python tools/measure/sweep.py \
    --trace-bin target/debug/layout-trace \
    --font-dir /System/Library/Fonts \
    --font-dir fixtures/fonts \
    --output /tmp/rsword-sweep-current
```

`--bundle captures/font-free-2026-09-17` 可重复指定，只跑所选包。默认扫描全部
`captures/*/META.json`，包括会明确记为 `UNDECIDABLE` 的 VOID 包。输出目录必须是新目录，
不覆盖采集。每包保存轨迹、`selfcheck`、原版 `compare` 结果与命令日志，汇总在
`summary.json` / `summary.md`；比较器仍用 0pt 容差、不排除任何约定。

夹具按 META 的采前、采后 SHA 与本仓库 `fixtures` 实际文件绑定，不依赖旧机器绝对路径。
字体从 `--font-dir` 递归读取 TTF/OTF/TTC/OTC 的 name 表，精确匹配所需族名、全名或
PostScript 名，再交给引擎 `--require` 核查。缺字体、缺身份、未执行比较各自列出，
不会算作通过。当前字体文件 SHA 和二进制 SHA 随输出保存；旧 META 未逐文件钉字体字节，
因此相同名字不能证明与采集机的字体版本相同。

对比修改前后时，分别传入两个二进制与两个新输出目录。退出码 0 表示全部通过，
1 表示有失败，2 表示有判不了且无失败；结构相同但非零几何差仍按原比较器记失败。

## 这台机器上的能力边界

### 同行上下标精度实验

`fixtures/vertical-precision.docx` 用三个本机字体、七种字号，在同一行内比较普通、
上标和下标。其逐段 UTF-16 范围、字体输入和判据在采集前固定，见
[`PREREG-2026-09-22-vertical-precision.md`](../../docs/PREREG-2026-09-22-vertical-precision.md)。
离线复算命令：

```sh
tools/measure/.venv/bin/python tools/measure/prereg_vertical_precision.py CAPTURE_DIR \
  --probes fixtures/vertical-precision.probes.json \
  --source-docx fixtures/vertical-precision.docx \
  --font-inputs fixtures/vertical-precision.font-inputs.json \
  --output evaluation.json
```

它要求新采集的完整重复扫描回执、来源哈希和逐字身份核验。输出分别报告绘制字号、
整 run 推进量和基线位移；候选公式不符仍保留为 FAIL。这个专项协议使用固定的
`1e-6 pt` 数值比较阈值，不修改通用引擎比较器的零容差规则。

### 平台范围

采集环境是 **Word for Mac 16.112**。方法 §6.6 说得很清楚：
**Mac Word 与 Windows Word 不能互相替代**（两套字体度量、`usePre2018iOSMacLayout`）。
本目录的一切读数只能写「在 Mac Word 上观测到」。

| 通道 | Windows（方法原文） | 这里（Mac） |
| --- | --- | --- |
| PDF 逐字形几何 | 有 | **有** |
| 行划分 | COM `Rectangles → Lines` | **`first character line number` 扫描**（§2.1 第三通道） |
| **行盒** `box.{left,top,width,height}` | 有 | **没有**——AppleScript 桥缺术语（§6.6） |
| 导出 | `ExportAsFixedFormat`，参数逐位显式 | `save as` + `format PDF`；**无 XPS** |
| PDF `fontSize` | 真实字号 | **逐条恒为 1**，字号在文本矩阵里，不可作字号读数 |

### 没有行盒的代价，以及它是怎么被兜住的

§3.3 的归行办法是**按行盒纵向包含**。Mac 拿不到行盒，所以这里退到
`DERIVED_FROM_LINE_NUMBERS`：用行号扫描把源字符分到行，再按每行预期字形数累加切分 PDF 字形。

**这是推算，不是测量**，前提写在 `pairing.PREMISES` 里，随每次结果一起印出来。
风险是计数模型一错就静默串行；闸门是：切分**前**先核**整页**预期总数与实测总数相等，
不等即判整页不可判，不猜。实测中这道闸门真的拦下过一次错误的计数模型（见下）。

## 落地时做的两处扩展

方法是通用的，落到实现上有两处必须写明的偏离。两处都按 §7 的纪律标注。

### 1. 源侧构造标注（`docxtext.marks`）

§4 的输入是源字符，但**字符本身分不出构造**：分节符与段落标记在 `Range.Text` 里
都只占 1 个字符位，前者画 **0** 个字形、后者画 **1** 个空格。
所以这里从 `document.xml` 逐字符标出构造，交给计数器照着算，而不是让它猜。

推导必须经 `docxtext.verify()` 与采集侧两个独立读数（`endOfContent`、逐段区间）对上才用。
`verify()` 同时**声明它核不了什么**：总长与段边界对终止符身份不敏感（§7.2）。

### 2. `MODE_COUNT_MODEL` 配对模式

§3.2 只给了「全字符」与「非空白字符」两种序列。实测有两者皆非的行——
差额来自**画 0 个字形的源字符**。`pairing.MODE_COUNT_MODEL` 按 §4 的逐字符模型配对。

**这条是看过 `PRECHECK_NEITHER` 读数之后才成形的，所以只靠它配上的行标 `backtest=True`，
不当独立检验**（§7.5）。

## 先跑 `selfcheck`，再跑 `compare`

Word 采集很贵（要真 Word、要授权、要核字体），而**有一类错在轨迹自己身上就能看出来**：
契约说了什么、轨迹又填了什么，两者对不上。这类错必须先清掉——否则拿去和 Word 比，
差值里混着记账错，分不出是布局错还是记账错。

`selfcheck` 查四条，都不碰 Word：

| | |
| --- | --- |
| 字形层覆盖 | 没接整形器时字形序列为空。那是**没覆盖**，不是「量过且为 0」（§7.4） |
| 源区间是全篇偏移 | 段内偏移与全篇偏移长得几乎一样，但对不上 Word 的 `Range` |
| 自报终止符字形数 vs 实画 | 两个数出自同一份记录，对不上就是自相矛盾 |
| 一行一条记录 | 按 run 出的记录被当成行，会让行层配对必然失败 |

## 验收定义里必须一起读的东西

报告不是只有一个 `state`。下面几项**与结论同等重要**，工具会强制把它们印出来：

- **`state` 永远排在 `maxAbs` 前面。** §6.5 的难例：两份不同文档比较，
  3 行结构失败、剩下 1 行 8 个字形距离 0.0000pt。只读 `maxAbs` 会把配对失败读成完全一致。
  这条钉在 `Comparison.to_dict()` 的键序与 `summary()` 里，不只写在文档。
- **三态出口**：成立／不成立／**判不了**。判不了不折叠进不成立，也不当成通过（§7.3）。
- **分母**：N ／ 成立 ／ 判不了，逐条列出。「未核」与「核过无发现」分两栏（§7.4）。
- **前提**：`pairing.PREMISES` 随每次结果输出，包括「PDF 内容流顺序等于源字符顺序」
  这条本量具**未独立验证**的前提。
- **分解**：`compare` 会按「每行第一个字形」把差值拆开——它前面没有推进量可累积，
  所以 Δx 只反映行起点（边距、缩进、对齐），Δy 只反映基线。一个 `maxAbs` 说不出该改哪里，
  这个拆法说得出：实测中它当场排除了「查缩进」整条路。
- **范围排除**：`--exclude-rule` 改的是**验收定义的范围**，不是判据。
  被排除的字形逐条点名给分母。**不要拿它当容差用**——噪声底是 0，放宽容差就是给错找地方藏。

## 证否条件（预注册）

四条写死在 `controls.FALSIFIERS` 里，**先写死再看数据**（§7.1）。任一条触发即量具本身不可信：

| | |
| --- | --- |
| **F1** | 同条件两次采集出现任何非零 `glyphOrigin` 差 |
| **F2** | 两份不同夹具比出 `state=OK` |
| **F3** | 某行判 OK，但配对后的字符身份核对不符 |
| **F4** | 存在判不了的页或行，而顶层 `state` 却是 OK |

F4 在开发中真的触发过一次：计数模型说判不了的行，被 §3.2 前置检查的**巧合相等**
盖过去判成了 OK。已修（`pairing.pair_line`），并留了回归测试。

## 引擎侧契约

`rsword-layout-trace/1`。形状由 [`crates/core/src/oracle.rs`](../../crates/core/src/oracle.rs) 定，
序列化由 [`crates/core/src/oracle_json.rs`](../../crates/core/src/oracle_json.rs) 做——
分开是有意的：契约的形状归 `oracle`，输出格式归 `oracle_json`，改一边不必动另一边。

每页、每行、每字形给：原点与推进、所属源字符区间、行终止符类型（§9.7）。

两个口径必须和 Word 侧对齐，对不齐就没法比：

- **坐标**：点，页内，**页顶向下**，原点在页左上角；
- **偏移空间**：**UTF-16 单位**，与 rsword 的坐标流及 Word 的 `Range.Start/End` 一致，
  每个段落标记占 1 个单位。

字形原点由度量实现给，用的是哪一种**随数一起走**（输出的 `glyphOriginMethod` 字段）：
桩度量按前缀推进量算（对前缀可加的度量准确），真度量直接用 shaper 的输出——
连字与 kerning 会让前缀和**不等于**逐字形推进，所以这两者不能混着读。

## 离线补全旧采集包的源标注

旧采集包缺少 `sweep.marks` 时，可以显式指定原始 DOCX：

```sh
./.venv/bin/python -m wordmeasure.cli model ../../captures/vmisc2-2026-09-17 \
  --source-docx ../../fixtures/vmisc2.docx --output /tmp/vmisc2-model.json
```

`compare` 同样接受 `--source-docx`；离线 `sweep.py` 自动传入按 SHA256 绑定的夹具。
补注必须同时通过采集前后文件哈希、`unchanged`、UTF-16 总长、逐段区间和完整正文核查。
只允许已由源标注识别的段落 CR 对应 Mac 传输的 LF。已有标注必须与源一致，
缺项才补充；失败返回 `UNDECIDABLE`，不继续猜控制符身份。原采集包不会被改写。

报告中的 `sourceAnnotation` 记录输入哈希、全部核查和 `derived/backtest=true`。
这是离线回测，不是新 Word 观测。Word 偏移始终按 UTF-16 单位解释；Python 的字符
下标只供计数层使用。缺失或重复扫描位置、越界区间和截断代理对都会判不了。

分页符自身独占一条行记录时画 0 个字形；与段落标记同处一行时，两个控制符
合计 2 个。识别依赖源标注，不把普通 LF 猜成段落标记。纯控制行的字体、定位
和两空格的源归属仍受内容流读序前提限制，计数相等不能证明这些几何属性正确。

## 跑测试

```sh
cd tools/measure && PYTHONPATH=. ./.venv/bin/python -m pytest tests -q
```

测试里的断言大多是**实测逼出来的**，注释写了是哪条读数逼出来的。改它们之前先读那条注释。
