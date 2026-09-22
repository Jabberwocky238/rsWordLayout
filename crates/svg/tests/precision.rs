use rsword_layout_core::{DrawCmd, FontSpec, LineTerminator, Paint, VectorCanvas};
use rsword_layout_svg::SvgCanvas;

#[test]
fn svg_preserves_precise_font_size_and_fragment_origin_without_shaping() {
    let cmd = DrawCmd::DrawGlyphs {
        glyphs: vec![],
        origin_x: 1440,
        origin_x_pt: 72.012345,
        origin_y: 1600,
        origin_y_fine: 7992,
        text: "sup".into(),
        font: FontSpec::new("test", 16).with_size_centipoints(792),
        paint: Paint::default(),
        terminator: LineTerminator::Wrapped,
        source: Some((0, 3)),
        line: 0,
    };
    let mut canvas = SvgCanvas::new();
    canvas.begin_page(12000, 16000).unwrap();
    canvas.draw(&cmd).unwrap();
    canvas.end_page().unwrap();
    let svg = &canvas.pages()[0];
    assert!(svg.contains("x=\"72.012345\" y=\"79.92\""), "{svg}");
    assert!(svg.contains("font-size=\"7.92\""), "{svg}");
}
