//! 源字符区间必须落在**全篇** UTF-16 偏移空间里。
//!
//! 这不是内部记账的细节，是比较器的依据：配对**不能靠 Unicode 身份**
//! （PDF 字形可能没有 ToUnicode 映射），只能按读序，而读序要靠源区间校验。
//! 区间对不上 Word 的 `Range.Start/End`，配对就没有依据。
//!
//! 这条错很隐蔽——段内偏移与全篇偏移长得几乎一样：都是小整数、都单调、
//! 都不越界。实测时是量具的 `selfcheck` 发现的：`sourceStart` 回到 0 的次数
//! 正好等于段落数（5 份夹具全中），而最大 `sourceEnd` 远小于篇长
//! （MR1 夹具 22 vs 287）。所以这里钉的是**跨段连续**，不只是「单调」。

use rsword_layout_core::{
    Color, Engine, FontSpec, LayoutRecord, PageSetup, Para, Run, SimpleMetrics, paint_document,
};

fn para(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
        rise: 0,
        }],
        ..Para::default()
    }
}

fn record(paras: &[Para]) -> LayoutRecord {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    LayoutRecord::from_paint(&paint_document(&engine.layout(paras), None, &[]))
}

/// 全部记录的源区间，按出现顺序。
fn ranges(record: &LayoutRecord) -> Vec<(u32, u32)> {
    record
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .filter_map(|l| l.source.map(|s| (s.start, s.end)))
        .collect()
}

#[test]
fn offsets_do_not_restart_at_each_paragraph() {
    // 三段各 5 个字符。段内偏移会给出 (0,5) (0,5) (0,5)；
    // 全篇偏移应当是 (0,5) (6,11) (12,17)——每段末尾隔一个终止符。
    let r = record(&[para("aaaaa"), para("bbbbb"), para("ccccc")]);
    let got = ranges(&r);

    let restarts = got.iter().filter(|(start, _)| *start == 0).count();
    assert_eq!(restarts, 1, "`sourceStart` 回到 0 的次数应当只有 1 次，实得 {restarts}：{got:?}");
    assert_eq!(got, vec![(0, 5), (6, 11), (12, 17)], "{got:?}");
}

#[test]
fn each_paragraph_terminator_occupies_one_unit() {
    // Word 的 `Range` 为每段末尾算一个终止符（`\r`，带 `w:sectPr` 的段是 `\x0c`，
    // 都占 1 个 UTF-16 单位）。少算它，后面每一段都会整体前移一格。
    let r = record(&[para("ab"), para("cd")]);
    let got = ranges(&r);
    assert_eq!(got[0], (0, 2));
    assert_eq!(got[1].0, 3, "第二段应当从 3 起（2 个字符 + 1 个终止符），实得 {}", got[1].0);
}

#[test]
fn offsets_are_utf16_units_not_bytes_or_chars() {
    // 契约写的是 UTF-16 单位，与 rsword 的坐标流一致。
    // "中" 是 1 个 UTF-16 单位但 3 个字节；星号平面的字符是 2 个单位但 1 个 char。
    let r = record(&[para("中"), para("\u{1F600}")]);
    let got = ranges(&r);
    assert_eq!(got[0], (0, 1), "一个汉字是 1 个 UTF-16 单位，不是 3 个字节");
    // 第二段从 2 起（1 个单位 + 1 个终止符），且占 2 个单位（代理对）。
    assert_eq!(got[1], (2, 4), "星号平面字符占 2 个 UTF-16 单位：{got:?}");
}

#[test]
fn ranges_stay_contiguous_across_a_wrapped_paragraph() {
    // 换行处被 trim 掉的空格在源侧**仍然占位**。不记进游标的话，
    // 本段后续片段会整体前移，而这种错在几何上看不出来——
    // 只会让配对悄悄错位，比报「判不了」糟得多。
    let long = "word ".repeat(60);
    let r = record(&[para(long.trim_end()), para("tail")]);
    let got = ranges(&r);

    // 段内各片段首尾相接，不能有空洞。
    let para_units = long.trim_end().encode_utf16().count() as u32;
    let first = got.iter().filter(|(s, _)| *s < para_units).copied().collect::<Vec<_>>();
    assert!(first.len() > 1, "这段应当换过行：{got:?}");
    for pair in first.windows(2) {
        assert_eq!(pair[0].1, pair[1].0, "片段之间有空洞（换行处的空格没记账）：{got:?}");
    }
    assert_eq!(first[0].0, 0);
    assert_eq!(
        first.last().unwrap().1,
        para_units,
        "本段止点应当等于它的 UTF-16 长度：{got:?}"
    );

    // 下一段从「本段长度 + 1 个终止符」起。
    let tail = got.iter().find(|(s, _)| *s >= para_units).copied().unwrap();
    assert_eq!(tail, (para_units + 1, para_units + 5), "{got:?}");
}

#[test]
fn empty_paragraphs_still_advance_the_cursor() {
    // 空段落也占一个终止符位；不推进游标的话，其后每段都会错一格。
    let r = record(&[para("ab"), para(""), para("cd")]);
    let got = ranges(&r);
    let last = got.last().copied().unwrap();
    // "ab"(2) + 终止符(1) + 空段(0) + 终止符(1) = 4
    assert_eq!(last, (4, 6), "空段落没有推进全篇游标：{got:?}");
}
