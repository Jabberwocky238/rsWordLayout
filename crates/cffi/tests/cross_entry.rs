//! C ABI 会话与 Rust 会话同输入同结果（R03）：经导出的 `extern "C"` 函数建会话，
//! 比整份规范化布局结果。文档覆盖隐藏内容、CJK 回退、表格；错误路径带诊断。

use std::ffi::CStr;

use rsword::package::Package;
use rsword_layout_cffi::*;
use rsword_layout_core::{LayoutOptions, Platform, PreparedDocument, View, WrapPolicy};

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

fn new_session(bytes: &[u8], fonts: *const RslFonts, options: Option<&RslOptions>) -> *mut RslSession {
    let options = options.map_or(std::ptr::null(), |o| o as *const RslOptions);
    unsafe { rsl_session_new_ex(bytes.as_ptr(), bytes.len(), 96.0, fonts, options) }
}

fn last_error() -> String {
    let e = rsl_last_error();
    assert!(!e.is_null());
    unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
}

fn layout_json(s: *const RslSession) -> String {
    let raw = unsafe { rsl_layout_json(s) };
    assert!(!raw.is_null());
    let json = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_owned();
    unsafe { rsl_string_free(raw) };
    json
}

fn diagnostics(s: *const RslSession) -> Vec<(String, String)> {
    (0..unsafe { rsl_diagnostic_count(s) })
        .map(|i| unsafe {
            (
                CStr::from_ptr(rsl_diagnostic_code(s, i)).to_str().unwrap().to_owned(),
                CStr::from_ptr(rsl_diagnostic_message(s, i)).to_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn approximate_sessions_match_rust() {
    let bytes = mixed();
    let options = RslOptions { platform: RSL_PLATFORM_ANDROID, view: RSL_VIEW_MOBILE, ..Default::default() };
    let s = new_session(&bytes, std::ptr::null(), Some(&options));
    assert!(!s.is_null(), "{}", last_error());
    let rust = PreparedDocument::load(&bytes)
        .unwrap()
        .layout_approximate(&LayoutOptions { platform: Platform::Android, view: View::Mobile, wrap: WrapPolicy::None })
        .unwrap();
    assert_eq!(layout_json(s), rust.layout_json());
    assert_eq!(unsafe { rsl_session_is_approximate(s) }, 1);
    let codes: Vec<_> = diagnostics(s).into_iter().map(|(code, _)| code).collect();
    let expected: Vec<_> = rust.diagnostics().iter().map(|d| d.code.as_str().to_owned()).collect();
    assert_eq!(codes, expected);
    assert!(codes.iter().any(|c| c == "METRICS_APPROXIMATE"));
    assert_eq!(unsafe { rsl_page_count(s) }, rust.pages().len());
    unsafe { rsl_session_free(s) };

    // 老入口就是默认选项的近似会话。
    let old = unsafe { rsl_session_new(bytes.as_ptr(), bytes.len(), 96.0) };
    let default = PreparedDocument::load(&bytes).unwrap().layout_approximate(&LayoutOptions::default()).unwrap();
    assert_eq!(layout_json(old), default.layout_json());
    unsafe { rsl_session_free(old) };
}

#[test]
fn errors_carry_their_diagnostics_and_bad_options_are_rejected() {
    let cell = |t: &str| format!("<w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let bytes = docx(&format!("<w:tbl><w:tr>{}{}</w:tr></w:tbl>", cell("a"), cell("b")));
    assert!(new_session(&bytes, std::ptr::null(), None).is_null());
    let error = last_error();
    assert!(error.starts_with("没有可排版的段落"), "{error}");
    assert!(error.contains("\nBLOCKS_SKIPPED: ") && error.contains("table not laid out"), "{error}");

    let good = mixed();
    for options in [
        RslOptions { platform: 7, ..Default::default() },
        RslOptions { view: 7, ..Default::default() },
        RslOptions { wrap: 7, ..Default::default() },
        RslOptions { vertical_grid: RSL_GRID_MAC, ..Default::default() },
    ] {
        assert!(new_session(&good, std::ptr::null(), Some(&options)).is_null(), "{options:?}");
        assert!(!last_error().is_empty());
    }
    assert!(last_error().contains("vertical_grid 只用于真字体"));
}

#[test]
fn a_table_only_document_is_laid_out() {
    let bytes = docx(TABLE);
    let s = new_session(&bytes, std::ptr::null(), None);
    assert!(!s.is_null(), "{}", last_error());
    assert!(diagnostics(s).iter().any(|(code, message)| code == "DOCUMENT_INPUT"
        && message.contains("table has no following paragraph")));
    unsafe { rsl_session_free(s) };
}

#[cfg(feature = "fontenv")]
mod fonts {
    use super::*;
    use rsword_layout_core::{FontSources, VerticalGrid};

    fn font(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../fixtures/fonts/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn add(f: *mut RslFonts, bytes: &[u8], fallback: bool) -> i32 {
        unsafe { rsl_fonts_add(f, bytes.as_ptr(), bytes.len(), 0, i32::from(fallback)) }
    }

    #[test]
    fn real_font_sessions_match_rust() {
        let bytes = mixed();
        let (sans, droid) = (font("LiberationSans-Regular.ttf"), font("DroidSansFallbackFull.ttf"));
        let fonts = rsl_fonts_new();
        assert_eq!(add(fonts, &sans, false), RSL_OK);
        assert_eq!(add(fonts, &droid, true), RSL_OK);
        let options = RslOptions { vertical_grid: RSL_GRID_MAC, ..Default::default() };
        let s = new_session(&bytes, fonts, Some(&options));
        assert!(!s.is_null(), "{}", last_error());

        let mut sources = FontSources::new();
        sources.add(sans, 0).unwrap();
        sources.add_fallback(droid.clone(), 0).unwrap();
        let rust = PreparedDocument::load(&bytes)
            .unwrap()
            .layout_with_fonts(sources.build(), VerticalGrid::MacWordThreeHundredthsInch, &LayoutOptions::default())
            .unwrap();
        let json = layout_json(s);
        assert_eq!(json, rust.layout_json());
        assert_eq!(unsafe { rsl_session_is_approximate(s) }, 0);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["fallbackFaces"].as_array().unwrap().len(), 1);
        assert!(value["tableLayout"].is_object());

        // 字体集之后再变，已建的会话不受影响。
        assert_eq!(add(fonts, &font("DejaVuSans.ttf"), false), RSL_OK);
        assert_eq!(layout_json(s), json);
        unsafe { rsl_session_free(s) };
        unsafe { rsl_fonts_free(fonts) };
    }

    #[test]
    fn invalid_font_bytes_report_their_code() {
        let fonts = rsl_fonts_new();
        assert_eq!(add(fonts, b"garbage", false), RSL_ERR_FAILED);
        assert_eq!(last_error(), "FONT_INVALID");
        assert_eq!(unsafe { rsl_fonts_add(std::ptr::null_mut(), b"x".as_ptr(), 1, 0, 0) }, RSL_ERR_NULL);
        // 只有回退字体：拒绝排版。
        assert_eq!(add(fonts, &font("DroidSansFallbackFull.ttf"), true), RSL_OK);
        assert!(new_session(&mixed(), fonts, None).is_null());
        assert!(last_error().starts_with("真字体度量至少要一个正文字体"));
        unsafe { rsl_fonts_free(fonts) };
    }
}
