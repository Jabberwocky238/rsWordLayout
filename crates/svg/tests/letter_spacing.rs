//! `w:spacing` / `w:w` 进到 SVG：布局按含间距与缩放的宽度断行，`<text>` 也得这样画。

use rsword_layout_core::{DrawCmd, FontSpec, LineTerminator, Paint, VectorCanvas};
use rsword_layout_svg::SvgCanvas;

fn svg_of(font: FontSpec) -> String {
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
fn svg_text_carries_letter_spacing_and_scale() {
    let mut font = FontSpec::new("test", 24);
    font.letter_spacing = 20;
    let svg = svg_of(font.clone());
    // Numeric user units map to layout points here; a `pt` suffix would introduce CSS unit conversion.
    assert!(svg.contains(" letter-spacing=\"1.0000\""), "{svg}");
    assert!(!svg.contains("transform="), "不缩放就不带 transform：{svg}");

    // 80%：以起点 72pt 为不动点横向压；间距处在压过的坐标系里，预先除以 0.8，页面上仍是 1pt。
    font.scale_pct = 80;
    let svg = svg_of(font.clone());
    assert!(svg.contains(" transform=\"matrix(0.8000 0 0 1 14.400000 0)\""), "{svg}");
    assert!(svg.contains(" letter-spacing=\"1.2500\""), "{svg}");

    // 紧缩：负值照写。
    font.scale_pct = 100;
    font.letter_spacing = -10;
    assert!(svg_of(font).contains(" letter-spacing=\"-0.5000\""));
}

#[test]
fn default_spacing_and_scale_leave_the_text_element_unchanged() {
    // 默认值不多出任何属性：与接入间距之前逐字节相同。
    let svg = svg_of(FontSpec::new("test", 24));
    assert!(!svg.contains("letter-spacing") && !svg.contains("transform="), "{svg}");
    assert!(
        svg.contains("<text x=\"72.000000\" y=\"80.00\" font-family=\"test\" font-size=\"12.00\" xml:space=\"preserve\">gamma</text>"),
        "{svg}"
    );
}
