//! WASM 会话与 Rust 会话同输入同结果（R03）：比的是整份规范化布局结果，
//! 不是页数。文档覆盖隐藏内容、CJK 回退、表格；错误路径带诊断。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, Platform, PreparedDocument, View, WrapPolicy};
use rsword_layout_wasm::LayoutSession;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(body: &str) -> Vec<u8> {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

const TABLE: &str = concat!(
    r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="nil"/>"#,
    r#"<w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/>"#,
    r#"<w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/>"#,
    r#"<w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/>"#,
    r#"</w:tblCellMar></w:tblPr>"#,
    r#"<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>cell 表</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"</w:tbl>"#,
);

/// 西文、隐藏的 run、eastAsia 槽点名 SimSun（没装，走回退链）的汉字、一张受支持的表格与尾段。
fn mixed() -> Vec<u8> {
    let fonts = r#"<w:rFonts w:ascii="Liberation Sans" w:hAnsi="Liberation Sans" w:eastAsia="SimSun"/>"#;
    let run = |text: &str, extra: &str| {
        format!(r#"<w:r><w:rPr>{fonts}{extra}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
    };
    docx(&format!(
        "<w:p>{}{}{}</w:p>{TABLE}<w:p>{}</w:p>",
        run("Visible ", ""),
        run("hidden ", "<w:vanish/>"),
        run("汉字排版", ""),
        run("tail", ""),
    ))
}

#[test]
fn approximate_sessions_match_rust() {
    let bytes = mixed();
    let options = LayoutOptions { platform: Platform::Android, view: View::Mobile, wrap: WrapPolicy::None };
    let wasm = LayoutSession::build_approximate(&bytes, 96.0, &options).unwrap();
    let rust = PreparedDocument::load(&bytes).unwrap().layout_approximate(&options).unwrap();
    assert_eq!(wasm.layout_json(), rust.layout_json());
    assert!(wasm.is_approximate());
    // 默认构造就是默认选项的近似会话。
    let default = PreparedDocument::load(&bytes).unwrap().layout_approximate(&LayoutOptions::default()).unwrap();
    assert_eq!(LayoutSession::build(&bytes, 0.0).unwrap().layout_json(), default.layout_json());
}

#[test]
fn nothing_layable_reports_the_diagnostics_in_the_error() {
    let cell = |t: &str| format!("<w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let bytes = docx(&format!("<w:tbl><w:tr>{}{}</w:tr></w:tbl>", cell("a"), cell("b")));
    let error = LayoutSession::build(&bytes, 96.0).err().unwrap();
    assert!(error.starts_with("没有可排版的段落"), "{error}");
    assert!(error.contains("\nBLOCKS_SKIPPED: "), "{error}");
    assert!(error.contains("table not laid out"), "{error}");
    assert!(LayoutSession::build(b"not a docx", 96.0).err().unwrap().starts_with("解析失败："));
}

#[test]
fn a_table_only_document_is_laid_out() {
    let session = LayoutSession::build(&docx(TABLE), 96.0).unwrap();
    assert_eq!(session.pages()[0].table_rows.len(), 1);
    let diagnostics: serde_json::Value = serde_json::from_str(&session.diagnostics_json()).unwrap();
    assert!(diagnostics.as_array().unwrap().iter().any(|d| d["code"] == "DOCUMENT_INPUT"
        && d["message"].as_str().unwrap().contains("table has no following paragraph")));
}

#[cfg(feature = "fontenv")]
mod fonts {
    use super::*;
    use rsword_layout_core::{FontSources, LayoutRecord, VerticalGrid};

    fn font(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../fixtures/fonts/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn sources() -> FontSources {
        let mut fonts = FontSources::new();
        fonts.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        fonts.add_fallback(font("DroidSansFallbackFull.ttf"), 0).unwrap();
        fonts
    }

    #[test]
    fn real_font_sessions_match_rust() {
        let bytes = mixed();
        let fonts = sources();
        let grid = VerticalGrid::MacWordThreeHundredthsInch;
        let options = LayoutOptions::default();
        let wasm = LayoutSession::build_with_fonts(&bytes, 96.0, &fonts, grid, &options).unwrap();
        let rust = PreparedDocument::load(&bytes).unwrap().layout_with_fonts(fonts.build(), grid, &options).unwrap();
        let json = wasm.layout_json();
        assert_eq!(json, rust.layout_json());

        // 比的东西确实覆盖了隐藏内容、回退与表格。
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let fallback = value["fallbackFaces"][0].as_str().unwrap().to_owned();
        let record = LayoutRecord::from_paint(&wasm.paint_all());
        let glyphs: Vec<_> = record.pages.iter().flat_map(|p| &p.lines).flat_map(|l| &l.glyphs).collect();
        assert!(glyphs.iter().any(|g| g.face == fallback), "汉字由回退字体画");
        let hidden = 8..15; // "hidden " 的 UTF-16 源区间
        assert!(glyphs.iter().all(|g| g.source.is_none_or(|s| s.end <= hidden.start || s.start >= hidden.end)),
            "隐藏的 run 不出字形");
        // 隐藏内容保留源位置：汉字从 15 起，不是 8。
        assert!(glyphs.iter().any(|g| g.face == fallback && g.source.is_some_and(|s| s.start == hidden.end)));
        assert!(value["tableLayout"]["pages"][0]["rows"].as_array().is_some_and(|r| r.len() == 1));
        assert_eq!(value["metrics"], "real");
        assert_eq!(value["verticalGrid"], "mac");
    }

    #[test]
    fn a_session_keeps_the_fonts_it_was_laid_out_with() {
        let bytes = mixed();
        let mut fonts = sources();
        let session = LayoutSession::build_with_fonts(&bytes, 96.0, &fonts, VerticalGrid::None, &LayoutOptions::default()).unwrap();
        let before = session.layout_json();
        assert!(session.session().same_fonts(fonts.registry()));

        fonts.add(font("DejaVuSans.ttf"), 0).unwrap();
        assert!(!session.session().same_fonts(fonts.registry()), "字体集变了，旧会话不再匹配");
        assert_eq!(session.layout_json(), before, "已排好的会话不受字体集后来的改动影响");
    }

    #[test]
    fn fonts_only_in_the_fallback_chain_are_rejected() {
        let mut fonts = FontSources::new();
        fonts.add_fallback(font("DroidSansFallbackFull.ttf"), 0).unwrap();
        let error = LayoutSession::build_with_fonts(&mixed(), 96.0, &fonts, VerticalGrid::None, &LayoutOptions::default())
            .err().unwrap();
        assert!(error.starts_with("真字体度量至少要一个正文字体"), "{error}");
        assert!(FontSources::new().add(b"garbage".to_vec(), 0).is_err());
    }
}
