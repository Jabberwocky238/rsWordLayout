//! Exact spacing must reserve the same height that advances the page cursor.
//! These boundary cases isolate engine consistency; they are not Word captures.

use std::collections::BTreeSet;

use rsword_layout_core::{
    BreakOpportunity, Color, Engine, FontMetrics, FontSpec, Fragment, LineRule, Margins, Page,
    PageSetup, Para, PlaceholderKind, Run, SimpleMetrics, Size, TextMetrics, Twips,
};

struct TallMetrics;

impl FontMetrics for TallMetrics {
    fn measure(&self, text: &str, _font: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.chars().count() as Twips * 100,
            ascent: 240,
            descent: 60,
            line_gap: 0,
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }
}

fn exact(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.to_owned(),
            font: FontSpec::new("Tall test font", 24),
            color: Color::BLACK,
            placeholders: text
                .matches('\u{fffc}')
                .map(|_| PlaceholderKind::LineBreak)
                .collect(),
            rise: 0,
            rise_fine: None,
        }],
        line_rule: LineRule::Exact,
        line_value: 100,
        ..Para::default()
    }
}

fn layout(paras: &[Para], content_height: Twips) -> Vec<Page> {
    let setup = PageSetup {
        size: Size::new(1000, content_height + 80),
        margins: Margins::uniform(40),
    };
    Engine::new(&TallMetrics, setup).layout(paras)
}

fn lines_per_page(pages: &[Page]) -> Vec<usize> {
    pages
        .iter()
        .map(|page| {
            page.fragments
                .iter()
                .filter_map(|fragment| match fragment {
                    Fragment::Text(text) => Some(text.line),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .len()
        })
        .collect()
}

#[test]
fn exact_lines_fill_the_page_before_overflowing() {
    let pages = layout(&[exact("a"), exact("b"), exact("c"), exact("d")], 300);
    assert_eq!(lines_per_page(&pages), [3, 1]);
    let baselines: Vec<_> = pages[0]
        .fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text.baseline_y),
            _ => None,
        })
        .collect();
    assert_eq!(baselines, [280, 380, 480]);
}

#[test]
fn exact_empty_and_soft_return_lines_use_the_same_pagination_height() {
    let pages = layout(&[exact("a\u{fffc}b\u{fffc}"), exact("")], 300);
    assert_eq!(lines_per_page(&pages), [3, 1]);
}

#[test]
fn keep_lines_uses_exact_height_for_the_entire_block() {
    let block = Para {
        keep_lines: true,
        ..exact("a\u{fffc}b\u{fffc}c")
    };
    let paras = [exact("intro"), block];
    assert_eq!(
        lines_per_page(&layout(&paras, 400)),
        [4],
        "the block fits exactly"
    );
    assert_eq!(
        lines_per_page(&layout(&paras, 350)),
        [1, 3],
        "the block must move together"
    );
}

#[test]
fn keep_next_reserves_the_next_lines_exact_height() {
    let lead = Para {
        keep_next: true,
        ..exact("lead")
    };
    let paras = [exact("intro"), lead, exact("next")];
    assert_eq!(
        lines_per_page(&layout(&paras, 300)),
        [3],
        "both linked lines fit"
    );
    assert_eq!(
        lines_per_page(&layout(&paras, 250)),
        [1, 2],
        "both linked lines must move"
    );
}

#[test]
fn nonpositive_exact_spacing_keeps_the_one_twip_minimum() {
    for line_value in [0, -100] {
        let para = Para {
            line_value,
            ..exact("a")
        };
        assert_eq!(lines_per_page(&layout(&vec![para; 4], 3)), [3, 1]);
    }
}

#[test]
fn auto_and_at_least_still_reserve_the_font_content_height() {
    for line_rule in [LineRule::Auto, LineRule::AtLeast] {
        let para = Para {
            line_rule,
            line_value: 100,
            ..exact("a")
        };
        assert_eq!(lines_per_page(&layout(&vec![para; 4], 300)), [1, 1, 1, 1]);
    }
}
