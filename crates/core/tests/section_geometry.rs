//! Document page geometry goes through the real DOCX parser before layout.
//!
//! Widths and exact-height capacities match Android Word observations in
//! word_analyse/reports/rsword-diff/margin.md. Multi-section SimpleMetrics cases
//! check internal source accounting and projection, not a new Word capture.

use rsword::bind::native::SessionTable;
use rsword_layout_core::{
    DrawCmd, Engine, Fragment, LayoutDocument, LayoutRecord, Page, PageOverrides, PageSetup, Rect,
    SimpleMetrics, Size, WrapContext, WrapRegion, document_from_json, load_document,
    paint_document,
};
use serde_json::{Value, json};

fn parsed(body: &str) -> Value {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    let mut sessions = SessionTable::default();
    let id = sessions
        .open(&rsword::save::blank_docx(None).unwrap(), None)
        .unwrap();
    let blank: Value = serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
    let op = json!({"op": "replacePartXml", "part": blank["mainPart"], "xml": xml});
    sessions.apply(&id, &op.to_string(), None).unwrap();
    let bytes = sessions.save(&id, None).unwrap();
    sessions.close(&id);
    load_document(&bytes).unwrap().json
}

fn paragraph(text: &str, section: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/>{section}</w:pPr><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#
    )
}

fn section(size: &str, margins: &str, kind: &str) -> String {
    format!(r#"<w:sectPr><w:type w:val="{kind}"/><w:pgSz {size}/><w:pgMar {margins}/></w:sectPr>"#)
}

fn uniform_section(width: i32, height: i32, margin: i32, kind: &str) -> String {
    section(
        &format!(r#"w:w="{width}" w:h="{height}""#),
        &format!(r#"w:top="{margin}" w:right="{margin}" w:bottom="{margin}" w:left="{margin}""#),
        kind,
    )
}

fn layout(doc: &LayoutDocument) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(doc)
}

fn line_sources(pages: &[Page]) -> Vec<(u32, u32)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|page| &page.lines)
        .map(|line| {
            let source = line.source.expect("text line has a source interval");
            (source.start, source.end)
        })
        .collect()
}

fn line_counts(pages: &[Page]) -> Vec<usize> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .map(|page| page.lines.len())
        .collect()
}

#[test]
fn declared_page_dimensions_and_independent_margins_reach_layout() {
    let geometry = section(
        r#"w:w="10000" w:h="12000""#,
        r#"w:top="1000" w:right="800" w:bottom="600" w:left="400""#,
        "nextPage",
    );
    let json = parsed(&format!("{}{geometry}", paragraph("A", "")));
    assert_eq!(json["sections"][0]["props"]["pageSize"]["w"], 10000);
    assert_eq!(json["sections"][0]["props"]["pageMargins"]["left"], 400);
    let doc = document_from_json(&json);
    assert_eq!(
        doc.skipped_blocks, 0,
        "body sectPr is geometry, not omitted content"
    );
    assert!(
        !doc.trace_metadata()["sourceCoverage"]
            .as_str()
            .unwrap()
            .contains("partial")
    );
    let pages = layout(&doc);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].size, Size::new(10000, 12000));
    assert_eq!(pages[0].content_area, Rect::new(400, 1000, 8800, 10400));
    let first_text = pages[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(first_text.x, 400);
}

#[test]
fn observed_gutter_adds_to_left_margin_and_header_distance_does_not_reduce_body() {
    let geometry = section(
        r#"w:w="11906" w:h="16838""#,
        r#"w:top="720" w:right="720" w:bottom="720" w:left="1440" w:gutter="720" w:header="1440" w:footer="1440""#,
        "nextPage",
    );
    let doc = document_from_json(&parsed(&format!("{}{geometry}", paragraph("A", ""))));
    assert_eq!(
        layout(&doc)[0].content_area,
        Rect::new(2160, 720, 9026, 15398)
    );
}

