//! Continuous-section column groups and source ownership through the public API.
//!
//! The 40-line cases follow the canonical Mac captures documented in
//! docs/COLUMN-BALANCE-MAC-2026-09-27.md. SimpleMetrics checks host bookkeeping,
//! not Word glyph coordinates. The multi-page case is an explicit synthetic
//! extension; the ignored test replays all seven captured inputs with real TNR.

use std::collections::BTreeSet;

use rsword_layout_core::{
    Engine, Fragment, LayoutDocument, LayoutRecord, LineTerminator, Page, PageSetup, Platform,
    Rect, SectionStart, SimpleMetrics, View, document_from_json, paint_document,
};
use serde_json::{Value, json};

const LINE: i32 = 480;
const TOP: i32 = 720;
const BODY_HEIGHT: i32 = 15398;

fn paragraph(text: &str) -> Value {
    json!({
        "kind": "text",
        "props": {
            "widowControl": false,
            "spacing": {"before": 0, "after": 0, "line": LINE, "lineRule": "exact"}
        },
        "inlines": [{"kind": "run", "text": text,
            "props": {"fonts": {"ascii": "Times New Roman", "hAnsi": "Times New Roman"},
                "size": 24}}]
    })
}

fn section(start: usize, end: usize, columns: usize, kind: &str) -> Value {
    json!({"blockRange": [start, end], "props": {
        "kind": kind,
        "pageSize": {"w": 11906, "h": 16838},
        "pageMargins": {"top": TOP, "right": 720, "bottom": 720, "left": 720, "gutter": 0},
        "columns": {"equalWidth": true, "num": columns, "space": 720},
    }})
}

fn document(lines: usize, successor_columns: Option<usize>, hard_break: bool) -> LayoutDocument {
    let mut paras: Vec<_> = (0..lines)
        .map(|index| paragraph(&format!("C{index:03}")))
        .collect();
    if hard_break {
        paras[9]["inlines"][0]["text"] = json!("C009\u{fffc}");
        paras[9]["inlines"][0]["segments"] =
            json!([{"kind": {"kind": "br", "breakKind": "column"}}]);
    }
    let mut sections = vec![section(0, lines, 2, "nextPage")];
    if let Some(columns) = successor_columns {
        paras[lines - 1]["facts"] = json!({"hasSectPr": true});
        paras.push(paragraph("END"));
        sections.push(section(lines, lines + 1, columns, "continuous"));
    }
    let mut document = document_from_json(&json!({"main": paras, "sections": sections}));
    document.compatibility.no_column_balance = Some(false);
    document
}

fn small_document(groups: Vec<(Vec<Value>, usize)>, height: i32) -> LayoutDocument {
    let count = groups.len();
    let mut paras = Vec::new();
    let mut sections = Vec::new();
    for (index, (mut group, columns)) in groups.into_iter().enumerate() {
        let start = paras.len();
        if index + 1 < count {
            group.last_mut().unwrap()["facts"] = json!({"hasSectPr": true});
        }
        paras.extend(group);
        let mut section = section(
            start,
            paras.len(),
            columns,
            if index == 0 { "nextPage" } else { "continuous" },
        );
        section["props"]["pageSize"] = json!({"w": 2640, "h": height});
        section["props"]["pageMargins"] =
            json!({"top": 0, "right": 0, "bottom": 0, "left": 0, "gutter": 0});
        section["props"]["columns"]["space"] = json!(240);
        sections.push(section);
    }
    let mut document = document_from_json(&json!({"main": paras, "sections": sections}));
    document.compatibility.no_column_balance = Some(false);
    document
}

fn record(pages: &[Page]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
}

fn column_counts(pages: &[Page]) -> Vec<Vec<usize>> {
    pages
        .iter()
        .map(|page| {
            let mut counts = vec![0; page.columns.len()];
            for &column in &page.line_columns {
                counts[column] += 1;
            }
            counts
        })
        .collect()
}

fn position(page: &Page, line: u32) -> (i32, i32) {
    page.fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.line == line => Some((text.x, text.baseline_y)),
            _ => None,
        })
        .expect("line has a text fragment")
}

