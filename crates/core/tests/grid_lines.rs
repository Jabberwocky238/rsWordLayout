//! Android 的行网格（`w:docGrid w:type="lines"`）。
//!
//! Word 实测（Android Word 打印视图，等线 11pt 单倍 298 twips，版心 15398；word_analyse
//! `reports/rsword-diff/docgrid.md`、`findings/pagination-path.md`）：行高是不小于单倍高的最小整数倍
//! 步距（297 加倍 26 行、298 不加倍 51 行、99 是 4 倍 39 行）；页末一行只要单倍高加上多出部分的一半
//! （151 是 50 行、290 是 26 行、`dg-decide-*` 三组的临界段数）。
//!
//! 这里用近似度量：先量出它自己的单倍高，再按同一条规则预测，不依赖等线的数。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const BODY_FINE: i64 = 15398 * 5;

fn docx(grid: &str, snap: &str) -> Vec<u8> {
    let paras: String = (0..120)
        .map(|i| format!(r#"<w:p><w:pPr>{snap}<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:t>P{i:03}</w:t></w:r></w:p>"#))
        .collect();
    let sect = format!(
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/>{grid}</w:sectPr>"#
    );
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    if let Some(styles) = package.find_name("word/styles.xml") {
        package.replace_part_xml(styles, &format!(r#"<w:styles xmlns:w="{W}"/>"#)).unwrap();
    }
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{paras}{sect}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

/// 页 0 的行数与第一行的推进量（1/7200 英寸）。
fn page0(bytes: &[u8], platform: Platform) -> (i64, i64) {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let options = LayoutOptions { platform, view: View::Print, wrap: WrapPolicy::None };
    let record = LayoutRecord::from_paint(&prepared.layout_approximate(&options).unwrap().paint());
    let lines = &record.pages[0].lines;
    (lines.len() as i64, lines[0].placement.unwrap().advance_fine)
}

fn grid(pitch: i64) -> String {
    format!(r#"<w:docGrid w:type="lines" w:linePitch="{pitch}"/>"#)
}

/// 照规则预测：(步长, 页 0 行数)。
fn predict(single_fine: i64, pitch: i64) -> (i64, i64) {
    let single = (single_fine + 2) / 5;
    let step = ((single + pitch - 1) / pitch).max(1) * pitch * 5;
    let required = single_fine + (step - single_fine) / 2;
    (step, 1 + (BODY_FINE - required) / step)
}

fn single() -> i64 {
    let (_, advance) = page0(&docx("", ""), Platform::Android);
    assert_eq!(advance % 5, 0, "前提：近似度量的单倍高是整 twips");
    advance
}

#[test]
fn a_line_takes_the_smallest_multiple_of_the_pitch_that_holds_it() {
    let s = single();
    let twips = s / 5;
    for pitch in [twips - 1, twips, twips / 3, twips / 2 + 1, twips + 14] {
        let (step, lines) = predict(s, pitch);
        assert_eq!(page0(&docx(&grid(pitch), ""), Platform::Android), (lines, step), "pitch {pitch}");
    }
    // 前提：单倍高减 1 加倍、等于单倍高不加倍。
    assert_eq!(predict(s, twips - 1).0, 2 * (twips - 1) * 5);
    assert_eq!(predict(s, twips).0, s);
}

#[test]
fn the_last_line_needs_its_single_height_and_half_the_rest() {
    let s = single();
    // 找一个「末行按整步长」与「按居中」分得开的步距，钉住居中。
    let pitch = (s / 5 / 2 + 1..s / 5)
        .find(|&p| {
            let (step, lines) = predict(s, p);
            lines != BODY_FINE / step
        })
        .expect("有分得开的步距");
    assert_eq!(page0(&docx(&grid(pitch), ""), Platform::Android).0, predict(s, pitch).1);
}

#[test]
fn snap_to_grid_false_and_desktop_ignore_the_grid() {
    let s = single();
    let pitch = s / 5 - 1;
    let plain = page0(&docx("", ""), Platform::Android);
    assert_eq!(page0(&docx(&grid(pitch), r#"<w:snapToGrid w:val="0"/>"#), Platform::Android), plain);
    assert_eq!(page0(&docx(&grid(pitch), ""), Platform::Desktop), page0(&docx("", ""), Platform::Desktop));
}
