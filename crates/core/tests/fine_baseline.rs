//! Fine baseline transport, using synthetic metrics rather than Word formulas.

use std::cell::Cell;

use rsword_layout_core::{
    BreakOpportunity, Color, Engine, FontMetrics, FontSpec, Fragment, LayoutRecord, LineRule,
    Margins, Page, PageSetup, Para, ParagraphMarkProperties, PlaceholderKind, Run, SimpleMetrics,
    Size, TextFragment, TextMetrics, TraceMeta, paint_document, to_trace_json,
};
use serde_json::json;

const HEIGHT_FINE: i64 = 978;

fn measured(text: &str) -> TextMetrics {
    TextMetrics {
        advance: text.chars().count() as i32 * 100,
        ascent: 136,
        descent: 34,
        line_gap: 25,
    }
}

struct FineMetrics {
    fraction: i64,
    quantize: bool,
}

impl FontMetrics for FineMetrics {
    fn measure(&self, text: &str, _: &FontSpec) -> TextMetrics {
        measured(text)
    }

    fn empty_line_metrics(&self, _: &FontSpec) -> TextMetrics {
        TextMetrics {
            ascent: 141,
            line_gap: 20,
            ..measured("")
        }
    }

    fn ascent_fine(&self, text: &str, font: &FontSpec, measured: &TextMetrics) -> i64 {
        assert!(!text.contains(['\t', '\u{fffc}']));
        let extra = if font.family == "tall" || text.contains('Z') {
            224
        } else {
            0
        };
        i64::from(measured.ascent) * 5 + self.fraction + extra
    }

    fn natural_height_fine(&self, _: &str, _: &FontSpec) -> i64 {
        HEIGHT_FINE
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }

    fn quantize_baseline_fine(&self, y: i64) -> i64 {
        if self.quantize { (y + 12) / 24 * 24 } else { y }
    }
}

fn run(text: &str, family: &str) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new(family, 24),
        color: Color::BLACK,
        placeholders: text
            .matches('\u{fffc}')
            .map(|_| PlaceholderKind::LineBreak)
            .collect(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn para(text: &str) -> Para {
    Para {
        runs: vec![run(text, "body")],
        ..Para::default()
    }
}

fn setup(width: i32, height: i32) -> PageSetup {
    PageSetup {
        size: Size::new(width, height),
        margins: Margins::uniform(0),
    }
}

fn layout(paras: &[Para], width: i32) -> Vec<Page> {
    Engine::new(
        &FineMetrics {
            fraction: 3,
            quantize: false,
        },
        setup(width, 5000),
    )
    .layout(paras)
}

fn record(pages: &[Page]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
}

fn sources(pages: &[Page]) -> Vec<(usize, u32, u32)> {
    record(pages)
        .pages
        .iter()
        .flat_map(|page| {
            page.lines.iter().map(move |line| {
                let source = line.source.unwrap();
                (page.index, source.start, source.end)
            })
        })
        .collect()
}

fn offsets(pages: &[Page]) -> Vec<i64> {
    pages
        .iter()
        .flat_map(|page| &page.line_placements)
        .map(|placement| placement.unwrap().baseline_offset_fine)
        .collect()
}

fn fragment(pages: &[Page], source: (u32, u32)) -> &TextFragment {
    pages
        .iter()
        .flat_map(|page| &page.fragments)
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.source == Some(source) => Some(text),
            _ => None,
        })
        .expect("fragment with the requested source interval")
}

