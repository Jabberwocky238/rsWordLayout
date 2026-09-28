//! `layout-trace` 走文档会话：轨迹就是会话 API 的结果，错误的最后一行不变
//! （验收面板的 `stderrTail` 只读最后一行）。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rsword_layout_core::font::FontRegistry;
use rsword_layout_core::{
    LayoutOptions, LayoutRecord, PreparedDocument, TraceMeta, VerticalGrid, WrapPolicy, to_trace_json,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn trace_cli(args: &[&str], output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_layout-trace"))
        .current_dir(root())
        .args(args)
        .arg(output)
        .output()
        .unwrap()
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rsword-layout-trace-cli-{}-{name}", std::process::id()))
}

#[test]
fn the_trace_is_the_session_result() {
    let output = temp("session.json");
    let result = trace_cli(&[
        "--vertical-grid", "mac", "--no-fallback",
        "--font", "fixtures/fonts/LiberationSans-Regular.ttf",
        "fixtures/cjk-plain.docx",
    ], &output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let cli: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&output).unwrap()).unwrap();
    let _ = std::fs::remove_file(&output);

    let bytes = std::fs::read(root().join("fixtures/cjk-plain.docx")).unwrap();
    let mut fonts = FontRegistry::new();
    fonts.add(std::fs::read(root().join("fixtures/fonts/LiberationSans-Regular.ttf")).unwrap(), 0).unwrap();
    let session = PreparedDocument::load(&bytes)
        .unwrap()
        .layout_with_fonts(fonts, VerticalGrid::MacWordThreeHundredthsInch, &LayoutOptions::default())
        .unwrap();
    let meta = TraceMeta {
        engine: String::new(),
        metrics: String::new(),
        glyph_origin_method: String::new(),
        source: String::new(),
        font_fingerprint: session.font_fingerprint().map(str::to_string),
    };
    let api: serde_json::Value =
        serde_json::from_str(&to_trace_json(&LayoutRecord::from_paint(&session.paint()), &meta)).unwrap();
    assert_eq!(cli["pages"], api["pages"]);
    assert_eq!(cli["fontFingerprint"], api["fontFingerprint"]);
    assert_eq!(cli["layoutInput"], session.document().trace_metadata());
    // 会话的字体覆盖诊断打到 stderr：只装 Liberation Sans，汉字按名义宽度画，
    // eastAsia 槽点名的 Songti SC 没装。（回退报告里也有「按名义 1 em」，所以核会话自己的措辞。）
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("没有字体盖得住，按名义 1 em 画 .notdef"), "{stderr}");
    assert!(stderr.contains("「Songti SC」"), "{stderr}");
}

/// `--platform` / `--view` 进了会话：缺省制表位看平台，段末分页符看视图。
#[test]
fn platform_and_view_flags_reach_the_session() {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let body = concat!(
        r#"<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>one</w:t><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>"#,
    );
    let mut package = rsword::package::Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{w}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    let bytes = package.save().unwrap();
    let docx = temp("platform.docx");
    std::fs::write(&docx, &bytes).unwrap();
    let output = temp("platform.json");
    let result = trace_cli(&[
        "--platform", "android", "--view", "mobile", "--no-fallback",
        "--font", "fixtures/fonts/LiberationSans-Regular.ttf",
        docx.to_str().unwrap(),
    ], &output);
    let _ = std::fs::remove_file(&docx);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let cli: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&output).unwrap()).unwrap();
    let _ = std::fs::remove_file(&output);

    let pages = |platform, view| {
        let mut fonts = FontRegistry::new();
        fonts.add(std::fs::read(root().join("fixtures/fonts/LiberationSans-Regular.ttf")).unwrap(), 0).unwrap();
        let options = LayoutOptions { platform, view, wrap: WrapPolicy::None };
        let session = PreparedDocument::load(&bytes)
            .unwrap()
            .layout_with_fonts(fonts, VerticalGrid::None, &options)
            .unwrap();
        let meta = TraceMeta {
            engine: String::new(),
            metrics: String::new(),
            glyph_origin_method: String::new(),
            source: String::new(),
            font_fingerprint: None,
        };
        let json = to_trace_json(&LayoutRecord::from_paint(&session.paint()), &meta);
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["pages"].clone()
    };
    use rsword_layout_core::{Platform, View};
    let expected = pages(Platform::Android, View::Mobile);
    assert_eq!(cli["pages"], expected);
    // 这份文档对两个开关都敏感，上面的相等才说明两者都传到了。
    assert_ne!(expected, pages(Platform::Desktop, View::Mobile));
    assert_ne!(expected, pages(Platform::Android, View::Print));
}

#[test]
fn nothing_layable_still_ends_with_the_same_error_line() {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let mut package = rsword::package::Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    // 一行两格的表格本版不排；此外什么都没有。
    let cell = |t: &str| format!("<w:tc><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let body = format!("<w:tbl><w:tr>{}{}</w:tr></w:tbl>", cell("a"), cell("b"));
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{w}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    let docx = temp("empty.docx");
    std::fs::write(&docx, package.save().unwrap()).unwrap();
    let output = temp("empty.json");
    let result = trace_cli(&["--no-fallback", docx.to_str().unwrap()], &output);
    let _ = std::fs::remove_file(&docx);
    assert!(!result.status.success());
    assert!(!output.exists(), "nothing is written");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("table not laid out"), "{stderr}");
    assert_eq!(stderr.trim().lines().last(), Some(r#"Error: "没有可排版的段落""#), "{stderr}");
}

#[test]
fn approximate_metrics_refuse_fonts() {
    let output = temp("simple.json");
    let result = trace_cli(&[
        "--metrics", "simple",
        "--font", "fixtures/fonts/LiberationSans-Regular.ttf",
        "fixtures/cjk-plain.docx",
    ], &output);
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("不能与 --font 同用"));
}