fn assert_sources(pages: &[Page], document: &LayoutDocument) {
    let trace = record(pages);
    let mut cursor = 0;
    for (page, captured) in pages.iter().zip(&trace.pages) {
        assert_eq!(page.line_columns.len(), captured.lines.len());
        let ids: BTreeSet<_> = page
            .fragments
            .iter()
            .filter_map(|fragment| match fragment {
                Fragment::Text(text) => Some(text.line),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            (0..captured.lines.len() as u32).collect(),
            "line numbering must not reset at a same-page group boundary"
        );
        for (index, line) in captured.lines.iter().enumerate() {
            let source = line.source.expect("source interval survives group replay");
            assert_eq!(
                source.start, cursor,
                "group replay loses or duplicates source"
            );
            assert!(source.end > source.start);
            cursor = source.end;
            let column = page.line_columns[index];
            assert!(column < page.columns.len());
            assert_eq!(
                line.column,
                Some(column),
                "trace uses page-wide region indices"
            );
        }
    }
    let expected: usize = document
        .paras
        .iter()
        .map(|para| {
            1 + para
                .runs
                .iter()
                .map(|run| run.text.encode_utf16().count())
                .sum::<usize>()
        })
        .sum();
    assert_eq!(cursor as usize, expected);
}

fn layout(document: &LayoutDocument) -> Vec<Page> {
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4())
        .with_platform(Platform::Desktop, View::Print)
        .layout_document(document);
    assert_sources(&pages, document);
    pages
}

fn assert_group_frames(page: &Page, top_lines: i32, successor_columns: usize) {
    let group_height = top_lines * LINE;
    let mut expected = vec![
        Rect::new(720, TOP, 4873, group_height),
        Rect::new(6313, TOP, 4873, group_height),
    ];
    let next_top = TOP + group_height;
    let remaining = BODY_HEIGHT - group_height;
    if successor_columns == 1 {
        expected.push(Rect::new(720, next_top, 10466, remaining));
    } else {
        expected.push(Rect::new(720, next_top, 4873, remaining));
        expected.push(Rect::new(6313, next_top, 4873, remaining));
    }
    assert_eq!(page.columns, expected);
}

#[test]
fn captured_terminal_last_page_remains_sequential() {
    let document = document(40, None, false);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![32, 8]]);
    assert_eq!(
        pages[0].columns,
        [
            Rect::new(720, TOP, 4873, BODY_HEIGHT),
            Rect::new(6313, TOP, 4873, BODY_HEIGHT),
        ]
    );
    assert_eq!(position(&pages[0], 0).1, position(&pages[0], 32).1);
}

#[test]
fn captured_continuous_single_column_starts_below_balanced_twenty_twenty() {
    let document = document(40, Some(1), false);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![20, 20, 1]]);
    assert_group_frames(&pages[0], 20, 1);
    assert_eq!(position(&pages[0], 0).1, position(&pages[0], 20).1);
    assert_eq!(
        position(&pages[0], 40),
        (720, position(&pages[0], 0).1 + 20 * LINE)
    );
    let trace = record(&pages);
    assert_eq!(
        trace.pages[0].lines[39].terminator,
        LineTerminator::SectionBreak
    );
    assert_eq!(trace.pages[0].lines[40].source.unwrap().start, 200);
    assert_eq!(trace.pages[0].lines[40].column, Some(2));
}

#[test]
fn captured_equal_column_count_successor_opens_a_lower_group() {
    let document = document(40, Some(2), false);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![20, 20, 1, 0]]);
    assert_group_frames(&pages[0], 20, 2);
    assert_eq!(
        position(&pages[0], 40),
        (720, position(&pages[0], 0).1 + 20 * LINE)
    );
    assert_eq!(
        pages[0].line_columns[40], 2,
        "END starts in the lower left region, not the preceding right column"
    );
}

#[test]
fn captured_explicit_no_column_balance_flags_keep_the_same_distribution() {
    for (successor, hard_break) in [(None, false), (Some(1), false), (Some(1), true)] {
        let mut document = document(40, successor, hard_break);
        let disabled = layout(&document);
        document.compatibility.no_column_balance = Some(true);
        let enabled = layout(&document);
        assert_eq!(record(&disabled), record(&enabled));
        let expected = match (successor, hard_break) {
            (None, _) => vec![32, 8],
            (_, false) => vec![20, 20, 1],
            (_, true) => vec![10, 31, 1],
        };
        assert_eq!(column_counts(&enabled), [expected]);
    }
}

#[test]
fn captured_hard_column_break_preserves_ten_thirty_one_and_separate_mark() {
    let document = document(40, Some(1), true);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![10, 31, 1]]);
    assert_group_frames(&pages[0], 31, 1);
    assert_eq!(
        position(&pages[0], 41),
        (720, position(&pages[0], 0).1 + 31 * LINE)
    );
    let trace = record(&pages);
    for (index, start, end, terminator, column) in [
        (9, 45, 50, LineTerminator::ColumnBreak, 0),
        (10, 50, 51, LineTerminator::ParagraphMark, 1),
        (11, 51, 56, LineTerminator::ParagraphMark, 1),
        (41, 201, 205, LineTerminator::ParagraphMark, 2),
    ] {
        let line = &trace.pages[0].lines[index];
        let source = line.source.unwrap();
        assert_eq!((source.start, source.end), (start, end));
        assert_eq!(line.terminator, terminator);
        assert_eq!(line.column, Some(column));
    }
}

