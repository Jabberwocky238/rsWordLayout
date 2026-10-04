//! Android：不写 `w:kern` 也做字距调整，写了照阈值；跨 run 照样调。
//!
//! Word 实测（Android Word 纸页 10466，Calibri 12pt，word_analyse `reports/rsword-diff/kern.md`）：
//! `kern-off`（`AVAV…`，不写 `w:kern`）与 `kern-on`（写 `w:kern w:val="2"`）都是每行 82 个字母。
//! Calibri 的 GPOS 两个方向都调（A→V −89、V→A −96 字体单位），82 个字母宽 10399 twips、84 个宽 10652；
//! 不调每行 76 个。Mac Word 不写 `w:kern` 就不调（`B11` 的两个 `1`），所以只有 Android 这样。
//!
//! 2026-10-04 打印视图补测（41 个 `AV`，调了 10399 twips 一行放得下，不调 11277 两行）：
//! `w:kern w:val="48"`（24pt 起调）两行——写了的阈值照样算数；每 10 个字母换一种颜色的 run
//! 一行——跨 run 的 V→A 也调。
//!
//! 这里用仓库的 Liberation Sans（A↔V 都是 −152 字体单位），走真实 DOCX 与公开会话（feature `fontenv`）。

#![cfg(feature = "fontenv")]

use rsword::package::Package;
use rsword_layout_core::{
    DrawCmd, FontMetrics, FontSpec, LayoutOptions, LayoutRecord, PageOverrides, Platform,
    PreparedDocument, RealMetrics, VerticalGrid, View, WrapPolicy, font::FontRegistry,
};
use std::path::PathBuf;

const WIDTH: i32 = 5300;

fn registry() -> FontRegistry {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/LiberationSans-Regular.ttf");
    let mut registry = FontRegistry::new();
    registry.add(std::fs::read(path).expect("字体读得到"), 0).expect("字体装得进");
    registry
}

fn docx(kern: &str) -> Vec<u8> {
    docx_split(kern, 120)
}

/// 每 `every` 个字母一个 run，相邻 run 只差颜色。
fn docx_split(kern: &str, every: usize) -> Vec<u8> {
    let text = "AV".repeat(60);
    let colors = ["C00000", "00A000", "0000FF"];
    let runs: String = (0..text.len()).step_by(every).map(|at| format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="Liberation Sans" w:hAnsi="Liberation Sans"/><w:color w:val="{}"/>{kern}<w:sz w:val="24"/></w:rPr><w:t>{}</w:t></w:r>"#,
        colors[at / every % 3], &text[at..(at + every).min(text.len())]
    )).collect();
    let body = format!("<w:p>{runs}</w:p>");
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(
            main,
            &format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#),
        )
        .unwrap();
    package.save().unwrap()
}

/// 版心 5300 上第一行收几个字母，以及第一行前两个字形的推进量合计（点）。
fn first_line(kern: &str, platform: Platform) -> (u32, f64) {
    first_line_of(&docx(kern), platform)
}

fn first_line_of(bytes: &[u8], platform: Platform) -> (u32, f64) {
    let mut prepared = PreparedDocument::load(bytes).unwrap();
    prepared
        .apply_page_overrides(PageOverrides { content_width: Some(WIDTH), ..PageOverrides::default() })
        .unwrap();
    let options = LayoutOptions { platform, view: View::Print, wrap: WrapPolicy::None };
    let session = prepared.layout_with_fonts(registry(), VerticalGrid::None, &options).unwrap();
    let paint = session.paint();
    let record = LayoutRecord::from_paint(&paint);
    let second = record.pages[0].lines[1].source.unwrap().start;
    let pair = paint.pages[0].cmds.iter().find_map(|cmd| match cmd {
        DrawCmd::DrawGlyphs { glyphs, .. } if glyphs.len() >= 2 => Some(glyphs[0].advance_x_pt + glyphs[1].advance_x_pt),
        _ => None,
    });
    (second, pair.unwrap())
}

