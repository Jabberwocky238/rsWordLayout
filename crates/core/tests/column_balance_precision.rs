//! Synthetic public-API checks of fine-unit column-group geometry, not Word captures.

use rsword_layout_core::{
    BreakOpportunity, Color, ColumnLayout, Engine, FontMetrics, FontSpec, Fragment, LayoutDocument,
    LayoutRecord, LayoutSection, LineTerminator, Margins, Page, PageSetup, Para, Run, SectionStart,
    Size, TextMetrics, Twips, document_from_json, paint_document,
};
use serde_json::json;

const FINE_PER_TWIP: i64 = 5;
const HEIGHT_FINE: i64 = 978; // 195.6 twips; the integer metrics round to 196.
const ASCENT: Twips = 160;
const ASCENT_FINE: i64 = ASCENT as i64 * FINE_PER_TWIP;

struct FractionalMetrics;

impl FontMetrics for FractionalMetrics {
    fn measure(&self, text: &str, _font: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.chars().count() as Twips * 100,
            ascent: ASCENT,
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

fn paragraph(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(),
            font: FontSpec::new("fractional-test", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        ..Para::default()
    }
}

fn document(groups: Vec<(Vec<Para>, usize)>, width: Twips) -> LayoutDocument {
    let setup = PageSetup {
        size: Size::new(width, 5000),
        margins: Margins::uniform(0),
    };
    let mut document = document_from_json(&json!({"main": []}));
    document.sections.clear();
    let group_count = groups.len();
    for (index, (mut paras, columns)) in groups.into_iter().enumerate() {
        if index + 1 < group_count {
            paras.last_mut().unwrap().terminator = LineTerminator::SectionBreak;
        }
        let start = document.paras.len();
        document.paras.extend(paras);
        let end = document.paras.len();
        document.sections.push(LayoutSection {
            block_range: start..end,
            para_range: start..end,
            setup,
            kind: if index == 0 {
                SectionStart::NextPage
            } else {
                SectionStart::Continuous
            },
            columns: ColumnLayout::equal(columns, 0).unwrap(),
            grid: Default::default(),
            fallback_fields: Vec::new(),
        });
    }
    document
}

fn layout(document: &LayoutDocument) -> Vec<Page> {
    Engine::new(&FractionalMetrics, PageSetup::a4()).layout_document(document)
}

fn baseline(page: &Page, line: u32) -> i64 {
    page.fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.line == line => Some(text.baseline_fine),
            _ => None,
        })
        .expect("each source line has a text fragment")
}

fn assert_sources(page: &Page, expected: &[(u32, u32)]) {
    let record = LayoutRecord::from_paint(&paint_document(std::slice::from_ref(page), None, &[]));
    let lines = &record.pages[0].lines;
    assert_eq!(lines.len(), expected.len());
    assert_eq!(page.line_columns.len(), expected.len());
    for (index, (line, &(start, end))) in lines.iter().zip(expected).enumerate() {
        let source = line.source.expect("replayed line retains source ownership");
        assert_eq!((source.start, source.end), (start, end));
        assert_eq!(line.column, Some(page.line_columns[index]));
        assert!(page.line_columns[index] < page.columns.len());
    }
}

