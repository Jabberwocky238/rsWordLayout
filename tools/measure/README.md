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
./wm compare bundle/ trace.json         # §9.6 引擎 vs Word
```

### 引擎侧还差一个出口

`compare` 要吃一份 `rsword-layout-trace/1` 的 JSON（契约见下）。
引擎里的 `crates/core/src/oracle.rs` 已经有「页 → 行 → 字形」三层记录了，但它目前
**只是进程内的结构，没有序列化出口**，所以还接不上量具。

补法：给 `oracle::LayoutRecord` 加一个按下面契约输出的序列化，再加一个小 bin 把它写出来。
那之后整条链就通了。

采集那一侧要落一条纪律：**申请了哪些字体族，就必须当场核过**。
度量兼容克隆的字体替换**在几何上完全不可见，只有字体名能发现**（§6.2）——
不核就跑，跑出来的是废数据，而且废得看不出来。

## 这台机器上的能力边界

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

`rsword-layout-trace/1`，由 [`crates/core/src/trace.rs`](../../crates/core/src/trace.rs) 出具。
每页、每行、每字形给：原点与推进、所属源字符区间、行终止符类型（§9.7）。

两个口径必须和 Word 侧对齐，对不齐就没法比：

- **坐标**：点，页内，**页顶向下**，原点在页左上角；
- **偏移空间**：与 Word `Range` 相同——各段文本依次拼接，**每段末尾算一个段落标记**。

字形原点由度量实现给，用的是哪一种**随数一起走**（输出的 `glyphOriginMethod` 字段）：
桩度量按前缀推进量算（对前缀可加的度量准确），真度量直接用 shaper 的输出——
连字与 kerning 会让前缀和**不等于**逐字形推进，所以这两者不能混着读。

## 跑测试

```sh
cd tools/measure && PYTHONPATH=. ./.venv/bin/python -m pytest tests -q
```

测试里的断言大多是**实测逼出来的**，注释写了是哪条读数逼出来的。改它们之前先读那条注释。
