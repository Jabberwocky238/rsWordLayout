# first-line 第一次采集：**VOID**（2026-09-17）

判据：`docs/PREREG-2026-09-17-first-line.md`。夹具 sha256 `a9f42e6a…`。

判定 **VOID**，触发 **F-B**：PDF 里出现「申请之外的字体名」
`BodoniSvtyTwoSCITCTT-Book`。

**这是量具的缺陷，不是字体被替换。** `Bodoni 72 Smallcaps Book` 这个文件
自己声明的 PostScript 名就是 `BodoniSvtyTwoSCITCTT-Book`（Svty Two =
Seventy Two = 72），而 PDF 的子集名用的是 PostScript 名。当时的核查拿**族名**
做字符串包含，族名与 PostScript 名可以毫无字面关系，于是把没被替换的字体判成了替换。

按预注册，F-B 触发即作废，**没有例外**——「我认为这是假阳性」不是放行的理由，
那正是预注册要挡的东西。所以这一批照判作废，先修量具，再用**同一份夹具、
同一套判据**重采。

要点：作废时判据脚本**不输出任何 T0 / T1 得分**（`VERDICT.json` 里 `predictions`
是空的）。所以重采不构成「看了结果再调」——关于预测的数据，一个数字都没见过。

只留 META / VERDICT / preflight 三份作记录，读数不留。
