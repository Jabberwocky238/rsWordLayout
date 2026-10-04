//! 正文以表格结尾时 Word 补的尾段（`LayoutDocument::implied_final_para`）。
//!
//! Word 实测（Android Word 打印视图，版心高 15398，无边框单列表，精确行高；word_analyse
//! `reports/rsword-diff/table*.md`）：30 行 × 503 剩 308 twips 是 1 页，30 行 × 504 剩 278 twips
//! 是 2 页；文档自己带尾段时（`table32-tail`）不再多补。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(rows: usize, height: u32, tail: bool) -> Vec<u8> {
    let row = format!(
        r#"<w:tr><w:trPr><w:trHeight w:val="{height}" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr>"#
    );
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>{}</w:tbl>"#,
        row.repeat(rows)
    );
    let tail = if tail { "<w:p/>" } else { "" };
    let sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
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
    assert_eq!(pages(&docx(30, 503, false)), 1, "剩 308 twips，补的尾段放得下");
    assert_eq!(pages(&docx(30, 504, false)), 2, "剩 278 twips，补的尾段换到下一页");
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
