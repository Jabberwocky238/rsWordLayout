# page-start 第一次采集：**VOID**（2026-09-17）

判据：`docs/PREREG-2026-09-17-page-start.md`。夹具 sha256 `78550ef1…`（第一版）。

判定 **VOID**，触发 **F-B**：PDF 里出现 **Cambria** —— 一个根本没申请的字体。

**这一次是真的字体替换，不是量具误报。** Kokonor 与 Gurmukhi MN **没有拉丁字形**
（读 cmap：Kokonor 缺全部数字与字母，Gurmukhi MN 缺全部字母），所以标签
`"d00"` 这样的 ASCII 被 Word 拿回退字体 Cambria 代画了。那几组根本不是用
申请的字体排的，读数当然不能用。

两点值得记下：

1. **能发现它，靠的是新补的那一问**——「PDF 里有没有账面之外的名字」。
   旧核查只问「申请的字体在不在」，Cambria 出现了也照样 PASS。
   这一问是上一次误报之后补的，这次就立了功。
2. 这道关本该更早。画不出标签是**夹具生成时**就能查的事，不该等采完。
   现在 `wordmeasure/fontcover.py` 的 `assert_can_draw` 在生成夹具时守着，
   `make_pagestart_fixture.py` 第一件事就是调它。

夹具随之换掉那两个族（改用 AppleMyungjo、Arial Unicode MS），sha256 变成
`e9c93373…`，判据文 §7.1 记了这次修订。**P0 / P1、容差、分母、排除项、
其余证否条件一个字未动**；作废时判据脚本在算 P0/P1 之前就短路，
`VERDICT.json` 的 `predictions` 是空的。

只留 META / VERDICT / preflight 作记录。