#[test]
fn fractional_origin_and_offset_are_added_before_quantization_and_trace_export() {
    let metrics = FineMetrics {
        fraction: 7,
        quantize: true,
    };
    let setup = PageSetup {
        margins: Margins::uniform(1),
        ..setup(1000, 5000)
    };
    let pages = Engine::new(&metrics, setup).layout(&[para("a"), para("b"), para("c"), para("d")]);
    assert_eq!(
        sources(&pages),
        [(0, 0, 2), (0, 2, 4), (0, 4, 6), (0, 6, 8)]
    );
    let placements: Vec<_> = pages[0]
        .line_placements
        .iter()
        .map(|p| p.unwrap())
        .collect();
    assert_eq!(
        placements.iter().map(|p| p.top_fine).collect::<Vec<_>>(),
        [5, 983, 1961, 2939]
    );
    assert_eq!(
        placements
            .iter()
            .map(|p| p.baseline_fine)
            .collect::<Vec<_>>(),
        [696, 1680, 2640, 3624]
    );
    assert_eq!(offsets(&pages), [687; 4]);
    assert_eq!(fragment(&pages, (2, 4)).baseline_fine, 1680);
    assert_eq!(fragment(&pages, (6, 8)).baseline_fine, 3624);
    // The legacy coarse ascent puts the second baseline on the preceding row:
    // 1663 rounds to 1656, whereas the fine sum 1670 rounds to 1680.
    assert_eq!(metrics.quantize_baseline_fine(983 + 680), 1656);
    // Separately quantizing the third origin and offset gives 1968 + 696,
    // whereas their sum 2648 must round once to 2640.
    assert_eq!(
        metrics.quantize_baseline_fine(1961) + metrics.quantize_baseline_fine(687),
        2664
    );
    let trace: serde_json::Value =
        serde_json::from_str(&to_trace_json(&record(&pages), &TraceMeta::default())).unwrap();
    assert_eq!(
        trace["pages"][0]["lines"][3]["engineVerticalDiagnostic"],
        json!({
            "unit": "1/7200in", "top": 2939, "advance": 978, "required": 978,
            "baselineOffset": 687, "baseline": 3624,
        })
    );
}

#[test]
fn automatic_wrap_and_final_line_keep_fractional_ascent() {
    let pages = layout(&[para("AAAAA")], 250);
    assert_eq!(sources(&pages), [(0, 0, 2), (0, 2, 4), (0, 4, 6)]);
    assert_eq!(offsets(&pages), [683; 3]);
}

#[test]
fn soft_breaks_cover_empty_lines_and_a_control_absorbed_after_wrapping() {
    let pages = layout(&[para("\u{fffc}A\u{fffc}")], 600);
    assert_eq!(sources(&pages), [(0, 0, 1), (0, 1, 3), (0, 3, 4)]);
    assert_eq!(offsets(&pages), [708, 683, 708]);

    let pages = layout(&[para("AB \u{fffc}")], 200);
    assert_eq!(sources(&pages), [(0, 0, 4), (0, 4, 5)]);
    assert_eq!(offsets(&pages), [683, 708]);
}

#[test]
fn hard_page_and_column_breaks_use_empty_metrics_then_reset_for_body_text() {
    for kind in [PlaceholderKind::PageBreak, PlaceholderKind::ColumnBreak] {
        let mut input = para("\u{fffc}A");
        input.runs[0].placeholders = vec![kind];
        let pages = layout(&[input], 600);
        assert_eq!(sources(&pages), [(0, 0, 1), (1, 1, 3)]);
        assert_eq!(offsets(&pages), [708, 683]);
    }
}

#[test]
fn empty_and_fully_hidden_paragraphs_keep_their_source_and_empty_metrics() {
    let mut hidden = para("hidden");
    hidden.runs[0].hidden = true;
    let pages = layout(&[para(""), hidden], 600);
    assert_eq!(sources(&pages), [(0, 0, 1), (0, 1, 8)]);
    assert_eq!(offsets(&pages), [708, 708]);
}

#[test]
fn retreat_recomputes_ascent_after_removing_a_run_or_truncating_a_piece() {
    for runs in [
        vec![run("a ", "body"), run("bc", "tall"), run("defg", "body")],
        vec![run("a Z", "body"), run("bcdef", "body")],
    ] {
        let pages = layout(
            &[Para {
                runs,
                ..Para::default()
            }],
            600,
        );
        // The tall contribution initially fits, but belongs to a word that must
        // move in full. Both font-based and text-based contributions must go.
        assert_eq!(sources(&pages), [(0, 0, 2), (0, 2, 9)]);
        assert_eq!(offsets(&pages), [683, 907]);
    }
}

#[test]
fn closing_a_tall_line_clears_its_fine_ascent_before_the_next_line() {
    let input = Para {
        runs: vec![run("A\u{fffc}", "tall"), run("b", "body")],
        ..Para::default()
    };
    let pages = layout(&[input], 600);
    assert_eq!(sources(&pages), [(0, 0, 2), (0, 2, 4)]);
    assert_eq!(offsets(&pages), [907, 683]);
}

