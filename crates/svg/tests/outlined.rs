#![cfg(feature = "fontenv")]

use rsword_layout_core::{
    Caps, Color, DrawCmd, Engine, FillRule, FontSpec, LineRule, LineTerminator, Margins, PageSetup,
    Paint, PaintList, PaintPage, Para, Path, PositionedGlyph, RealMetrics, Rect, Run, Size,
    TextShaper, Transform, font::FontRegistry, paint_document,
};
use rsword_layout_svg::{OutlineError, render_outlined_html};
use skrifa::{FontRef, MetadataProvider, raw::TableProvider};

const SANS: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf");
const SERIF: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");

fn registry() -> (FontRegistry, String) {
    let mut fonts = FontRegistry::new();
    let face = fonts.add(SANS.to_vec(), 0).unwrap();
    (fonts, face)
}

fn glyph(fonts: &FontRegistry, face: &str, ch: char) -> PositionedGlyph {
    let (data, index) = fonts.face_data(face).unwrap();
    let font = FontRef::from_index(data, index).unwrap();
    PositionedGlyph {
        face: face.into(),
        glyph_id: font.charmap().map(ch).unwrap().to_u32(),
        x: -900,
        y: -900,
        x_pt: 12.123456789,
        y_fine: 7992,
        advance_x: 10000,
        advance_x_pt: 999.0,
        advance_y: 10000,
        size_half_points: 99,
        size_centipoints: 792,
        source: None,
    }
}

fn command(glyphs: Vec<PositionedGlyph>, text: &str, font: FontSpec) -> DrawCmd {
    DrawCmd::DrawGlyphs {
        glyphs,
        origin_x: 9000,
        origin_x_pt: 450.0,
        origin_y: 9000,
        origin_y_fine: 45000,
        text: text.into(),
        font,
        paint: Paint::default(),
        terminator: LineTerminator::ParagraphMark,
        source: None,
        line: 0,
    }
}

fn list(cmds: Vec<DrawCmd>) -> PaintList {
    let mut page = PaintPage::new(12000, 16000, Rect::new(0, 0, 12000, 16000));
    page.cmds = cmds;
    PaintList { pages: vec![page] }
}

fn outlined_paths(html: &str) -> Vec<&str> {
    html.split("<path ")
        .skip(1)
        .map(|rest| rest.split("/>").next().unwrap())
        .filter(|path| path.contains("transform=\"matrix("))
        .collect()
}

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

