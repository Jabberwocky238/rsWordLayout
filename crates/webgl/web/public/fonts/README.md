# 默认字体集

运行时由 `web/src/main.js` fetch 后传给 wasm，**不编进二进制**——
wasm 因此保持 1.8MB 左右，字体则可被浏览器缓存、可按需只拉西文那一份。

| 文件 | 覆盖 | 许可 |
| --- | --- | --- |
| `DejaVuSans.ttf` | 拉丁、希腊、西里尔、部分符号 | Bitstream Vera（DejaVu 的改动属公有领域） |
| `DroidSansFallbackFull.ttf` | CJK 及大量回退字形 | Apache-2.0 |

两者都允许再分发且不传染。选它们而不是文泉驿微米黑，是因为后者是
Apache-2.0 或 GPL-3+ 双许可，随产品分发要先选定一条并保留相应声明。

字形选择不按文件名硬编码：由 `docx_layout::fontenv` 按码位查覆盖，
缺字时报 `FONT_MISSING` 而不是默默画错。
