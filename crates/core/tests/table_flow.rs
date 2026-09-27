//! First table slice: single-cell rows with exact heights, zero cell margins and
//! no visible borders, the `table32-tail.docx` shape from word_analyse.
//!
//! The report-level Word constraint for table32-tail is one print page. The
//! other expectations here (row boxes, row-boundary continuation, source
//! projection) are the engine's contract, not new Word captures. The source
//! projection adds one unit per paragraph end and one per cell end; it matches
//! the `tablepage` FormatLine starts 0, 35, 70 only for single-cell,
//! single-paragraph rows.

use rsword::package::Package;
use rsword_layout_core::{
    Engine, Fragment, LayoutDocument, LayoutRecord, Page, PageSetup, SimpleMetrics, load_document,
    paint_document,
};

fn docx(body: &str, styles: Option<&str>) -> Vec<u8> {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{w}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    if let Some(styles) = styles {
        let part = package.find_name("word/styles.xml").unwrap();
        package.replace_part_xml(part, styles).unwrap();
    }
    package.save().unwrap()
}

fn document(body: &str) -> LayoutDocument {
    load_document(&docx(body, None)).unwrap().layout_document()
}

const TABLE_PR: &str = r#"<w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>"#;

fn cell_paragraph(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#
    )
}

fn row(content: &str) -> String {
    format!(r#"<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc>{content}</w:tc></w:tr>"#)
}

fn label(index: usize) -> String {
    format!("R{index:03} line of text for page break.")
}

fn table(rows: usize) -> String {
    let rows: String = (0..rows).map(|i| row(&cell_paragraph(&label(i)))).collect();
    format!("<w:tbl>{TABLE_PR}{rows}</w:tbl>")
}

const TAIL: &str = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/></w:pPr></w:p>"#;

fn section(height: i32) -> String {
    format!(r#"<w:sectPr><w:pgSz w:w="11906" w:h="{height}"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#)
}

fn layout(doc: &LayoutDocument) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(doc)
}

fn page_sources(pages: &[Page]) -> Vec<Vec<(u32, u32)>> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .map(|page| {
            page.lines
                .iter()
                .map(|line| {
                    let source = line.source.expect("table and tail lines have sources");
                    (source.start, source.end)
                })
                .collect()
        })
        .collect()
}

fn page_text(page: &Page) -> String {
    page.fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn table32_tail_shape_formats_every_cell_and_an_independent_tail() {
    let doc = document(&format!("{}{TAIL}{}", table(32), section(16838)));
    assert_eq!(doc.skipped_blocks, 0, "the supported table is not omitted: {:?}", doc.diagnostics);
    assert_eq!(doc.tables.len(), 1);
    assert_eq!(doc.tables[0].rows.len(), 32);
    assert_eq!(doc.paras.len(), 1, "only the tail is a main-story paragraph");
    assert!(!doc.trace_metadata()["sourceCoverage"].as_str().unwrap().contains("partial"));

    let pages = layout(&doc);
    // 32 x 480 + 20 = 15380 <= 15398: the report-level Word constraint is one page.
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    let text = page_text(page);
    for index in 0..32 {
        assert!(text.contains(&label(index)), "cell {index} text is painted");
    }

    let mut expected: Vec<(u32, u32)> = (0..32).map(|i| (35 * i, 35 * i + 34)).collect();
    expected.push((1120, 1121));
    assert_eq!(page_sources(&pages), vec![expected]);

    assert_eq!(page.table_rows.len(), 32);
    for (index, row) in page.table_rows.iter().enumerate() {
        assert_eq!((row.table, row.row, row.column), (0, index, 0));
        assert_eq!(row.rect.y, 720 + 480 * index as i32);
        assert_eq!((row.rect.x, row.rect.width, row.rect.height), (720, 10466, 480));
        assert_eq!(row.cells.len(), 1);
        let cell = &row.cells[0];
        assert_eq!(cell.source, (35 * index as u32, 35 * index as u32 + 35));
        assert_eq!(cell.lines, index as u32..index as u32 + 1);
        assert_eq!(cell.overflow_fine, 0);
    }
    let tail = LayoutRecord::from_paint(&paint_document(&pages, None, &[])).pages[0].lines[32].clone();
    assert_eq!(tail.placement.unwrap().top_fine, (720 + 32 * 480) * 5);
}

#[test]
fn rows_continue_at_row_boundaries_without_loss_or_duplication() {
    // Body 10 x 480 + 100: ten rows per page, then the tail after the last row.
    let height = 720 + 10 * 480 + 100 + 720;
    let doc = document(&format!("{}{TAIL}{}", table(32), section(height)));
    let pages = layout(&doc);
    assert_eq!(pages.len(), 4);
    let rows: Vec<Vec<usize>> = pages
        .iter()
        .map(|page| page.table_rows.iter().map(|row| row.row).collect())
        .collect();
    assert_eq!(rows, vec![
        (0..10).collect::<Vec<_>>(),
        (10..20).collect(),
        (20..30).collect(),
        vec![30, 31],
    ]);
    for page in &pages {
        assert_eq!(page.table_rows[0].rect.y, 720, "a continued row starts at the body top");
    }
    let sources: Vec<(u32, u32)> = page_sources(&pages).into_iter().flatten().collect();
    assert_eq!(sources.len(), 33);
    for (index, pair) in sources.windows(2).enumerate() {
        assert_eq!(pair[0].1 + 1, pair[1].0, "line {index}: cell mark joins adjacent rows");
    }
    assert_eq!(sources.last(), Some(&(1120, 1121)));
    assert_eq!(page_sources(&pages)[3].len(), 3, "rows 30, 31 and the tail");
}

#[test]
fn paragraphs_around_a_table_keep_their_own_flow() {
    let before = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="240" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>Before</w:t></w:r></w:p>"#;
    let after = r#"<w:p><w:pPr><w:spacing w:before="120" w:after="0" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>After</w:t></w:r></w:p>"#;
    let doc = document(&format!("{before}{}{after}{}", table(2), section(16838)));
    assert_eq!(doc.tables[0].before_para, 1);
    let pages = layout(&doc);
    let page = &pages[0];
    // Paragraph after-space precedes the table; before-space does not collapse
    // with a table and applies after the last row.
    assert_eq!(page.table_rows[0].rect.y, 720 + 480 + 240);
    assert_eq!(page.table_rows[1].rect.y, 720 + 480 + 240 + 480);
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    let lines = &record.pages[0].lines;
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[3].placement.unwrap().top_fine, (720 + 480 + 240 + 960 + 120) * 5);
    let sources: Vec<_> = lines.iter().map(|line| line.source.unwrap()).map(|s| (s.start, s.end)).collect();
    // "Before" 6 + mark; two rows of 33 + mark + cell; "After" 5 + mark.
    assert_eq!(sources, vec![(0, 7), (7, 41), (42, 76), (77, 83)]);
}

