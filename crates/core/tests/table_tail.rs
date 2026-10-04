//! 正文以表格结尾时 Word 补的尾段（`LayoutDocument::implied_final_para`）。
//!
//! Word 实测（Android Word 打印视图，版心高 15398，无边框单列表，精确行高；word_analyse
//! `reports/rsword-diff/table*.md`）：30 行 × 500、503 剩 398、308 twips 是 1 页，30 行 × 504、510
//! 剩 278、98 twips 是 2 页；文档自己带尾段时（`table32-tail`）不再多补。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(rows: usize, height: u32, tail: bool) -> Vec<u8> {
    let row = format!(
        r#"<w:tr><w:trPr><w:trHeight w:val="{height}" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr>"#
    );
    docx_rows(&row, rows, tail)
}

/// 没写行高、每格一段精确 480 的行（word_analyse `table24`…`table32` 的形状）。
fn docx_content(rows: usize) -> Vec<u8> {
    let row = r#"<w:tr><w:tc><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>R</w:t></w:r></w:p></w:tc></w:tr>"#;
    docx_rows(row, rows, false)
}

fn docx_rows(row: &str, rows: usize, tail: bool) -> Vec<u8> {
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>{}</w:tbl>"#,
        row.repeat(rows)
    );
    let tail = if tail { "<w:p/>" } else { "" };
    let sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    // 与 word_analyse 的表格夹具同形：不带任何样式与文档默认值。
    if let Some(styles) = package.find_name("word/styles.xml") {
        package.replace_part_xml(styles, &format!(r#"<w:styles xmlns:w="{W}"/>"#)).unwrap();
    }
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{table}{tail}{sect}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

fn pages(bytes: &[u8]) -> usize {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let options = LayoutOptions { platform: Platform::Android, view: View::Print, wrap: WrapPolicy::None };
    LayoutRecord::from_paint(&prepared.layout_approximate(&options).unwrap().paint()).page_count()
}

#[test]
fn a_body_final_table_is_followed_by_an_implied_empty_paragraph() {
    // 用离空段高度远的两档（`table30-h500` 1 页、`table30-h510` 2 页）：近似度量下空段的高
    // 不是等线的 298，503 / 504 那一对只分得开 (278, 308] 之间的空段。
    assert_eq!(pages(&docx(30, 500, false)), 1, "剩 398 twips，补的尾段放得下");
    assert_eq!(pages(&docx(30, 510, false)), 2, "剩 98 twips，补的尾段换到下一页");
}

#[test]
fn a_document_with_its_own_tail_paragraph_gets_no_second_one() {
    let bytes = docx(30, 503, true);
    let prepared = PreparedDocument::load(&bytes).unwrap();
    assert!(prepared.document().implied_final_para.is_none());
    assert_eq!(prepared.document().paras.len(), 1);
    assert_eq!(pages(&bytes), 1);
}

#[test]
fn the_source_document_keeps_no_paragraphs() {
    let bytes = docx(2, 480, false);
    let prepared = PreparedDocument::load(&bytes).unwrap();
    assert!(prepared.document().paras.is_empty(), "补的尾段只在排版时接上");
    assert!(prepared.document().implied_final_para.is_some());
}

/// 没写行高的行随内容高。Word（Android 打印视图，`findings/pagination-path.md`）：24、28、30、31 行一页，
/// 32 行两页——31 × 480 加补的空段放得下，32 × 480 = 15360 再加空段放不下。
#[test]
fn rows_without_a_declared_height_grow_to_their_content() {
    for (rows, expected) in [(24, 1), (31, 1), (32, 2)] {
        assert_eq!(pages(&docx_content(rows)), expected, "{rows} 行");
    }
    let bytes = docx_content(2);
    let prepared = PreparedDocument::load(&bytes).unwrap();
    assert_eq!(prepared.document().tables[0].rows[0].height, None);
    let options = LayoutOptions { platform: Platform::Android, view: View::Print, wrap: WrapPolicy::None };
    let session = prepared.layout_approximate(&options).unwrap();
    assert_eq!(session.pages()[0].table_rows[0].height_fine, 480 * 5);
}