#[test]
fn three_balanced_groups_preserve_fractional_tops_instead_of_accumulating_rect_rounding() {
    let mut groups: Vec<_> = (0..3)
        .map(|_| ((0..6).map(|_| paragraph("A")).collect(), 2))
        .collect();
    groups.push((vec![paragraph("E")], 1));
    let pages = layout(&document(groups, 2000));
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    assert_eq!(page.columns.len(), 7);
    assert_eq!(
        page.line_columns,
        [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6]
    );

    let group_height = 3 * HEIGHT_FINE;
    for group in 0..3 {
        let top = group as i64 * group_height;
        for offset in 0..6 {
            assert_eq!(
                baseline(page, (group * 6 + offset) as u32),
                top + (offset % 3) as i64 * HEIGHT_FINE + ASCENT_FINE
            );
        }
    }
    assert_eq!(baseline(page, 18), 3 * group_height + ASCENT_FINE);
    for pair in [0, 6, 12, 18].windows(2) {
        assert_eq!(
            baseline(page, pair[1]) - baseline(page, pair[0]),
            group_height
        );
    }
    assert_eq!(
        page.columns.iter().map(|area| area.y).collect::<Vec<_>>(),
        [0, 0, 587, 587, 1174, 1174, 1760]
    );
    assert_ne!(
        baseline(page, 6) - ASCENT_FINE,
        i64::from(page.columns[2].y) * FINE_PER_TWIP,
        "the rounded diagnostic rectangle must not become the next group's exact top"
    );
    assert_sources(
        page,
        &(0..19).map(|i| (i * 2, i * 2 + 2)).collect::<Vec<_>>(),
    );
}

#[test]
fn replay_and_group_transitions_consume_paragraph_spacing_once() {
    let first = Para {
        space_before: 11,
        space_after: 17,
        ..paragraph("AAAA")
    };
    let second = Para {
        space_before: 19,
        space_after: 23,
        ..paragraph("B")
    };
    let third = Para {
        space_before: 29,
        ..paragraph("C")
    };
    let pages = layout(&document(
        vec![(vec![first], 2), (vec![second], 1), (vec![third], 1)],
        200,
    ));
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    assert_eq!(page.line_columns, [0, 0, 1, 1, 2, 3]);
    assert_eq!(baseline(page, 0), 11 * FINE_PER_TWIP + ASCENT_FINE);
    assert_eq!(baseline(page, 1) - baseline(page, 0), HEIGHT_FINE);
    assert_eq!(baseline(page, 2), ASCENT_FINE);
    assert_eq!(baseline(page, 3) - baseline(page, 2), HEIGHT_FINE);

    // The right column ends after two lines plus the paragraph's one trailing gap.
    let first_bottom = 2 * HEIGHT_FINE + 17 * FINE_PER_TWIP;
    let second_top = first_bottom + 19 * FINE_PER_TWIP;
    assert_eq!(baseline(page, 4), second_top + ASCENT_FINE);
    let second_bottom = second_top + HEIGHT_FINE + 23 * FINE_PER_TWIP;
    assert_eq!(
        baseline(page, 5),
        second_bottom + 29 * FINE_PER_TWIP + ASCENT_FINE
    );
    assert_sources(page, &[(0, 1), (1, 2), (2, 3), (3, 5), (5, 7), (7, 9)]);
}

#[test]
fn negative_gaps_cannot_erase_the_earlier_taller_lines_occupied_bottom() {
    let tall = Para {
        line_value: 720,
        ..paragraph("A")
    };
    let overlapping = Para {
        space_before: -500,
        space_after: -100,
        ..paragraph("B")
    };
    let pages = layout(&document(
        vec![(vec![tall, overlapping], 2), (vec![paragraph("E")], 1)],
        2000,
    ));
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    let tall_bottom = 3 * HEIGHT_FINE;
    let later_top = tall_bottom - 500 * FINE_PER_TWIP;
    let later_bottom = later_top + HEIGHT_FINE;
    assert!(later_bottom < tall_bottom);
    assert_eq!(page.line_columns, [0, 0, 2]);
    assert_eq!(baseline(page, 0), ASCENT_FINE);
    assert_eq!(baseline(page, 1), later_top + ASCENT_FINE);
    assert_eq!(
        baseline(page, 2),
        tall_bottom + ASCENT_FINE,
        "the successor starts below the maximum occupied bottom, not the retracted cursor"
    );
    assert_eq!(page.columns[0].height, 587);
    assert_eq!(page.columns[1].height, 587);
    assert_eq!(page.columns[2].y, 587);
    assert_sources(page, &[(0, 2), (2, 4), (4, 6)]);
}
