//! Android 的缺省段后，以及带可见边框、缺省单元格边距的表格行高。
//!
//! Word 实测（Android Word 打印视图，2026-10-04；`docs/WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md` 补测第 10、11 条）：
//! 包里没有样式 part 时，没写段后的段落段后 160 twips（60 段精确 480 一页 24 段，写明段后 0 是 32）；
//! `talltable-080` 一页 21 行（480 + 160 + 边框 10），格内写明段后 0 是 28 行，再去掉边框 29 行，
//! 再把单元格边距写成 0 仍是 29 行。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const SECT: &str = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;

/// 不加载样式 part 的包：去掉主文档到 `styles.xml` 的关系（Word 不加载，见 `contextual_spacing.rs`）。
fn docx(body: &str) -> Vec<u8> {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{body}{SECT}</w:body></w:document>"#))
        .unwrap();
    let rels = package.find_name("word/_rels/document.xml.rels").unwrap();
    let xml = String::from_utf8(package.read_bytes(rels).unwrap()).unwrap();
    let mut out = String::new();
    let mut rest = xml.as_str();
    while let Some(at) = rest.find("<Relationship ") {
        let end = at + rest[at..].find("/>").unwrap() + 2;
        out.push_str(&rest[..at]);
        if !rest[at..end].contains("/relationships/styles\"") {
            out.push_str(&rest[at..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    assert_ne!(out, xml, "前提：去掉了样式关系");
    package.replace_part_bytes(rels, out.into_bytes());
    package.save().unwrap()
}

fn record(bytes: &[u8], platform: Platform) -> (usize, usize) {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let options = LayoutOptions { platform, view: View::Print, wrap: WrapPolicy::None };
    let session = prepared.layout_approximate(&options).unwrap();
    let rows = session.pages()[0].table_rows.len();
    (LayoutRecord::from_paint(&session.paint()).pages[0].lines.len(), rows)
}

fn paragraphs(spacing: &str) -> String {
    (0..60)
        .map(|i| format!(r#"<w:p><w:pPr><w:spacing {spacing}w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>P{i:03}</w:t></w:r></w:p>"#))
        .collect()
}

#[test]
fn android_gives_paragraphs_without_space_after_the_default_160() {
    let bare = docx(&paragraphs(""));
    assert_eq!(record(&bare, Platform::Android).0, 24);
    assert_eq!(record(&bare, Platform::Desktop).0, 32, "桌面照旧");
    let zero = docx(&paragraphs(r#"w:after="0" "#));
    assert_eq!(record(&zero, Platform::Android).0, 32, "写了的段后照用");
}

fn table(spacing: &str, borders: bool) -> Vec<u8> {
    let border = |side: &str| if borders {
        format!(r#"<w:{side} w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#)
    } else {
        format!(r#"<w:{side} w:val="nil"/>"#)
    };
    let borders: String = ["top", "left", "bottom", "right", "insideH", "insideV"].iter().map(|s| border(s)).collect();
    let rows: String = (0..80)
        .map(|i| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing {spacing}w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>row{i:02}</w:t></w:r></w:p></w:tc></w:tr>"#))
        .collect();
    docx(&format!(r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr>{rows}</w:tbl><w:p/>"#))
}

#[test]
fn bordered_auto_width_rows_take_the_default_space_after_and_the_border() {
    // 版心 15398：650 一行 23 行，490 一行 31 行，480 一行 32 行。
    assert_eq!(record(&table("", true), Platform::Android).1, 23);
    assert_eq!(record(&table(r#"w:before="0" w:after="0" "#, true), Platform::Android).1, 31);
    assert_eq!(record(&table(r#"w:before="0" w:after="0" "#, false), Platform::Android).1, 32);
    assert_eq!(record(&table("", true), Platform::Desktop).1, 31, "桌面没有缺省段后");
}
