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

/// run 自己的 `w:sz` 与段落基准不同时，**字号与抬升必须来自同一个数**。
///
/// 这条是量具量出来的。`tools/measure/prereg_probe.py` 对 10 / 12 / 18pt 三个
/// 字号读引擎轨迹，字号缩放对（20 / 24 / 36 半点 → 13 / 16 / 24），抬升却
/// **恒定 4.05pt**——三个字号一模一样。
///
/// 原因是 `bridge.rs` 里两处取了不同的数：`run_font` 用 run 自己的 `w:sz`，
/// 而 `Run::rise` 用段落基准。段落基准恰好是 12pt，所以 12pt 那一档看着是对的，
/// 上下都错。**本文件原有的用例全都让 run 与段落同号**，所以一条也照不出来——
/// 检验实例必须在被判的那一层上与设计实例不同（量具方法 §7.2）。
#[test]
fn rise_scales_with_the_runs_own_size_not_the_paragraph_base() {
    // 段落基准是 24 半点（12pt）：前一个 run 不带 size，走默认。
    for (half_points, want_sup, want_sub) in [(20u32, 68i32, -16i32), (24, 81, -19), (36, 122, -28)]
    {
        let sup = runs(&paint_with_props(
            "x",
            json!({"size": half_points, "vertAlign": "superscript"}),
        ));
        let sub = runs(&paint_with_props(
            "x",
            json!({"size": half_points, "vertAlign": "subscript"}),
        ));
        // [0] 是基准 run（12pt，无抬升），[1] 是带 vertAlign 的那个。
        let base_y = sup[0].0;
        assert_eq!(
            base_y - sup[1].0,
            want_sup,
            "{half_points} 半点的上标抬升不对——抬升没跟着 run 自己的字号走"
        );
        assert_eq!(
            base_y - sub[1].0,
            want_sub,
            "{half_points} 半点的下标下沉不对——下沉没跟着 run 自己的字号走"
        );
    }
}

/// 自证上面那条有鉴别力：三个字号的抬升必须**两两不等**。
///
/// 若哪天常数改成与字号无关，上面那条会因为期望值也被一起改而继续通过；
/// 这一条盯的是「它到底有没有随字号变」，不依赖具体数值。
#[test]
fn the_three_sizes_give_three_different_rises() {
    let rises: Vec<i32> = [20u32, 24, 36]
        .iter()
        .map(|&hp| {
            let r = runs(&paint_with_props(
                "x",
                json!({"size": hp, "vertAlign": "superscript"}),
            ));
            r[0].0 - r[1].0
        })
        .collect();
    assert!(
        rises[0] != rises[1] && rises[1] != rises[2] && rises[0] != rises[2],
        "三个字号给出同一个抬升 {rises:?}——这组输入照不出「抬升不随字号变」"
    );
}
