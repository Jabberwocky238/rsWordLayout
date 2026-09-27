//! `w:caps` / `w:smallCaps` 进到 SVG：`<text>` 保留原文，大小写交给渲染器（近似，见 `caps_attr`）。

use rsword_layout_core::{Caps, DrawCmd, FontSpec, LineTerminator, Paint, VectorCanvas};
use rsword_layout_svg::SvgCanvas;

fn svg_of(caps: Caps) -> String {
    let mut font = FontSpec::new("test", 24);
    font.caps = caps;
    let cmd = DrawCmd::DrawGlyphs {
        glyphs: vec![],
        origin_x: 1440,
        origin_x_pt: 72.0,
        origin_y: 1600,
        origin_y_fine: 8000,
        text: "gamma".into(),
        font,
        paint: Paint::default(),
        terminator: LineTerminator::Wrapped,
        source: Some((0, 5)),
        line: 0,
    };
    let mut canvas = SvgCanvas::new();
    canvas.begin_page(12000, 16000).unwrap();
    canvas.draw(&cmd).unwrap();
    canvas.end_page().unwrap();
    canvas.pages()[0].clone()
}

#[test]
fn svg_text_keeps_source_text_and_carries_the_caps_mode() {
    let all = svg_of(Caps::All);
    assert!(all.contains(" style=\"text-transform:uppercase\""), "{all}");
    assert!(all.contains(">gamma</text>"), "原文不改：{all}");
    let small = svg_of(Caps::Small);
    assert!(small.contains(" font-variant=\"small-caps\""), "{small}");
    assert!(small.contains(">gamma</text>"), "{small}");
    let none = svg_of(Caps::None);
    assert!(!none.contains("text-transform") && !none.contains("font-variant"), "{none}");
}
