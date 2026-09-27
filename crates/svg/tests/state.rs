use rsword_layout_core::{
    Color, DrawCmd, FillRule, Paint, PaintOp, Path, Rect, Transform, VectorCanvas,
};
use rsword_layout_svg::SvgCanvas;

fn rect(color: Color) -> DrawCmd {
    DrawCmd::DrawPath {
        path: Path::rect(Rect::new(0, 0, 200, 200)),
        op: PaintOp::Fill {
            paint: Paint {
                color,
                ..Paint::default()
            },
            rule: FillRule::NonZero,
        },
    }
}

#[test]
fn restore_closes_all_transform_and_clip_groups_since_the_matching_save() {
    let mut canvas = SvgCanvas::new();
    canvas.begin_page(2000, 2000).unwrap();
    for cmd in [
        DrawCmd::Save,
        DrawCmd::Transform(Transform {
            e: 400.0,
            ..Transform::default()
        }),
        DrawCmd::Clip {
            path: Path::rect(Rect::new(0, 0, 500, 500)),
            rule: FillRule::NonZero,
        },
        rect(Color::rgb(255, 0, 0)),
        DrawCmd::Restore,
        rect(Color::rgb(0, 0, 255)),
    ] {
        canvas.draw(&cmd).unwrap();
    }
    canvas.end_page().unwrap();
    let svg = &canvas.pages()[0];
    let before_blue = svg.split("fill=\"#0000ff\"").next().unwrap();
    assert_eq!(
        before_blue.matches("<g").count(),
        before_blue.matches("</g>").count(),
        "{svg}"
    );
}

#[test]
fn repeated_clips_and_different_pages_use_distinct_document_ids() {
    let mut canvas = SvgCanvas::new();
    for _ in 0..2 {
        canvas.begin_page(2000, 2000).unwrap();
        for _ in 0..2 {
            canvas.draw(&DrawCmd::Save).unwrap();
            canvas
                .draw(&DrawCmd::Clip {
                    path: Path::rect(Rect::new(0, 0, 500, 500)),
                    rule: FillRule::NonZero,
                })
                .unwrap();
            canvas.draw(&DrawCmd::Restore).unwrap();
            // Extra restores are ignored by the canvas contract. They must not
            // make a later clip reuse an existing document-scoped identifier.
            canvas.draw(&DrawCmd::Restore).unwrap();
        }
        canvas.end_page().unwrap();
    }
    let ids: Vec<_> = canvas
        .pages()
        .iter()
        .flat_map(|svg| {
            svg.split("<clipPath id=\"")
                .skip(1)
                .map(|part| part.split('"').next().unwrap())
        })
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), 4);
    assert_eq!(unique.len(), 4, "{ids:?}");
}

#[test]
fn selectable_font_family_is_escaped_as_an_xml_attribute() {
    let mut canvas = SvgCanvas::new();
    canvas.begin_page(2000, 2000).unwrap();
    canvas
        .draw(&DrawCmd::DrawGlyphs {
            glyphs: vec![],
            origin_x: 0,
            origin_x_pt: 0.0,
            origin_y: 0,
            origin_y_fine: 0,
            text: "safe".into(),
            font: rsword_layout_core::FontSpec::new("a\"b'&<", 24),
            paint: Paint::default(),
            terminator: Default::default(),
            source: None,
            line: 0,
        })
        .unwrap();
    canvas.end_page().unwrap();
    assert!(canvas.pages()[0].contains("font-family=\"a&quot;b&#39;&amp;&lt;\""));
}

#[test]
fn transform_keeps_fractional_twip_translation() {
    let mut canvas = SvgCanvas::new();
    canvas.begin_page(2000, 2000).unwrap();
    canvas
        .draw(&DrawCmd::Transform(Transform {
            e: 0.25,
            f: -0.75,
            ..Transform::default()
        }))
        .unwrap();
    canvas.draw(&rect(Color::BLACK)).unwrap();
    canvas.end_page().unwrap();
    assert!(
        canvas.pages()[0].contains("matrix(1 0 0 1 0.0125 -0.0375)"),
        "{}",
        canvas.pages()[0]
    );
}
