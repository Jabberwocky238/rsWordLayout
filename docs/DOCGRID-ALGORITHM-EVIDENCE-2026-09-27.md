# docGrid 算法的本地静态证据复核

日期：2026-09-27。先只读 `../word_analyse` 的本地报告、Android 原生库和保存的
Web 脚本，末节补记本机 macOS 框架的窄查。没有启动 Word、连接设备、请求网络或
采集新布局读数。

**尚未完成 Word docGrid 的输入到输出映射。** 文末的宿主续查已找到整数取整和
分量分配路径，并发现另一个受开关控制的计算入口；这些仍不能直接等同完整网格算法。
可以确认字体度量有条件覆盖路径，
Web 有普通行距的转换和行高/基线消费路径；不能从这些代码推出自然高度如何取
`linePitch` 的倍数、网格基线偏移或网格与 exact/atLeast 的完整关系。
另有一处可以明确纠正的旧解释：`LineGapMutator` 附近的三个位掩码是同一个带有效标志
的查表结果，并非 ascent、descent、line gap 三个度量相加。

输入事实与旧日志可信度分别沿用
[DOCGRID-INPUTS](DOCGRID-INPUTS-2026-09-27.md) 和
[ANDROID-PAGE-EVIDENCE](ANDROID-PAGE-EVIDENCE-2026-09-27.md)。本次不重复拟合
298、319、350，不把旧报告标注的 `verified` 自动当作经过当前审计的新证据。

## 材料身份与覆盖范围

| 材料 | 本次身份与用途 |
| --- | --- |
| `../word_analyse/native/arm64/libwlibandroid.so` | SHA-256 `15f2d4559fd0dc646d0b767f625772b6c10f2da651618ad3d30e6733f0a3a9ce`；以下原生地址均是该 ELF 的虚拟地址，不是文件偏移 |
| `tools/cdp/out/pretty/39__wordeditordsclosurebundle.js`，相对于 `word_analyse` | SHA-256 `587798169cfff8dbdf944a301eee06a68b3efbfdad3f86d8dd94ebc69a64bdbb`；以下 Web 行号均指此保存的格式化版本 |
| Android 版本记录 | [android-word-layout.md:3](../../word_analyse/findings/android-word-layout.md:3) 记录 Word `16.0.20513.20014`；本次没有重新验证设备安装状态 |
| parser 输入边界 | 钉住 `399e36a3c645e9b9001531fb06969f8ce5347e1f`；本次不改变节 `DocumentGrid` 或段落 `snap_to_grid` 的投影合同 |

检索了 `findings/`、`reports/decomp-index/`、相关符号/源码路径清单，以及
`tools/cdp/out/pretty/` 的 10 份和 `word-scripts/` 的 55 份保存脚本。
`docGrid`、`linePitch`、`snapToGrid`、`DyaLinePitch`、`gridHeight`、`baselineGrid`
在这两组 JS 中没有字面命中。编译器会缩短字段名，因此这个负结果不能证明 Web
没有实现网格。保存目录中也没有 `.wasm` 或 `.map` 文件。

原生索引本身有覆盖限制：
[decomp-index/README.txt:1](../../word_analyse/reports/decomp-index/README.txt:1)
说明直接调用闭包不包含全部间接调用；其字符串索引只捕捉部分 `ADRP+ADD` 字符串引用，
不包含仅经指针读取的字符串。因此“没找到 sprm 引用”不等于实现不存在。

## 原生字体度量路径：查表覆盖，不是三段求和

[auto-lineheight-code.md:41](../../word_analyse/findings/rules/auto-lineheight-code.md:41)
曾根据 `0xb44cbc` 的 `0x10000 / 0xff00 / 0xff` 三个掩码，猜测返回值被拆成
ascent 标志、descent、line gap，并据此提出 auto 高度合成解释。
本次用 `xcrun llvm-objdump --disassemble` 直接展开了此前未读的 `0xb4476c`。

函数入口存在于
[eh-frame-starts.txt:62010](../../word_analyse/reports/decomp-index/eh-frame-starts.txt:62010)；
`LineGapMutator` 的槽 2 指向 `0xb45168`，见
[vtable-edges.tsv:90](../../word_analyse/reports/decomp-index/vtable-edges.tsv:90)。
调用边 `0xb45168 -> 0xb44cbc -> 0xb4476c` 与
[callgraph.tsv:32919](../../word_analyse/reports/decomp-index/callgraph.tsv:32919)
一致。

