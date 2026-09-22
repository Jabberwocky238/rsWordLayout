//! 段内手动分页符要真的翻页。
//!
//! `Para::page_break_before` 对应的是**段落属性** `w:pageBreakBefore`；
//! 段内任意位置的 `w:br w:type="page"` 是另一回事，必须挂在行上。
//!
//! 难点在于占位符**不区分种类**：分页符、软回车、行内图在 run 文本里都是同一个
//! U+FFFC。种类只在 rsword 的 `segments[].kind` 里，靠 [`Run::placeholders`] 带进来。
//! 分不出来的后果是双向的——要么该翻页的不翻，要么一张图把文档劈成两页。
//!
//! 实测逼出来的：MR1 夹具 8 个手动分页符，Word 排 11 页而引擎排 1 页；
//! 仓库自带的 `breaks.docx`（1 个分页符）也只排 1 页。

use rsword_layout_core::{
    Color, Engine, FontSpec, PageSetup, Para, PlaceholderKind, Run, SimpleMetrics,
};

fn run(text: &str, placeholders: &[PlaceholderKind]) -> Run {
    Run {
        text: text.to_string(),
        font: FontSpec::new("Test", 24),
        color: Color::BLACK,
        placeholders: placeholders.to_vec(),
        rise: 0,
        rise_fine: None,
    }
}

fn pages(runs: Vec<Run>) -> usize {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let para = Para { runs, ..Para::default() };
    engine.layout(&[para]).len()
}

#[test]
fn page_break_mid_paragraph_splits_the_page() {
    // 实测形状：`breaks.docx` 的 `'分页符之前￼分页符之后'`。
    assert_eq!(
        pages(vec![run("前\u{FFFC}后", &[PlaceholderKind::PageBreak])]),
        2,
        "段内手动分页符没有翻页"
    );
}

#[test]
fn inline_object_must_not_paginate() {
    // 反向的错同样要挡住：`wrap.docx` 的占位符是一张行内图，
    // 把它当分页符会凭空多出一页。**凭空多出来的页在比较器里只会报结构失败，
    // 查起来比少一页更费事。**
    assert_eq!(
        pages(vec![run("前\u{FFFC}后", &[PlaceholderKind::Object])]),
        1,
        "行内图被当成了分页符"
    );
}

#[test]
fn soft_return_breaks_the_line_but_not_the_page() {
    assert_eq!(
        pages(vec![run("前\u{FFFC}后", &[PlaceholderKind::LineBreak])]),
        1,
        "软回车不该翻页"
    );
}

#[test]
fn unknown_placeholder_kind_falls_back_to_object() {
    // 桥接层拿不到种类时（`placeholders` 短了）要保守：宁可少一次分页，
    // 也不凭空造出一次。
    assert_eq!(pages(vec![run("前\u{FFFC}后", &[])]), 1, "种类未知时不该翻页");
}

#[test]
fn consecutive_page_breaks_make_an_empty_page() {
    // 实测夹具里有连续分页符，Word 确实产生只有一条行记录的页。
    // 这里两个分页符 ⇒ 三页（前、空、后）。
    assert_eq!(
        pages(vec![run(
            "前\u{FFFC}\u{FFFC}后",
            &[PlaceholderKind::PageBreak, PlaceholderKind::PageBreak]
        )]),
        3,
        "连续分页符应当排出一张空页"
    );
}

#[test]
fn page_break_at_paragraph_start_still_paginates() {
    // 实测形状：MR1 的 `'\u{FFFC}B07 leading break'`——分页符在段首。
    assert_eq!(
        pages(vec![run("\u{FFFC}后", &[PlaceholderKind::PageBreak])]),
        2,
        "段首分页符没有翻页"
    );
}

#[test]
fn page_break_does_not_depend_on_remaining_space() {
    // 与「放不下就翻页」不同：源里写了分页就是分页，**不看还剩多少空间**。
    // 一行字远远放得下，照样得翻。
    let count = pages(vec![run("a\u{FFFC}b", &[PlaceholderKind::PageBreak])]);
    assert_eq!(count, 2, "还有大量空间时分页符被忽略了");
}
