//! Engine column bookkeeping and trace serialization, not Word measurements.

use rsword_layout_core::{
    Color, Engine, FontSpec, Fragment, LayoutRecord, LineTerminator, Page, PageSetup, Para, Rect,
    Run, SimpleMetrics, SourceRange, TraceMeta, paint_document, paint_page, to_trace_json,
};
use serde_json::{Value, json};

fn run(text: &str, family: &str) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new(family, 24),
        color: Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn column_page() -> Page {
    let input = [
        Para {
            runs: vec![run("a", "A"), run("b", "B")],
            ..Para::default()
        },
        Para {
            runs: vec![run("c", "A"), run("d", "B")],
            ..Para::default()
        },
    ];
    let mut pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&input);
    assert_eq!(pages.len(), 1);
    let mut page = pages.remove(0);
    page.columns = vec![
        Rect::new(720, 720, 4873, 15398),
        Rect::new(6313, 720, 4873, 15398),
    ];
    page.line_columns = vec![0, 1];
    let first_baseline = page
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) => Some((text.baseline_y, text.baseline_fine)),
            _ => None,
        })
        .unwrap();
    for fragment in &mut page.fragments {
        if let Fragment::Text(text) = fragment {
            let x = if text.line == 0 { 720 } else { 6313 };
            text.x = x;
            text.x_pt = f64::from(x) / 20.0;
            text.baseline_y = first_baseline.0;
            text.baseline_fine = first_baseline.1;
        }
    }
    page
}

fn record(page: &Page) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(std::slice::from_ref(page), None, &[]))
}

fn trace(record: &LayoutRecord) -> Value {
    serde_json::from_str(&to_trace_json(record, &TraceMeta::default())).unwrap()
}

#[test]
fn column_frames_and_sparse_page_line_ids_survive_paint() {
    let mut page = column_page();
    for fragment in &mut page.fragments {
        if let Fragment::Text(text) = fragment {
            text.line = if text.line == 0 { 7 } else { 11 };
        }
    }
    page.line_columns = vec![0; 12];
    page.line_columns[11] = 1;
    let painted = paint_page(&page, None, &[]);
    assert_eq!(painted.columns, page.columns);
    assert_eq!(painted.line_columns, page.line_columns);
    let output = record(&page);
    assert_eq!(output.pages[0].columns, page.columns);
    assert_eq!(output.pages[0].lines.len(), 2);
    assert_eq!(output.pages[0].lines[0].column, Some(0));
    assert_eq!(output.pages[0].lines[1].column, Some(1));
}

#[test]
fn same_baseline_in_different_columns_does_not_merge_source_lines() {
    let output = record(&column_page());
    let lines = &output.pages[0].lines;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].source, Some(SourceRange::new(0, 3)));
    assert_eq!(lines[1].source, Some(SourceRange::new(3, 6)));
    assert!(lines.iter().all(|line| line.glyphs.is_empty()));
    assert!(
        lines
            .iter()
            .all(|line| line.terminator == LineTerminator::ParagraphMark)
    );
}

#[test]
fn ownership_is_not_guessed_from_text_coordinates() {
    let mut page = column_page();
    page.line_columns.clear();
    assert!(
        record(&page).pages[0]
            .lines
            .iter()
            .all(|line| line.column.is_none())
    );
    page.line_columns = vec![0, 99];
    let output = record(&page);
    assert_eq!(output.pages[0].lines[0].column, Some(0));
    assert_eq!(output.pages[0].lines[1].column, None);
}

#[test]
fn multiple_column_frames_use_points_and_lines_keep_their_column_numbers() {
    let output = trace(&record(&column_page()));
    assert_eq!(
        output["pages"][0]["columns"],
        json!([
            {"index": 0, "x": 36.0, "y": 36.0, "width": 243.65, "height": 769.9},
            {"index": 1, "x": 315.65, "y": 36.0, "width": 243.65, "height": 769.9}
        ])
    );
    assert_eq!(output["pages"][0]["lines"][0]["column"], 0);
    assert_eq!(output["pages"][0]["lines"][1]["column"], 1);
    assert_eq!(output["pages"][0]["lines"][1]["index"], 1);
}

#[test]
fn unknown_column_is_explicit_null_on_multiple_column_pages() {
    let mut page = column_page();
    page.line_columns.clear();
    let output = trace(&record(&page));
    for line in output["pages"][0]["lines"].as_array().unwrap() {
        assert!(line.as_object().unwrap().contains_key("column"));
        assert!(line["column"].is_null());
    }
}

#[test]
fn single_column_payload_is_unchanged_by_internal_column_metadata() {
    let mut output = record(&column_page());
    output.pages[0].columns.clear();
    for line in &mut output.pages[0].lines {
        line.column = None;
    }
    let before = to_trace_json(&output, &TraceMeta::default());
    output.pages[0].columns = vec![Rect::new(720, 720, 10466, 15398)];
    for line in &mut output.pages[0].lines {
        line.column = Some(0);
    }
    let after = to_trace_json(&output, &TraceMeta::default());
    assert_eq!(before, after);
    assert!(!after.contains("\"columns\""));
    assert!(!after.contains("\"column\""));
}

#[test]
fn assumed_column_control_count_has_a_distinct_trace_terminator() {
    let mut output = record(&column_page());
    output.pages[0].lines[0].terminator = LineTerminator::ColumnBreak;
    let value = trace(&output);
    assert_eq!(value["pages"][0]["lines"][0]["terminator"], "COLUMN_BREAK");
    // Preserves the old soft-return convention; no Word glyph capture proves it.
    assert_eq!(value["pages"][0]["lines"][0]["terminatorExpectedGlyphs"], 1);
}
