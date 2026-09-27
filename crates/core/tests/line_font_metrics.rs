//! Synthetic provider contracts, independent of native Word metrics or fonts.

use std::cell::RefCell;

use rsword_layout_core::{
    BreakOpportunity, Color, Engine, FontMetrics, FontSpec, LayoutRecord, LinePlacement, Margins,
    MeasuredFontSpan, Page, PageSetup, Para, Run, SimpleMetrics, Size, TextMetrics, paint_document,
};

fn run(text: &str, family: &str) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new(family, 24),
        color: Color::BLACK,
        placeholders: vec![],
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn setup(width: i32) -> PageSetup {
    PageSetup {
        size: Size::new(width, 20_000),
        margins: Margins::uniform(0),
    }
}

fn sources(pages: &[Page]) -> Vec<(u32, u32)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|page| &page.lines)
        .map(|line| {
            let source = line.source.expect("synthetic line source");
            (source.start, source.end)
        })
        .collect()
}

fn placements(pages: &[Page]) -> Vec<LinePlacement> {
    pages
        .iter()
        .flat_map(|page| &page.line_placements)
        .map(|value| value.expect("engine line placement"))
        .collect()
}

struct SuppliedMetrics;

impl FontMetrics for SuppliedMetrics {
    fn measure(&self, _: &str, _: &FontSpec) -> TextMetrics {
        panic!("the supplied span metrics must be reused")
    }

    fn break_opportunities(&self, _: &str) -> Vec<BreakOpportunity> {
        unreachable!()
    }

    fn natural_height_fine(&self, text: &str, _: &FontSpec) -> i64 {
        match text {
            "a" => 83,
            "b" => 159,
            "c" => 87,
            _ => unreachable!(),
        }
    }

    fn ascent_fine(&self, text: &str, _: &FontSpec, measured: &TextMetrics) -> i64 {
        i64::from(measured.ascent) * 5 + if text == "b" { 301 } else { 2 }
    }
}

#[test]
fn default_line_metrics_keep_independent_legacy_and_fine_contracts_through_dyn() {
    let font = FontSpec::new("synthetic", 24);
    let spans = [("a", 11, 2, 3), ("b", 8, 7, 0), ("c", 1, 1, 29)].map(
        |(text, ascent, descent, line_gap)| MeasuredFontSpan {
            text,
            font: &font,
            measured: TextMetrics {
                advance: 123,
                ascent,
                descent,
                line_gap,
            },
        },
    );
    let metrics: &dyn FontMetrics = &SuppliedMetrics;
    let result = metrics.line_metrics(&spans);
    assert_eq!(
        (result.ascent, result.descent, result.natural_height),
        (11, 7, 31)
    );
    assert_eq!((result.natural_height_fine, result.ascent_fine), (159, 341));
}

#[derive(Default)]
struct CachedMetrics {
    measured: RefCell<Vec<String>>,
    distinct_fit: bool,
}

impl CachedMetrics {
    fn ordinary(text: &str, font: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.chars().count() as i32 * 100,
            ascent: if font.family == "tall" || text.contains('Z') {
                60
            } else {
                10
            },
            descent: 2,
            line_gap: 0,
        }
    }

    fn calls_for(&self, text: &str) -> usize {
        self.measured
            .borrow()
            .iter()
            .filter(|value| value.as_str() == text)
            .count()
    }
}

impl FontMetrics for CachedMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        self.measured.borrow_mut().push(text.into());
        Self::ordinary(text, font)
    }

    fn fit(&self, text: &str, font: &FontSpec, width: i32) -> Option<(usize, TextMetrics)> {
        if self.distinct_fit && text == "ABCD" && width == 250 {
            return Some((
                2,
                TextMetrics {
                    advance: 200,
                    ascent: 41,
                    descent: 7,
                    line_gap: 9,
                },
            ));
        }
        self.break_opportunities(text)
            .into_iter()
            .filter(|point| point.offset > 0)
            .map(|point| (point.offset, Self::ordinary(&text[..point.offset], font)))
            .take_while(|(_, metrics)| metrics.advance <= width)
            .last()
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }

    fn advance_pt(&self, text: &str, _: &FontSpec) -> f64 {
        text.chars().count() as f64 * 5.0
    }

    fn natural_height_fine(&self, _: &str, _: &FontSpec) -> i64 {
        503
    }

    fn ascent_fine(&self, _: &str, _: &FontSpec, measured: &TextMetrics) -> i64 {
        i64::from(measured.ascent) * 5 + 3
    }
}

