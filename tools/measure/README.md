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

## 离线全量回放

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
