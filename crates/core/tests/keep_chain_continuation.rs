//! Synthetic keep-reservation priorities, not newly measured Word pagination.

use rsword_layout_core::{
    Color, ColumnLayout, ColumnSpec, Engine, FontSpec, LayoutDocument, LayoutRecord, LayoutSection,
    LineRule, Margins, Page, PageSetup, Para, PlaceholderKind, Platform, Run, SectionStart,
    SimpleMetrics, Size, View, document_from_json, paint_document,
};
use serde_json::json;

const LINE: i32 = 480;

fn para(label: &str, count: usize, keep: bool) -> Para {
    Para {
        runs: vec![Run {
            text: (0..count)
                .map(|i| format!("{label}{i}"))
                .collect::<Vec<_>>()
                .join("\u{fffc}"),
            font: FontSpec::new("synthetic", 24),
            color: Color::BLACK,
            placeholders: vec![PlaceholderKind::LineBreak; count - 1],
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        line_rule: LineRule::Exact,
        line_value: LINE,
        keep_next: keep,
        ..Para::default()
    }
}

fn setup(lines: i32) -> PageSetup {
    PageSetup {
        size: Size::new(2400, lines * LINE),
        margins: Margins::uniform(0),
    }
}

fn record(pages: &[Page]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
}

fn assert_source(pages: &[Page], paras: &[Para]) {
    let mut source = 0;
    for line in record(pages).pages.iter().flat_map(|page| &page.lines) {
        let range = line.source.unwrap();
        assert_eq!(range.start, source);
        assert!(range.end > range.start);
        source = range.end;
    }
    assert_eq!(
        source,
        paras
            .iter()
            .map(|p| {
                p.runs
                    .iter()
                    .map(|r| r.text.encode_utf16().count() as u32)
                    .sum::<u32>()
                    + 1
            })
            .sum::<u32>()
    );
}

fn assert_counts(paras: &[Para], expected: &[usize]) {
    for platform in [Platform::Desktop, Platform::Android] {
        for view in [View::Print, View::Mobile] {
            let pages = Engine::new(&SimpleMetrics, setup(4))
                .with_platform(platform, view)
                .layout(paras);
            let counts: Vec<_> = pages.iter().map(|p| p.line_placements.len()).collect();
            assert_eq!(counts, expected, "{platform:?}/{view:?}");
            assert_source(&pages, paras);
        }
    }
}

fn document(
    paras: Vec<Para>,
    groups: &[(usize, usize, SectionStart)],
    height: i32,
) -> LayoutDocument {
    let mut document = document_from_json(&json!({"main": []}));
    let mut start = 0;
    document.sections = groups
        .iter()
        .map(|&(end, columns, kind)| {
            let section = LayoutSection {
                block_range: start..end,
                para_range: start..end,
                setup: setup(height),
                columns: ColumnLayout::equal(columns, 240).unwrap(),
                kind,
                grid: Default::default(),
                fallback_fields: vec![],
            };
            start = end;
            section
        })
        .collect();
    document.paras = paras;
    document
}

fn column_counts(page: &Page) -> Vec<usize> {
    (0..page.columns.len())
        .map(|c| page.line_columns.iter().filter(|&&x| x == c).count())
        .collect()
}

#[test]
fn assumed_successor_does_not_move_away_from_its_accepted_prefix() {
    let paras = [para("A", 1, true), para("B", 3, true), para("C", 3, false)];
    assert_counts(&paras, &[3, 4]);
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout(&paras);
    let trace = record(&pages);
    assert_eq!(trace.pages[0].lines.last().unwrap().source.unwrap().end, 9);
    assert_eq!(trace.pages[1].lines[0].source.unwrap().start, 9);
}

#[test]
fn assumed_relaxed_continuation_does_not_rebuild_a_whole_tail_for_each_member() {
    let paras = [
        para("A", 1, true),
        para("B", 1, true),
        para("C", 2, true),
        para("D", 3, false),
    ];
    assert_counts(&paras, &[3, 4]);
}

#[test]
fn assumed_accepted_whole_prefix_takes_priority_over_a_conflicting_outgoing_link() {
    for widow in [false, true] {
        let mut middle = para("B", 3, true);
        middle.keep_lines = !widow;
        middle.widow_control = widow;
        // Both links plus an intact three-line B need five slots. Preserve the
        // already accepted A/B promise and B's legal shape, then release B/C.
        let paras = [para("A", 1, true), middle, para("C", 3, false)];
        assert_counts(&paras, &[4, 3]);
    }
}

#[test]
fn assumed_feasible_keep_lines_is_not_broken_by_an_impossible_outgoing_reservation() {
    let mut lead = para("A", 4, true);
    lead.keep_lines = true;
    assert_counts(&[lead, para("B", 1, false)], &[4, 1]);
    let mut lead = para("A", 3, true);
    lead.keep_lines = true;
    let mut tail = para("B", 2, false);
    tail.widow_control = true;
    assert_counts(&[lead, tail], &[3, 2]);
}

#[test]
fn assumed_feasible_widow_is_not_broken_by_an_impossible_outgoing_reservation() {
    let mut lead = para("A", 3, true);
    lead.widow_control = true;
    let mut tail = para("B", 2, false);
    tail.widow_control = true;
    assert_counts(&[lead, tail], &[3, 2]);
}

#[test]
fn assumed_trial_cannot_invent_a_physical_keep_conflict() {
    for widow in [false, true] {
        for (height, expected) in [(4, vec![3, 2, 1]), (6, vec![5, 0, 1])] {
            let mut lead = para("A", 3, true);
            lead.keep_lines = !widow;
            lead.widow_control = widow;
            let mut tail = para("B", 2, false);
            tail.widow_control = true;
            let doc = document(
                vec![lead, tail, para("E", 1, false)],
                &[
                    (2, 2, SectionStart::NextPage),
                    (3, 1, SectionStart::Continuous),
                ],
                height,
            );
            let pages = Engine::new(&SimpleMetrics, setup(height)).layout_document(&doc);
            assert_eq!(pages.len(), 1);
            assert_eq!(column_counts(&pages[0]), expected);
            assert_source(&pages, &doc.paras);
        }
    }
}

#[test]
fn assumed_long_single_line_chain_releases_only_unavoidable_links() {
    let paras: Vec<_> = (0..11).map(|i| para(&format!("P{i}"), 1, true)).collect();
    assert_counts(&paras, &[4, 4, 3]);
}

#[test]
fn assumed_hard_page_break_ends_an_accepted_prefix_and_continuation() {
    let mut middle = para("B", 3, true);
    middle.runs[0].placeholders[0] = PlaceholderKind::PageBreak;
    let paras = [para("A", 1, true), middle, para("C", 3, false)];
    assert_counts(&paras, &[2, 4, 1]);
}

#[test]
fn assumed_final_hard_page_starts_the_following_reservation_independently() {
    let mut lead = para("A", 1, true);
    lead.runs[0].text.push('\u{fffc}');
    lead.runs[0].placeholders.push(PlaceholderKind::PageBreak);
    let paras = [lead, para("B", 3, true), para("C", 3, false)];
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout(&paras);
    let counts: Vec<_> = pages.iter().map(|p| p.line_placements.len()).collect();
    assert_eq!(counts, [1, 4, 2]);
    assert_source(&pages, &paras);
}

#[test]
fn assumed_page_break_before_is_a_keep_barrier() {
    let mut last = para("C", 3, false);
    last.page_break_before = true;
    let paras = [para("A", 1, true), para("B", 3, true), last];
    assert_counts(&paras, &[4, 3]);
}

#[test]
fn assumed_automatic_column_continuation_keeps_source_order_and_both_links() {
    let doc = document(
        vec![para("A", 1, true), para("B", 3, true), para("C", 3, false)],
        &[(3, 2, SectionStart::NextPage)],
        4,
    );
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout_document(&doc);
    assert_eq!(pages.len(), 1);
    assert_eq!(column_counts(&pages[0]), [3, 4]);
    assert_source(&pages, &doc.paras);
}

#[test]
fn assumed_next_column_section_does_not_carry_a_prefix_promise() {
    let doc = document(
        vec![para("A", 1, true), para("B", 3, true), para("C", 3, false)],
        &[
            (1, 2, SectionStart::NextPage),
            (3, 2, SectionStart::NextColumn),
        ],
        4,
    );
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout_document(&doc);
    assert_eq!(pages.len(), 2);
    assert_eq!(column_counts(&pages[0]), [1, 4]);
    assert_eq!(column_counts(&pages[1]), [2, 0]);
    assert_source(&pages, &doc.paras);
}

#[test]
fn assumed_unequal_columns_reflow_the_uncommitted_source_after_a_promise() {
    let mut middle = para("B", 1, true);
    middle.runs[0].text = "abcdefgh".into();
    let mut doc = document(
        vec![para("A", 1, true), middle, para("C", 1, false)],
        &[(3, 2, SectionStart::NextPage)],
        4,
    );
    doc.sections[0].columns = ColumnLayout::explicit(vec![
        ColumnSpec {
            width: 360,
            gap_after: 240,
        },
        ColumnSpec {
            width: 720,
            gap_after: 0,
        },
    ])
    .unwrap();
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout_document(&doc);
    assert_eq!(pages.len(), 1);
    assert_eq!(column_counts(&pages[0]), [3, 2]);
    let trace = record(&pages);
    let sources: Vec<_> = trace.pages[0]
        .lines
        .iter()
        .map(|l| {
            let s = l.source.unwrap();
            (s.start, s.end)
        })
        .collect();
    assert_eq!(sources, [(0, 3), (3, 6), (6, 9), (9, 12), (12, 15)]);
    assert_source(&pages, &doc.paras);
}

#[test]
fn assumed_balance_uses_physical_capacity_to_choose_reservation_relaxation() {
    for (height, expected) in [(4, vec![3, 4]), (6, vec![5, 2])] {
        let doc = document(
            vec![
                para("A", 1, true),
                para("B", 3, true),
                para("C", 3, false),
                para("E", 1, false),
            ],
            &[
                (3, 2, SectionStart::NextPage),
                (4, 1, SectionStart::Continuous),
            ],
            height,
        );
        let pages = Engine::new(&SimpleMetrics, setup(height)).layout_document(&doc);
        assert_eq!(&column_counts(&pages[0])[..2], expected);
        assert_source(&pages, &doc.paras);
    }
}

#[test]
fn assumed_balance_checkpoint_retains_relaxation_from_an_earlier_physical_page() {
    let doc = document(
        vec![
            para("A", 9, true),
            para("B", 2, true),
            para("C", 1, false),
            para("E", 1, false),
        ],
        &[
            (3, 2, SectionStart::NextPage),
            (4, 1, SectionStart::Continuous),
        ],
        4,
    );
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout_document(&doc);
    assert_eq!(pages.len(), 2);
    assert_eq!(column_counts(&pages[0]), [4, 4]);
    // The last page begins within A. Reclassifying its short remaining suffix
    // as a fresh unrelaxed reservation would incorrectly require 4 + 0 columns.
    assert_eq!(column_counts(&pages[1]), [2, 2, 1]);
    let trace = record(&pages);
    assert_eq!(trace.pages[1].lines[0].source.unwrap().start, 24);
    assert_source(&pages, &doc.paras);
}

#[test]
fn assumed_partial_band_does_not_use_its_short_height_as_physical_capacity() {
    let doc = document(
        vec![
            para("F", 1, false),
            para("A", 1, true),
            para("B", 3, true),
            para("C", 3, false),
        ],
        &[
            (1, 1, SectionStart::NextPage),
            (4, 2, SectionStart::Continuous),
        ],
        4,
    );
    let pages = Engine::new(&SimpleMetrics, setup(4)).layout_document(&doc);
    assert_eq!(pages.len(), 2);
    assert_eq!(column_counts(&pages[0]), [1, 3, 3]);
    assert_eq!(column_counts(&pages[1]), [1, 0]);
    assert_source(&pages, &doc.paras);
}