#[test]
fn observed_landscape_flag_does_not_swap_explicit_dimensions() {
    for (width, height, orient) in [
        (11906, 16838, r#"w:orient="landscape""#),
        (16838, 11906, ""),
    ] {
        let geometry = section(
            &format!(r#"w:w="{width}" w:h="{height}" {orient}"#),
            r#"w:top="720" w:right="720" w:bottom="720" w:left="720""#,
            "nextPage",
        );
        let doc = document_from_json(&parsed(&format!("{}{geometry}", paragraph("A", ""))));
        assert_eq!(layout(&doc)[0].size, Size::new(width, height));
    }
}

#[test]
fn observed_page_heights_and_asymmetric_margins_control_exact_line_capacity() {
    for (width, height, top, bottom, expected) in [
        (11906, 16838, 1440, 720, vec![30, 10]),
        (11906, 16838, 720, 1440, vec![30, 10]),
        (10000, 12000, 720, 720, vec![22, 18]),
        (16838, 11906, 720, 720, vec![21, 19]),
    ] {
        let geometry = section(
            &format!(r#"w:w="{width}" w:h="{height}""#),
            &format!(r#"w:top="{top}" w:right="720" w:bottom="{bottom}" w:left="720""#),
            "nextPage",
        );
        let body = format!("{}{geometry}", paragraph("0", "").repeat(40));
        let doc = document_from_json(&parsed(&body));
        let pages = layout(&doc);
        assert_eq!(
            line_counts(&pages),
            expected,
            "{width} x {height}, top {top}, bottom {bottom}"
        );
        let sources = line_sources(&pages);
        assert_eq!(sources.first(), Some(&(0, 2)));
        assert_eq!(sources.last(), Some(&(78, 80)));
        assert!(sources.windows(2).all(|pair| pair[0].1 == pair[1].0));
    }
}

#[test]
fn next_page_section_changes_dimensions_without_resetting_source_positions() {
    let first = uniform_section(2880, 5000, 720, "nextPage");
    let second = uniform_section(4320, 6000, 720, "nextPage");
    let body = format!(
        "{}{}{second}",
        paragraph(&"0".repeat(22), &first),
        paragraph(&"0".repeat(22), "")
    );
    let doc = document_from_json(&parsed(&body));
    assert_eq!(doc.sections[0].para_range, 0..1);
    assert_eq!(doc.sections[1].para_range, 1..2);
    let pages = layout(&doc);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].size, Size::new(2880, 5000));
    assert_eq!(pages[1].size, Size::new(4320, 6000));
    assert_eq!(line_counts(&pages), vec![2, 1]);
    let sources = line_sources(&pages);
    assert_eq!(sources.first().unwrap().0, 0);
    assert_eq!(sources.last().unwrap().1, 46);
    assert!(sources.windows(2).all(|pair| pair[0].1 == pair[1].0));
    assert_eq!(sources.last().unwrap().0, 23);
}

#[test]
fn continuous_section_with_identical_geometry_keeps_current_page_and_y() {
    let first = uniform_section(10000, 12000, 720, "nextPage");
    let second = uniform_section(10000, 12000, 720, "continuous");
    let body = format!("{}{}{second}", paragraph("A", &first), paragraph("B", ""));
    let doc = document_from_json(&parsed(&body));
    let pages = layout(&doc);
    assert_eq!(line_counts(&pages), vec![2]);
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    assert_eq!(record.pages[0].lines.len(), 2);
    assert_eq!(line_sources(&pages), vec![(0, 2), (2, 4)]);
}

#[test]
fn section_beginning_with_a_skipped_table_still_switches_page_geometry() {
    let first = uniform_section(10000, 12000, 720, "nextPage");
    let second = uniform_section(9000, 11000, 720, "nextPage");
    let table =
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>not projected</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let body = format!(
        "{}{table}{}{second}",
        paragraph("A", &first),
        paragraph("B", "")
    );
    let json = parsed(&body);
    assert_eq!(json["main"][1]["kind"], "table");
    let doc = document_from_json(&json);
    // The body-level sectPr is metadata; only the table is omitted content.
    assert_eq!(doc.skipped_blocks, 1);
    assert!(
        doc.trace_metadata()["sourceCoverage"]
            .as_str()
            .unwrap()
            .contains("partial")
    );
    assert_eq!(doc.sections[0].block_range, 0..1);
    assert_eq!(doc.sections[1].block_range, 1..4);
    assert_eq!(doc.sections[1].para_range, 1..2);
    let pages = layout(&doc);
    assert_eq!(line_counts(&pages), vec![1, 1]);
    assert_eq!(pages[1].size, Size::new(9000, 11000));
    assert_eq!(line_sources(&pages), vec![(0, 2), (2, 4)]);
}

#[test]
fn absent_document_geometry_uses_explicit_host_fallback() {
    let doc = document_from_json(&parsed(&paragraph("A", "")));
    let pages = layout(&doc);
    let fallback = PageSetup::a4();
    assert_eq!(pages[0].size, fallback.size);
    assert_eq!(pages[0].content_area, fallback.content_area());
}

#[test]
fn missing_fields_fall_back_independently_without_erasing_declared_values() {
    let geometry = section(r#"w:w="10000""#, r#"w:left="720""#, "nextPage");
    let doc = document_from_json(&parsed(&format!("{}{geometry}", paragraph("A", ""))));
    let pages = layout(&doc);
    assert_eq!(pages[0].size, Size::new(10000, 16838));
    assert_eq!(pages[0].content_area, Rect::new(720, 1440, 7840, 13958));
}

#[test]
fn invalid_geometry_reports_fallback_without_negative_or_overflowing_content_area() {
    for margins in [
        r#"w:top="720" w:right="6000" w:bottom="720" w:left="6000""#,
        r#"w:top="720" w:right="720" w:bottom="720" w:left="2147483647" w:gutter="2147483647""#,
    ] {
        let geometry = section(r#"w:w="10000" w:h="12000""#, margins, "nextPage");
        let doc = document_from_json(&parsed(&format!("{}{geometry}", paragraph("A", ""))));
        let pages = layout(&doc);
        assert_eq!(pages[0].size, PageSetup::a4().size);
        assert_eq!(pages[0].content_area, PageSetup::a4().content_area());
        assert!(
            !doc.diagnostics.is_empty(),
            "invalid input must be observable"
        );
    }
}

#[test]
fn diagnostic_overrides_apply_to_all_sections_and_invalid_override_is_atomic() {
    let first = uniform_section(10000, 12000, 720, "nextPage");
    let second = uniform_section(9000, 11000, 720, "nextPage");
    let body = format!("{}{}{second}", paragraph("A", &first), paragraph("B", ""));
    let mut doc = document_from_json(&parsed(&body));
    let original: Vec<_> = doc.sections.iter().map(|section| section.setup).collect();
    assert!(
        doc.apply_page_overrides(PageOverrides {
            content_width: Some(9500),
            ..PageOverrides::default()
        })
        .is_err()
    );
    assert_eq!(
        doc.sections
            .iter()
            .map(|section| section.setup)
            .collect::<Vec<_>>(),
        original
    );
    doc.apply_page_overrides(PageOverrides {
        margin: Some(500),
        page_width: Some(8000),
        content_width: Some(6001),
    })
    .unwrap();
    let pages = layout(&doc);
    assert_eq!(pages.len(), 2);
    for page in pages {
        assert_eq!(page.size.width, 8000);
        assert_eq!(page.content_area.x, 999);
        assert_eq!(page.content_area.width, 6001);
        assert_eq!(page.content_area.y, 500);
    }
    assert_eq!(doc.trace_metadata()["overrides"]["contentWidth"], 6001);
}

#[test]
fn explicit_column_list_without_num_is_projected_without_an_unsupported_flow_diagnostic() {
    let geometry = uniform_section(10000, 12000, 720, "nextPage").replace(
        "</w:sectPr>",
        r#"<w:cols w:equalWidth="0"><w:col w:w="3000" w:space="720"/><w:col w:w="4840"/></w:cols></w:sectPr>"#,
    );
    let json = parsed(&format!("{}{geometry}", paragraph("A", "")));
    assert_eq!(
        json["sections"][0]["props"]["columns"]["col"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let doc = document_from_json(&json);
    assert_eq!(doc.sections[0].columns.count(), 2);
    assert_eq!(
        doc.sections[0].columns.areas(doc.sections[0].setup.content_area()).unwrap(),
        vec![Rect::new(720, 720, 3000, 10560), Rect::new(4440, 720, 4840, 10560)],
    );
    assert!(!doc.diagnostics.iter().any(|message| message.contains("balancing is not implemented")));
    assert!(!doc.diagnostics.iter().any(|message| message.contains("invalid columns")));
}

fn top_left_wrap() -> WrapContext {
    let mut wrap = WrapContext::new();
    wrap.add(WrapRegion::rect(Rect::new(720, 720, 720, 480), 0));
    wrap
}

fn line_first_x(page: &Page, line: u32) -> i32 {
    page.fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.line == line && !text.text.is_empty() => Some(text.x),
            _ => None,
        })
        .unwrap()
}

#[test]
fn page_break_before_requeries_wrap_at_the_new_page_top() {
    let geometry = uniform_section(4320, 2400, 720, "nextPage");
    let body = format!(
        "{}{}{geometry}",
        paragraph("A", ""),
        paragraph(&"0".repeat(28), "")
    );
    let mut doc = document_from_json(&parsed(&body));
    doc.paras[1].page_break_before = true;
    let pages =
        Engine::with_wrap(&SimpleMetrics, PageSetup::a4(), top_left_wrap()).layout_document(&doc);
    assert_eq!(line_counts(&pages), vec![1, 2]);
    assert_eq!(line_first_x(&pages[1], 0), 1440);
    assert_eq!(line_first_x(&pages[1], 1), 720);
    assert_eq!(line_sources(&pages), vec![(0, 2), (2, 20), (20, 31)]);
}

#[test]
fn keep_lines_move_requeries_wrap_before_committing_lines() {
    let geometry = uniform_section(4320, 2400, 720, "nextPage");
    let body = format!(
        "{}{}{geometry}",
        paragraph("A", ""),
        paragraph(&"0".repeat(40), "")
    );
    let mut doc = document_from_json(&parsed(&body));
    doc.paras[1].keep_lines = true;
    let pages =
        Engine::with_wrap(&SimpleMetrics, PageSetup::a4(), top_left_wrap()).layout_document(&doc);
    assert_eq!(line_counts(&pages), vec![1, 2]);
    assert_eq!(line_first_x(&pages[1], 0), 1440);
    assert_eq!(line_first_x(&pages[1], 1), 720);
    assert_eq!(line_sources(&pages), vec![(0, 2), (2, 20), (20, 43)]);
}

#[test]
fn paragraph_suffix_requeries_wrap_after_natural_page_overflow() {
    let geometry = uniform_section(4320, 2400, 720, "nextPage");
    let body = format!(
        "{}{}{geometry}",
        paragraph("A", ""),
        paragraph(&"0".repeat(50), "")
    );
    let doc = document_from_json(&parsed(&body));
    let pages =
        Engine::with_wrap(&SimpleMetrics, PageSetup::a4(), top_left_wrap()).layout_document(&doc);
    assert_eq!(line_counts(&pages), vec![2, 2]);
    assert_eq!(line_first_x(&pages[1], 0), 1440);
    assert_eq!(line_first_x(&pages[1], 1), 720);
    assert_eq!(
        line_sources(&pages),
        vec![(0, 2), (2, 26), (26, 44), (44, 53)]
    );
}

#[test]
fn assumed_continuous_geometry_change_reformats_suffix_when_next_page_starts() {
    let first = uniform_section(4320, 2400, 720, "nextPage");
    let second = uniform_section(2880, 2400, 720, "continuous");
    let body = format!(
        "{}{}{second}",
        paragraph("A", &first),
        paragraph(&"0".repeat(50), "")
    );
    let doc = document_from_json(&parsed(&body));
    assert!(
        doc.diagnostics
            .iter()
            .any(|message| message.contains("continuous"))
    );
    let pages = layout(&doc);
    assert_eq!(line_counts(&pages), vec![2, 2, 1]);
    assert_eq!(pages[0].content_area.width, 2880);
    assert!(
        pages[1..]
            .iter()
            .all(|page| page.content_area.width == 1440)
    );
    assert_eq!(
        line_sources(&pages),
        vec![(0, 2), (2, 26), (26, 38), (38, 50), (50, 53)]
    );
}

#[test]
fn repeated_page_overrides_retain_geometry_and_provenance_of_earlier_calls() {
    let geometry = uniform_section(10000, 12000, 720, "nextPage");
    let mut doc = document_from_json(&parsed(&format!("{}{geometry}", paragraph("A", ""))));
    doc.apply_page_overrides(PageOverrides {
        margin: Some(500),
        content_width: Some(6001),
        ..PageOverrides::default()
    })
    .unwrap();
    doc.apply_page_overrides(PageOverrides {
        page_width: Some(9000),
        ..PageOverrides::default()
    })
    .unwrap();
    let page = &layout(&doc)[0];
    assert_eq!(page.size.width, 9000);
    assert_eq!(page.content_area, Rect::new(1499, 500, 6001, 11000));
    let before = doc.trace_metadata();
    assert_eq!(
        before["overrides"],
        json!({"margin": 500, "pageWidth": 9000, "contentWidth": 6001})
    );
    assert!(
        doc.apply_page_overrides(PageOverrides {
            page_width: Some(5000),
            ..PageOverrides::default()
        })
        .is_err()
    );
    assert_eq!(doc.trace_metadata(), before);
}

fn painted_text(pages: &[Page]) -> String {
    paint_document(pages, None, &[])
        .pages
        .iter()
        .flat_map(|page| &page.cmds)
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn resumed_source_cursor_preserves_hidden_non_bmp_tabs_and_breaks() {
    // The distant wrap region forces the resume path without changing line width.
    // Varying page height must only redistribute existing source lines across pages.
    let paragraph = format!(
        r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>A😀{}</w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>HIDDEN😀</w:t></w:r><w:r><w:t>{}</w:t><w:tab/><w:t>{}</w:t><w:br/><w:t>{}</w:t><w:br w:type="page"/><w:t>{}</w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>TAIL😀</w:t></w:r></w:p>"#,
        "0".repeat(22),
        "B".repeat(12),
        "C".repeat(12),
        "D".repeat(12),
        "E".repeat(50)
    );
    let mut runs = Vec::new();
    for height in [2400, 20000] {
        let geometry = uniform_section(4320, height, 720, "nextPage");
        let doc = document_from_json(&parsed(&format!("{paragraph}{geometry}")));
        let source_end = doc.paras[0]
            .runs
            .iter()
            .map(|run| run.text.encode_utf16().count() as u32)
            .sum::<u32>()
            + 1;
        let mut wrap = WrapContext::new();
        wrap.add(WrapRegion::rect(Rect::new(100000, 100000, 720, 720), 0));
        let pages = Engine::with_wrap(&SimpleMetrics, PageSetup::a4(), wrap).layout_document(&doc);
        let sources = line_sources(&pages);
        assert_eq!(sources.first().unwrap().0, 0);
        assert_eq!(sources.last().unwrap().1, source_end);
        assert!(sources.windows(2).all(|pair| pair[0].1 == pair[1].0));
        let text = painted_text(&pages);
        assert!(!text.contains("HIDDEN"));
        assert!(!text.contains("TAIL"));
        assert_eq!(text.matches('😀').count(), 1);
        for (ch, count) in [('0', 22), ('B', 12), ('C', 12), ('D', 12), ('E', 50)] {
            assert_eq!(text.matches(ch).count(), count);
        }
        runs.push((pages.len(), sources, text));
    }
    assert!(runs[0].0 > runs[1].0);
    assert_eq!(runs[0].1, runs[1].1);
    assert_eq!(runs[0].2, runs[1].2);
}

// These are synthetic physical-page contracts, not Word captures. The external
// sect-odd/sect-even fixtures put their type on the first section and therefore
// cannot measure the parity requested by the following section.
#[test]
fn assumed_odd_page_inserts_a_blank_page_where_even_page_already_matches() {
    for (kind, expected) in [("oddPage", vec![1, 0, 1]), ("evenPage", vec![1, 1])] {
        let first = uniform_section(10000, 12000, 720, "nextPage");
        let second = uniform_section(9000, 11000, 720, kind);
        let body = format!("{}{}{second}", paragraph("A", &first), paragraph("B", ""));
        let doc = document_from_json(&parsed(&body));
        let pages = layout(&doc);
        assert_eq!(line_counts(&pages), expected, "{kind}");
        assert_eq!(line_sources(&pages), vec![(0, 2), (2, 4)]);
        assert_eq!(pages.last().unwrap().size, Size::new(9000, 11000));
        if kind == "oddPage" {
            assert!(pages[1].fragments.is_empty());
        }
    }
}

#[test]
fn assumed_even_page_inserts_a_blank_when_previous_section_ends_on_page_two() {
    let first = uniform_section(2880, 1920, 720, "nextPage");
    let second = uniform_section(9000, 11000, 720, "evenPage");
    let body = format!(
        "{}{}{second}",
        paragraph(&"0".repeat(22), &first),
        paragraph("B", "")
    );
    let doc = document_from_json(&parsed(&body));
    let pages = layout(&doc);
    assert_eq!(line_counts(&pages), vec![1, 1, 0, 1]);
    assert!(pages[2].fragments.is_empty());
    assert_eq!(line_sources(&pages), vec![(0, 12), (12, 23), (23, 25)]);
}

#[test]
fn assumed_first_section_parity_does_not_insert_leading_blank_pages() {
    for kind in ["oddPage", "evenPage"] {
        let geometry = uniform_section(10000, 12000, 720, kind);
        let doc = document_from_json(&parsed(&format!("{}{geometry}", paragraph("A", ""))));
        let pages = layout(&doc);
        assert_eq!(line_counts(&pages), vec![1], "{kind}");
        assert_eq!(line_sources(&pages), vec![(0, 2)]);
    }
}

#[test]
fn assumed_next_page_section_reuses_page_opened_by_explicit_page_break() {
    let first = uniform_section(10000, 12000, 720, "nextPage");
    let second = uniform_section(9000, 11000, 720, "nextPage");
    let first_para = paragraph("A", &first).replace("</w:r>", r#"<w:br w:type="page"/></w:r>"#);
    let body = format!("{first_para}{}{second}", paragraph("B", ""));
    let doc = document_from_json(&parsed(&body));
    let pages = layout(&doc);
    assert_eq!(line_counts(&pages), vec![1, 1]);
    assert_eq!(pages[1].size, Size::new(9000, 11000));
    assert_eq!(line_sources(&pages), vec![(0, 3), (3, 5)]);
}