#[test]
fn assumed_multi_page_section_rebalances_only_its_last_physical_page() {
    let terminal = layout(&document(104, None, false));
    let continuous = layout(&document(104, Some(1), false));
    assert_eq!(column_counts(&terminal), [vec![32, 32], vec![32, 8]]);
    assert_eq!(column_counts(&continuous), [vec![32, 32], vec![20, 20, 1]]);
    let original = record(&terminal);
    let balanced = record(&continuous);
    assert_eq!(
        balanced.pages[0], original.pages[0],
        "the completed page must not change when the last page is balanced"
    );
    assert_eq!(balanced.pages[1].lines[0].source.unwrap().start, 320);
    assert_eq!(balanced.pages[1].lines[40].source.unwrap().start, 520);
    assert_group_frames(&continuous[1], 20, 1);
}

#[test]
fn assumed_keep_lines_that_fits_full_body_is_not_relaxed_for_a_short_trial() {
    let mut kept = paragraph(&"0".repeat(40));
    kept["props"]["keepLines"] = json!(true);
    let document = small_document(vec![(vec![kept], 2), (vec![paragraph("END")], 1)], 4 * LINE);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![4, 0], vec![1]]);
    let trace = record(&pages);
    assert_eq!(trace.pages[0].lines[3].source.unwrap().end, 41);
    assert_eq!(trace.pages[1].lines[0].source.unwrap().start, 41);
}

#[test]
fn assumed_three_line_widow_paragraph_cannot_balance_as_one_plus_two() {
    let mut kept = paragraph(&"0".repeat(30));
    kept["props"]["widowControl"] = json!(true);
    let document = small_document(vec![(vec![kept], 2), (vec![paragraph("END")], 1)], 4 * LINE);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![3, 0, 1]]);
    assert_eq!(pages[0].columns[2], Rect::new(0, 3 * LINE, 2640, LINE));
    assert_eq!(record(&pages).pages[0].lines[3].source.unwrap().start, 31);
}

#[test]
fn assumed_keep_next_chain_uses_full_body_feasibility_during_balancing() {
    let mut chain: Vec<_> = (0..4)
        .map(|index| paragraph(&format!("K{index}")))
        .collect();
    for para in &mut chain[..3] {
        para["props"]["keepNext"] = json!(true);
    }
    let document = small_document(vec![(chain, 2), (vec![paragraph("END")], 1)], 6 * LINE);
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![4, 0, 1]]);
    assert_eq!(pages[0].columns[2].y, 4 * LINE);
    assert_eq!(&pages[0].line_columns[..4], &[0, 0, 0, 0]);
}

#[test]
fn assumed_successor_with_less_than_one_line_of_space_moves_to_a_new_page() {
    let first: Vec<_> = (0..8)
        .map(|index| paragraph(&format!("A{index}")))
        .collect();
    let document = small_document(
        vec![(first, 2), (vec![paragraph("END")], 1)],
        4 * LINE + LINE / 2,
    );
    let pages = layout(&document);
    assert_eq!(pages.len(), 2);
    let counts = column_counts(&pages);
    assert_eq!(&counts[0][..2], &[4, 4]);
    assert!(
        counts[0][2..].iter().all(|count| *count == 0),
        "an opened lower region must not force a line into its half-line capacity"
    );
    assert_eq!(counts[1], [1]);
    assert_eq!(
        pages[1].columns,
        [Rect::new(0, 0, 2640, 4 * LINE + LINE / 2)]
    );
    assert_eq!(record(&pages).pages[1].lines[0].source.unwrap().start, 24);
}

#[test]
fn assumed_three_continuous_groups_keep_distinct_page_wide_regions() {
    let first = (0..4)
        .map(|index| paragraph(&format!("A{index}")))
        .collect();
    let second = (0..4)
        .map(|index| paragraph(&format!("B{index}")))
        .collect();
    let document = small_document(
        vec![(first, 2), (second, 2), (vec![paragraph("END")], 1)],
        8 * LINE,
    );
    let pages = layout(&document);
    assert_eq!(column_counts(&pages), [vec![2, 2, 2, 2, 1]]);
    assert_eq!(
        pages[0].columns,
        [
            Rect::new(0, 0, 1200, 2 * LINE),
            Rect::new(1440, 0, 1200, 2 * LINE),
            Rect::new(0, 2 * LINE, 1200, 2 * LINE),
            Rect::new(1440, 2 * LINE, 1200, 2 * LINE),
            Rect::new(0, 4 * LINE, 2640, 4 * LINE),
        ]
    );
    assert_eq!(pages[0].line_columns, [0, 0, 1, 1, 2, 2, 3, 3, 4]);
    let trace = record(&pages);
    assert_eq!(
        trace.pages[0].lines[3].terminator,
        LineTerminator::SectionBreak
    );
    assert_eq!(
        trace.pages[0].lines[7].terminator,
        LineTerminator::SectionBreak
    );
    assert_eq!(trace.pages[0].lines[8].source.unwrap().start, 24);
}

