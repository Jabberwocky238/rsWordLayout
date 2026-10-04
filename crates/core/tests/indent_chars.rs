//! 段落缩进的字符单位（`*Chars`）与移动视图的缩进缩放。
//!
//! Word 实测（Android Word，Calibri 12pt，word_analyse `reports/rsword-diff/indent.md`）：
//!
//! - 字符单位非零时优先于同一边的 twips：`left=720` + `leftChars=100`、`firstLine=720` +
//!   `firstLineChars=100` 都按字符单位排；`leftChars` / `rightChars` 每行都缩，`firstLineChars`
//!   只缩首行；每百份与 12pt 的一个 em 成同一比例（八档 `firstLineChars` 收到 122.0–122.6 twips，
//!   即 240 × 5329 / 10466）。
//! - 那些读数取自纸页路径（10466），当时手机在移动视图里：缩进整体乘了视图宽与纸页版心宽之比，
//!   五档左缩进的有效比例共同落在 0.507–0.519（5329 / 10466 = 0.509），`wm size` 改窄视图后比例
//!   跟着变；窄路径 `ind-left` 每行 40 个 `0`（不缩是 37）。
//!
//! 引擎在移动视图（[`View::Mobile`]）按这个比例缩正文段落的缩进，打印视图不缩：真正打印视图下
//! Word 缩不缩没有读数。乘上比例之后，上面 22 份纸页路径读数与当前引擎逐行相同（离线回放，
//! 见 `docs/WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md`）。
//!
//! 这里用桩度量（12pt 下拉丁字 120 twips）与真实 DOCX 字节，走公开的装载与会话。

use rsword::package::Package;
use rsword_layout_core::{
    LayoutOptions, LayoutRecord, PageOverrides, Platform, PreparedDocument, View, WrapPolicy,
    load_document,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(body: &str) -> Vec<u8> {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

/// 一段 12pt 的 `0`，带给定的 `w:pPr` 内容；节为 A4 纵向、四边 720（版心 10466）。
fn zeros(ppr: &str, count: usize) -> String {
    format!(
        r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>{}</w:t></w:r></w:p>
        <w:sectPr><w:pgSz w:w="11906" w:h="16838"/>
        <w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#,
        "0".repeat(count)
    )
}

/// 第一段的 (左, 右, 首行) 缩进。
fn indents(ppr: &str) -> (i32, i32, i32) {
    let document = load_document(&docx(&zeros(ppr, 10))).unwrap().layout_document();
    let para = &document.paras[0];
    (para.indent_left, para.indent_right, para.indent_first_line)
}

#[test]
fn character_units_win_over_twips_on_the_same_side() {
    assert_eq!(indents(r#"<w:ind w:left="720" w:leftChars="100"/>"#), (240, 0, 0));
    assert_eq!(indents(r#"<w:ind w:firstLine="720" w:firstLineChars="100"/>"#), (0, 0, 240));
    assert_eq!(indents(r#"<w:ind w:rightChars="200"/>"#), (0, 480, 0));
    assert_eq!(indents(r#"<w:ind w:left="480" w:hangingChars="50"/>"#), (480, 0, -120));
}

#[test]
fn zero_character_units_fall_back_to_twips() {
    // Word 写 `*Chars="0"` 清掉样式继承来的字符缩进（假设，没有夹具）。
    assert_eq!(indents(r#"<w:ind w:leftChars="0" w:left="720"/>"#), (720, 0, 0));
    assert_eq!(indents(r#"<w:ind w:firstLineChars="0" w:firstLine="360"/>"#), (0, 0, 360));
}

#[test]
fn a_character_is_one_em_of_the_first_run() {
    // 正文 run 是 12pt：一字 240 twips，段落标记的字号（这里写成五号 10.5pt，或文档缺省的 11pt）
    // 不参与。用哪个字号是偏向数据的假设，见 `bridge::indent_char_unit`。
    assert_eq!(indents(r#"<w:rPr><w:sz w:val="21"/></w:rPr><w:ind w:leftChars="100"/>"#), (240, 0, 0));
    assert_eq!(indents(r#"<w:ind w:leftChars="100"/>"#), (240, 0, 0));
    // 空段落没有正文 run，按段落标记属性补的那个 run 的字号。
    let body = r#"<w:p><w:pPr><w:rPr><w:sz w:val="21"/></w:rPr><w:ind w:leftChars="100"/></w:pPr></w:p>"#;
    let document = load_document(&docx(body)).unwrap().layout_document();
    assert_eq!(document.paras[0].indent_left, 210);
}

/// 第一段各行的起点（UTF-16）。
fn starts(ppr: &str, view: View, content_width: Option<i32>) -> Vec<u32> {
    let bytes = docx(&zeros(ppr, 100));
    let mut prepared = PreparedDocument::load(&bytes).unwrap();
    prepared
        .apply_page_overrides(PageOverrides { content_width, ..PageOverrides::default() })
        .unwrap();
    let options = LayoutOptions { platform: Platform::Android, view, wrap: WrapPolicy::None };
    let session = prepared.layout_approximate(&options).unwrap();
    LayoutRecord::from_paint(&session.paint())
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| l.source.unwrap().start)
        .collect()
}

#[test]
fn mobile_view_scales_indents_by_view_width_over_declared_width() {
    // 720 × 5329 / 10466 ≈ 367：(5329 − 367) / 120 → 每行 41 个。不缩是 38 个。
    assert_eq!(starts(r#"<w:ind w:left="720"/>"#, View::Mobile, Some(5329)), [0, 41, 82]);
    // 字符单位同样缩：240 → 122，每行 43 个。
    assert_eq!(starts(r#"<w:ind w:leftChars="100"/>"#, View::Mobile, Some(5329)), [0, 43, 86]);
}

#[test]
fn print_view_and_unoverridden_mobile_view_keep_written_indents() {
    // 打印视图不缩，哪怕版心被覆盖窄了：(5329 − 720) / 120 → 38 个。
    assert_eq!(starts(r#"<w:ind w:left="720"/>"#, View::Print, Some(5329)), [0, 38, 76]);
    // 没有覆盖时视图宽就是声明宽，比例为 1：(10466 − 720) / 120 → 81 个。
    assert_eq!(starts(r#"<w:ind w:left="720"/>"#, View::Mobile, None), [0, 81]);
    assert_eq!(starts(r#"<w:ind w:left="720"/>"#, View::Print, None), [0, 81]);
}