#[test]
fn cell_text_wraps_inside_the_declared_table_width() {
    let text = "word ".repeat(40);
    let narrow = TABLE_PR.replace(r#"w:w="5000" w:type="pct""#, r#"w:w="2500" w:type="pct""#);
    let tall = r#"<w:tr><w:trPr><w:trHeight w:val="4800" w:hRule="exact"/></w:trPr><w:tc>"#;
    let body = format!(
        "<w:tbl>{narrow}{tall}{}</w:tc></w:tr></w:tbl>{TAIL}{}",
        cell_paragraph(text.trim_end()),
        section(16838)
    );
    let doc = document(&body);
    let pages = layout(&doc);
    let row = &pages[0].table_rows[0];
    assert_eq!((row.rect.x, row.rect.width), (720, 5233));
    assert!(row.cells[0].lines.len() > 1, "cell paragraph wraps in its cell width");
    for fragment in &pages[0].fragments {
        if let Fragment::Text(text) = fragment
            && !text.text.is_empty()
        {
            assert!(text.x >= 720 && text.x < 720 + 5233);
        }
    }
}

#[test]
fn cell_paragraphs_use_effective_style_properties() {
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="40"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#;
    let bytes = docx(&format!("{}{TAIL}{}", table(1), section(16838)), Some(styles));
    let doc = load_document(&bytes).unwrap().layout_document();
    let run = &doc.tables[0].rows[0].cells[0].paras[0].runs[0];
    assert_eq!(run.font.size_half_points, 40, "docDefaults reach cell runs");
}

#[test]
fn content_taller_than_an_exact_row_is_reported_not_resized() {
    let two = format!("{}{}", cell_paragraph("first"), cell_paragraph("second"));
    let doc = document(&format!("<w:tbl>{TABLE_PR}{}</w:tbl>{TAIL}{}", row(&two), section(16838)));
    let pages = layout(&doc);
    let row = &pages[0].table_rows[0];
    assert_eq!(row.rect.height, 480, "exact height is not grown by content");
    assert_eq!(row.cells[0].lines, 0..2);
    assert_eq!(row.cells[0].overflow_fine, 480 * 5, "second line overflows; clipping is not implemented");
    assert_eq!(row.cells[0].source, (0, 14), "first 5+1, second 6+1, cell end 1");
}

#[test]
fn unsupported_table_shapes_remain_omitted_and_reported() {
    let cases = [
        ("two cells", format!(
            r#"<w:tbl>{TABLE_PR}<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>"#,
            cell_paragraph("a"), cell_paragraph("b"))),
        ("atLeast height", table(1).replace(r#"w:hRule="exact""#, r#"w:hRule="atLeast""#)),
        ("default cell margins", table(1).replace(
            r#"<w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar>"#, "")),
        ("visible border", table(1).replace(r#"<w:top w:val="nil"/>"#, r#"<w:top w:val="single" w:sz="4"/>"#)),
        ("table style", table(1).replace("<w:tblPr>", r#"<w:tblPr><w:tblStyle w:val="TableGrid"/>"#)),
    ];
    for (name, table) in cases {
        let doc = document(&format!("{table}{TAIL}{}", section(16838)));
        assert!(doc.tables.is_empty(), "{name}: unsupported");
        assert_eq!(doc.skipped_blocks, 1, "{name}: still reported as omitted");
        assert!(
            doc.diagnostics.iter().any(|d| d.contains("table") && d.contains("not laid out")),
            "{name}: {:?}", doc.diagnostics
        );
    }
}

#[test]
fn table_without_following_paragraph_is_diagnosed() {
    let doc = document(&format!("{}{}", table(2), section(16838)));
    assert_eq!(doc.tables.len(), 1);
    assert!(doc.paras.is_empty());
    assert!(doc.diagnostics.iter().any(|d| d.contains("no following paragraph")), "{:?}", doc.diagnostics);
    let pages = layout(&doc);
    assert_eq!(pages[0].table_rows.len(), 2);
}

#[test]
fn keep_next_into_a_table_is_reported_and_not_linked_across_it() {
    let before = r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#;
    // Filler and heading fit on the first page; the row moves to the second.
    let height = 720 + 2 * 480 + 100 + 720;
    let before = format!("{}{before}", cell_paragraph("Filler"));
    // A link to this successor would not fit beside the heading and would move it.
    let successor = cell_paragraph("After");
    let doc = document(&format!("{before}{}{successor}{}", table(1), section(height)));
    assert!(doc.diagnostics.iter().any(|d| d.contains("keepNext") && d.contains("table")), "{:?}", doc.diagnostics);
    let pages = layout(&doc);
    assert!(page_text(&pages[0]).contains("Heading"), "heading is not moved by a link past the table");
    assert!(pages[0].table_rows.is_empty());
    assert_eq!(pages[1].table_rows.len(), 1);
}

#[test]
#[ignore = "needs sibling word_analyse fixtures"]
fn real_table32_tail_fixture_fits_one_page() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../word_analyse/fixtures/table32-tail.docx");
    let doc = load_document(&std::fs::read(path).unwrap()).unwrap().layout_document();
    assert_eq!((doc.tables.len(), doc.skipped_blocks), (1, 0), "{:?}", doc.diagnostics);
    let pages = layout(&doc);
    assert_eq!(pages.len(), 1, "report-level Word constraint");
    assert_eq!(pages[0].table_rows.len(), 32);
    let text = page_text(&pages[0]);
    for index in 0..32 {
        assert!(text.contains(&label(index)));
    }
}

#[test]
fn a_multi_column_band_with_table_rows_is_not_replayed() {
    let columns = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/><w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720"/><w:cols w:num="2" w:space="720"/></w:sectPr></w:pPr><w:r><w:t>Band end</w:t></w:r></w:p>"#;
    let last = format!(r#"{}<w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720"/></w:sectPr>"#, cell_paragraph("Single"));
    // Replay would start at "Lead" and re-format paragraphs only, dropping rows.
    let doc = document(&format!("{}{}{columns}{last}", cell_paragraph("Lead"), table(4)));
    assert!(doc.diagnostics.iter().any(|d| d.contains("multi-column") && d.contains("table")), "{:?}", doc.diagnostics);
    let pages = layout(&doc);
    let rows: Vec<usize> = pages.iter().flat_map(|page| &page.table_rows).map(|row| row.row).collect();
    assert_eq!(rows, vec![0, 1, 2, 3]);
    let row = &pages[0].table_rows[0];
    assert_eq!(row.rect.width, (10466 - 720) / 2, "pct width resolves against the column");
    let sources: Vec<(u32, u32)> = page_sources(&pages).into_iter().flatten().collect();
    assert_eq!(sources.len(), 7);
    for pair in sources.windows(2) {
        assert!(pair[0].1 < pair[1].0 + 2 && pair[0].0 < pair[1].0, "ordered, no duplicates: {sources:?}");
    }
    assert!(page_text(&pages[0]).contains("Single"));
}
