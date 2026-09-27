//! Paragraph-gap bookkeeping with deterministic metrics, not new Word captures.

use rsword_layout_core::{
    Color, ColumnLayout, Engine, FontSpec, Fragment, LayoutDocument, LayoutRecord, LayoutSection,
    LineRule, Margins, Page, PageSetup, Para, PlaceholderKind, Rect, Run, SectionStart,
    SimpleMetrics, Size, WrapContext, WrapRegion, document_from_json, paint_document,
};
use serde_json::json;

const LINE: i32 = 100;

fn para(text: &str, before: i32, after: i32) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(),
            font: FontSpec::new("synthetic", 24),
            color: Color::BLACK,
            placeholders: vec![PlaceholderKind::LineBreak; text.matches('\u{fffc}').count()],
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        line_rule: LineRule::Exact,
        line_value: LINE,
        space_before: before,
        space_after: after,
        ..Para::default()
    }
}

fn setup(height: i32) -> PageSetup {
    PageSetup {
        size: Size::new(2000, height),
        margins: Margins::uniform(0),
    }
}

fn tops(pages: &[Page]) -> Vec<Vec<i64>> {
    pages
        .iter()
        .map(|page| {
            page.line_placements
                .iter()
                .map(|line| {
                    line.as_ref()
                        .expect("committed line has placement")
                        .top_fine
                })
                .collect()
        })
        .collect()
}

