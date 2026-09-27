//! Sequential column flow through the native document projection.
//!
//! Android Word reports establish the 32-lines-per-column capacity, the 64/65
//! physical-page boundary, and distinct column/page breaks. Synthetic metrics
//! below test source accounting and inferred flow constraints, not new Word
//! captures. Final-page balancing is deliberately outside these expectations.

use std::collections::BTreeSet;

use rsword_layout_core::{
    Engine, Fragment, LayoutDocument, LayoutRecord, Page, PageSetup, Platform, Rect, SimpleMetrics,
    View, document_from_json, paint_document,
};
use serde_json::{Value, json};

const LINE: i32 = 480;
const MODES: [(Platform, View); 4] = [
    (Platform::Desktop, View::Print),
    (Platform::Desktop, View::Mobile),
    (Platform::Android, View::Print),
    (Platform::Android, View::Mobile),
];

fn paragraph(text: &str) -> Value {
    json!({
        "kind": "text",
        "props": {"spacing": {"before": 0, "after": 0, "line": LINE, "lineRule": "exact"}},
        "inlines": [{"kind": "run", "text": text,
            "props": {"fonts": {"ascii": "Calibri", "hAnsi": "Calibri"}, "size": 24}}]
    })
}

fn break_paragraph(text: &str, kind: &str) -> Value {
    let mut para = paragraph(text);
    para["inlines"][0]["segments"] = json!([{"kind": {"kind": "br", "breakKind": kind}}]);
    para
}

fn input(paras: Vec<Value>, columns: Value, width: i32, height: i32, margin: i32) -> Value {
    let count = paras.len();
    json!({
        "main": paras,
        "sections": [{"blockRange": [0, count], "props": {
            "kind": "nextPage", "pageSize": {"w": width, "h": height},
            "pageMargins": {"top": margin, "right": margin, "bottom": margin,
                "left": margin, "gutter": 0}, "columns": columns,
        }}]
    })
}

fn document(paras: Vec<Value>, columns: Value, width: i32, height: i32) -> LayoutDocument {
    document_from_json(&input(paras, columns, width, height, 0))
}

fn equal_columns(count: usize, gap: i32) -> Value {
    json!({"equalWidth": true, "num": count, "space": gap})
}

fn two_columns(paras: Vec<Value>, lines: i32) -> LayoutDocument {
    document(paras, equal_columns(2, 240), 2640, lines * LINE)
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

fn first_position(page: &Page, line: u32) -> (i32, i64) {
    page.fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.line == line => Some((text.x, text.baseline_fine)),
            _ => None,
        })
        .expect("line has a fragment")
}

fn assert_sources_and_page_line_ids(pages: &[Page], document: &LayoutDocument) {
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
            "line IDs belong to the physical page"
        );
        for line in &captured.lines {
            let source = line.source.expect("line retains source interval");
            assert_eq!(
                source.start, cursor,
                "column transition loses or duplicates source"
            );
            assert!(source.end > source.start);
            cursor = source.end;
        }
    }
    let units: usize = document
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
    assert_eq!(cursor as usize, units);
}

fn layout(document: &LayoutDocument, platform: Platform, view: View) -> Vec<Page> {
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4())
        .with_platform(platform, view)
        .layout_document(document);
    assert_sources_and_page_line_ids(&pages, document);
    pages
}

#[test]
fn reported_two_column_capacity_with_assumed_sequential_last_page() {
    for count in [64, 65, 70] {
        let document = document_from_json(&input(
            vec![paragraph("x"); count],
            equal_columns(2, 720),
            11906,
            16838,
            720,
        ));
        let pages = layout(&document, Platform::Android, View::Print);
        assert_eq!(pages.len(), if count == 64 { 1 } else { 2 });
        assert_eq!(
            pages[0].columns,
            [
                Rect::new(720, 720, 4873, 15398),
                Rect::new(6313, 720, 4873, 15398)
            ]
        );
        assert_eq!(column_counts(&pages)[0], [32, 32]);
        if count > 64 {
            assert_eq!(column_counts(&pages)[1], [count - 64, 0]);
            assert_eq!(record(&pages).pages[1].lines[0].source.unwrap().start, 128);
        }
    }
}