直接指令支持以下结论：

| 位置 | 观察到的操作 | 可以下的结论 |
| --- | --- | --- |
| `0xb4476c` 至 `0xb447e4` | 从输入读取表指针/数量，步长 `0x28`；建立中间项地址 | 查询固定 40 字节表项 |
| `0xb447e8` 至 `0xb4493c` | 对两组长度和 16 位字符逐项比较，按结果收缩搜索范围，最后再检查相等 | 键由两组 UTF-16 序列构成；不是对字体高度做乘除或取整 |
| `0xb44940` | `ldrh w8, [x8, #0x20]` | 命中后读取表项中的一个 16 位值 |
| `0xb44944` 至 `0xb44950` | 拆出该值的高/低字节，并设置 `0x10000` | bit 16 是命中/有效标志；低 16 位是查得的值 |
| `0xb44954` 至 `0xb44978` | 失配时置零；返回前用 `orr` 合并 | 返回值是标志与负载的打包，没有把三个量相加 |
| `0xb44d20` 至 `0xb44d68` | 再拆同样三段，跨清理调用保存，随后 `orr` 合回 | 这只是保留返回值，不是新的行高公式 |
| `0xb4518c` 至 `0xb45194` | 比较返回值是否至少 `0x10000`，成立时 `strh` 写 `x3+6` | 有效时覆盖目标的 16 位字段；不是“实际行高小于 65536 就不修改” |

可用不绑定未知类型的伪代码概括命中部分：

```text
lookup(two_utf16_keys):
    matched = search(entries, two_utf16_keys)
    if not matched:
        return 0
    return 0x10000 | load_u16(matched + 0x20)

mutator(..., target):
    tagged = conditional_lookup(...)
    if tagged >= 0x10000:
        store_u16(target + 6, tagged & 0xffff)
```

这否定了旧报告在这段指令上建立的“三段求和”解释，**不否定**行高的其他真实路径
可能组合 ascent、descent、gap。两组键的完整业务身份、表的构造来源、16 位负载的
单位和符号解释、目标结构的准确声明都没有在这里恢复，不能直接将负载作为 twips。
类名支持“字体 line gap 覆盖”这一方向，单凭类名仍不能把 `target+6` 标为最终行高。

此覆盖还受条件控制：`0xb45174` 读取 `target+0x2c` 并转成布尔参数；
`0xb44cdc` 在该参数置位时返回无有效负载，否则还经过 `0xb44b40` 的检查与一个
虚调用获取两组键。`0xb44b40` 在 `0xb44b6c` 处引用 `0x17461e` 的 UTF-16
字符串 `O365`。目前只确认了字符串比较路径，未把它解释成特定字体、许可或兼容规则。
这些分支没有读取可辨认的 `docGrid`、pitch、段落 exact/atLeast 输入。

复核命令，在本仓库根目录执行即可；不用加载该库：

```sh
xcrun llvm-objdump --disassemble --start-address=0xb4476c --stop-address=0xb44980 ../word_analyse/native/arm64/libwlibandroid.so
xcrun llvm-objdump --disassemble --start-address=0xb44cbc --stop-address=0xb44d84 ../word_analyse/native/arm64/libwlibandroid.so
xcrun llvm-objdump --disassemble --start-address=0xb45168 --stop-address=0xb451a4 ../word_analyse/native/arm64/libwlibandroid.so
```

## PTS baseline 名称尚未通向网格规则

现有卡片中的 `0x20aec00` 由错误路径字符串关联到
`ptls7/pts/src/fsbaselineinfoapi.cpp`，见
[0x20aec00.md:47](../../word_analyse/findings/function-cards/chain-c/0x20aec00.md:47)。
本次也直接反汇编了 `0x20aec00..0x20aecac`，确认它只校验两个 `i32` 字段的范围，
各有一个跳过标志，成功返回 0，失败返回 -100。两个字段的业务含义仍未恢复，
函数中没有 pitch 的除法、取整或基线分配。

因此 `fsbaselineinfoapi` 只能作为后续查找调用者的入口，不能作为
“基线对齐到节网格”的实现证据。其他以 `baseline` 为检索词找到的许多函数卡片，
其中该词只是“基准夹具”的意思；显示数学对象的基线也不能替代普通正文的网格基线。

## 保存的 Web 脚本可以确认什么

### 普通 auto、exact、atLeast 的分流

