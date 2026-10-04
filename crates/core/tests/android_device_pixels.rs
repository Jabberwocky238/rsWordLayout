//! Android：移动视图按设备像素量字宽；行尾空格不计入放不放得下。
//!
//! Word 实测（Android Word，Calibri 12pt，窄路径 5329 twips = 2879 个每英寸 778 的像素，
//! word_analyse `tab.md`、`char-scale.md`）：`i-plain` 每行 95 个 `i`（精确宽度给 96）、
//! `zero-scale`（`w:w=80`）每行 55 个 `0`（精确比例给 54）、`latinscale` 第三行收到 197
//! （精确宽度、或像素宽带上行尾空格都放不下）。像素量法的两个常数来自另两条动态读数：
//! 视图换算 `w3 = (1440 × rect + 389) / 778`（Q15）与 P0-1b 的 run 度量。
//!
//! 行尾空格那一条用桩度量（10pt：拉丁字 100 twips、空格 50）钉规则本身；
//! 像素那一条要真字体（feature `fontenv`），用仓库的 Liberation Sans，期望值从字体表独立算。

use rsword_layout_core::{
    Color, Engine, FontSpec, LayoutRecord, Margins, PageSetup, Para, Platform, Run, SimpleMetrics,
    Size, Twips, View, paint_document,
};

