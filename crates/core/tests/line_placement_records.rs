//! Engine diagnostic provenance, not Word line-box measurements.

use rsword_layout_core::{
    BreakOpportunity, Color, Engine, FontMetrics, FontSpec, LayoutRecord, LineRule,
    Margins, Page, PageSetup, Para, PlaceholderKind, Run, SimpleMetrics, Size,
    TextMetrics, TraceMeta, document_from_json, paint_document, to_trace_json,
};
use serde_json::{Value, json};

struct FineMetrics;

impl FontMetrics for FineMetrics {
    fn measure(&self, text: &str, _: &FontSpec) -> TextMetrics {
        TextMetrics { advance: text.len() as i32 * 50, ascent: 136, descent: 34, line_gap: 25 }
    }

    fn natural_height_fine(&self, _: &str, _: &FontSpec) -> i64 { 978 }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }

    fn quantize_baseline_fine(&self, y: i64) -> i64 { (y + 12) / 24 * 24 }
}

fn para(text: &str, rise: i64) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(), font: FontSpec::new("synthetic", 17), color: Color::BLACK,
            placeholders: text.matches('\u{fffc}').map(|_| PlaceholderKind::LineBreak).collect(),
            rise: 0, rise_fine: Some(rise), hidden: false,
        }],
        ..Para::default()
    }
}

fn record(pages: &[Page]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
}

fn trace(pages: &[Page]) -> Value {
    serde_json::from_str(&to_trace_json(&record(pages), &TraceMeta::default())).unwrap()
}

#[test]
fn exact_integer_decisions_survive_paint_before_run_shifts() {
    let setup = PageSetup { size: Size::new(10000, 10000), margins: Margins::uniform(1) };
    let mut raised = para("b", 357).runs.remove(0);
    raised.font.family = "second".into();
    let input = [Para { runs: vec![para("a", 0).runs.remove(0), raised], ..Para::default() }, para("c", -84)];
    let pages = Engine::new(&FineMetrics, setup).layout(&input);
    let data = trace(&pages);
    let lines = data["pages"][0]["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["engineVerticalDiagnostic"], json!({
        "unit": "1/7200in", "top": 5, "advance": 978, "required": 978,
        "baselineOffset": 680, "baseline": 696,
    }));
    assert_eq!(lines[1]["engineVerticalDiagnostic"], json!({
        "unit": "1/7200in", "top": 983, "advance": 978, "required": 978,
        "baselineOffset": 680, "baseline": 1656,
    }));
    let origins: Vec<_> = pages[0].fragments.iter().filter_map(|f| match f {
        rsword_layout_core::Fragment::Text(t) => Some(t.baseline_fine), _ => None,
    }).collect();
    assert_eq!(origins, [696, 339, 1740]);
}

#[test]
fn final_page_placement_excludes_discarded_keep_trials() {
    let setup = PageSetup { size: Size::new(10000, 781), margins: Margins::uniform(0) };
    let input = [para("intro", 0), Para { keep_lines: true, ..para("a\u{fffc}b\u{fffc}c", 0) }];
    let pages = Engine::new(&FineMetrics, setup).layout(&input);
    let output = record(&pages);
    assert_eq!(output.pages.iter().map(|p| p.lines.len()).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(pages.iter().map(|p| p.line_placements.len()).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(output.pages[1].lines.iter().map(|l| l.placement.unwrap().top_fine).collect::<Vec<_>>(), [0, 978, 1956]);
}

#[test]
fn balanced_columns_and_continuous_groups_keep_actual_page_origins() {
    let p = || json!({"kind": "text", "props": {"spacing": {"before": 0, "after": 0, "lineRule": "exact", "line": 481}},
        "inlines": [{"kind": "run", "text": "a", "props": {"size": 17}}]});
    let mut paras: Vec<_> = (0..6).map(|_| p()).collect();
    paras[0]["props"]["spacing"]["lineRule"] = json!("auto");
    paras[0]["props"]["spacing"]["line"] = json!(240);
    paras[0]["facts"] = json!({"hasSectPr": true});
    paras[4]["facts"] = json!({"hasSectPr": true});
    let section = |range, columns, kind| json!({"blockRange": range, "props": {"kind": kind,
        "pageSize": {"w": 10000, "h": 10000}, "pageMargins": {"top": 1, "bottom": 0, "left": 0, "right": 0},
        "columns": {"num": columns, "space": 100, "equalWidth": true}}});
    let doc = document_from_json(&json!({"main": paras, "sections": [
        section([0, 1], 1, "nextPage"), section([1, 5], 2, "continuous"),
        section([5, 6], 1, "continuous"),
    ]}));
    let pages = Engine::new(&FineMetrics, PageSetup::a4()).layout_document(&doc);
    let output = record(&pages);
    assert_eq!(output.pages.len(), 1);
    assert_eq!(output.pages[0].lines.iter().map(|l| l.column).collect::<Vec<_>>(), [Some(0), Some(1), Some(1), Some(2), Some(2), Some(3)]);
    assert_eq!(output.pages[0].lines.iter().map(|l| l.placement.unwrap().top_fine).collect::<Vec<_>>(), [5, 983, 3388, 983, 3388, 5793]);
}

#[test]
fn sparse_and_missing_placements_are_not_inferred_from_glyph_origins() {
    let mut page = Engine::new(&FineMetrics, PageSetup::a4()).layout(&[Para {
        line_rule: LineRule::Exact, line_value: 480, ..para("a", 357)
    }]).remove(0);
    let placement = page.line_placements[0];
    for f in &mut page.fragments {
        if let rsword_layout_core::Fragment::Text(t) = f { t.line = 7; }
    }
    assert!(trace(&[page.clone()])["pages"][0]["lines"][0]["engineVerticalDiagnostic"].is_null());
    page.line_placements.resize(8, None);
    page.line_placements[7] = placement;
    assert_eq!(record(&[page.clone()]).pages[0].lines[0].placement, placement);
    page.line_placements.clear();
    assert!(record(&[page]).pages[0].lines[0].placement.is_none());
}

#[test]
fn source_only_empty_section_line_keeps_its_placement() {
    let pages = Engine::new(&FineMetrics, PageSetup::a4()).layout(&[Para {
        terminator: rsword_layout_core::LineTerminator::SectionBreak,
        ..Para::default()
    }]);
    let output = record(&pages);
    let line = &output.pages[0].lines[0];
    assert_eq!(line.source, Some(rsword_layout_core::SourceRange::new(0, 1)));
    assert!(line.glyphs.is_empty());
    assert_eq!(line.placement, pages[0].line_placements[0]);
    assert!(line.placement.is_some());
}
