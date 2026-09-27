# 镜像页边距

本片把已解析但原先只产生 unsupported 诊断的 `w:mirrorMargins` 接入共享分页器。
依据是明确的页面几何合同；本片没有新增 Word 采集，不能把下列引擎回归称为
Mac、Android 或 Windows 的实测验收。

## 页面合同

ECMA-376 17.15.1.57 要求每第二个物理页面交换节声明的左右边距，页码标签的任意
起始值不改变此相位。本机原文摘录保存在
`../docx-layout/docs/research/ecma-376-word-notes.md`；另参见
[Microsoft 的 MirrorMargins 说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.mirrormargins?view=openxml-3.0.1)。
因此单页中的换栏和连续分节不翻转，实际翻页和为奇偶分节补出的空白页都计入物理序列。

非 RTL 的侧边装订线先与左侧模板边距合成，再随镜像交换，保持面对页的内侧空间。
镜像启用时不使用 gutterAtTop，这一优先级由
[gutterAtTop 规范摘录](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.gutterattop?view=openxml-3.0.1)
明确规定；Word 的 [Gutter 属性说明](https://learn.microsoft.com/en-us/office/vba/api/word.pagesetup.gutter)
也将镜像情况下的装订线归入内边距。

`rtlGutter` 与非零 gutter 的镜像组合继续采用既有侧边合成后交换，并明确保留未验证
诊断。现有 [rtlGutter 规范摘录](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.gutteronright?view=openxml-3.0.1)
说明了普通右侧装订线，但没有在本片闭合其与镜像、节方向的组合优先级。该分支不能称为
已验证的 Word 内侧装订规则；书籍折页、两页合印和 RTL 栏顺序也不是本片实现范围。

## 分页接缝

节的 PageSetup 保持模板，实际 Page.content_area 和栏矩形由物理页序号派生。CLI
几何覆盖先作用于模板；对称边距自然在交换后保持相同。连续节的新几何仍延迟到下一
物理页应用，当前页版心保持权威。段落搬移与 widow/keep 预排查询同一派生规则。

下一栏可能仍在当前页；未来两个区域也可能跨两页。预测必须按区域数计算具体物理页，
不能统一把所有越界预览都当作“下一页”。连续栏重放保留物理页相位，补空白页也通过
真实分页状态转换，避免正文页重复使用空白页的版心。页码字段的重启不驱动物理相位。

## 回归范围

新增 15 项集成测试与 2 项区域状态测试，覆盖三物理页、页码重启、补空白页、硬分页
开启空页后的分节复用、换栏、不等宽栏、偶数页连续栏组平衡、延迟几何、绝对坐标环绕
与 keepLines 搬移，以及缺省/false/非法开关。真实 DOCX 路径使用独立 settings 部件
与文档关系，检查两种命名空间前缀和 OnOff 写法。

A4 回退案例进一步施加 10001 twips 的版心宽度，使左右余量为 952/953；三页的实际
正文起点必须依次为 952、953、952。区域测试比较未来一栏/两栏与真正推进结果，包含
当前和 pending 配置各为一至三栏的组合，以及空页重置、同页新栏组。

追踪中的节边距和栏区域是模板。实际矩形保存在内存 `Page.content_area` 与
`Page.columns`；JSON 输出字形位置及多区域页的栏框，单区域 JSON 不单独记录版心。
这项可观测性限制已写入追踪说明，不能从单栏空白页 JSON 推出实际镜像版心。

## 验证记录

作者先用旧生产实现运行初始 13 项回归，全部失败；实现后通过。最终作者专项为
112 项通过。途中新增硬分页测试漏写 run 文本中的 U+FFFC，修正输入后保留原源区间
断言；失败日志和原始源码快照均保留，没有修改既有测试期望。

根代理完成追踪措辞与 A4 回退测试补强后，workspace/fontenv 为 692 项通过、12 项
忽略，核心默认特性为 567 项通过、0 忽略，workspace 全目标 Clippy `-D warnings`
通过。114 个源码/测试/清单输入在五次命令前后哈希一致；6 个 examples 未纳入旧运行器
的前后采样，审计仅确认其当前内容与 HEAD 相同，不把这一检查追认成运行时采样。
最终回放程序 SHA256 为
`97b44129362f6619c1ba4c6ee8e01fce16932e569908f2caa59a875ec6ee62cf`。

Android 原有 11 份窄路径回放保持 186/186 源行区间条件匹配。与显式 RGB 切片的完整
轨迹相比，仅有新增 `layoutInput.mirrorMargins` 和栏模板 `areasScope` 说明变化；
精确校验这两项后，其余所有字段相同。这不是 Android 镜像或打印分页实测。

Mac 用同一批旧采集分别运行 RGB 基线和最终镜像二进制：30 个包中 25 个产生有效轨迹，
其完整 356 页、3834 行、18150 个字形均相同；完整轨迹仅有上述两处说明变化。
comparison/selfcheck 只忽略经过核对的输出路径，其余全等。原有 23 个 FAIL、7 个
UNDECIDABLE、0 个 OK 保留，其中 5 个原 VOID 未运行比较。旧样本均未启用镜像，
这证明未启用时的回归稳定，不是镜像功能的 Word 实测。

专项历史记录位于 `artifacts/mirror-margins-2026-09-27/agent-focused/`，最终验证位于
`artifacts/mirror-margins-checks-2026-09-27/`，Mac 前后离线回放位于
`artifacts/mirror-margins-mac-replay-2026-09-27/`。

作者专项冻结清单 SHA256：
`03da550cf2c3415bf35040534c4a2a854ac357c8cca768bbf238a4dfefff8ca7`。
最终验证冻结清单 SHA256：
`daae163dd1894f0355880910fadb96005a5a69d23ad1bf624019c0e10ff23b7d`。
Mac 前后回放冻结清单 SHA256：
`34e2bd932bc95133a92f17e67aea25bc245ca166ebab1a0667d34b146f13101d`。