#[test]
fn assumed_next_column_section_boundary_survives_balance_replay() {
    let first = (0..10)
        .map(|index| paragraph(&format!("A{index}")))
        .collect();
    let second = (0..5)
        .map(|index| paragraph(&format!("B{index}")))
        .collect();
    let mut document = small_document(
        vec![(first, 2), (second, 2), (vec![paragraph("END")], 1)],
        16 * LINE,
    );
    document.sections[1].kind = SectionStart::NextColumn;
    let pages = layout(&document);
    assert_eq!(
        column_counts(&pages),
        [vec![10, 5, 1]],
        "nextColumn is a mandatory boundary during replay, not a balance opportunity"
    );
    assert_eq!(
        pages[0].columns,
        [
            Rect::new(0, 0, 1200, 10 * LINE),
            Rect::new(1440, 0, 1200, 10 * LINE),
            Rect::new(0, 10 * LINE, 2640, 6 * LINE),
        ]
    );
    assert_eq!(position(&pages[0], 10).1, position(&pages[0], 0).1);
    assert_eq!(
        position(&pages[0], 15),
        (0, position(&pages[0], 0).1 + 10 * LINE)
    );
    let trace = record(&pages);
    assert_eq!(trace.pages[0].lines[10].source.unwrap().start, 30);
    assert_eq!(trace.pages[0].lines[15].source.unwrap().start, 45);
}

#[cfg(feature = "fontenv")]
#[test]
#[ignore = "needs RSWORD_TEST_TIMES_NEW_ROMAN"]
fn captured_canonical_mac_groups_with_real_times_new_roman() {
    use rsword_layout_core::{RealMetrics, font::FontRegistry, load_document};

    let font = std::env::var_os("RSWORD_TEST_TIMES_NEW_ROMAN")
        .expect("set RSWORD_TEST_TIMES_NEW_ROMAN to Times New Roman.ttf");
    let mut registry = FontRegistry::new();
    registry.add(std::fs::read(font).unwrap(), 0).unwrap();
    assert!(registry.covers_family("Times New Roman"));
    let metrics = RealMetrics::new(&registry);
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (directory, name, expected) in [
        (
            "column-balance-canonical-2026-09-27",
            "terminal40-noBalance0.docx",
            vec![32, 8],
        ),
        (
            "column-balance-canonical-2026-09-27",
            "terminal40-noBalance1.docx",
            vec![32, 8],
        ),
        (
            "column-balance-canonical-2026-09-27",
            "continuous40-noBalance0.docx",
            vec![20, 20, 1],
        ),
        (
            "column-balance-canonical-2026-09-27",
            "continuous40-noBalance1.docx",
            vec![20, 20, 1],
        ),
        (
            "column-balance-canonical-2026-09-27",
            "continuous40-break10-noBalance0.docx",
            vec![10, 31, 1],
        ),
        (
            "column-balance-canonical-2026-09-27",
            "continuous40-break10-noBalance1.docx",
            vec![10, 31, 1],
        ),
        (
            "column-balance-canonical-same-columns-2026-09-27",
            "continuous40-sameCols-noBalance0.docx",
            vec![20, 20, 1, 0],
        ),
    ] {
        let document = load_document(&std::fs::read(fixtures.join(directory).join(name)).unwrap())
            .unwrap()
            .layout_document();
        let pages = Engine::new(&metrics, PageSetup::a4())
            .with_platform(Platform::Desktop, View::Print)
            .layout_document(&document);
        assert_sources(&pages, &document);
        assert_eq!(column_counts(&pages), [expected], "{name}");
        if name.starts_with("continuous") {
            let hard_break = name.contains("break10");
            let end_line = if hard_break { 41 } else { 40 };
            let top_lines = if hard_break { 31 } else { 20 };
            assert_eq!(
                position(&pages[0], end_line),
                (720, position(&pages[0], 0).1 + top_lines * LINE),
                "{name}"
            );
            let trace = record(&pages);
            assert_eq!(
                trace.pages[0].lines[end_line as usize]
                    .source
                    .unwrap()
                    .start,
                if hard_break { 201 } else { 200 },
                "{name}"
            );
        }
    }
}
