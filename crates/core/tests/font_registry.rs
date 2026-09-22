#![cfg(feature = "fontenv")]

use rsword_layout_core::{
    Color, DrawCmd, Engine, FontMetrics, FontSpec, PageSetup, Para, RealMetrics, Run,
    font::{FontRegistry, GlyphKey, Rasterizer},
    paint_document,
};

const REGULAR: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Bold.ttf");
const SANS: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf");
const CJK: &[u8] = include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf");
const DEJAVU: &[u8] = include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf");

#[test]
fn full_and_postscript_names_select_the_named_face() {
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    registry.add(REGULAR.to_vec(), 0).unwrap();
    let bold = registry.add(BOLD.to_vec(), 0).unwrap();

    // A face name already identifies the weight, even if w:b is absent.
    for name in [
        "Liberation Serif Bold",
        "LiberationSerif-Bold",
        "  LIBERATION SERIF BOLD ",
    ] {
        assert!(registry.covers_family(name), "{name}");
        assert_eq!(
            registry.select_face(name, 'A', false, false),
            Some(bold.clone()),
            "{name}"
        );
    }
}

#[test]
fn family_names_keep_weight_and_italic_selection() {
    let mut registry = FontRegistry::new();
    let bold = registry.add(BOLD.to_vec(), 0).unwrap();
    let regular = registry.add(REGULAR.to_vec(), 0).unwrap();
    // Reuse a committed font and change only its style metadata to exercise
    // family selection without depending on installed italic fonts.
    let mut italic_bytes = REGULAR.to_vec();
    let os2 = table_offset(&italic_bytes, b"OS/2");
    let flags = read_u16(&italic_bytes, os2 + 62);
    italic_bytes[os2 + 62..os2 + 64].copy_from_slice(&((flags | 1) & !64).to_be_bytes());
    let head = table_offset(&italic_bytes, b"head");
    let flags = read_u16(&italic_bytes, head + 44);
    italic_bytes[head + 44..head + 46].copy_from_slice(&(flags | 2).to_be_bytes());
    let italic = registry.add(italic_bytes, 0).unwrap();

    assert_eq!(
        registry.select_face("Liberation Serif", 'A', false, false),
        Some(regular)
    );
    assert_eq!(
        registry.select_face("Liberation Serif", 'A', true, false),
        Some(bold)
    );
    assert_eq!(
        registry.select_face("Liberation Serif", 'A', false, true),
        Some(italic)
    );
}

#[test]
fn require_checks_registered_names_without_fallback_or_suffix_guessing() {
    let mut registry = FontRegistry::new();
    registry.add(REGULAR.to_vec(), 0).unwrap();

    for absent in [
        "Times New Roman",
        "Liberation Serif Book",
        "Liberation Serif Bold",
        "",
    ] {
        assert!(registry.select_face(absent, 'A', false, false).is_some());
        assert!(
            !registry.covers_family(absent),
            "fallback must not pass the name check: {absent}"
        );
    }
    assert!(!FontRegistry::new().covers_family("Liberation Serif"));
}

#[test]
fn a_registered_face_name_still_falls_back_for_missing_glyphs() {
    let mut registry = FontRegistry::new();
    registry.add(BOLD.to_vec(), 0).unwrap();
    let cjk = registry.add(CJK.to_vec(), 0).unwrap();

    assert!(registry.covers_family("Liberation Serif Bold"));
    assert_eq!(
        registry.select_face("Liberation Serif Bold", '\u{4e2d}', false, false),
        Some(cjk)
    );
    assert_eq!(
        registry.select_face("Liberation Serif Bold", '\u{10ffff}', false, false),
        None
    );
}

#[test]
fn face_selection_is_independent_of_registration_order() {
    let mut forward = FontRegistry::new();
    let mut reverse = FontRegistry::new();
    for bytes in [REGULAR, BOLD, SANS] {
        forward.add(bytes.to_vec(), 0).unwrap();
    }
    for bytes in [SANS, BOLD, REGULAR] {
        reverse.add(bytes.to_vec(), 0).unwrap();
    }
    for name in [
        "Liberation Serif",
        "Liberation Serif Bold",
        "LiberationSerif-Bold",
        "missing",
    ] {
        for bold in [false, true] {
            assert_eq!(
                forward.select_face(name, 'A', bold, false),
                reverse.select_face(name, 'A', bold, false)
            );
        }
    }
    assert_eq!(forward.fingerprint(), reverse.fingerprint());
}

#[test]
fn collection_faces_keep_distinct_data_shaping_and_raster_identities() {
    let bytes = collection(&[REGULAR, BOLD]);
    let mut registry = FontRegistry::new();
    let regular = registry.add(bytes.clone(), 0).unwrap();
    let bold = registry.add(bytes.clone(), 1).unwrap();
    assert_ne!(regular, bold);
    assert_eq!(registry.face_data(&regular).unwrap().1, 0);
    assert_eq!(registry.face_data(&bold).unwrap().1, 1);
    assert!(registry.rasterizer_mut().has_face(&regular));
    assert!(registry.rasterizer_mut().has_face(&bold));
    assert_eq!(
        registry.select_face("Liberation Serif", 'A', false, false),
        Some(regular.clone())
    );
    assert_eq!(
        registry.select_face("Liberation Serif Bold", 'A', false, false),
        Some(bold.clone())
    );

    let shaped_regular = registry.shape_text("A", &FontSpec::new("Liberation Serif", 24));
    let shaped_bold = registry.shape_text("A", &FontSpec::new("Liberation Serif Bold", 24));
    assert_eq!(registry.face_ids()[shaped_regular[0].face_index], regular);
    assert_eq!(registry.face_ids()[shaped_bold[0].face_index], bold);
    assert_ne!(shaped_regular[0].face_index, shaped_bold[0].face_index);
    let regular_glyph = registry
        .rasterizer_mut()
        .rasterize(&GlyphKey::new(&regular, shaped_regular[0].glyph_id, 24))
        .unwrap();
    let bold_glyph = registry
        .rasterizer_mut()
        .rasterize(&GlyphKey::new(&bold, shaped_bold[0].glyph_id, 24))
        .unwrap();
    assert_ne!(regular_glyph.coverage, bold_glyph.coverage);

    assert_eq!(registry.add(bytes, 1).unwrap(), bold);
    assert_eq!(
        registry.face_ids().len(),
        2,
        "registering the same face twice is idempotent"
    );
}

