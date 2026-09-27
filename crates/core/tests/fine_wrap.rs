//! Wrap queries must use the same fractional origin and advance as placement.
//! These synthetic exclusion edges test engine consistency, not Word grid rules.

use rsword_layout_core::{
    BreakOpportunity, Color, ColumnLayout, Engine, FontMetrics, FontSpec, Fragment, LayoutRecord,
    LayoutSection, LineTerminator, Margins, Page, PageSetup, Para, PlaceholderKind, Rect, Run,
    SectionStart, Size, TextMetrics, Twips, WrapContext, WrapRegion, document_from_json,
    paint_document,
};

const HEIGHT_FINE: i64 = 978; // 195.6 twips, rounded coarse metrics: 196.

struct FractionalMetrics;

impl FontMetrics for FractionalMetrics {
    fn measure(&self, text: &str, _font: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.chars().count() as Twips * 100,
            ascent: 160,
            descent: 36,
            line_gap: 0,
        }
    }

    fn natural_height_fine(&self, _text: &str, _font: &FontSpec) -> i64 {
        HEIGHT_FINE
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        text.char_indices()
            .map(|(offset, ch)| BreakOpportunity {
                offset: offset + ch.len_utf8(),
                hyphen: false,
            })
            .collect()
    }
}

fn para(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(),
            font: FontSpec::new("fractional-test", 24),
            color: Color::BLACK,
            placeholders: vec![PlaceholderKind::LineBreak; text.matches('\u{fffc}').count()],
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        ..Para::default()
    }
}

fn layout(paras: &[Para], exclusion_bottom: Twips) -> Vec<Page> {
    let mut wrap = WrapContext::new();
    wrap.add(WrapRegion::rect(Rect::new(0, 0, 400, exclusion_bottom), 0));
    Engine::with_wrap(
        &FractionalMetrics,
        PageSetup {
            size: Size::new(800, 5000),
            margins: Margins::uniform(0),
        },
        wrap,
    )
    .layout(paras)
}

fn assert_lines(pages: &[Page], expected_sources: &[(u32, u32)], expected_x: &[Twips]) {
    assert_eq!(pages.len(), 1);
    let record = LayoutRecord::from_paint(&paint_document(pages, None, &[]));
    let sources: Vec<_> = record.pages[0]
        .lines
        .iter()
        .map(|line| {
            let source = line.source.unwrap();
            (source.start, source.end)
        })
        .collect();
    assert_eq!(sources, expected_sources);
    for (index, &x) in expected_x.iter().enumerate() {
        let text = pages[0]
            .fragments
            .iter()
            .find_map(|fragment| match fragment {
                Fragment::Text(text) if text.line == index as u32 => Some(text),
                _ => None,
            })
            .unwrap();
        assert_eq!(text.x, x, "line {index}");
        assert_eq!(text.baseline_fine, index as i64 * HEIGHT_FINE + 800);
    }
}

#[test]
fn automatic_wrap_does_not_accumulate_coarse_height_before_querying_an_exclusion() {
    let pages = layout(&[para(&"A".repeat(40))], 783);
    // The fifth line starts at 782.4, still beside the exclusion. Accumulating
    // 196 per line would incorrectly start that query at 784 and admit 8 chars.
    assert_lines(
        &pages,
        &[
            (0, 4),
            (4, 8),
            (8, 12),
            (12, 16),
            (16, 20),
            (20, 28),
            (28, 36),
            (36, 41),
        ],
        &[400, 400, 400, 400, 400, 0, 0, 0],
    );
}

#[test]
fn soft_breaks_advance_wrap_queries_by_the_placed_lines_fine_height() {
    let input = ["AAAA", "AAAA", "AAAA", "AAAA", "AAAAAAAA"].join("\u{fffc}");
    let pages = layout(&[para(&input)], 783);
    assert_lines(
        &pages,
        &[(0, 5), (5, 10), (10, 15), (15, 20), (20, 24), (24, 29)],
        &[400, 400, 400, 400, 400, 0],
    );
}

#[test]
fn paragraph_entry_preserves_the_previous_paragraphs_fractional_advance() {
    let pages = layout(&[para("A"), para(&"B".repeat(24))], 392);
    // Rounding 195.6 at paragraph entry and then adding 195.6 moves the next
    // query to 392 instead of 391, losing the final row beside this exclusion.
    assert_lines(
        &pages,
        &[(0, 2), (2, 6), (6, 10), (10, 18), (18, 27)],
        &[400, 400, 400, 0, 0],
    );
}

#[test]
fn keep_lines_preflight_preserves_the_next_columns_fractional_band_top() {
    let setup = PageSetup {
        size: Size::new(1600, 1000),
        margins: Margins::uniform(0),
    };
    let mut document = document_from_json(&serde_json::json!({"main": []}));
    document.paras = vec![
        Para {
            terminator: LineTerminator::SectionBreak,
            ..para("A")
        },
        para("F"),
        para("F"),
        Para {
            keep_lines: true,
            ..para(&"B".repeat(24))
        },
    ];
    document.sections = vec![
        LayoutSection {
            block_range: 0..1,
            para_range: 0..1,
            setup,
            kind: SectionStart::NextPage,
            columns: ColumnLayout::default(),
            grid: Default::default(),
            fallback_fields: Vec::new(),
        },
        LayoutSection {
            block_range: 1..4,
            para_range: 1..4,
            setup,
            kind: SectionStart::Continuous,
            columns: ColumnLayout::equal(2, 0).unwrap(),
            grid: Default::default(),
            fallback_fields: Vec::new(),
        },
    ];
    let mut wrap = WrapContext::new();
    wrap.add(WrapRegion::rect(Rect::new(800, 0, 400, 392), 0));
    let pages = Engine::with_wrap(&FractionalMetrics, setup, wrap).layout_document(&document);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].columns[2].y, 196);
    assert_eq!(pages[0].line_columns, [0, 1, 1, 2, 2, 2, 2]);
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    let kept = &record.pages[0].lines[3..];
    let sources: Vec<_> = kept
        .iter()
        .map(|line| {
            let source = line.source.unwrap();
            (source.start, source.end)
        })
        .collect();
    assert_eq!(sources, [(6, 10), (10, 14), (14, 22), (22, 31)]);
    for (offset, x) in [1200, 1200, 800, 800].into_iter().enumerate() {
        let text = pages[0]
            .fragments
            .iter()
            .find_map(|fragment| match fragment {
                Fragment::Text(text) if text.line == (offset + 3) as u32 => Some(text),
                _ => None,
            })
            .unwrap();
        assert_eq!(text.x, x);
        assert_eq!(text.baseline_fine, (offset as i64 + 1) * HEIGHT_FINE + 800);
    }
}
