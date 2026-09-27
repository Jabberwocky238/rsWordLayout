# docGrid 页面容纳边界探针

已有 21 份规范输入验证了标签原点和部分步长关系，但全部在一页内，不能识别页面
容纳规则。本片新增 11 份输入，只改变正文高度，并显式关闭段落保留约束。
这是网格算法的分页判别输入，不是已实现的网格公式。

## 输入与复现

生成器：`tools/measure/make_docgrid_page_fit_fixture.py`。
本次源目录：`artifacts/docgrid-page-fit-source-2026-09-27/`。
`manifest.json` SHA-256：
`0b1a0835ffb3877946d2204cbd4648e70299ffb1b40c301cc6ae242fa96cb419`。

| 组 | 正文高度（twips） | 网格 | snapToGrid |
| --- | --- | --- | --- |
| 网格 | 4200、4240、4260、4280、4300、4320、4360 | lines，pitch 360 | 显式 true |
| 对照 | 3300、3310、3320、3340 | 未声明 | 显式 true |

每份输入包含 12 个独立段落 G000 至 G011，正文与标记均为 Times New Roman 12 pt，
四字体槽及 sz/szCs 全部明确声明。行距 auto 240，段前后距为零，左对齐，
keepNext、keepLines、widowControl 均显式 false。单栏，兼容模式 15，页宽 11906，
四边距均 720，页高等于正文高度加 1440。没有表格、软换行、硬分页或页眉页脚内容。
源流为 60 个 UTF-16 单元，每个四字符标签后跟一个段落标记。

与旧大页探针相比，本片改变页高、明确关闭 keepNext/keepLines，并在无网格对照中
显式设置 snapToGrid=true。因此不能把旧输入的 Word 输出当作本片已经测得的结果。
3310 的采样位置用来区分两种候选容纳量；这只是选点依据，没有写入预期页数。

```sh
python3 -B tools/measure/make_docgrid_page_fit_fixture.py --out NEW_DIRECTORY
python3 -B tools/measure/make_docgrid_page_fit_fixture.py --check EXISTING_DIRECTORY
PYTHONPATH=tools/measure python3 -m pytest -q tools/measure/tests/test_docgrid_page_fit_fixture.py
```

生成拒绝已有目录；检查仅只读核验确定性包字节、XML 属性顺序、字体、页几何、源 CP
与清单。清单保留生成器哈希，因此应使用同一版本检查旧批次。源目录不添加采集结果。
两次独立生成字节相同，检查保留文件修改时间，破坏输入或增加/删除文件会被拒绝。
12 项测试已通过；这些验证的是采样输入，不是 Word 分页正确性。

## 观察与解释边界

采集须保存实际 PDF 的所有页，以及完整标签集合、每页首末源 CP、全部字符的双次
原生扫描、字体和字号核验。真实空尾页也要保留；不能只用首页标签数量代替页数。
每个 PDF 标签按几何和唯一文本绑定源位置，再核对其原生页码，不按字形序号猜测。

正文高度扫描首先报告每份输入的完整页归属。只有同组输入具备一致的源流、字体、
段落约束和布局环境时，才能用相邻失败/成功高度约束本组的容纳边界。有限采样不证明
任意高度上的单调性，也不能把边界直接命名为某一行的 required extent。
PDF 字形原点仍不代表行盒顶、底或占高；步长、首行位置和分页容量分别检验。

本片与 [网格原点证据](DOCGRID-WORD-EVIDENCE-2026-09-27.md) 共同约束后续算法。
原生分量的单位、首末行裁剪与实际行高仍需逐项映射，不把取整步长自动复制到占高。