#[test]
fn fitted_prefix_keeps_the_metrics_returned_by_fit() {
    let metrics = CachedMetrics {
        distinct_fit: true,
        ..CachedMetrics::default()
    };
    let pages = Engine::new(&metrics, setup(250)).layout(&[Para {
        runs: vec![run("ABCD", "body")],
        ..Para::default()
    }]);
    assert_eq!(sources(&pages), [(0, 2), (2, 5)]);
    let lines = placements(&pages);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.baseline_offset_fine)
            .collect::<Vec<_>>(),
        [208, 53]
    );
    assert_eq!(
        lines
            .iter()
            .map(|line| line.advance_fine)
            .collect::<Vec<_>>(),
        [503, 503]
    );
    assert_eq!(lines[1].top_fine, 503);
    assert_eq!(
        metrics.calls_for("AB"),
        0,
        "fit already supplied the accepted prefix metrics"
    );
}

#[test]
fn rollback_drops_a_tall_run_without_remeasuring_the_retained_prefix() {
    let metrics = CachedMetrics::default();
    let pages = Engine::new(&metrics, setup(600)).layout(&[Para {
        runs: vec![run("a ", "body"), run("bc", "tall"), run("defg", "body")],
        ..Para::default()
    }]);
    assert_eq!(sources(&pages), [(0, 2), (2, 9)]);
    assert_eq!(
        placements(&pages)
            .iter()
            .map(|line| line.baseline_offset_fine)
            .collect::<Vec<_>>(),
        [53, 303],
    );
    assert_eq!(
        metrics.calls_for("a "),
        1,
        "unchanged accepted text keeps its cached metrics"
    );
}

#[test]
fn partial_rollback_replaces_only_the_truncated_piece_metrics() {
    let metrics = CachedMetrics::default();
    let pages = Engine::new(&metrics, setup(600)).layout(&[Para {
        runs: vec![run("a Z", "body"), run("bcdef", "body")],
        ..Para::default()
    }]);
    assert_eq!(sources(&pages), [(0, 2), (2, 9)]);
    assert_eq!(
        placements(&pages)
            .iter()
            .map(|line| line.baseline_offset_fine)
            .collect::<Vec<_>>(),
        [53, 303],
    );
    assert_eq!(metrics.calls_for("a Z"), 1);
    assert_eq!(
        metrics.calls_for("a "),
        1,
        "the shortened piece needs exactly one new measurement"
    );
}

struct NegativeEmpty;

impl FontMetrics for NegativeEmpty {
    fn measure(&self, _: &str, _: &FontSpec) -> TextMetrics {
        panic!("empty_line_metrics already supplied this line")
    }

    fn empty_line_metrics(&self, _: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: 0,
            ascent: -2,
            descent: 3,
            line_gap: 0,
        }
    }

    fn natural_height_fine(&self, _: &str, _: &FontSpec) -> i64 {
        7
    }

    fn ascent_fine(&self, _: &str, _: &FontSpec, measured: &TextMetrics) -> i64 {
        assert_eq!(measured.ascent, -2);
        -11
    }

    fn advance_pt(&self, _: &str, _: &FontSpec) -> f64 {
        0.0
    }

    fn break_opportunities(&self, _: &str) -> Vec<BreakOpportunity> {
        unreachable!()
    }
}

#[test]
fn empty_and_hidden_lines_preserve_signed_empty_metrics_without_max_zero() {
    let mut hidden = run("hidden", "body");
    hidden.hidden = true;
    let pages = Engine::new(&NegativeEmpty, setup(600)).layout(&[
        Para {
            runs: vec![run("", "body")],
            ..Para::default()
        },
        Para {
            runs: vec![hidden],
            ..Para::default()
        },
    ]);
    assert_eq!(sources(&pages), [(0, 1), (1, 8)]);
    let lines = placements(&pages);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.baseline_offset_fine)
            .collect::<Vec<_>>(),
        [-11, -11]
    );
    assert_eq!(
        lines
            .iter()
            .map(|line| line.advance_fine)
            .collect::<Vec<_>>(),
        [7, 7]
    );
    assert_eq!(lines[1].baseline_fine, -4);
}
