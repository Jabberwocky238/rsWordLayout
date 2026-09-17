//! `w:vertAlign`（上下标）与 `w:position`（抬升）。
//!
//! 两者是**不同的机制**，不要合并：
//!
//! - `w:vertAlign` **同时**缩小字号并挪基线；
//! - `w:position`（半点，可负）**只挪基线，不改字号**。
//!
//! 比例与偏移是**从真 Word 量出来的，不是字体给的**。
//! 实测（Liberation Serif 12pt，Word for Mac）把两者分得很开：
//!
//! | | 字体 OS/2 说 | Word 实际用 |
//! | --- | ---: | ---: |
//! | 字号 | 0.6499 em（7.7988pt） | **0.66 em（7.92pt）** |
//! | 上标偏移 | 0.4531 em（5.4375pt） | **0.34 em（4.08pt）** |
//! | 下标偏移 | 0.1431 em（1.7168pt） | **0.08 em（0.96pt）** |
//!
//! 所以**不要「改进」成读 `ySuperscriptYSize`**——那看着更正确，却与 Word 对不上。
//!
//! **证据强度 n=1**（一种字体、一个字号），按量具方法 §7.5 属「回测」。

use rsword_layout_core::{
    Color, DrawCmd, Engine, FontSpec, PageSetup, Para, PaintList, Run, SimpleMetrics,
    paint_document, paras_from_document,
};
use serde_json::json;

/// 用桥接层造 run，好把 `w:vertAlign` / `w:position` 的读取也纳入检验。
fn paint_with_props(text: &str, props: serde_json::Value) -> PaintList {
    let doc = json!({
        "main": [{
            "kind": "text",
            "inlines": [
                {"kind": "run", "text": "base", "props": {"size": 24}},
                {"kind": "run", "text": text, "props": props},
            ],
            "props": {}
        }],
        "sections": []
    });
    let (paras, _) = paras_from_document(&doc);
    let metrics = SimpleMetrics;
    paint_document(&Engine::new(&metrics, PageSetup::a4()).layout(&paras), None, &[])
}

/// 每条绘制指令的 (基线 y, 字号半点)。
fn runs(list: &PaintList) -> Vec<(i32, u32)> {
    let mut out = Vec::new();
    for page in &list.pages {
        for cmd in &page.cmds {
            if let DrawCmd::DrawGlyphs { origin_y, font, .. } = cmd {
                out.push((*origin_y, font.size_half_points));
            }
        }
    }
    out
}

#[test]
fn superscript_shrinks_and_rises() {
    let r = runs(&paint_with_props("sup", json!({"size": 24, "vertAlign": "superscript"})));
    let (base_y, base_size) = r[0];
    let (sup_y, sup_size) = r[1];

    assert_eq!(base_size, 24, "正文该是 12pt");
    // 0.66 × 24 = 15.84 半点；整数半点取最近的 16。
    assert_eq!(sup_size, 16, "上标字号该缩到 0.66 em 的最近整数半点");
    // 12pt = 240 twips，抬升 0.34 em = 81.6 → 81 twips（向上，故 y 减小）。
    assert_eq!(base_y - sup_y, 81, "上标抬升量不对：{r:?}");
}

#[test]
fn subscript_shrinks_and_drops() {
    let r = runs(&paint_with_props("sub", json!({"size": 24, "vertAlign": "subscript"})));
    let (base_y, base_size) = r[0];
    let (sub_y, sub_size) = r[1];

    assert_eq!(base_size, 24);
    assert_eq!(sub_size, 16, "下标与上标用同一个缩放比例");
    // 下沉 0.08 em = 19.2 → 19 twips（向下，故 y 增大）。
    assert_eq!(sub_y - base_y, 19, "下标下沉量不对：{r:?}");
}

#[test]
fn position_raises_without_resizing() {
    // `w:position` 的单位是**半点**。8 半点 = 4pt = 80 twips。
    let r = runs(&paint_with_props("up", json!({"size": 24, "position": 8})));
    let (base_y, base_size) = r[0];
    let (up_y, up_size) = r[1];

    assert_eq!(up_size, base_size, "`w:position` 不该改字号——这是它与上下标的关键区别");
    assert_eq!(base_y - up_y, 80, "抬升量该是 8 半点 = 80 twips：{r:?}");
}

#[test]
fn negative_position_lowers() {
    let r = runs(&paint_with_props("down", json!({"size": 24, "position": -6})));
    let (base_y, _) = r[0];
    let (down_y, down_size) = r[1];
    assert_eq!(down_size, 24);
    assert_eq!(down_y - base_y, 60, "负的 `w:position` 该下沉：{r:?}");
}

#[test]
fn plain_run_is_not_shifted() {
    let r = runs(&paint_with_props("plain", json!({"size": 24})));
    assert_eq!(r[0].0, r[1].0, "没有 vertAlign / position 的 run 不该被挪");
    assert_eq!(r[0].1, r[1].1);
}

#[test]
fn rise_does_not_change_the_advance() {
    // 抬升只改落笔的 y，**不改推进量**——所以它挂在 `Run::rise` 上而不是 `FontSpec` 里。
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let make = |rise| Para {
        runs: vec![Run {
            text: "abcdef".into(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise,
        }],
        ..Para::default()
    };
    let flat = paint_document(&engine.layout(&[make(0)]), None, &[]);
    let lifted = paint_document(&engine.layout(&[make(100)]), None, &[]);

    let xs = |list: &PaintList| -> Vec<i32> {
        list.pages
            .iter()
            .flat_map(|p| p.cmds.iter())
            .filter_map(|c| match c {
                DrawCmd::DrawGlyphs { origin_x, .. } => Some(*origin_x),
                _ => None,
            })
            .collect()
    };
    assert_eq!(xs(&flat), xs(&lifted), "抬升不该影响横向位置");
}
