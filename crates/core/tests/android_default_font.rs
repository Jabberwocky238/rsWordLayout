//! Android 的缺省字体与东亚字体的行高。
//!
//! Word 实测（Android Word 打印视图，2026-10-04；`docs/WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md` §4）：
//! 不写字体、字号的文档用等线（DengXian）11pt 排；声明了东亚代码页的字体单倍行高是 hhea 行高 × 1.3
//! （等线 1.0420 em → 1.3546 em）；auto 行距大于单倍时，页末一行只要单倍高放得下
//! （2 倍 26 行、3 倍 17 行）。桌面照旧。

use rsword::package::Package;
use rsword_layout_core::{DrawCmd, LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const BODY_HEIGHT: i64 = 15398;

fn docx(rpr: &str, spacing: &str, paras: usize) -> Vec<u8> {
    docx_text(rpr, spacing, paras, "P")
}

fn docx_text(rpr: &str, spacing: &str, paras: usize, text: &str) -> Vec<u8> {
    let body: String = (0..paras)
        .map(|i| format!(r#"<w:p><w:pPr><w:spacing {spacing}/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t>{text}{i:03}</w:t></w:r></w:p>"#))
        .collect();
    let sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    // 空白模板的 styles.xml 带文档默认字体与字号；这里要「什么都没写」，整份样式表清空。
    if let Some(styles) = package.find_name("word/styles.xml") {
        package.replace_part_xml(styles, &format!(r#"<w:styles xmlns:w="{W}"/>"#)).unwrap();
    }
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{body}{sect}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

fn options(platform: Platform) -> LayoutOptions {
    LayoutOptions { platform, view: View::Print, wrap: WrapPolicy::None }
}

fn first_glyph_size(bytes: &[u8], platform: Platform) -> u32 {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let paint = prepared.layout_approximate(&options(platform)).unwrap().paint();
    paint.pages[0].cmds.iter().find_map(|cmd| match cmd {
        DrawCmd::DrawGlyphs { font, .. } => Some(font.size_half_points),
        _ => None,
    }).unwrap()
}

#[test]
fn the_bridge_marks_fallback_family_and_size() {
    let bytes = docx("", r#"w:after="0""#, 1);
    let bare = PreparedDocument::load(&bytes).unwrap();
    let font = &bare.document().paras[0].runs[0].font;
    assert!(font.family_is_fallback && font.size_is_fallback, "{font:?}");

    let declared = r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/>"#;
    let bytes = docx(declared, r#"w:after="0""#, 1);
    let set = PreparedDocument::load(&bytes).unwrap();
    let font = &set.document().paras[0].runs[0].font;
    assert!(!font.family_is_fallback && !font.size_is_fallback, "{font:?}");
}

#[test]
fn android_lays_out_an_undeclared_size_at_eleven_points() {
    let family_only = docx(r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/>"#, r#"w:after="0""#, 1);
    assert_eq!(first_glyph_size(&family_only, Platform::Android), 22);
    assert_eq!(first_glyph_size(&family_only, Platform::Desktop), 24, "桌面照旧");
    let sized = docx(r#"<w:sz w:val="24"/>"#, r#"w:after="0""#, 1);
    assert_eq!(first_glyph_size(&sized, Platform::Android), 24, "写了的字号照用");
    // 近似度量量不了等线：没写字体的 run 字体、字号都照原来的替身。
    let bare = docx("", r#"w:after="0""#, 1);
    assert_eq!(first_glyph_size(&bare, Platform::Android), 24);
}

/// 声称装了所有字体的近似度量：只用来看缺省字体换不换。
struct WithEveryFamily(rsword_layout_core::SimpleMetrics);

impl rsword_layout_core::FontMetrics for WithEveryFamily {
    fn measure(&self, text: &str, font: &rsword_layout_core::FontSpec) -> rsword_layout_core::TextMetrics {
        self.0.measure(text, font)
    }
    fn break_opportunities(&self, text: &str) -> Vec<rsword_layout_core::BreakOpportunity> {
        self.0.break_opportunities(text)
    }
    fn has_family(&self, _family: &str) -> bool {
        true
    }
}

#[test]
fn android_lays_out_an_undeclared_family_in_dengxian_when_it_can() {
    let bytes = docx("", r#"w:after="0""#, 1);
    let document = PreparedDocument::load(&bytes).unwrap().document().clone();
    let metrics = WithEveryFamily(rsword_layout_core::SimpleMetrics);
    let fonts = |platform| {
        let pages = rsword_layout_core::Engine::new(&metrics, document.sections[0].setup)
            .with_platform(platform, View::Print)
            .layout_document(&document);
        let paint = rsword_layout_core::paint_document(&pages, None, &[]);
        paint.pages[0].cmds.iter().find_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { font, .. } => Some((font.family.clone(), font.size_half_points)),
            _ => None,
        }).unwrap()
    };
    assert_eq!(fonts(Platform::Android), ("DengXian".to_string(), 22));
    assert_ne!(fonts(Platform::Desktop).0, "DengXian");
}

/// 页 0 的行数与第一行的推进量（1/7200 英寸）。
fn page0(bytes: &[u8], platform: Platform) -> (usize, i64) {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let record = LayoutRecord::from_paint(&prepared.layout_approximate(&options(platform)).unwrap().paint());
    let lines = &record.pages[0].lines;
    (lines.len(), lines[0].placement.unwrap().advance_fine)
}

#[test]
fn android_fits_the_last_auto_multiple_line_by_its_single_height() {
    let declared = r#"<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/>"#;
    let double = docx(declared, r#"w:before="0" w:after="0" w:line="480" w:lineRule="auto""#, 80);
    let (desktop, step) = page0(&double, Platform::Desktop);
    let (android, android_step) = page0(&double, Platform::Android);
    assert_eq!(step, android_step, "推进量不变");
    let body = BODY_HEIGHT * 5;
    assert_eq!(desktop as i64, body / step);
    let single = step / 2;
    assert_eq!(android as i64, 1 + (body - single) / step);
    assert!(android > desktop, "前提：这个版心上两种算法分得开");
}

#[cfg(feature = "fontenv")]
mod real_fonts {
    use super::*;
    use rsword_layout_core::{VerticalGrid, font::FontRegistry};

    fn registry() -> FontRegistry {
        let mut registry = FontRegistry::new();
        for name in ["LiberationSans-Regular.ttf", "DroidSansFallbackFull.ttf"] {
            let path = format!("{}/../../fixtures/fonts/{name}", env!("CARGO_MANIFEST_DIR"));
            registry.add(std::fs::read(path).unwrap(), 0).unwrap();
        }
        registry
    }

    fn advance(family: &str, platform: Platform) -> i64 {
        let rpr = format!(r#"<w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:eastAsia="{family}"/><w:sz w:val="24"/>"#);
        // 汉字走 eastAsia 槽：东亚字体本来就这么用（拉丁字母会落到 Liberation Sans）。
        let text = if family.starts_with("Droid") { "汉字" } else { "P" };
        let bytes = docx_text(&rpr, r#"w:after="0""#, 2, text);
        let prepared = PreparedDocument::load(&bytes).unwrap();
        let session = prepared.layout_with_fonts(registry(), VerticalGrid::None, &options(platform)).unwrap();
        LayoutRecord::from_paint(&session.paint()).pages[0].lines[0].placement.unwrap().advance_fine
    }

    #[test]
    fn android_scales_east_asian_code_page_fonts_by_one_point_three() {
        // Droid Sans Fallback 声明了东亚代码页，Liberation Sans 没有。
        let (desktop, android) = (advance("Droid Sans Fallback", Platform::Desktop), advance("Droid Sans Fallback", Platform::Android));
        assert!((android as f64 / desktop as f64 - 1.3).abs() < 0.01, "{desktop} → {android}");
        assert_eq!(advance("Liberation Sans", Platform::Android), advance("Liberation Sans", Platform::Desktop));
    }
}
