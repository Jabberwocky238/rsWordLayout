//! Synthetic threshold and source-replay invariants, not Word measurements.

use std::collections::BTreeSet;

use rsword_layout_core::{
    BreakOpportunity, Color, ColumnLayout, ColumnSpec, Engine, FontMetrics, FontSpec, Fragment,
    LayoutDocument, LayoutRecord, LayoutSection, LineTerminator, Margins, Page, PageSetup, Para,
    PlaceholderKind, Run, SectionStart, Size, TextMetrics, Twips, document_from_json,
    paint_document,
};
use serde_json::json;

const UNIT: Twips = 100;

/// Glyph widths stay fixed while the tall run has ten times the line height.
struct VariableHeightMetrics;

impl FontMetrics for VariableHeightMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.chars().count() as Twips * UNIT,
            ascent: if font.family == "tall" {
                10 * UNIT
            } else {
                UNIT
            },
            descent: 0,
            line_gap: 0,
        }
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

fn run(text: &str, family: &str) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new(family, 24),
        color: Color::BLACK,
        hidden: false,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
    }
}

fn paragraph(text: &str) -> Para {
    Para {
        runs: vec![run(text, "low")],
        ..Para::default()
    }
}

fn document(
    mut paras: Vec<Para>,
    widths: &[Twips],
    height: Twips,
    successor: bool,
) -> LayoutDocument {
    let count = paras.len();
    let setup = PageSetup {
        size: Size::new(widths.iter().sum(), height),
        margins: Margins::uniform(0),
    };
    let mut document = document_from_json(&json!({"main": []}));
    document.sections = vec![LayoutSection {
        block_range: 0..count,
        para_range: 0..count,
        setup,
        kind: SectionStart::NextPage,
        columns: ColumnLayout::explicit(
            widths
                .iter()
                .map(|&width| ColumnSpec {
                    width,
                    gap_after: 0,
                })
                .collect(),
        )
        .unwrap(),
        grid: Default::default(),
        fallback_fields: Vec::new(),
    }];
    if successor {
        paras.last_mut().unwrap().terminator = LineTerminator::SectionBreak;
        paras.push(paragraph("E"));
        document.sections.push(LayoutSection {
            block_range: count..count + 1,
            para_range: count..count + 1,
            setup,
            kind: SectionStart::Continuous,
            columns: ColumnLayout::default(),
            grid: Default::default(),
            fallback_fields: Vec::new(),
        });
    }
    document.paras = paras;
    document
}

fn layout(document: &LayoutDocument) -> Vec<Page> {
    Engine::new(&VariableHeightMetrics, PageSetup::a4()).layout_document(document)
}

fn record(pages: &[Page]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
}

fn counts(page: &Page) -> Vec<usize> {
    let mut counts = vec![0; page.columns.len()];
    for &column in &page.line_columns {
        counts[column] += 1;
    }
    counts
}

fn assert_complete_sources(pages: &[Page], document: &LayoutDocument) {
    let captured = record(pages);
    let mut source_end = 0;
    let mut surrogate_interiors = BTreeSet::new();
    let mut expected_end = 0;
    for para in &document.paras {
        for run in &para.runs {
            for ch in run.text.chars() {
                if ch.len_utf16() == 2 {
                    surrogate_interiors.insert(expected_end + 1);
                }
                expected_end += ch.len_utf16() as u32;
            }
        }
        expected_end += 1;
    }
    for page in &captured.pages {
        for line in &page.lines {
            let source = line.source.expect("every replayed line retains its source");
            assert_eq!(source.start, source_end);
            assert!(source.end > source.start);
            assert!(!surrogate_interiors.contains(&source.start));
            assert!(!surrogate_interiors.contains(&source.end));
            source_end = source.end;
        }
    }
    assert_eq!(source_end, expected_end);
}

fn mixed_height_paragraph() -> Para {
    Para {
        runs: vec![run(&"L".repeat(13), "low"), run("TT", "tall")],
        ..Para::default()
    }
}