保存脚本保留了规则名称：
[39__wordeditordsclosurebundle.js:21614](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:21614)
将 `B6=1` 标为 Exact，`B6=2` 标为 At Least，`B6=0` 标为 Single/Multiple。
[R$b.tPa:118176](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:118176)
把 auto 转为倍数单位 `unit=2`，exact/atLeast 转为 `unit=3`；这里的本地回退值
也不能作为 DOCX 缺省或 Android/桌面全局默认值。

HTML 路径
[ihb:33903](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:33903)
按字体和字号在 DOM 测量垫上取 `Math.round(bounds.height)`，缓存为 `n`。
随后在 [33928](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:33928)：

```text
unit == 2: m = n * value
unit == 3: m = g.oo(value, 1, false)
rule == 2: m = max(m, n)
CSS lineHeight = m + "px"
```

这是可读的普通行距规则：倍数、指定高度、指定高度下限。该片段没有节网格输入。
它的 `n` 是浏览器测量结果，不是已证实的原生字体表自然高度，不能迁移其整数像素
舍入来解释 Android docGrid 阈值。

Web LineServices 的输入提供者
[mkc.getData:167675](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:167675)
另外明确传递了规则和数值：

| 输入 | 发给 LineServices 的 `rke` | `xqb` |
| --- | --- | --- |
| 倍数单位 | 0 | `value * 240` |
| 指定高度、At Least | 2 | `value * 96 / 1440 * 294912 / 96` |
| 指定高度、其他规则分支 | 1 | 同上 |
| 整个 spacing 缺失 | 0 | 240 |

这是某个保存版本的桥接约定。未在此提供者中识别出 docGrid 投影，也没有足够证据
把其他缩写字段重新命名成网格属性。不能由普通 exact 分支推导所有网格类型均被忽略，
也不能由 `max(m,n)` 推导 atLeast 在网格取整之前还是之后应用。

### 基线与行高的消费，不是网格计算

[dkc.formatLine:167407](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:167407)
调用 `this.xN.formatLine`，将结果的 `dvrAscent`、`dvrDescent` 乘以 `96/294912`
供 Web 使用。消费者
[formatLine:147037](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:147037)
直接执行：

```text
baselineOffset = dvrAscent
lineHeight = dvrAscent + dvrDescent
origin.v = current_v
current_v += dvrAscent + dvrDescent
```

`HK` 确实是 baseline offset，而非本次猜测：保存的布局导出器在
[61622](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:61622)
把 `HK` 命名为 `baselineOffset`，同时单独导出 `lineHeight` 和 origin。
这支持以后采集每行基线、原点和行高，避免只由页容量反推步长。但此处消费的是已算好
的行度量，未揭示这些度量内部是否、何时经过网格处理。

该运算主体在本地材料中缺失：
[167286](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:167286)
构造 `LineLayout-Core.wasm` 路径，
[167307](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:167307)
从 worker 模块或 fetch 得到它。当前保存目录没有该 WASM。本次没有下载。
原生算法单位、Web WASM 单位和本仓库 `fine=1/7200 inch` 也不可混为一个量化单位。

另一个
[ILineFormatter:157970](../../word_analyse/tools/cdp/out/pretty/39__wordeditordsclosurebundle.js:157970)
走 DOM Range 断行/矩形测量，行高来自 `getBoundingClientRect()` 或 `getClientRects()`。
它同样没有暴露网格公式，也不能因名字叫 formatter 就视为 Android PTS 的同一条路径。

## 当前可以用于开发的边界

| 问题 | 本次结论 |
| --- | --- |
| 自然字体高度到 pitch 倍数 | 未定位到消费 pitch 的原生或保存 JS 函数；没有公式可移植 |
| 网格基线偏移及首行相位 | Web 能输出 baseline offset；生成该值的网格规则未找到 |
| 最后一行 required extent 与行间 advance | 本仓库分离这两个量仍有必要，但本次不能给出网格情况下的数值关系 |
| exact 与 docGrid | 普通 Web exact 路径可读；旧 Android 报告的样本级观察不能扩大为所有类型/段落的完整例外规则 |
| atLeast 与 docGrid | 普通 Web 下限处理可读；与网格的处理顺序尚无证据 |
| snap 缺省、run snap、网格跨节继承 | 沿用输入文档中的未决边界，本次静态搜索没有补足 |

