//! Exact arithmetic for the current sub/superscript model.
//! The ratios remain a backtest from one Liberation Serif 12pt Mac Word capture;
//! these synthetic cases verify representation and propagation, not new Word evidence.

use rsword_layout_core::{
    BreakOpportunity, Color, DrawCmd, Engine, FontMetrics, FontSpec, Fragment, Margins, PageSetup,
    Para, PlaceholderKind, Run, ShapedRun, SimpleMetrics, Size, TextMetrics, TextShaper,
    paint_document, paras_from_document,
};
use serde_json::json;

fn bridge(props: serde_json::Value) -> Run {
    let doc = json!({"main": [{"kind": "text", "props": {}, "inlines": [
        {"kind": "run", "text": "x", "props": props}
    ]}]});
    paras_from_document(&doc).0.remove(0).runs.remove(0)
}

#[test]
fn bridge_keeps_fractional_size_and_rise_from_the_same_run_size() {
    for (hp, size_cp, rise, drop) in [
        (20, 660, 340, -80),
        (21, 693, 357, -84),
        (24, 792, 408, -96),
        (36, 1188, 612, -144),
    ] {
        for (kind, fine) in [("superscript", rise), ("subscript", drop)] {
            let run = bridge(json!({"size": hp, "vertAlign": kind}));
            assert_eq!(run.font.effective_size_centipoints(), size_cp);
            assert_eq!(run.effective_rise_fine(), fine);
            assert_eq!(run.rise_fine, Some(fine));
        }
    }
    let sup = bridge(json!({"size": 24, "vertAlign": "superscript"}));
    assert_eq!(sup.font.size_half_points, 16);
    assert_eq!(sup.rise, 81);
    assert_ne!(i64::from(sup.rise) * 5, sup.effective_rise_fine());
}

#[test]
fn kerning_threshold_uses_the_effective_size_before_legacy_rounding() {
    let below = bridge(json!({"size": 24, "vertAlign": "superscript", "kern": 16}));
    assert_eq!(below.font.size_half_points, 16);
    assert_eq!(below.font.effective_size_centipoints(), 792);
    assert!(!below.font.kerning);
    let above = bridge(json!({"size": 24, "vertAlign": "superscript", "kern": 15}));
    assert!(above.font.kerning);
}

struct GridMetrics;

impl FontMetrics for GridMetrics {
    fn measure(&self, text: &str, _: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.chars().count() as i32 * 50,
            ascent: 200,
            descent: 50,
            line_gap: 0,
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }

    fn quantize_baseline_fine(&self, y: i64) -> i64 {
        (y + 12) / 24 * 24
    }
}

struct OffsetShaper;

impl TextShaper for OffsetShaper {
    fn shape(&self, text: &str, _: &FontSpec) -> Vec<ShapedRun> {
        text.chars()
            .enumerate()
            .map(|(index, _)| ShapedRun {
                face_index: 0,
                glyph_id: 1,
                source: Some((index as u32, index as u32 + 1)),
                x_advance: 50,
                x_advance_pt: 2.5,
                x_offset: 0,
                y_offset: 3,
            })
            .collect()
    }
}

#[test]
fn rise_survives_fragments_paint_and_positioned_glyphs_after_line_quantization() {
    let base = bridge(json!({"size": 24}));
    let sup = bridge(json!({"size": 21, "vertAlign": "superscript"}));
    let sub = bridge(json!({"size": 21, "vertAlign": "subscript"}));
    let pages = Engine::new(&GridMetrics, PageSetup::a4()).layout(&[Para {
        runs: vec![base, sup, sub],
        ..Para::default()
    }]);
    let fragments: Vec<_> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    // The ordinary baseline is quantized once: round((7200 + 1000) / 24) * 24.
    assert_eq!(fragments[0].baseline_fine, 8208);
    assert_eq!(fragments[1].baseline_fine, 7851);
    assert_eq!(fragments[2].baseline_fine, 8292);
    assert_eq!(fragments[1].rise_fine, 357);
    assert_eq!(fragments[2].rise_fine, -84);
    assert_ne!(fragments[1].baseline_fine % 24, 0);

    for shaper in [None, Some(&OffsetShaper as &dyn TextShaper)] {
        let paint = paint_document(&pages, shaper, &["synthetic face".into()]);
        for (cmd, fragment) in paint.pages[0].cmds.iter().zip(fragments.iter()) {
            let DrawCmd::DrawGlyphs {
                origin_y_fine,
                glyphs,
                ..
            } = cmd
            else {
                panic!("expected a text command");
            };
            assert_eq!(*origin_y_fine, fragment.baseline_fine);
            if shaper.is_some() {
                assert!(!glyphs.is_empty());
                assert!(
                    glyphs
                        .iter()
                        .all(|g| g.y_fine == fragment.baseline_fine - 15)
                );
                assert!(
                    glyphs.iter().all(|g| {
                        g.size_centipoints == fragment.font.effective_size_centipoints()
                    })
                );
            } else {
                assert!(glyphs.is_empty());
            }
        }
    }
}