fn matrix(path: &str) -> [f64; 6] {
    let value = attr(path, "transform")
        .strip_prefix("matrix(")
        .unwrap()
        .strip_suffix(')')
        .unwrap();
    value
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn assert_geometry(path: &str, glyph: &PositionedGlyph, font: &FontSpec, fonts: &FontRegistry) {
    let (data, index) = fonts.face_data(&glyph.face).unwrap();
    let upem = FontRef::from_index(data, index)
        .unwrap()
        .head()
        .unwrap()
        .units_per_em();
    let sy = glyph.size_centipoints as f64 / 100.0 / f64::from(upem);
    let scale = if font.scale_pct > 0 {
        f64::from(font.scale_pct) / 100.0
    } else {
        1.0
    };
    let expected = [
        sy * scale,
        0.0,
        0.0,
        -sy,
        glyph.x_pt,
        glyph.y_fine as f64 / 100.0,
    ];
    for (actual, expected) in matrix(path).into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}

#[test]
fn paths_use_each_glyphs_precise_origin_face_and_size_without_advancing_a_pen() {
    let (fonts, face) = registry();
    let a = glyph(&fonts, &face, 'A');
    let mut b = glyph(&fonts, &face, 'S');
    b.x_pt = 99.345678901;
    b.y_fine = 12345;
    b.size_centipoints = 1105;
    let mut font = FontSpec::new("must not select this family", 48);
    font.scale_pct = 80;
    font.letter_spacing = -180;
    font.caps = Caps::Small;
    font.bold = true;
    font.italic = true;
    let html = render_outlined_html(
        &list(vec![command(
            vec![a.clone(), b.clone()],
            "ignored",
            font.clone(),
        )]),
        &fonts,
        "<A&B>",
    )
    .unwrap();
    let paths = outlined_paths(&html);
    assert_eq!(paths.len(), 2);
    assert_geometry(paths[0], &a, &font, &fonts);
    assert_geometry(paths[1], &b, &font, &fonts);
    assert!(attr(paths[1], "d").contains('Q'));
    assert!(!html.contains("<text "));
    assert!(
        !html.contains("letter-spacing")
            && !html.contains("font-variant")
            && !html.contains("font-style")
    );
    assert!(html.contains("<title>&lt;A&amp;B&gt;</title>"));
    assert!(!html.contains("文字可选中"));
}

#[test]
fn real_fallback_caps_tracking_and_dn_positions_are_consumed_once() {
    let (mut fonts, _) = registry();
    fonts
        .add_fallback(
            include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf").to_vec(),
            0,
        )
        .unwrap();
    let mut font = FontSpec::new("Liberation Sans", 24).with_size_centipoints(1105);
    font.scale_pct = 125;
    font.letter_spacing = 17;
    font.caps = Caps::Small;
    let run = Run {
        text: "\u{6c49}0a".into(),
        font: font.clone(),
        color: Color::rgb(12, 34, 56),
        placeholders: vec![],
        rise: 0,
        rise_fine: None,
        hidden: false,
    };
    let para = Para {
        runs: vec![run],
        line_rule: LineRule::Exact,
        line_value: 480,
        ..Para::default()
    };
    let metrics = RealMetrics::new(&fonts);
    let pages = Engine::new(
        &metrics,
        PageSetup {
            size: Size::new(6000, 6000),
            margins: Margins::uniform(0),
        },
    )
    .layout(&[para]);
    let paint = paint_document(&pages, Some(&fonts), &fonts.face_ids());
    let glyphs: Vec<_> = paint
        .pages
        .iter()
        .flat_map(|p| &p.cmds)
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .collect();
    assert_ne!(glyphs[0].face, glyphs[1].face);
    assert!(glyphs[2].size_centipoints < font.effective_size_centipoints());
    let html = render_outlined_html(&paint, &fonts, "real fallback").unwrap();
    let paths = outlined_paths(&html);
    assert_eq!(paths.len(), glyphs.len());
    for (path, glyph) in paths.iter().zip(glyphs) {
        assert_geometry(path, glyph, &font, &fonts);
    }
    assert!(html.contains("fill=\"#0c2238\""));
}

#[test]
fn required_ligature_glyph_is_not_replaced_by_original_unicode_text() {
    let mut fonts = FontRegistry::new();
    let face = fonts
        .add(
            include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf").to_vec(),
            0,
        )
        .unwrap();
    let font = FontSpec::new("DejaVu Sans", 24);
    let shaped = fonts.shape("\u{644}\u{627}", &font);
    assert_eq!(shaped.len(), 1);
    assert_eq!(shaped[0].source, Some((0, 2)));
    let mut g = glyph(&fonts, &face, 'A');
    g.glyph_id = shaped[0].glyph_id;
    g.source = Some((0, 2));
    let actual = render_outlined_html(
        &list(vec![command(
            vec![g.clone()],
            "\u{644}\u{627}",
            font.clone(),
        )]),
        &fonts,
        "ligature",
    )
    .unwrap();
    let other_text = render_outlined_html(
        &list(vec![command(vec![g], "unrelated text", font)]),
        &fonts,
        "ligature",
    )
    .unwrap();
    assert_eq!(actual, other_text);
    assert_eq!(outlined_paths(&actual).len(), 1);
}

#[test]
fn registered_space_is_empty_but_missing_glyphs_and_bad_ids_are_errors() {
    let (fonts, face) = registry();
    let font = FontSpec::new("unused", 24);
    let space = glyph(&fonts, &face, ' ');
    let html = render_outlined_html(
        &list(vec![command(vec![space.clone()], " ", font.clone())]),
        &fonts,
        "space",
    )
    .unwrap();
    assert_eq!(outlined_paths(&html).len(), 1);
    assert_eq!(attr(outlined_paths(&html)[0], "d"), "");
    let empty = render_outlined_html(
        &list(vec![command(vec![], "", font.clone())]),
        &fonts,
        "empty",
    )
    .unwrap();
    assert!(outlined_paths(&empty).is_empty());
    let err = render_outlined_html(
        &list(vec![command(vec![], " ", font.clone())]),
        &fonts,
        "missing",
    )
    .unwrap_err();
    assert!(matches!(err, OutlineError::MissingPositionedGlyphs { .. }));
    let mut bad = space.clone();
    bad.glyph_id = u32::MAX;
    assert!(matches!(
        render_outlined_html(
            &list(vec![command(vec![bad], "x", font.clone())]),
            &fonts,
            "bad"
        ),
        Err(OutlineError::InvalidGlyphId { .. })
    ));
    let mut missing = space.clone();
    missing.face = "unregistered".into();
    assert!(matches!(
        render_outlined_html(
            &list(vec![command(vec![missing], "x", font.clone())]),
            &fonts,
            "bad"
        ),
        Err(OutlineError::MissingFace { .. })
    ));
    let mut nan = space;
    nan.x_pt = f64::NAN;
    assert!(matches!(
        render_outlined_html(&list(vec![command(vec![nan], "x", font)]), &fonts, "bad"),
        Err(OutlineError::InvalidGlyphGeometry { .. })
    ));
}

fn table_record(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    (0..count)
        .map(|i| 12 + i * 16)
        .find(|&at| &bytes[at..at + 4] == tag)
        .unwrap()
}

#[test]
fn unusable_outline_tables_do_not_turn_nonempty_glyphs_into_silent_spaces() {
    let mut bytes = SANS.to_vec();
    let at = table_record(&bytes, b"glyf");
    bytes[at..at + 4].copy_from_slice(b"xxxx");
    let mut fonts = FontRegistry::new();
    let face = fonts.add(bytes, 0).unwrap();
    let g = glyph(&fonts, &face, 'A');
    let result = render_outlined_html(
        &list(vec![command(vec![g], "A", FontSpec::new("unused", 24))]),
        &fonts,
        "unsupported",
    );
    assert!(matches!(
        result,
        Err(OutlineError::UnsupportedOutlines { .. })
    ));
}

fn table_start(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let at = table_record(bytes, tag) + 8;
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

#[test]
fn invalid_units_per_em_and_corrupt_glyph_ranges_are_reported() {
    for upem in [0u16, 15, 16385] {
        let mut bytes = SANS.to_vec();
        let at = table_start(&bytes, b"head") + 18;
        bytes[at..at + 2].copy_from_slice(&upem.to_be_bytes());
        let mut fonts = FontRegistry::new();
        let face = fonts.add(bytes, 0).unwrap();
        let result = render_outlined_html(
            &list(vec![command(
                vec![glyph(&fonts, &face, 'A')],
                "A",
                FontSpec::new("unused", 24),
            )]),
            &fonts,
            "bad upem",
        );
        assert!(
            matches!(result, Err(OutlineError::InvalidFont { .. })),
            "{result:?}"
        );
    }
    let mut bytes = SANS.to_vec();
    let gid = FontRef::new(&bytes)
        .unwrap()
        .charmap()
        .map('A')
        .unwrap()
        .to_u32() as usize;
    let loca = table_start(&bytes, b"loca");
    let head = table_start(&bytes, b"head");
    if i16::from_be_bytes(bytes[head + 50..head + 52].try_into().unwrap()) == 0 {
        let at = loca + gid * 2;
        bytes[at..at + 2].copy_from_slice(&u16::MAX.to_be_bytes());
    } else {
        let at = loca + gid * 4;
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    }
    let mut fonts = FontRegistry::new();
    let face = fonts.add(bytes, 0).unwrap();
    let result = render_outlined_html(
        &list(vec![command(
            vec![glyph(&fonts, &face, 'A')],
            "A",
            FontSpec::new("unused", 24),
        )]),
        &fonts,
        "bad glyph",
    );
    assert!(
        matches!(result, Err(OutlineError::GlyphOutline { .. })),
        "{result:?}"
    );
}

#[test]
fn outlined_glyphs_preserve_nested_state_color_and_alpha() {
    let (fonts, face) = registry();
    let font = FontSpec::new("unused", 24);
    let make = |x| {
        let mut g = glyph(&fonts, &face, 'A');
        g.x_pt = x;
        let mut cmd = command(vec![g], "A", font.clone());
        if let DrawCmd::DrawGlyphs { paint, .. } = &mut cmd {
            paint.color = Color::rgb(12, 34, 56);
            paint.alpha = 0.375;
        }
        cmd
    };
    let html = render_outlined_html(
        &list(vec![
            DrawCmd::Save,
            DrawCmd::Transform(Transform {
                e: 400.0,
                ..Transform::default()
            }),
            DrawCmd::Save,
            DrawCmd::Clip {
                path: Path::rect(Rect::new(0, 0, 2000, 2000)),
                rule: FillRule::EvenOdd,
            },
            make(10.0),
            DrawCmd::Restore,
            make(20.0),
            DrawCmd::Restore,
            make(30.0),
        ]),
        &fonts,
        "state",
    )
    .unwrap();
    let paths = outlined_paths(&html);
    assert_eq!(paths.len(), 3);
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(attr(path, "fill"), "#0c2238");
        assert_eq!(attr(path, "fill-opacity"), "0.375");
        assert_eq!(matrix(path)[4], 10.0 + index as f64 * 10.0);
        let prefix = &html[..html.find(path).unwrap()];
        let depth = prefix.matches("<g").count() - prefix.matches("</g>").count();
        assert_eq!(depth, [4, 2, 0][index], "{html}");
    }
}

fn collection(fonts: &[&[u8]]) -> Vec<u8> {
    // Minimal parser fixture: table offsets are relative to the TTC, while
    // offsets inside individual tables retain their original bases.
    let mut out = b"ttcf\0\x01\0\0".to_vec();
    out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    out.resize(12 + fonts.len() * 4, 0);
    for (index, font) in fonts.iter().enumerate() {
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        let base = out.len() as u32;
        out[12 + index * 4..16 + index * 4].copy_from_slice(&base.to_be_bytes());
        let mut face = font.to_vec();
        let count = u16::from_be_bytes(face[4..6].try_into().unwrap()) as usize;
        for record in 0..count {
            let at = 12 + record * 16 + 8;
            let offset = u32::from_be_bytes(face[at..at + 4].try_into().unwrap());
            face[at..at + 4].copy_from_slice(&(offset + base).to_be_bytes());
        }
        out.extend_from_slice(&face);
    }
    out
}

#[test]
fn collection_face_index_selects_the_same_outline_as_the_standalone_font() {
    let mut fonts = FontRegistry::new();
    let ttc = collection(&[SANS, SERIF]);
    let sans = fonts.add(ttc.clone(), 0).unwrap();
    let serif = fonts.add(ttc, 1).unwrap();
    let standalone = fonts.add(SERIF.to_vec(), 0).unwrap();
    let render = |face: &str| {
        render_outlined_html(
            &list(vec![command(
                vec![glyph(&fonts, face, 'a')],
                "a",
                FontSpec::new("unused", 24),
            )]),
            &fonts,
            "TTC",
        )
        .unwrap()
    };
    let a = render(&sans);
    let b = render(&serif);
    let c = render(&standalone);
    assert_eq!(
        attr(outlined_paths(&b)[0], "d"),
        attr(outlined_paths(&c)[0], "d")
    );
    assert_ne!(
        attr(outlined_paths(&a)[0], "d"),
        attr(outlined_paths(&b)[0], "d")
    );
}