fn assert_sources(pages: &[Page], paras: &[Para]) {
    let record = LayoutRecord::from_paint(&paint_document(pages, None, &[]));
    let mut cursor = 0;
    for line in record.pages.iter().flat_map(|page| &page.lines) {
        let source = line.source.expect("source-bearing line");
        assert_eq!(source.start, cursor);
        assert!(source.end > source.start);
        cursor = source.end;
    }
    assert_eq!(
        cursor,
        paras
            .iter()
            .map(|para| {
                para.runs
                    .iter()
                    .map(|run| run.text.encode_utf16().count() as u32)
                    .sum::<u32>()
                    + 1
            })
            .sum::<u32>()
    );
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
        .map(|&(end, count, kind)| {
            let section = LayoutSection {
                block_range: start..end,
                para_range: start..end,
                setup: setup(height),
                kind,
                columns: ColumnLayout::equal(count, 200).unwrap(),
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

#[test]
fn adjacent_nonnegative_spaces_use_the_larger_value() {
    for (after, before) in [
        (200, 120),
        (80, 120),
        (120, 120),
        (0, 120),
        (120, 0),
        (0, 0),
    ] {
        let paras = [para("A", 30, after), para("B", before, 0)];
        let pages = Engine::new(&SimpleMetrics, setup(1000)).layout(&paras);
        assert_eq!(
            tops(&pages),
            [vec![150, i64::from(30 + LINE + after.max(before)) * 5]]
        );
        assert_sources(&pages, &paras);
    }
}

#[test]
fn a_negative_space_preserves_the_signed_sum() {
    for (after, before) in [(-40, 120), (80, -20), (-40, -20)] {
        let paras = [para("A", 100, after), para("B", before, 0)];
        let pages = Engine::new(&SimpleMetrics, setup(1000)).layout(&paras);
        assert_eq!(
            tops(&pages),
            [vec![500, i64::from(100 + LINE + after + before) * 5]]
        );
        assert_sources(&pages, &paras);
    }
}

#[test]
fn collapsed_gap_changes_actual_page_capacity() {
    let paras = [para("A", 0, 120), para("B", 80, 0)];
    let pages = Engine::new(&SimpleMetrics, setup(320)).layout(&paras);
    assert_eq!(tops(&pages), [vec![0, 1100]]);
    assert_sources(&pages, &paras);
    let pages = Engine::new(&SimpleMetrics, setup(319)).layout(&paras);
    assert_eq!(tops(&pages), [vec![0], vec![0]]);
    assert_sources(&pages, &paras);
}

#[test]
fn keep_preflight_uses_the_same_gap_as_commit() {
    for kept_lines in [false, true] {
        let mut lead = para("A", 0, 80);
        lead.keep_next = true;
        let mut tail = para(if kept_lines { "B\u{fffc}C" } else { "B" }, 120, 0);
        tail.keep_lines = kept_lines;
        let paras = [para("F", 0, 0), lead, tail];
        let pages =
            Engine::new(&SimpleMetrics, setup(if kept_lines { 520 } else { 420 })).layout(&paras);
        let expected = if kept_lines {
            vec![0, 500, 1600, 2100]
        } else {
            vec![0, 500, 1600]
        };
        assert_eq!(tops(&pages), [expected]);
        assert_sources(&pages, &paras);
    }
}

#[test]
fn a_kept_chain_collapses_each_boundary_once() {
    let mut a = para("A", 0, 80);
    a.keep_next = true;
    let mut b = para("B", 120, 60);
    b.keep_next = true;
    let paras = [para("F", 0, 0), a, b, para("C", 40, 0)];
    let pages = Engine::new(&SimpleMetrics, setup(580)).layout(&paras);
    assert_eq!(tops(&pages), [vec![0, 500, 1600, 2400]]);
    assert_sources(&pages, &paras);
}

#[test]
fn page_break_before_discards_previous_region_spacing_state() {
    let mut next = para("B", 120, 0);
    next.page_break_before = true;
    let paras = [para("A", 0, 200), next];
    let pages = Engine::new(&SimpleMetrics, setup(1000)).layout(&paras);
    assert_eq!(tops(&pages), [vec![0], vec![600]]);
    assert_sources(&pages, &paras);
}

#[test]
fn a_hard_page_end_does_not_claim_adjacency_in_the_new_empty_region() {
    let mut first = para("A\u{fffc}", 0, 200);
    first.runs[0].placeholders = vec![PlaceholderKind::PageBreak];
    let paras = [first, para("B", 120, 0)];
    let pages = Engine::new(&SimpleMetrics, setup(1000)).layout(&paras);
    // Preserve existing consumption of after-space following a final hard page
    // break. The previous paragraph has no line in the newly opened region.
    assert_eq!(tops(&pages), [vec![0], vec![1600]]);
    assert_sources(&pages, &paras);
}

#[test]
fn next_column_section_starts_with_its_full_before_space() {
    let doc = document(
        vec![para("A", 0, 200), para("B", 120, 0)],
        &[
            (1, 2, SectionStart::NextPage),
            (2, 2, SectionStart::NextColumn),
        ],
        1000,
    );
    let pages = Engine::new(&SimpleMetrics, setup(1000)).layout_document(&doc);
    assert_eq!(tops(&pages), [vec![0, 600]]);
    assert_eq!(pages[0].line_columns, [0, 1]);
    assert_sources(&pages, &doc.paras);
}

#[test]
fn a_continued_paragraph_does_not_reapply_before_space() {
    let paras = [
        para("A", 0, 80),
        para("B\u{fffc}C\u{fffc}D", 120, 40),
        para("E", 20, 0),
    ];
    let pages = Engine::new(&SimpleMetrics, setup(350)).layout(&paras);
    assert_eq!(tops(&pages), [vec![0, 1100], vec![0, 500, 1200]]);
    assert_sources(&pages, &paras);
}

#[test]
fn continuous_band_boundary_does_not_collapse_against_the_previous_band() {
    let doc = document(
        vec![para("A", 0, 200), para("B", 120, 0)],
        &[
            (1, 1, SectionStart::NextPage),
            (2, 2, SectionStart::Continuous),
        ],
        1000,
    );
    let pages = Engine::new(&SimpleMetrics, setup(1000)).layout_document(&doc);
    assert_eq!(tops(&pages), [vec![0, 2100]]);
    assert_eq!(pages[0].line_columns, [0, 1]);
    assert_sources(&pages, &doc.paras);
}

#[test]
fn continuous_single_column_keep_reserves_the_additive_band_boundary() {
    let mut lead = para("A", 0, 80);
    lead.keep_next = true;
    let doc = document(
        vec![para("F", 0, 0), lead, para("B", 120, 0)],
        &[
            (2, 1, SectionStart::NextPage),
            (3, 1, SectionStart::Continuous),
        ],
        420,
    );
    let pages = Engine::new(&SimpleMetrics, setup(420)).layout_document(&doc);
    // The kept pair requires 100 + 80 + 120 + 100, so the filler must be left
    // behind. A max-gap preflight would accept 420, then strand B on page two.
    assert_eq!(tops(&pages), [vec![0], vec![0, 1500]]);
    assert_eq!(pages[1].line_columns, [0, 1]);
    assert_sources(&pages, &doc.paras);
}

#[test]
fn continuous_deferred_geometry_preserves_section_boundary_spacing() {
    let mut doc = document(
        vec![para("A", 0, 200), para("B", 120, 0)],
        &[
            (1, 1, SectionStart::NextPage),
            (2, 1, SectionStart::Continuous),
        ],
        1000,
    );
    doc.sections[1].setup.size = Size::new(2100, 1100);
    let pages = Engine::new(&SimpleMetrics, setup(1000)).layout_document(&doc);
    // The second section stays in the current region until a physical page
    // opens, but this slice does not change spacing across section boundaries.
    assert_eq!(tops(&pages), [vec![0, 2100]]);
    assert_eq!(pages[0].line_columns, [0, 0]);
    assert_eq!(pages[0].size, setup(1000).size);
    assert_sources(&pages, &doc.paras);
}

#[test]
fn balance_replay_preserves_the_consumed_first_before_and_collapses_later_gaps() {
    let mut paras = vec![
        para("A", 40, 40),
        para("B", 40, 40),
        para("C", 40, 40),
        para("D", 40, 40),
    ];
    paras.push(para("E", 30, 0));
    let doc = document(
        paras,
        &[
            (4, 2, SectionStart::NextPage),
            (5, 1, SectionStart::Continuous),
        ],
        1000,
    );
    let pages = Engine::new(&SimpleMetrics, setup(1000)).layout_document(&doc);
    assert_eq!(pages[0].line_columns, [0, 0, 1, 1, 2]);
    assert_eq!(tops(&pages), [vec![200, 900, 0, 700, 1750]]);
    assert_sources(&pages, &doc.paras);
}

#[test]
fn wrap_uses_the_collapsed_origin_before_committing_the_paragraph() {
    let paras = [para("A", 0, 120), para("abcdefgh", 80, 0)];
    let mut wrap = WrapContext::new();
    wrap.add(WrapRegion::rect(Rect::new(0, 0, 480, 280), 0));
    let page_setup = PageSetup {
        size: Size::new(960, 1000),
        margins: Margins::uniform(0),
    };
    let pages = Engine::with_wrap(&SimpleMetrics, page_setup, wrap).layout(&paras);
    assert_eq!(tops(&pages), [vec![0, 1100, 1600]]);
    let positions: Vec<_> = pages[0]
        .fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) if !text.text.is_empty() => Some((text.line, text.x)),
            _ => None,
        })
        .collect();
    assert!(positions.contains(&(1, 480)));
    assert!(positions.contains(&(2, 0)));
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    let sources: Vec<_> = record.pages[0]
        .lines
        .iter()
        .map(|line| {
            let source = line.source.unwrap();
            (source.start, source.end)
        })
        .collect();
    assert_eq!(sources, [(0, 2), (2, 6), (6, 11)]);
    assert_sources(&pages, &paras);
}
