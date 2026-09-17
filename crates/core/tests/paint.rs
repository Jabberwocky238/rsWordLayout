//! 绘制契约的性质测试。
//!
//! 核心断言只有一条：**core 的产物与分辨率无关**。这是这层契约存在的理由——
//! 旧的 `build_page(page, dpi, ..)` 把 DPI 烘进了顶点坐标，导致缩放只能拉伸位图；
//! 现在坐标一律是 twips，换算留给后端，SVG/PDF 因此根本不必栅格化。

use rsword_layout_core::canvas::Color;
use rsword_layout_core::engine::{Engine, PageSetup, Para, Run};
use rsword_layout_core::measure::FontSpec;
use rsword_layout_core::paint::{PaintCmd, paint_document};
use rsword_layout_core::simple_metrics::SimpleMetrics;

fn doc() -> rsword_layout_core::fragment::LaidOutDocument {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let para = Para {
        runs: vec![Run {
            text: "Hello 世界".to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
        }],
        ..Para::default()
    };
    engine.layout(&[para])
}

#[test]
fn paint_output_carries_no_pixels() {
    // 页面尺寸应当是 A4 的 twips 值，而不是任何像素数。
    let list = paint_document(&doc(), None, &[]);
    let page = &list.pages[0];
    assert_eq!(page.width, 11906, "A4 宽应为 11906 twips");
    assert_eq!(page.height, 16838, "A4 高应为 16838 twips");
}

#[test]
fn text_survives_without_shaper() {
    // 没有 shaper 时仍要保留原文：SVG / PDF 后端直接排文字，不需要字形序列。
    let list = paint_document(&doc(), None, &[]);
    let texts: Vec<&str> = list.pages[0]
        .cmds
        .iter()
        .filter_map(|c| match c {
            PaintCmd::Glyphs { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(!texts.is_empty(), "应当有文字指令");
    assert!(texts.iter().any(|t| t.contains("Hello")), "原文应当保留");
}

#[test]
fn geometry_is_stable_across_invocations() {
    // 同一份布局重复转换必须给出同一结果——后端据此做快照回归。
    let d = doc();
    let a = paint_document(&d, None, &[]);
    let b = paint_document(&d, None, &[]);
    assert_eq!(a, b, "绘制指令必须是确定性的");
}

#[test]
fn content_area_excludes_margins() {
    let list = paint_document(&doc(), None, &[]);
    let area = list.pages[0].content_area;
    // 1 英寸页边距 = 1440 twips。
    assert_eq!(area.x, 1440);
    assert_eq!(area.y, 1440);
    assert_eq!(area.width, 11906 - 2880);
}