#[test]
fn unequal_column_fit_is_nonmonotone_with_mixed_line_heights() {
    // H=11: left has 11 L; right has LL / TT, needing 1+10 units.
    // H=12: left has 12 L; right would need LT / T, needing 10+10.
    // H=13: left has 13 L; right has TT, needing 10 units.
    for (height, expected_pages, first_counts) in [
        (11, 1, vec![11, 2]),
        (12, 2, vec![12, 1]),
        (13, 1, vec![13, 1]),
    ] {
        let document = document(
            vec![mixed_height_paragraph()],
            &[UNIT, 2 * UNIT],
            height * UNIT,
            false,
        );
        let pages = layout(&document);
        assert_eq!(pages.len(), expected_pages, "height={height}");
        assert_eq!(counts(&pages[0]), first_counts, "height={height}");
        assert_complete_sources(&pages, &document);
    }
}

#[test]
fn balancing_finds_the_first_feasible_height_before_the_nonmonotone_hole() {
    let document = document(
        vec![mixed_height_paragraph()],
        &[UNIT, 2 * UNIT],
        25 * UNIT,
        true,
    );
    let pages = layout(&document);
    assert_eq!(pages.len(), 1);
    assert_eq!(counts(&pages[0]), [11, 2, 1]);
    assert_eq!(pages[0].columns[0].height, 11 * UNIT);
    assert_eq!(pages[0].columns[1].height, 11 * UNIT);
    assert_eq!(pages[0].columns[2].y, 11 * UNIT);
    assert_eq!(
        record(&pages).pages[0]
            .lines
            .last()
            .unwrap()
            .source
            .unwrap()
            .start,
        16
    );
    assert_complete_sources(&pages, &document);
}

#[test]
fn short_narrow_trial_cannot_split_keep_lines_using_the_wider_columns_extent() {
    let para = Para {
        keep_lines: true,
        ..paragraph(&"L".repeat(13))
    };
    let document = document(vec![para], &[UNIT, 10 * UNIT], 20 * UNIT, true);
    let pages = layout(&document);
    let captured = record(&pages);
    let paragraph_regions: BTreeSet<_> = captured
        .pages
        .iter()
        .enumerate()
        .flat_map(|(page, record)| record.lines.iter().map(move |line| (page, line)))
        .filter(|(_, line)| line.source.unwrap().start < 14)
        .map(|(page, line)| (page, line.column.unwrap()))
        .collect();
    assert_eq!(
        paragraph_regions.len(),
        1,
        "keepLines split across {paragraph_regions:?}"
    );
    assert_complete_sources(&pages, &document);
}

#[test]
fn keep_lines_still_protects_the_segment_after_a_hard_column_break() {
    let mut para = paragraph("L\u{fffc}LLL");
    para.keep_lines = true;
    para.runs[0].placeholders = vec![PlaceholderKind::ColumnBreak];
    let document = document(vec![para], &[UNIT, UNIT, UNIT], 10 * UNIT, true);
    let pages = layout(&document);
    assert_eq!(pages.len(), 1);
    let captured = record(&pages);
    let tail: Vec<_> = captured.pages[0]
        .lines
        .iter()
        .filter(|line| (2..6).contains(&line.source.unwrap().start))
        .collect();
    assert_eq!(tail.len(), 3);
    assert!(
        tail.iter().all(|line| line.column == Some(1)),
        "tail crossed columns: {tail:?}"
    );
    assert_eq!(pages[0].columns[0].height, 3 * UNIT);
    assert_complete_sources(&pages, &document);
}

#[test]
fn replay_resumes_inside_a_paragraph_with_hidden_non_bmp_and_tab_source() {
    let mut hidden = run("hidden\u{1f600}", "low");
    hidden.hidden = true;
    let para = Para {
        runs: vec![
            run(&"L".repeat(27), "low"),
            run("\u{1f600}L", "low"),
            hidden,
            run("\tLLLL\u{1f600}\tLL", "low"),
        ],
        indent_first_line: UNIT,
        default_tab_stop: Some(2 * UNIT),
        ..Para::default()
    };
    let terminal = document(vec![para.clone()], &[3 * UNIT, 4 * UNIT], 4 * UNIT, false);
    let continuous = document(vec![para], &[3 * UNIT, 4 * UNIT], 4 * UNIT, true);
    let original_pages = layout(&terminal);
    let pages = layout(&continuous);
    let original = record(&original_pages);
    let replayed = record(&pages);
    assert_eq!(pages.len(), 2);
    assert_eq!(
        replayed.pages[0], original.pages[0],
        "replay changed an already completed page"
    );
    let resume_cp = replayed.pages[1].lines[0].source.unwrap().start;
    assert!(
        resume_cp > 0 && resume_cp <= 27,
        "expected a paragraph continuation before the special source: {resume_cp}"
    );
    let first = pages[1]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        first.x, 0,
        "a resumed line must not regain the first-line indent"
    );
    let visible: String = pages
        .iter()
        .flat_map(|page| &page.fragments)
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        // The final paragraph mark may contribute a rendered trailing space;
        // its glyph count is not part of this source-replay regression.
        visible.trim_end_matches(' '),
        format!("{}\u{1f600}L\tLLLL\u{1f600}\tLLE", "L".repeat(27))
    );
    assert_complete_sources(&pages, &continuous);
}