旧报告
[docgrid-decided.md:34](../../word_analyse/reports/diff/docgrid-decided.md:34)
提到第一版 exact 480 夹具未表现出预期网格影响，并记录了 CP 行首序列。
CP 序列本身不包含基线或行高，不能单独证明高度被哪条公式固定。
其页数结论仍受本仓库 Android 证据审计约束，不能因为此次发现普通行距分支便升级。
尤其是旧报告
[d6-docgrid-analysis.md:214](../../word_analyse/findings/rules/d6-docgrid-analysis.md:214)
自己也说明没有读取 docGrid 算式，拟合结果不构成静态实现证据。

下一步有依据的调查入口是：确认 `0xb4476c` 的表构造及两组键，确认字体覆盖后的
真实度量进入哪个测行回调；再沿节网格输入的消费点找取整和基线分配，或取得能同时
绑定输入、字体、模式及完成状态的逐行原点/基线读数。仅记录 `0xb45168` 的命中或
最终页数，都不能补齐该链路。本次没有据此修改现有自然高度、网格默认值或布局算法。

## 补查：本机 macOS PTLS7 导出与网格接口

同日对本机已安装框架做了一次窄查，没有启动或控制 Word。
`Info.plist` 当前仍为 `16.112.3` / `16.112.26083020`，与
[macos-word-layout.md:3](../../word_analyse/findings/macos-word-layout.md:3) 相同。
材料是
[MicrosoftPTLS7](</Applications/Microsoft Word.app/Contents/Frameworks/MicrosoftPTLS7.framework/Versions/A/MicrosoftPTLS7>)，
完整 universal 文件 SHA-256 为
`cf7699ab47748bdf16ad1a5e8ba06803d99f7e4c4533bd57974ba9bbf2c89492`。
以下地址来自其中 **arm64** 的导出符号表，与前面的 Android 地址无对应偏移关系。

`nm -arch arm64 -gU -C` 确实提供了比 Android 已剥离符号更直接的接口名和参数类型：

| arm64 地址 | 导出名及参数类型 |
| --- | --- |
| `0x16b094` | `PTLS7::FsSnapGridVertical(PTLS7::fscontext*, unsigned int, int, int, int*)` |
| `0x18ad24` | `PTLS7::FscbkSnapGridVertical(PTLS7::_fstext*, unsigned int, int, int, int*)` |
| `0x113948` | `PTLS7::ApplySnapGridReal(PTLS7::lschnke*, int, int)` |
| `0x30b64` | `PTLS7::LsModifyLineHeight(PTLS7::lscontext*, PTLS7::CLsLine*, int, int, int, int)` |
| `0x8d9d4` | `PTLS7::LsModifyDisplayLineHeight(PTLS7::lscontext*, PTLS7::CLsDisplayLine*, int, int, int, int)` |
| `0x3beac` | `PTLS7::FsValidateBlinfo(PTLS7::fsbaselineinfo*)` |
| `0x3c1f4` | `PTLS7::FsShiftBlinfo(PTLS7::fsbaselineinfo*, int)` |
| `0x3c208` | `PTLS7::FsCombineBlinfo(PTLS7::fsbaselineinfo*, PTLS7::fsbaselineinfo*)` |

另有 `FsFormatLineChainW`（`0x192398`）的长签名带
`PTLS7::tagLineHeightsWord*`，`CLsSpanNode::GetBaselineOffset`（`0xe4b64`）带两个
`int*`。这些是真实符号与类型名；C++ 符号不保留参数变量名，不能把某个 `int`
直接标注成 `linePitch`，也没有由这些签名取得结构体字段布局。

**两个名字最直接的垂直网格函数在此版本中并未暴露算法。** 本次只展开了这两个短函数：

```text
FsSnapGridVertical, 0x16b094..0x16b0b8:
    ReportErrorTag(context, 0, 0x1e022723)
    return -10000

FscbkSnapGridVertical, 0x18ad24..0x18ad70:
    context = load_pointer(fstext + 8)
    result = FsSnapGridVertical(context, arg1, arg2, arg3, output)
    if result != 0:
        return result
    validate_range(*output)
```

第一函数在 `0x16b09c` 覆写 `w1=0`，在 `0x16b0a0`/`0x16b0a4` 装入错误 tag，
`0x16b0a8` 调 `ReportErrorTag`，`0x16b0ac` 装入 `-0x2710` 并返回。
没有消费其他网格参数，也没有写输出指针。第二函数在 `0x18ad38` 调它，
`0x18ad3c` 检查返回值，非零直接退出；输出范围检查只位于成功分支。
这里不为 `-10000` 补造未取得的枚举名。