fn run(text: &str) -> Run {
    Run {
        text: text.to_owned(),
        font: FontSpec::new("synthetic", 20),
        color: Color::BLACK,
        placeholders: vec![],
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn starts(platform: Platform, width: Twips, runs: Vec<Run>) -> Vec<u32> {
    let setup = PageSetup { size: Size::new(width, 1_000_000), margins: Margins::uniform(0) };
    let pages = Engine::new(&SimpleMetrics, setup)
        .with_platform(platform, View::Print)
        .layout(&[Para { runs, ..Para::default() }]);
    LayoutRecord::from_paint(&paint_document(&pages, None, &[]))
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| l.source.unwrap().start)
        .collect()
}

#[test]
fn trailing_spaces_hang_on_android() {
    // `aa bb` 宽 450，带上行尾空格 500，行宽 460。Android 收下它，空格挂出；桌面计入空格，退回 `aa `。
    assert_eq!(starts(Platform::Android, 460, vec![run("aa bb cc")]), [0, 6]);
    assert_eq!(starts(Platform::Desktop, 460, vec![run("aa bb cc")]), [0, 3]);
}

#[test]
fn hanging_spaces_do_not_depend_on_run_splits() {
    // 空格在 run 末尾时同样不计：整截放下，下一截在空格之后的交界收行。
    assert_eq!(starts(Platform::Android, 460, vec![run("aa bb "), run("cc")]), [0, 6]);
    assert_eq!(starts(Platform::Android, 460, vec![run("aa "), run("bb "), run("cc")]), [0, 6]);
    assert_eq!(starts(Platform::Desktop, 460, vec![run("aa "), run("bb "), run("cc")]), [0, 3]);
}

#[cfg(feature = "fontenv")]
mod pixels {
    use super::*;
    use rsword::package::Package;
    use rsword_layout_core::{
        FontMetrics, HorizontalGrid, LayoutOptions, PageOverrides, PreparedDocument, RealMetrics,
        VerticalGrid, WrapPolicy, font::FontRegistry,
    };
    use std::path::PathBuf;

    const PER_INCH: u32 = 778;

    fn font_bytes() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/fonts/LiberationSans-Regular.ttf");
        std::fs::read(path).expect("字体读得到")
    }

    fn registry() -> FontRegistry {
        let mut registry = FontRegistry::new();
        registry.add(font_bytes(), 0).expect("字体装得进");
        registry
    }

    /// 字体表里的推进量与 upem，不经引擎。
    fn units(ch: char) -> (f64, f64) {
        use skrifa::{FontRef, MetadataProvider};
        let bytes = font_bytes();
        let font = FontRef::new(&bytes).unwrap();
        let (size, loc) = (skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
        let upem = f64::from(font.metrics(size, loc).units_per_em);
        let gid = font.charmap().map(ch).unwrap();
        (f64::from(font.glyph_metrics(size, loc).advance_width(gid).unwrap()), upem)
    }

    /// 12pt 下一个字形的像素宽：ppem = round(12 × 778 / 72) = 130。
    fn px(ch: char) -> i64 {
        let (advance, upem) = units(ch);
        (advance * 130.0 / upem).round() as i64
    }

    fn twips(px: i64) -> i32 {
        (px as f64 * 1440.0 / f64::from(PER_INCH)).round() as i32
    }

    fn spec() -> FontSpec {
        FontSpec::new("Liberation Sans", 24)
    }

    #[test]
    fn advances_are_whole_device_pixels() {
        let registry = registry();
        let grid = HorizontalGrid::DevicePixels { per_inch: PER_INCH };
        let metrics = RealMetrics::new(&registry).with_horizontal_grid(grid);
        assert_eq!(metrics.measure("iiii", &spec()).advance, twips(4 * px('i')));
        assert_eq!(metrics.measure("ia0 ", &spec()).advance, twips(px('i') + px('a') + px('0') + px(' ')));
        let pt = metrics.advance_pt("iiii", &spec());
        assert!((pt - 4.0 * px('i') as f64 * 72.0 / f64::from(PER_INCH)).abs() < 1e-9, "{pt}");
    }

    #[test]
    fn scaling_truncates_each_pixel_width_and_spacing_is_whole_pixels() {
        // `w:w=80`：每个字形 px × 80 / 100 截断（Word `zero-scale` 每行 55 个 `0` 只在截断下成立）；
        // 字符间距 20 twips 在每英寸 778 下是 10.8 像素，取整为 11。
        let registry = registry();
        let metrics = RealMetrics::new(&registry)
            .with_horizontal_grid(HorizontalGrid::DevicePixels { per_inch: PER_INCH });
        let mut scaled = spec();
        scaled.scale_pct = 80;
        assert_eq!(metrics.measure("00", &scaled).advance, twips(2 * (px('0') * 80 / 100)));
        let mut spaced = spec();
        spaced.letter_spacing = 20;
        assert_eq!(metrics.measure("00", &spaced).advance, twips(2 * (px('0') + 11)));
    }

    fn docx(body: &str) -> Vec<u8> {
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

    /// 一段 Liberation Sans 12pt 的 150 个 `i`，排在版心 5300 上：各行起点与绘制出的字形推进量（点）。
    fn laid(platform: Platform, view: View) -> (Vec<u32>, Vec<f64>) {
        let body = format!(
            r#"<w:p><w:r><w:rPr><w:rFonts w:ascii="Liberation Sans" w:hAnsi="Liberation Sans"/><w:sz w:val="24"/></w:rPr><w:t>{}</w:t></w:r></w:p>"#,
            "i".repeat(150)
        );
        let bytes = docx(&body);
        let mut prepared = PreparedDocument::load(&bytes).unwrap();
        prepared
            .apply_page_overrides(PageOverrides { content_width: Some(5300), ..PageOverrides::default() })
            .unwrap();
        let options = LayoutOptions { platform, view, wrap: WrapPolicy::None };
        let session = prepared.layout_with_fonts(registry(), VerticalGrid::None, &options).unwrap();
        let record = LayoutRecord::from_paint(&session.paint());
        let starts = record.pages.iter().flat_map(|p| &p.lines).map(|l| l.source.unwrap().start).collect();
        let advances = session.paint().pages[0].cmds.iter().flat_map(|cmd| match cmd {
            rsword_layout_core::DrawCmd::DrawGlyphs { glyphs, .. } => glyphs.iter().map(|g| g.advance_x_pt).collect(),
            _ => Vec::new(),
        }).collect();
        (starts, advances)
    }

    #[test]
    fn only_android_mobile_view_breaks_on_pixel_widths() {
        // 5300 twips：精确宽度放得下 99 个 `i`（53.32 twips），像素宽 29 px = 53.68 twips 只放得下 98 个。
        let exact = (5300.0 / (units('i').0 / units('i').1 * 240.0)).floor() as u32;
        let pixel = (1..300).rev().find(|&n| twips(n * px('i')) <= 5300).unwrap() as u32;
        assert_eq!((exact, pixel), (99, 98), "前提：两种量法在这个版心上分得开");
        assert_eq!(laid(Platform::Android, View::Mobile).0[..2], [0, pixel]);
        assert_eq!(laid(Platform::Android, View::Print).0[..2], [0, exact]);
        assert_eq!(laid(Platform::Desktop, View::Mobile).0[..2], [0, exact]);
    }

    #[test]
    fn painted_advances_use_the_same_pixels_as_line_breaking() {
        let step = 72.0 / f64::from(PER_INCH);
        let (_, advances) = laid(Platform::Android, View::Mobile);
        assert!(!advances.is_empty());
        for advance in advances {
            let pixels = advance / step;
            assert!((pixels - pixels.round()).abs() < 1e-6, "{advance} pt 不是整像素");
        }
        let (_, print) = laid(Platform::Android, View::Print);
        assert!(print.iter().any(|a| ((a / step) - (a / step).round()).abs() > 1e-3), "打印视图不量化");
    }
}