#[test]
fn tab_uses_space_metrics_for_its_fine_ascent() {
    let pages = layout(&[para("\t")], 1000);
    assert_eq!(sources(&pages), [(0, 0, 2)]);
    assert_eq!(offsets(&pages), [683]);
}

#[test]
fn mark_rise_is_independent_of_the_fine_line_baseline_and_exact_height() {
    let mut input = para("a\u{fffc}");
    input.runs[0].placeholders = vec![PlaceholderKind::PageBreak];
    input.runs[0].rise_fine = Some(7);
    input.line_rule = LineRule::Exact;
    input.line_value = 100;
    input.mark = ParagraphMarkProperties::from_json(
        None,
        Some(&json!({
            "fonts": {"ascii": "tall", "hAnsi": "tall", "eastAsia": "tall", "cs": "tall"},
            "size": 60, "position": 3,
        })),
    );
    let pages = layout(&[input], 600);
    assert_eq!(sources(&pages), [(0, 0, 3)]);
    let placement = pages[0].line_placements[0].unwrap();
    assert_eq!(
        (placement.advance_fine, placement.required_fine),
        (500, 500)
    );
    assert_eq!(placement.baseline_offset_fine, 683);
    let body_and_break = fragment(&pages, (0, 2));
    let mark = fragment(&pages, (2, 3));
    assert_eq!(body_and_break.baseline_fine, 676);
    assert_eq!(mark.rise_fine, 150);
    assert_eq!(mark.baseline_fine, 533);
}

#[test]
fn changing_only_fine_ascent_preserves_height_page_breaks_and_source_intervals() {
    for rule in [LineRule::Auto, LineRule::AtLeast, LineRule::Exact] {
        let input: Vec<_> = (0..8)
            .map(|_| Para {
                line_rule: rule,
                line_value: 240,
                ..para("A")
            })
            .collect();
        let coarse = FineMetrics {
            fraction: 0,
            quantize: false,
        };
        let fine = FineMetrics {
            fraction: 3,
            quantize: false,
        };
        let before = Engine::new(&coarse, setup(600, 600)).layout(&input);
        let after = Engine::new(&fine, setup(600, 600)).layout(&input);
        assert!(before.len() > 1);
        assert_eq!(sources(&after), sources(&before));
        assert_eq!(after.len(), before.len());
        for (old_page, new_page) in before.iter().zip(&after) {
            assert_eq!(
                old_page.line_placements.len(),
                new_page.line_placements.len()
            );
            for (old, new) in old_page
                .line_placements
                .iter()
                .zip(&new_page.line_placements)
            {
                let (old, new) = (old.unwrap(), new.unwrap());
                assert_eq!(
                    (new.top_fine, new.advance_fine, new.required_fine),
                    (old.top_fine, old.advance_fine, old.required_fine)
                );
                assert_eq!(new.baseline_fine, old.baseline_fine + 3);
            }
        }
    }
}

struct CountingDefaultAscent(Cell<usize>);

impl FontMetrics for CountingDefaultAscent {
    fn measure(&self, text: &str, _: &FontSpec) -> TextMetrics {
        self.0.set(self.0.get() + 1);
        measured(text)
    }

    fn empty_line_metrics(&self, _: &FontSpec) -> TextMetrics {
        TextMetrics {
            ascent: 141,
            line_gap: 20,
            ..measured("")
        }
    }

    fn natural_height_fine(&self, _: &str, _: &FontSpec) -> i64 {
        HEIGHT_FINE
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }
}

#[test]
fn default_ascent_reuses_the_provided_empty_metrics_without_measuring_again() {
    let metrics = CountingDefaultAscent(Cell::new(0));
    let pages = Engine::new(&metrics, setup(600, 600)).layout(&[para("")]);
    assert_eq!(offsets(&pages), [705]);
    assert_eq!(fragment(&pages, (0, 1)).baseline_fine, 705);
    assert_eq!(
        metrics.0.get(),
        0,
        "empty_line_metrics already provided the ascent"
    );
}
