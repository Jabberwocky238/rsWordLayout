//! Synthetic paint-placement contracts, not measured Word control geometry.

use super::*;
use crate::{BreakOpportunity, SimpleMetrics, TextMetrics};

struct ContextMetrics;

impl FontMetrics for ContextMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let mut metrics = SimpleMetrics.measure(text, font);
        metrics.advance = (self.advance_pt(text, font) * 20.0).round() as Twips;
        metrics
    }

    fn advance_pt(&self, text: &str, font: &FontSpec) -> f64 {
        match text {
            "A" => 5.0,
            "A " => 6.125,
            " " => 2.0,
            "  " => 4.5,
            "\t" => 19.0,
            _ => SimpleMetrics.advance_pt(text, font),
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }
}

fn run(text: &str, size: u32) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new("synthetic", size),
        color: Color::BLACK,
        placeholders: text
            .chars()
            .filter(|&ch| ch == '\u{fffc}')
            .map(|_| PlaceholderKind::PageBreak)
            .collect(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn placed(runs: Vec<Run>) -> Vec<TextFragment> {
    let setup = PageSetup {
        size: Size::new(10000, 10000),
        margins: Margins::uniform(0),
    };
    let engine = Engine::new(&ContextMetrics, setup);
    let para = Para {
        runs,
        ..Para::default()
    };
    let area = setup.content_area();
    let mut lines = engine.break_paragraph(&para, area, 0, 0);
    assert_eq!(lines.len(), 1);
    let line = &mut lines[0];
    assert_eq!(line.tails.len(), 2);
    assert_eq!(line.tails[0].source, (1, 2));
    assert_eq!(line.tails[1].source, (2, 3));
    // Exercise future independently styled tails without enabling mark inputs.
    line.tails[1].font = FontSpec::new("synthetic", 28);
    line.tails[1].rise_fine = -25;
    let mut page = Page::new(setup.size, area);
    engine.place_line(&mut page, line, &para, 0, 0);
    page.fragments
        .into_iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn appended_control_uses_combined_shaping_advance_before_a_new_style() {
    let fragments = placed(vec![run("A\u{fffc}", 20)]);
    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0].text, "A ");
    assert_eq!(fragments[0].source, Some((0, 2)));
    assert_eq!(fragments[1].source, Some((2, 3)));
    assert_eq!(fragments[1].x_pt, 6.125);
    assert_ne!(fragments[1].x_pt, 7.0, "isolated advances are not additive");
    assert_eq!(fragments[1].baseline_fine - fragments[0].baseline_fine, 25);
    assert_eq!(fragments[0].terminator, crate::LineTerminator::Wrapped);
    assert_eq!(
        fragments[1].terminator,
        crate::LineTerminator::PageBreak(crate::PageBreakPosition::BeforeMark)
    );
}

#[test]
fn different_tail_styles_after_tab_keep_the_tab_advance_independent() {
    let fragments = placed(vec![run("\t\u{fffc}", 20)]);
    assert_eq!(fragments.len(), 3);
    assert_eq!(fragments[0].text, "\t");
    assert_eq!(fragments[0].tab_advance_pt, Some(36.0));
    assert_eq!(fragments[1].text, " ");
    assert_eq!(fragments[1].x_pt, 36.0);
    assert_eq!(fragments[2].x_pt, 38.0);
    // The replaced tab glyph stays outside both tail shaping groups. Neither
    // the 19pt U+0009 nor the contextual width of "  " predicts their origins.
    assert_eq!(fragments[0].source, Some((0, 1)));
    assert_eq!(fragments[1].source, Some((1, 2)));
    assert_eq!(fragments[2].source, Some((2, 3)));
}

#[test]
fn separately_painted_controls_advance_before_the_next_tail_style() {
    let mut control = run("\u{fffc}", 30);
    control.rise_fine = Some(75);
    let fragments = placed(vec![run("A", 20), control]);
    assert_eq!(fragments.len(), 3);
    assert_eq!(fragments[0].text, "A");
    assert_eq!(fragments[1].source, Some((1, 2)));
    assert_eq!(fragments[2].source, Some((2, 3)));
    assert_eq!(fragments[1].x_pt, 5.0);
    assert_eq!(fragments[2].x_pt, 7.0);
    assert_eq!(fragments[2].baseline_fine - fragments[1].baseline_fine, 100);
}
