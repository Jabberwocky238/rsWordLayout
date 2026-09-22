//! Real-font regressions for S-1: optional Latin ligatures and UTF-16 ownership.

#![cfg(feature = "shape")]

use rsword_layout_core::{
    Color, DrawCmd, Engine, FontSpec, PageSetup, Para, Run, RustybuzzShaper, ShapedRun,
    SimpleMetrics, paint_document,
};
use rustybuzz::{Face, UnicodeBuffer};

const DEJAVU: &[u8] = include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf");

fn shaper() -> RustybuzzShaper {
    let mut shaper = RustybuzzShaper::new();
    shaper.add_face("DejaVu Sans", DEJAVU.to_vec(), 0);
    shaper
}

fn spans(glyphs: &[ShapedRun]) -> Vec<(u32, u32)> {
    glyphs
        .iter()
        .map(|glyph| glyph.source.expect("source cluster"))
        .collect()
}

fn nominal_glyphs(text: &str) -> Vec<u32> {
    let face = Face::from_slice(DEJAVU, 0).unwrap();
    text.chars()
        .map(|ch| u32::from(face.glyph_index(ch).expect("fixture font coverage").0))
        .collect()
}

#[test]
fn latin_ligatures_are_disabled_independently_of_kerning() {
    let text = "office ffi fi fl ff ";
    let face = Face::from_slice(DEJAVU, 0).unwrap();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    assert!(
        rustybuzz::shape(&face, &[], buffer).len() < text.chars().count(),
        "the checked-in font must exercise default Latin ligature substitution"
    );

    for kerning in [false, true] {
        let actual = shaper().shape_with_face(0, text, 24, kerning);
        assert_eq!(
            actual
                .iter()
                .map(|glyph| glyph.glyph_id)
                .collect::<Vec<_>>(),
            nominal_glyphs(text),
            "Word's default Latin text must retain individual glyphs"
        );
        assert_eq!(
            spans(&actual),
            (0..text.len() as u32)
                .map(|start| (start, start + 1))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn non_bmp_clusters_use_utf16_offsets() {
    let text = "\u{1f600}fi ";
    let actual = shaper().shape_with_face(0, text, 24, false);
    assert_eq!(
        actual
            .iter()
            .map(|glyph| glyph.glyph_id)
            .collect::<Vec<_>>(),
        nominal_glyphs(text)
    );
    assert_eq!(spans(&actual), [(0, 2), (2, 3), (3, 4), (4, 5)]);
}

#[test]
fn combining_clusters_share_only_their_own_source_span() {
    // a + acute composes; x + acute remains two glyphs in one cluster.
    let actual = shaper().shape_with_face(0, "a\u{301} x\u{301} ", 24, false);
    assert_eq!(spans(&actual), [(0, 2), (2, 3), (3, 5), (3, 5), (5, 6)]);
    assert_eq!(actual[0].glyph_id, nominal_glyphs("\u{e1}")[0]);
    assert_eq!(actual.last().unwrap().glyph_id, nominal_glyphs(" ")[0]);
}

#[test]
fn rtl_required_ligatures_remain_enabled_and_keep_the_trailing_space_separate() {
    // Arabic lam-alef is a required ligature; visual order puts the space first.
    let shaper = shaper();
    let space = nominal_glyphs(" ")[0];
    for kerning in [false, true] {
        let actual = shaper.shape_with_face(0, "\u{644}\u{627} ", 24, kerning);
        assert_eq!(
            actual.len(),
            2,
            "required script shaping must remain enabled"
        );
        for glyph in &actual {
            assert_ne!(glyph.glyph_id, 0);
            let expected = if glyph.glyph_id == space {
                (2, 3)
            } else {
                (0, 2)
            };
            assert_eq!(glyph.source, Some(expected));
        }
        if kerning {
            // rustybuzz 0.20.1's simple-kern path has a separate RTL-order bug
            // with kern=0. Exercise reversed output without pinning that bug.
            assert_eq!(spans(&actual), [(2, 3), (0, 2)]);
        }
    }
}

#[test]
fn rtl_reordered_marks_share_the_ligature_cluster() {
    let actual = shaper().shape_with_face(0, "\u{644}\u{64e}\u{627} ", 24, true);
    assert_eq!(spans(&actual), [(3, 4), (0, 3), (0, 3)]);
    assert_eq!(actual[0].glyph_id, nominal_glyphs(" ")[0]);
    assert!(actual.iter().all(|glyph| glyph.glyph_id != 0));
}

fn painted_sources(text: &str) -> Vec<(u32, u32)> {
    let para = Para {
        runs: vec![Run {
            text: text.to_string(),
            font: FontSpec::new("DejaVu Sans", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
        }],
        ..Para::default()
    };
    let shaper = shaper();
    let engine = Engine::new(&SimpleMetrics, PageSetup::a4());
    paint_document(&engine.layout(&[para]), Some(&shaper), &shaper.face_ids())
        .pages
        .into_iter()
        .flat_map(|page| page.cmds)
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .map(|glyph| glyph.source.expect("positioned source cluster"))
        .collect()
}

#[test]
fn painting_preserves_non_bmp_and_combining_cluster_ownership() {
    assert_eq!(
        painted_sources("\u{1f600}a\u{301}"),
        [(0, 2), (2, 4), (4, 5)],
        "the paragraph mark must own its own UTF-16 unit after shaping"
    );
}

#[test]
fn painting_preserves_required_ligature_and_paragraph_mark_ownership() {
    let mut actual = painted_sources("\u{644}\u{627}");
    actual.sort_unstable();
    assert_eq!(actual, [(0, 2), (2, 3)]);
}