/// 引擎自己的度量：不调与调字距时一行各放得下几个字母，以及行中一对 `AV` 的宽（点）——
/// 后面还跟着 `A`，所以 V 也带 V→A 那一份：取 `AVA` 减去末尾的 `A`。
fn expected() -> ((u32, f64), (u32, f64)) {
    let registry = registry();
    let metrics = RealMetrics::new(&registry);
    let count = |kerning: bool| {
        let mut font = FontSpec::new("Liberation Sans", 24);
        font.kerning = kerning;
        let text = "AV".repeat(60);
        let fits = (1..text.len()).rev().find(|&n| metrics.measure(&text[..n], &font).advance <= WIDTH).unwrap();
        (fits as u32, metrics.advance_pt("AVA", &font) - metrics.advance_pt("A", &font))
    };
    (count(false), count(true))
}

#[test]
fn android_kerns_without_w_kern() {
    let (plain, kerned) = expected();
    assert!(kerned.0 > plain.0, "前提：这个版心上调不调字距分得开：{plain:?} {kerned:?}");
    assert_eq!(first_line("", Platform::Android).0, kerned.0);
    assert_eq!(first_line("", Platform::Desktop).0, plain.0);
}

#[test]
fn desktop_still_follows_w_kern() {
    let (_, kerned) = expected();
    // `w:kern w:val="2"`：1pt 起调，12pt 的 run 调。
    assert_eq!(first_line(r#"<w:kern w:val="2"/>"#, Platform::Desktop).0, kerned.0);
    assert_eq!(first_line(r#"<w:kern w:val="2"/>"#, Platform::Android).0, kerned.0);
}

#[test]
fn painted_glyphs_carry_the_same_kerning() {
    let (plain, kerned) = expected();
    let (_, android_pair) = first_line("", Platform::Android);
    let (_, desktop_pair) = first_line("", Platform::Desktop);
    assert!((android_pair - kerned.1).abs() < 1e-9, "{android_pair} vs {}", kerned.1);
    assert!((desktop_pair - plain.1).abs() < 1e-9, "{desktop_pair} vs {}", plain.1);
}

#[test]
fn android_honors_a_declared_threshold_above_the_size() {
    let (plain, _) = expected();
    // `w:kern w:val="48"`：24pt 起调，12pt 的 run 不调。
    assert_eq!(first_line(r#"<w:kern w:val="48"/>"#, Platform::Android).0, plain.0);
}

#[test]
fn android_kerns_across_runs_that_differ_only_in_color() {
    let (plain, kerned) = expected();
    // 每 10 个字母一个 run，run 边界都是 V→A。
    let split = docx_split("", 10);
    assert_eq!(first_line_of(&split, Platform::Android).0, kerned.0);
    // 桌面没有读数，跨 run 不调：写了 `w:kern` 也比整段一个 run 少放。
    let declared = docx_split(r#"<w:kern w:val="2"/>"#, 10);
    let desktop = first_line_of(&declared, Platform::Desktop).0;
    assert!(desktop < kerned.0 && desktop > plain.0, "{desktop} {plain:?} {kerned:?}");
}

#[test]
fn kerning_across_runs_paints_where_it_measures() {
    let session = |bytes: &[u8]| {
        let mut prepared = PreparedDocument::load(bytes).unwrap();
        prepared
            .apply_page_overrides(PageOverrides { content_width: Some(WIDTH), ..PageOverrides::default() })
            .unwrap();
        let options = LayoutOptions { platform: Platform::Android, view: View::Print, wrap: WrapPolicy::None };
        let paint = prepared.layout_with_fonts(registry(), VerticalGrid::None, &options).unwrap().paint();
        paint.pages[0].cmds.iter().filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { origin_x_pt, glyphs, .. } => {
                let mut x = *origin_x_pt;
                Some(glyphs.iter().map(|g| { let at = x; x += g.advance_x_pt; at }).collect::<Vec<_>>())
            }
            _ => None,
        }).flatten().take(40).collect::<Vec<f64>>()
    };
    let whole = session(&docx(""));
    let split = session(&docx_split("", 10));
    assert_eq!(whole.len(), split.len());
    for (a, b) in whole.iter().zip(&split) {
        assert!((a - b).abs() < 1e-6, "{whole:?}\n{split:?}");
    }
}