#[test]
fn resumed_first_line_does_not_apply_paragraph_space_before_again() {
    let para = Para {
        space_before: UNIT,
        ..paragraph(&"L".repeat(31))
    };
    let document = document(vec![para], &[UNIT, UNIT], 10 * UNIT, true);
    let pages = layout(&document);
    assert_eq!(pages.len(), 2);
    assert_eq!(counts(&pages[0]), [9, 10]);
    assert_eq!(counts(&pages[1]), [6, 6, 1]);
    assert_eq!(pages[1].columns[0].height, 6 * UNIT);
    assert_eq!(record(&pages).pages[1].lines[0].source.unwrap().start, 19);
    assert_complete_sources(&pages, &document);
}

#[test]
fn matching_later_section_geometry_cannot_open_a_wider_band_on_an_old_page() {
    let mut document = document(
        vec![paragraph("A"), paragraph("B"), paragraph("C")],
        &[UNIT, UNIT],
        10 * UNIT,
        false,
    );
    let mut first = document.sections[0].clone();
    first.block_range = 0..1;
    first.para_range = 0..1;
    let mut second = first.clone();
    second.block_range = 1..2;
    second.para_range = 1..2;
    second.kind = SectionStart::Continuous;
    second.setup.size.width = 3 * UNIT;
    second.columns = ColumnLayout::equal(2, 0).unwrap();
    let mut third = second.clone();
    third.block_range = 2..3;
    third.para_range = 2..3;
    third.columns = ColumnLayout::default();
    document.sections = vec![first, second, third];
    let pages = layout(&document);
    assert_eq!(pages[0].size.width, 2 * UNIT);
    for page in &pages {
        assert!(
            page.columns
                .iter()
                .all(|column| column.x >= page.content_area.x
                    && column.right() <= page.content_area.right()),
            "a later declaration escaped the active physical page: {:?}",
            page.columns
        );
    }
    assert_complete_sources(&pages, &document);
}

fn pending_height_document(
    prefix_lines: usize,
    old_height: Twips,
    pending_height: Twips,
) -> LayoutDocument {
    let kept = Para {
        keep_lines: true,
        ..paragraph(&"L".repeat(6))
    };
    let mut document = document(
        vec![paragraph(&"L".repeat(prefix_lines)), kept],
        &[UNIT, UNIT],
        old_height,
        false,
    );
    let mut first = document.sections[0].clone();
    first.block_range = 0..1;
    first.para_range = 0..1;
    let mut second = first.clone();
    second.block_range = 1..2;
    second.para_range = 1..2;
    second.kind = SectionStart::Continuous;
    second.setup.size.height = pending_height;
    document.sections = vec![first, second];
    document
}

#[test]
fn smaller_pending_page_does_not_split_a_keep_group_that_fits_the_current_next_column() {
    let document = pending_height_document(8, 10 * UNIT, 5 * UNIT);
    let pages = layout(&document);
    assert_eq!(pages.len(), 1);
    assert_eq!(counts(&pages[0]), [8, 6]);
    assert_eq!(pages[0].size.height, 10 * UNIT);
    assert_complete_sources(&pages, &document);
}

#[test]
fn larger_pending_page_does_not_make_a_currently_oversized_keep_group_fit() {
    let document = pending_height_document(3, 5 * UNIT, 10 * UNIT);
    let pages = layout(&document);
    assert_eq!(pages.len(), 1);
    assert_eq!(counts(&pages[0]), [5, 4]);
    assert_eq!(pages[0].size.height, 5 * UNIT);
    assert_complete_sources(&pages, &document);
}
