# docGrid 算法的本地静态证据复核

日期：2026-09-27。先只读 `../word_analyse` 的本地报告、Android 原生库和保存的
Web 脚本，末节补记本机 macOS 框架的窄查。没有启动 Word、连接设备、请求网络或
采集新布局读数。

**尚未找到足以实现 Word docGrid 的静态公式。** 可以确认字体度量有条件覆盖路径，
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
框架内部调用或间接调用。`ApplySnapGridReal` 本次只记录候选符号，没有展开其长体，
也没有根据名字将它归为正文纵向网格。

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