所以此导出是一个可复核的错误返回桩，不能作为 pitch 取整公式或 Word 正文实际
调用路径的证据。它也不能证明 Word 不支持 docGrid：客户端或其他路径仍可能处理。
Word 主程序的 `nm -arch arm64 -u -C` 筛选结果包含两个 `LsModify*LineHeight`
导入，没有包含上述两个 `SnapGridVertical` 导入。这只限定直接导入面，不排除
框架内部调用或间接调用。此前的导出窄查只记录了 `ApplySnapGridReal` 候选符号，
没有展开其长体，也没有根据名字将它归为正文纵向网格；其实现及两个行高接口的
[后续指令补查](#macos-grid-line-height-followup) 见下节。

本机两个 PTLS 框架目录没有 `Headers/` 或 `.h/.hpp`。对 Word 应用包、当前本地代码
目录、Homebrew include 与 CommandLineTools SDK 的相关头文件名窄查，未找到 Microsoft
PTS/LS 头文件；匹配到的 Apple `LaunchServices/LSInfo.h` 属于应用启动服务，与此无关。
这不是对整台机器所有路径的不存在证明。

可复核的窄命令如下；Mach-O 使用 `--dis-symname` 限定函数，避免将全框架反汇编误当作
指定地址区间的输出：

```sh
nm -arch arm64 -gU -C '/Applications/Microsoft Word.app/Contents/Frameworks/MicrosoftPTLS7.framework/Versions/A/MicrosoftPTLS7' | rg 'SnapGrid|ApplySnapGridReal|ModifyLineHeight|Blinfo'
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS718FsSnapGridVerticalEPNS_9fscontextEjiiPi' '/Applications/Microsoft Word.app/Contents/Frameworks/MicrosoftPTLS7.framework/Versions/A/MicrosoftPTLS7'
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS721FscbkSnapGridVerticalEPNS_7_fstextEjiiPi' '/Applications/Microsoft Word.app/Contents/Frameworks/MicrosoftPTLS7.framework/Versions/A/MicrosoftPTLS7'
```

该窄查增加了可定位接口，也排除了直接照搬 `FsSnapGridVertical` 的路线，仍没有补齐
字体自然高度到 pitch 倍数、基线相位或 exact/atLeast 与网格组合的规则。

<a id="macos-grid-line-height-followup"></a>

## 补查：字符 U 轴网格与行高写入边界

继续只读分析上节同一个 SHA-256 的 macOS 框架 arm64 指令，没有启动或控制 Word。
以下函数范围结合 Mach-O `LC_FUNCTION_STARTS` 表确认；导出符号之间可能包含匿名函数，
不能把 `--dis-symname` 输出中相邻的匿名辅助函数全部算作该导出函数本体。

### ApplySnapGridReal 处理字符及 glyph cluster

真实签名仍为 `PTLS7::ApplySnapGridReal(PTLS7::lschnke*, int, int)`，本体范围为
`0x113948..0x1140d4`，后者是下一个匿名函数的起点。两个整数用于 chunk 索引范围，
按 32 字节步长寻址条目；没有证据把其中任何一个标成 pitch。

| 指令位置 | 可以直接核对的行为 |
| --- | --- |
| `0x113984` 起 | 读取 chunk 条目关联的文本对象，整理 UTF16、字符位置、逐字符关联对象和标记数组 |
| `0x113b88` | 调用 `LsdnExternalNameNextChar`，生成字符位置 |
| `0x113d08` | 调用 `PTLS7::LsdnGetUrPenAtBeginningOfChunk(PTLS7::CLsDnode*, unsigned int*, int*, int*, int*)` |
| `0x113d38..0x113d50` | 从函数表 `+0x258` 取地址间接调用，传入文本相关数组、字符数和输出指针；回调业务名未知 |
| `0x113dc0..0x113e10` | 对前述输出进行有符号整数除法、余数和位置量化运算 |
| `0x113fa0` | 调用 `LsFIwchFirstInCluster`，检查 cluster 边界 |
| `0x114224..0x1142bc` | 匿名辅助函数定位首尾 glyph，把调整量拆分后分别传给 `LsApplyChanges` 的 `SIDE(1)`、`SIDE(2)` |

最后一个辅助函数的关键指令为：

```asm
114258: asr w20, w5, #1
11425c: sub w21, w5, w20
11426c: bl  LsIgindBaseFirstFromIwch
114278: mov w3, #1
11427c: mov w4, w21
114280: bl  LsApplyChanges
114294: bl  LsIgindBaseLastFromIwch
1142a0: mov w3, #2
1142a4: mov w4, w20
1142b8: b   LsApplyChanges
```

这里的调用名为便于阅读省略了 mangling；真实被调接口包括
`PTLS7::LsApplyChanges(int*, int*, int, PTLS7::SIDE, int)`，地址 `0x10d870`。
其指令按 glyph 索引修改两组整数数组：`SIDE(1)` 给第一组对应项加调整量，
`SIDE(2)` 给第二组对应项加调整量。没有取得这两组数组的字段名，不能擅自将其
命名为 advance、bearing 或其他具体度量。

这些字符、cluster、U 坐标和逐 glyph 写入共同支持将此路径判为沿文本 **U 轴** 的
字符网格调整。在普通横排中它沿水平方向；此处不把 U 轴无条件等同于页面水平轴。
已读到的整数运算不能移作自然行高到垂直 `linePitch` 的公式，运算数与 XML
`charSpace`、`linePitch` 的对应也尚未确认。

框架内直接调用点为 `0x29cd8`。函数起点表将其归入 `0x29820` 开始、
`0x29dac` 结束的匿名函数；它不是前面最近的导出符号
`LsCopyGmapWithGivenIgind`。该调用受对象 `+0x24` 字节等于 `1` 的分支控制，
没有字段名证据将这个字节直接解释为某种 OOXML 网格类型。

### 两个行高接口保存四个显式整数

真实导出签名及本体范围：

```cpp
// 0x30b64..0x30c24
PTLS7::LsModifyLineHeight(
    PTLS7::lscontext*, PTLS7::CLsLine*, int, int, int, int);

// 0x8d9d4..0x8da48
PTLS7::LsModifyDisplayLineHeight(
    PTLS7::lscontext*, PTLS7::CLsDisplayLine*, int, int, int, int);

// 0x90dc4..0x90dd0
PTLS7::CLsDisplayMainSubline::ModifyLineHeight(int, int, int, int);
```

以上只列符号实际提供的名称和参数类型，不补造返回类型或参数业务名。
按传入顺序暂称四个整数为 `a`、`b`、`c`、`d`，指令确认其写入位置如下：

| 参数 | 格式化 subline 偏移 | Display main-subline 偏移 |
| --- | --- | --- |
| `a` | `+0x15c` | `+0x84` |
| `b` | `+0x190` | `+0x78` |
| `c` | `+0x194` | `+0x7c` |
| `d` | `+0x160` | `+0x80` |

`LsModifyLineHeight` 校验 context、line 及其所属关系后保存原值；`b` 或 `c`
变化时还把 subline `+0x1b4` 的 `0x200` 位置位。其函数体没有网格取整、
行高倍率计算或把某个输入压成布尔值的操作。Display 包装器在
`0x8da28..0x8da38` 将四个输入原样转交给成员函数，成员函数本体仅为：

```asm
90dc4: stp w4, w1, [x0, #0x80]
90dc8: stp w2, w3, [x0, #0x78]
90dcc: ret
```

`PTLS7::LsGetObjDimSublineCore(PTLS7::CLsSubline const*, PTLS7::heights*,
PTLS7::heights*, int*)` 的一条读取分支，在 `0x2eba0..0x2ebb4` 将
`+0x190/+0x194/+0x198` 复制到一个 `heights` 输出。因此 `b/c` 确实对应该结构的
前两个高度分量。现有本地符号和头文件材料没有给出这两个分量的字段名及顺序，
也没有给出 `a/d` 的业务名；不能据此把它们填成 ascent、descent 或 multiline。
四个参数均为 `int` 本身既不能证明、也不能排除某个参数承载了宿主定义的状态。

本次对框架内直接分支指令的筛选没有发现两个公开 `LsModify*LineHeight` 的内部
直接调用；主程序确实导入它们的证据仍仅限定导入面。尚未取得主程序调用时四个
整数的来源，更没有验证哪个调用与 docGrid、exact 或 atLeast 对应。

### 复现与开发边界

```sh
ptls='/Applications/Microsoft Word.app/Contents/Frameworks/MicrosoftPTLS7.framework/Versions/A/MicrosoftPTLS7'

nm -arch arm64 -gU -C "$ptls" | rg 'ApplySnapGridReal|ModifyLineHeight|ModifyDisplayLineHeight|LsGetObjDimSublineCore'
xcrun llvm-objdump --macho --arch=arm64 --function-starts=both "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS717ApplySnapGridRealEPNS_7lschnkeEii' "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS714LsApplyChangesEPiS0_iNS_4SIDEEi' "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS718LsModifyLineHeightEPNS_9lscontextEPNS_7CLsLineEiiii' "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS725LsModifyDisplayLineHeightEPNS_9lscontextEPNS_14CLsDisplayLineEiiii' "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS721CLsDisplayMainSubline16ModifyLineHeightEiiii' "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble --dis-symname '__ZN5PTLS722LsGetObjDimSublineCoreEPKNS_10CLsSublineEPNS_7heightsES4_Pi' "$ptls"
xcrun llvm-objdump --macho --arch=arm64 --disassemble "$ptls" | rg '[[:space:]]b(l)?[[:space:]].*(__ZN5PTLS717ApplySnapGridReal|__ZN5PTLS718LsModifyLineHeight|__ZN5PTLS725LsModifyDisplayLineHeight)'
```

本轮确认了字符 U 轴调整与四整数行高写入的边界，可以继续保留自然度量和宿主最终
行度量之间的分离。它没有补齐垂直 docGrid 的公式、基线相位或 exact/atLeast 例外，
不能据此新增网格常数或把未命名分量固化进引擎 API。

## 后续宿主追踪：聚合输入与受开关控制的替代路径

以下续查在原有材料冻结后完成，均为本机 Word 16.112.3 / 16.112.26083020 的
离线 ARM64 静态分析，没有运行原生函数、附加进程或更改已有采集。

`artifacts/docgrid-component-producers-2026-09-27/` 冻结 26 个文件，清单 SHA-256
为 `abfe0ad1a7d5976ad1890a49c7e646f7d7bb0557a8a3cbab8467960a1cd4e9d4`。
较早宿主主函数 `0x1003789a8` 中的 `0x100379940` 调用并不是一个标量行高生产器；
它按记录中的两个范围字段更新上下文。用于取整的通常输入来自调用前已保存的
`R+54 + max(R+4c,R+64)`，另有条件替换路径。没有足够证据把这三个字段直接
命名为字体表 ascent、descent 或 lineGap。

网格状分支先将周期用截断乘除换算到当前整数域，再做向上取整和下限比较。正余量
被分成 floor(delta/2) 与剩余部分，写入不同分量；尤其 **Fc8 也会增加**。
若没有后续调整，条件式总量为 `chosen + initialFc8 + max(other-chosen,0)`，
不能无条件简写成 chosen。该分支的中性模型通过 7648 个合成输入、54/54 条保存
指令的解释器核对，以及 512 个截断乘除边界检查。首次发现的模型溢出分支错误与
修正日志一并保存，不计为原生执行或 Word 实测验证。

`artifacts/docgrid-caller-mapping-2026-09-27/` 的冻结清单 SHA-256 为
`cfb325c4af4d335dcb9fe14e508d13ec2a0eba87b1b2ef535167526c456f248f`。
对直接调用者的检查发现 `0x1003786e4` 会根据条件和 FeatureGate 选择另一条路径：
`0x103668744 -> 0x100a65e24`。诊断分支可以先在记录副本上运行较早函数，再比较
两路的六个字段。这证明存在两条计算路径，不能证明本次探针实际走了哪条路径。

替代路径把规则计算拆成独立函数，其中网格状 helper 为
`0x100a656bc..0x100a65900`。它接收 24 字节分量元组、周期、参数和尺度结构。
三个完整 getter 已确认参数来自 parent+b0、parent+d0 和 parent+140；规则参数是
parent+e0，和此前保存的三 halfword 转换/写入形成明确的表示接缝。公开 OOXML
属性名称与实际对象实例仍未由这一片证明，不能仅靠相同偏移认定全部输入语义。

正常适配分支将结果 O 写回：`Fd0=O[0]`、`Fc8=O[8]`、`Fc0=O[4]+O[c]`、
`Fd8=O[c]`、`Fe0=O[10]`、`Fb8=O[0]+O[8]+O[c]`，这里下标都是字节偏移。
另一个标志分支会交换两个分量，之后仍有清零、尺度转换、余数协调及可选 hook。
因此可以继续分开开发行内分量计算与最终绘制/分页消费，但不能把任何一个中间字段
直接当作 PDF 基线或 required extent。

新的 [页面容量探针](DOCGRID-PAGE-FIT-2026-09-27.md) 正用于约束末行是否能容纳。
本片没有把一个拟合的步长同时当作首行偏移、逐行推进和分页占高，也没有改变生产网格策略。

## 独立网格 helper 的整数算法已闭合

`artifacts/docgrid-pure-helper-2026-09-27/` 冻结 19 个文件，清单 SHA-256 为
`5cc1909764ca4d1965c8569865aefd1f9d29809dfb5b9302fecf76bf54c1930d`。
580 字节 helper 的两个必要算术 leaf 已闭合。正常非负且不溢出的输入可写成：

```text
H, A, B, C, D = input tuple words at offsets 0, 4, 8, c, 10
S = unsigned scale word at scaleObject+8
p = trunc(period*S/1440), saturated to signed32
other = trunc(p*(tag==2 ? parameter[4] : 240)/240)
multiple = p>=1 ? ceil(H/p)*p : H
chosen = max(other,multiple)
lo = max(chosen-H,0)/2 rounded down
hi = max(chosen-H,0)-lo
outer = tag==2 ? H : max(nearest(parameter[2]*S/1440),H)
output = [H, A, B+hi+max(outer-chosen,0), C+lo, D+lo]
```

实际代码还包含零周期/零尺度、参数转换旁路、饱和、32 位溢出、诊断和 C 的夹限；
完整中性模型保存这些分支，不能以本段简式替代它。原生先截断转换周期，参数中的
另一个 u16 值却用最近整数转换，两者不可互换。helper 不改元组的 0 和 4 字段。

20045 项倒数乘法检查与普通整数截断相符；10000 组正数域输入在新旧分支各自的
正常消费者投影后，六字段全部相同。这个跨路径检查条件为 tags 0/2、H>=A>=0、
初始 C/D 为零且没有后处理；它不是新函数完整 ARM 解释器、原生运行或 PDF 对齐。

可复用的算法部分现在是“整数域转换、ceil、分量余量分配”。接入真实引擎仍须绑定
尺度 S、H/A 的字体来源以及分页和绘制消费者；不能直接把 H 替换为当前
`natural_height_fine`，或把任意 twip pitch 的 ceil 当作等价实现。为检验这个临界点，
又准备了 274 至 277 twips、270 锚点及无网格的 6 份大页输入，manifest SHA-256 为
`63aaf75f8c4b37a9b423881857a0be359f5f8fee885dfa91b460c2daa5170d3b`，目录
`artifacts/docgrid-pitch-threshold-source-2026-09-27/`。这些是根据既有证据选择的新输入，
不称为盲测，不预置 Word 的页数或步长结果。

后续 [原生数据流映射](DOCGRID-NATIVE-DATAFLOW-2026-09-27.md) 已把两条路径的周期和
尺度接到共同生产者，并闭合普通同尺度分支到 LS 四参数的存储关系。运行模式、
字体分量及最终分页/绘制消费仍按各自证据边界保留，不由 helper 的简式代替。
[字体回调与聚合](DOCGRID-FONT-COMPONENTS-2026-09-27.md) 和
[PTS 行高返回](DOCGRID-PTS-HEIGHTS-2026-09-27.md) 已补齐更多静态读写边；它们
明确保留字体提供者、运行尺度、特殊节点及页面边界语义的未决项。

## 可重复运行的整数参考入口

冻结 helper 模型现以原始字节纳入
`tools/measure/wordmeasure/docgrid_reference.py`，SHA-256 为
`10cbf5b056719da29e6bee97afcb5691c93b47239d8f0d385a940baa4763fb7e`。
新增 `tools/measure/docgrid_native.py` 校验显式 JSON 输入并生成带哈希的逐案例报告，
不依赖本机未提交的证据目录。此处是复用参考模型，没有另写一套计算或启用生产网格。

```sh
python3 tools/measure/docgrid_native.py \
  tools/measure/cases/docgrid-native-synthetic.json \
  --output artifacts/docgrid-helper-replay.json
```

仓库的四个样例覆盖偶数/奇数余量、先转换周期再取倍数，以及独立外部下限。
所有输入都是合成整数；尺度 300/1440 不作为任何 Word 探针的实际模式声明。
报告保留中间值、诊断、opaque word 和源哈希，明确标为 `ARITHMETIC_REFERENCE`。
输入单位、运行分支和来源尚未认证，不能据此报告 Word 或引擎验收通过。

专项回归覆盖 32 项输入、整数边界和文件输出合同。模型沿用原冻结检查的范围：
不是新函数的完整指令解释器，诊断调用假定返回且不改输入，最终消费者另行验证。