#[test]
fn assumed_long_paragraph_reflows_at_each_column_without_reusing_page_line_ids() {
    let document = two_columns(vec![paragraph(&"0".repeat(65))], 2);
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![2, 2], vec![2, 1]]);
        assert_eq!(first_position(&pages[0], 0).0, 0);
        assert_eq!(first_position(&pages[0], 2).0, 1440);
        assert_eq!(
            first_position(&pages[0], 0).1,
            first_position(&pages[0], 2).1
        );
        assert_eq!(record(&pages).pages[1].lines[0].source.unwrap().start, 40);
    }
}

#[test]
fn assumed_unequal_columns_reformat_the_suffix_at_the_new_width() {
    let document = document(
        vec![paragraph(&"0".repeat(50))],
        json!({"equalWidth": false, "col": [{"w": 600, "space": 240}, {"w": 1200}]}),
        2040,
        2 * LINE,
    );
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![2, 2], vec![2, 1]]);
        let starts: Vec<Vec<_>> = record(&pages)
            .pages
            .iter()
            .map(|p| {
                p.lines
                    .iter()
                    .map(|line| line.source.unwrap().start)
                    .collect()
            })
            .collect();
        assert_eq!(starts, [vec![0, 5, 10, 20], vec![30, 35, 40]]);
        assert_eq!(first_position(&pages[0], 2).0, 840);
    }
}

#[test]
fn reported_column_break_moves_region_and_keeps_its_paragraph_mark_separate() {
    for count in [1, 2] {
        let document = document(
            vec![
                paragraph("a"),
                break_paragraph("\u{fffc}", "column"),
                paragraph("b"),
            ],
            equal_columns(count, 240),
            if count == 1 { 1200 } else { 2640 },
            4 * LINE,
        );
        for (platform, view) in MODES {
            let pages = layout(&document, platform, view);
            let expected = if count == 1 {
                vec![vec![2], vec![2]]
            } else {
                vec![vec![2, 2]]
            };
            assert_eq!(column_counts(&pages), expected, "{platform:?} {view:?}");
            let ranges: Vec<_> = record(&pages)
                .pages
                .iter()
                .flat_map(|p| &p.lines)
                .map(|line| {
                    let s = line.source.unwrap();
                    (s.start, s.end)
                })
                .collect();
            assert_eq!(ranges, [(0, 2), (2, 3), (3, 4), (4, 6)]);
        }
    }
}

#[test]
fn assumed_explicit_page_break_skips_unused_columns() {
    let document = two_columns(vec![break_paragraph("a\u{fffc}b", "page")], 4);
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![1, 0], vec![1, 0]]);
        assert_eq!(first_position(&pages[1], 0).0, 0);
    }
}

#[test]
fn assumed_page_break_before_targets_a_physical_page() {
    let mut next = paragraph("b");
    next["props"]["pageBreakBefore"] = json!(true);
    let document = two_columns(vec![paragraph("a"), next], 4);
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![1, 0], vec![1, 0]]);
    }
}

#[test]
fn assumed_keep_lines_moves_to_the_next_column_before_emitting_its_first_line() {
    let mut block = paragraph(&"0".repeat(15));
    block["props"]["keepLines"] = json!(true);
    let document = two_columns(
        vec![paragraph("x"), paragraph("x"), paragraph("x"), block],
        4,
    );
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![3, 2]]);
        assert_eq!(record(&pages).pages[0].lines[3].source.unwrap().start, 6);
    }
}