fn legacy_run(rise: i32) -> Run {
    Run {
        text: "x".into(),
        font: FontSpec::new("synthetic", 24),
        color: Color::BLACK,
        placeholders: vec![],
        rise,
        rise_fine: None,
    }
}

#[test]
fn legacy_rise_remains_a_fallback_and_an_explicit_zero_takes_precedence() {
    let mut run = legacy_run(-81);
    assert_eq!(run.effective_rise_fine(), -405);
    run.rise_fine = Some(-408);
    assert_eq!(run.effective_rise_fine(), -408);
    run.rise_fine = Some(0);
    let pages = Engine::new(&GridMetrics, PageSetup::a4()).layout(&[Para {
        runs: vec![run],
        ..Para::default()
    }]);
    let Fragment::Text(fragment) = &pages[0].fragments[0] else {
        panic!("text");
    };
    assert_eq!(fragment.baseline_fine, 8208);
    assert_eq!(fragment.rise_fine, 0);
}

#[test]
fn fine_rise_survives_wrapping_forced_fit_and_empty_lines() {
    for width in [1, 75] {
        for text in ["\u{4e2d}\u{6587}\u{4e2d}", ""] {
            let setup = PageSetup {
                size: Size::new(width, 10000),
                margins: Margins::uniform(0),
            };
            let laid_out = |rise_fine| {
                let mut run = legacy_run(0);
                run.text = text.into();
                run.rise_fine = Some(rise_fine);
                let pages = Engine::new(&GridMetrics, setup).layout(&[Para {
                    runs: vec![run],
                    ..Para::default()
                }]);
                pages
                    .into_iter()
                    .flat_map(|page| page.fragments)
                    .filter_map(|fragment| match fragment {
                        Fragment::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let flat = laid_out(0);
            let raised = laid_out(357);
            assert_eq!(flat.len(), raised.len());
            assert!(flat.len() >= if text.is_empty() { 1 } else { 3 });
            for (flat, raised) in flat.iter().zip(&raised) {
                assert_eq!(flat.baseline_fine - raised.baseline_fine, 357);
                assert_eq!(flat.source, raised.source);
                assert_eq!(flat.x_pt, raised.x_pt);
                assert_eq!(raised.rise_fine, 357);
            }
        }
    }
}

#[test]
fn control_suffixes_with_different_fine_rises_do_not_merge() {
    let mut text = legacy_run(81);
    text.rise_fine = Some(405);
    let mut control = legacy_run(81);
    control.text = "\u{fffc}".into();
    control.placeholders = vec![PlaceholderKind::LineBreak];
    control.rise_fine = Some(406);
    let pages = Engine::new(&GridMetrics, PageSetup::a4()).layout(&[Para {
        runs: vec![text, control],
        ..Para::default()
    }]);
    let fragments: Vec<_> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(fragments[0].text, "x");
    assert_eq!(fragments[0].source, Some((0, 1)));
    assert_eq!(fragments[0].rise_fine, 405);
    assert_eq!(fragments[1].text, " ");
    assert_eq!(fragments[1].source, Some((1, 2)));
    assert_eq!(fragments[1].rise_fine, 406);
    assert_eq!(fragments[0].baseline_fine - fragments[1].baseline_fine, 1);
    // The empty paragraph line after the soft return keeps its terminal style.
    assert_eq!(fragments[2].source, Some((2, 3)));
    assert_eq!(fragments[2].rise_fine, 406);
}
