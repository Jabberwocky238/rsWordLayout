//! 段距的行单位：`w:beforeLines` / `w:afterLines`（1/100 行）。
//!
//! Word 实测（Android Word 打印视图，精确行距 480，版心高 15398；word_analyse
//! `reports/rsword-diff/before-lines.md` 与 2026-10-04 的两份补测）：
//!
//! - `beforeLines=100` 每页 21 行、`50` 是 25 行：一行 240 twips，页顶段前也算；
//! - 把字号改成 24pt 仍是 21 行：一行不随字号；
//! - 加 `w:docGrid w:type="lines" w:linePitch="312"` 是 19 行：有行网格时一行是 `linePitch`；
//! - 同时写 `w:before` 与 `w:beforeLines`（段后同）时行单位优先：32 行；
//! - `afterLines=100`、行距 360 是 26 行：页末段后不计。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(spacing: &str, sz: Option<u32>, grid: &str) -> Vec<u8> {
    let rpr = sz.map(|s| format!(r#"<w:rPr><w:sz w:val="{s}"/></w:rPr>"#)).unwrap_or_default();
    let paras: String = (0..60)
        .map(|i| format!(r#"<w:p><w:pPr><w:spacing {spacing}/></w:pPr><w:r>{rpr}<w:t>P{i:03}</w:t></w:r></w:p>"#))
        .collect();
    let sect = format!(
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/>{grid}</w:sectPr>"#
    );
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{paras}{sect}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

fn first_page_lines(bytes: &[u8]) -> usize {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let options = LayoutOptions { platform: Platform::Android, view: View::Print, wrap: WrapPolicy::None };
    LayoutRecord::from_paint(&prepared.layout_approximate(&options).unwrap().paint()).pages[0].lines.len()
}

const EXACT480: &str = r#"w:after="0" w:line="480" w:lineRule="exact""#;

#[test]
fn one_line_is_240_twips_without_a_grid() {
    assert_eq!(first_page_lines(&docx(&format!(r#"w:beforeLines="100" {EXACT480}"#), None, "")), 21);
    assert_eq!(first_page_lines(&docx(&format!(r#"w:beforeLines="50" {EXACT480}"#), None, "")), 25);
    // 字号翻倍，一行不变。
    assert_eq!(first_page_lines(&docx(&format!(r#"w:beforeLines="100" {EXACT480}"#), Some(48), "")), 21);
}

#[test]
fn one_line_is_the_line_pitch_with_a_line_grid() {
    let grid = r#"<w:docGrid w:type="lines" w:linePitch="312"/>"#;
    assert_eq!(first_page_lines(&docx(&format!(r#"w:beforeLines="100" {EXACT480}"#), None, grid)), 19);
}

#[test]
fn line_units_win_over_twips_and_zero_falls_back() {
    let both = r#"w:before="240" w:beforeLines="50" w:after="0" w:line="360" w:lineRule="exact""#;
    assert_eq!(first_page_lines(&docx(both, None, "")), 32);
    let after = r#"w:before="0" w:after="240" w:afterLines="50" w:line="360" w:lineRule="exact""#;
    assert_eq!(first_page_lines(&docx(after, None, "")), 32);
    // 行单位写成 0：照 twips 值（假设，与字符单位缩进同一口径）。
    let zero = r#"w:before="240" w:beforeLines="0" w:after="0" w:line="360" w:lineRule="exact""#;
    assert_eq!(first_page_lines(&docx(zero, None, "")), 25);
}

#[test]
fn the_last_paragraph_on_a_page_drops_its_line_unit_space_after() {
    let after = r#"w:before="0" w:afterLines="100" w:line="360" w:lineRule="exact""#;
    assert_eq!(first_page_lines(&docx(after, None, "")), 26);
}