#[test]
fn assumed_keep_next_chain_moves_together_into_the_next_column() {
    let mut kept = paragraph("a");
    kept["props"]["keepNext"] = json!(true);
    let document = two_columns(
        vec![
            paragraph("x"),
            paragraph("x"),
            kept.clone(),
            kept,
            paragraph("b"),
        ],
        4,
    );
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![2, 3]]);
    }
}

#[test]
fn assumed_widow_control_applies_on_both_sides_of_column_boundaries() {
    for (filler, zeros) in [(3, 15), (1, 35)] {
        let mut block = paragraph(&"0".repeat(zeros));
        block["props"]["widowControl"] = json!(true);
        let mut paras = vec![paragraph("x"); filler];
        paras.push(block);
        let document = two_columns(paras, 4);
        for (platform, view) in MODES {
            let pages = layout(&document, platform, view);
            assert_eq!(column_counts(&pages), [vec![3, 2]]);
        }
    }
}

#[test]
fn assumed_an_oversized_line_still_advances_in_an_empty_second_column() {
    let mut tall = paragraph("t");
    tall["props"]["spacing"]["line"] = json!(1200);
    tall["props"]["keepLines"] = json!(true);
    tall["props"]["widowControl"] = json!(true);
    let document = two_columns(
        vec![paragraph("x"), paragraph("x"), tall, paragraph("z")],
        2,
    );
    for (platform, view) in MODES {
        let pages = layout(&document, platform, view);
        assert_eq!(column_counts(&pages), [vec![2, 1], vec![1, 0]]);
    }
}

#[test]
fn assumed_next_column_section_uses_an_already_open_empty_region() {
    for previous_page_break in [false, true] {
        let mut first = if previous_page_break {
            break_paragraph("a\u{fffc}", "page")
        } else {
            paragraph("a")
        };
        first["facts"] = json!({"hasSectPr": true});
        let mut json = input(
            vec![first, paragraph("b")],
            equal_columns(2, 240),
            2640,
            4 * LINE,
            0,
        );
        let mut first_section = json["sections"][0].clone();
        first_section["blockRange"] = json!([0, 1]);
        let mut next_section = first_section.clone();
        next_section["blockRange"] = json!([1, 2]);
        next_section["props"]["kind"] = json!("nextColumn");
        json["sections"] = json!([first_section, next_section]);
        let document = document_from_json(&json);
        for (platform, view) in MODES {
            let pages = layout(&document, platform, view);
            let expected = if previous_page_break {
                vec![vec![1, 0], vec![1, 0]]
            } else {
                vec![vec![1, 1]]
            };
            assert_eq!(column_counts(&pages), expected);
        }
    }
}

#[cfg(feature = "fontenv")]
#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI and sibling word_analyse fixtures"]
fn reported_android_calibri_column_page_counts() {
    use rsword_layout_core::{RealMetrics, font::FontRegistry, load_document};
    let font = std::env::var_os("RSWORD_TEST_CALIBRI").expect("set RSWORD_TEST_CALIBRI");
    let mut registry = FontRegistry::new();
    registry.add(std::fs::read(font).unwrap(), 0).unwrap();
    let metrics = RealMetrics::new(&registry);
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../word_analyse/fixtures");
    for (name, expected) in [
        ("twocol64.docx", vec![vec![32, 32]]),
        ("twocol65.docx", vec![vec![32, 32], vec![1, 0]]),
        ("twocol70.docx", vec![vec![32, 32], vec![6, 0]]),
        ("br-column.docx", vec![vec![11, 6]]),
        ("br-column-one.docx", vec![vec![11], vec![6]]),
    ] {
        let loaded = load_document(&std::fs::read(fixtures.join(name)).unwrap()).unwrap();
        let document = loaded.layout_document();
        let pages = Engine::new(&metrics, PageSetup::a4())
            .with_platform(Platform::Android, View::Print)
            .layout_document(&document);
        assert_eq!(column_counts(&pages), expected, "{name}");
        assert_sources_and_page_line_ids(&pages, &document);
    }
}