#[test]
fn nonfirst_collection_face_survives_real_metrics_and_paint() {
    let bytes = collection(&[REGULAR, DEJAVU]);
    let mut registry = FontRegistry::new();
    let first = registry.add(bytes.clone(), 0).unwrap();
    let second = registry.add(bytes, 1).unwrap();
    let mut standalone = FontRegistry::new();
    standalone.add(DEJAVU.to_vec(), 0).unwrap();
    let font = FontSpec::new("DejaVu Sans", 24);

    let metrics = RealMetrics::new(&registry);
    let actual = metrics.measure("A", &font);
    let expected = RealMetrics::new(&standalone).measure("A", &font);
    let wrong = metrics.measure("A", &FontSpec::new("Liberation Serif", 24));
    // Same-family regular/bold faces may share vertical metrics, hiding index 0 reuse.
    assert_ne!(
        (actual.ascent, actual.descent, actual.line_gap),
        (wrong.ascent, wrong.descent, wrong.line_gap)
    );
    assert_eq!(
        (
            actual.advance,
            actual.ascent,
            actual.descent,
            actual.line_gap
        ),
        (
            expected.advance,
            expected.ascent,
            expected.descent,
            expected.line_gap
        )
    );

    let paint = |fonts: &FontRegistry| {
        let para = Para {
            runs: vec![Run {
                text: "A".into(),
                font: font.clone(),
                color: Color::BLACK,
                placeholders: Vec::new(),
                rise: 0,
            }],
            ..Para::default()
        };
        let real = RealMetrics::new(fonts);
        let pages = Engine::new(&real, PageSetup::a4()).layout(&[para]);
        paint_document(&pages, Some(fonts), &fonts.face_ids())
            .pages
            .into_iter()
            .flat_map(|page| page.cmds)
            .filter_map(|cmd| match cmd {
                DrawCmd::DrawGlyphs { glyphs, .. } => Some(glyphs),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>()
    };
    let glyphs = paint(&registry);
    let control = paint(&standalone);
    assert_eq!(
        glyphs.len(),
        2,
        "body glyph and paragraph mark must both be painted"
    );
    assert_eq!(glyphs.len(), control.len());
    for (glyph, expected) in glyphs.iter().zip(&control) {
        assert_eq!(glyph.face, second);
        assert_ne!(glyph.face, first);
        assert_eq!(
            (
                glyph.glyph_id,
                glyph.x_pt,
                glyph.y_fine,
                glyph.advance_x_pt,
                glyph.source
            ),
            (
                expected.glyph_id,
                expected.x_pt,
                expected.y_fine,
                expected.advance_x_pt,
                expected.source
            )
        );
        assert!(
            registry
                .rasterizer_mut()
                .rasterize(&GlyphKey::new(
                    &glyph.face,
                    glyph.glyph_id,
                    glyph.size_half_points
                ))
                .is_some()
        );
    }
}

#[test]
fn source_spans_remain_relative_to_the_whole_text_across_faces() {
    let mut registry = FontRegistry::new();
    registry.add(REGULAR.to_vec(), 0).unwrap();
    registry.add(CJK.to_vec(), 0).unwrap();
    let font = FontSpec::new("Liberation Serif", 24);
    let shaped = registry.shape_text("A\u{4e2d}B", &font);
    assert_eq!(
        shaped.iter().map(|run| run.source).collect::<Vec<_>>(),
        vec![Some((0, 1)), Some((1, 2)), Some((2, 3))]
    );
    assert_ne!(shaped[0].face_index, shaped[1].face_index);
    assert_eq!(shaped[0].face_index, shaped[2].face_index);

    // An unsupported supplementary scalar is skipped, but still owns two UTF-16 units.
    let shaped = registry.shape_text("A\u{10ffff}B", &font);
    assert_eq!(
        shaped.iter().map(|run| run.source).collect::<Vec<_>>(),
        vec![Some((0, 1)), Some((3, 4))]
    );
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
    (0..usize::from(read_u16(bytes, 4)))
        .map(|index| 12 + index * 16)
        .find(|offset| &bytes[*offset..*offset + 4] == tag)
        .map(|offset| read_u32(bytes, offset + 8) as usize)
        .expect("fixture contains the requested table")
}

// TTC table offsets are relative to the collection, not each embedded sfnt.
fn collection(fonts: &[&[u8]]) -> Vec<u8> {
    let mut bytes = b"ttcf\x00\x01\x00\x00".to_vec();
    bytes.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    bytes.resize(12 + 4 * fonts.len(), 0);
    for (index, font) in fonts.iter().enumerate() {
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let start = bytes.len();
        bytes[12 + 4 * index..16 + 4 * index].copy_from_slice(&(start as u32).to_be_bytes());
        let mut sfnt = font.to_vec();
        for table in 0..usize::from(read_u16(&sfnt, 4)) {
            let at = 12 + table * 16 + 8;
            let absolute = read_u32(&sfnt, at) + start as u32;
            sfnt[at..at + 4].copy_from_slice(&absolute.to_be_bytes());
        }
        bytes.extend_from_slice(&sfnt);
    }
    bytes
}
