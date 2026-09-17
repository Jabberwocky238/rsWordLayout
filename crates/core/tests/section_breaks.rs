//! 分节起始要不要另起一页。
//!
//! 口径是**实测对过的**，不是照规范推的：`w:sectPr/w:type` 说的是
//! **这一节自己怎么开始**，不是「上一节之后怎么断」。
//! MR1 夹具 5 个节的页归属逐条相符（5/5）：
//!
//! | 节 | `blockRange` | `kind` | Word |
//! | --- | --- | --- | --- |
//! | s0 | `[0,9)` | `nextPage` | 起于文档开头，不额外起页 |
//! | s1 | `[9,10)` | `continuous` | 紧接上一节 |
//! | s2 | `[10,11)` | `nextPage` | **起新页** |
//! | s3 | `[11,12)` | `continuous` | 紧接上一节 |
//! | s4 | `[12,16)` | `nextPage` | **起新页** |
//!
//! 此前本仓库的文档一度写「分节起始翻页已覆盖」——那是错的：
//! 桥接层处理段内 `sectPr` 只是为了定终止符，`sections[].blockRange` 根本没被读。

use rsword_layout_core::{Engine, PageSetup, SimpleMetrics, paras_from_document};
use serde_json::{Value, json};

/// 一个最小的文本块。
fn block(text: &str) -> Value {
    json!({
        "kind": "text",
        "inlines": [{
            "kind": "run",
            "text": text,
            "segments": [{"kind": {"kind": "text"}, "text": [0, text.len()]}],
            "props": {}
        }],
        "props": {}
    })
}

fn doc(blocks: usize, sections: Vec<(usize, usize, &str)>) -> Value {
    json!({
        "main": (0..blocks).map(|i| block(&format!("P{i}"))).collect::<Vec<_>>(),
        "sections": sections
            .into_iter()
            .map(|(start, end, kind)| json!({
                "blockRange": [start, end],
                "props": {"kind": kind}
            }))
            .collect::<Vec<_>>(),
    })
}

fn pages(doc: &Value) -> usize {
    let (paras, _) = paras_from_document(doc);
    let metrics = SimpleMetrics;
    Engine::new(&metrics, PageSetup::a4()).layout(&paras).len()
}

#[test]
fn next_page_section_starts_a_new_page() {
    // 两节：第二节 nextPage ⇒ 第二块另起一页。
    let d = doc(2, vec![(0, 1, "nextPage"), (1, 2, "nextPage")]);
    assert_eq!(pages(&d), 2, "nextPage 的分节起点没有另起一页");
}

#[test]
fn continuous_section_does_not() {
    let d = doc(2, vec![(0, 1, "nextPage"), (1, 2, "continuous")]);
    assert_eq!(pages(&d), 1, "continuous 不该另起一页");
}

#[test]
fn first_section_does_not_add_a_blank_page() {
    // 文档开头那一节即使是 nextPage，也不该在它前面多出一张空页。
    let d = doc(1, vec![(0, 1, "nextPage")]);
    assert_eq!(pages(&d), 1, "文档开头多出了一张空页");
}

#[test]
fn next_column_is_not_a_page_break() {
    // 换栏不是换页。本版不实现分栏，当成换页会**凭空多出页**——
    // 而凭空多出的页在比较器里只会报结构失败，查起来比少一页更费事。
    let d = doc(2, vec![(0, 1, "nextPage"), (1, 2, "nextColumn")]);
    assert_eq!(pages(&d), 1, "nextColumn 被当成了换页");
}

#[test]
fn even_and_odd_page_start_a_new_page_too() {
    // 它们还要求落在偶／奇页上，必要时补空页——**那一层未实现也未测**，
    // 这里只钉「至少要另起一页」，不钉页面奇偶。
    for kind in ["evenPage", "oddPage"] {
        let d = doc(2, vec![(0, 1, "nextPage"), (1, 2, kind)]);
        assert_eq!(pages(&d), 2, "{kind} 没有另起一页");
    }
}

#[test]
fn mr1_section_layout_reproduces_word() {
    // MR1 夹具的节结构（见模块文档）：15 个文本块 + 1 个 protected 块，
    // 其中 s2、s4 是不在开头的 nextPage，各起一页。
    // 这里不带手动分页符，只看分节贡献：1 + 2 = 3 页。
    let d = doc(
        15,
        vec![
            (0, 9, "nextPage"),
            (9, 10, "continuous"),
            (10, 11, "nextPage"),
            (11, 12, "continuous"),
            (12, 16, "nextPage"),
        ],
    );
    assert_eq!(pages(&d), 3, "分节应当贡献 2 次翻页（s2、s4）");
}

#[test]
fn sections_are_indexed_by_block_not_by_paragraph() {
    // `blockRange` 用的是 `main` 的块下标，而非文本块（表格、绘图）**会被跳过**。
    // 拿段落序号去查就会错位——这里第 1 块是非文本块。
    // 头一节故意用 continuous，这样只有**块下标 2** 被标记；
    // 若实现拿段落序号去查，就会错标到段落 0（它是块 0）。
    let mut d = doc(3, vec![(0, 2, "continuous"), (2, 3, "nextPage")]);
    d["main"][1] = json!({"kind": "table"});
    let (paras, skipped) = paras_from_document(&d);
    assert_eq!(skipped, 1, "非文本块应当被跳过");
    assert_eq!(paras.len(), 2);
    assert!(!paras[0].page_break_before, "块 0 不是分节起点，不该被标记");
    assert!(paras[1].page_break_before, "按段落序号查会错位，标不到块下标 2");
}
