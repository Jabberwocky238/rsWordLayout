use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct OutputDir(PathBuf);

impl OutputDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rsword-svg-cli-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for OutputDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str], output: &Path) -> Output {
    run_on(args, Path::new("fixtures/cjk-plain.docx"), output)
}

fn run_on(args: &[&str], input: &Path, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_render"))
        .current_dir(root())
        .args(args)
        .arg(input)
        .arg(output)
        .output()
        .unwrap()
}

/// 只有一张受支持的表格、表后没有尾段（word_analyse `table32-rowh` 的形状）。
fn table_only_docx(dir: &Path) -> PathBuf {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let row = |text: &str| format!(
        r#"<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr>"#
    );
    let body = format!(
        concat!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="nil"/>"#,
            r#"<w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/>"#,
            r#"<w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/>"#,
            r#"<w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/>"#,
            r#"</w:tblCellMar></w:tblPr>{}{}</w:tbl>"#,
        ),
        row("cell one"),
        row("cell two"),
    );
    let mut package = rsword::package::Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{w}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    let path = dir.join("table-only.docx");
    std::fs::write(&path, package.save().unwrap()).unwrap();
    path
}

#[test]
fn help_describes_font_requirements_and_selection_tradeoff() {
    let output = Command::new(env!("CARGO_BIN_EXE_render")).arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for word in ["text|outlines", "--font", "--fallback-font", "without text selection", "fontenv"] {
        assert!(help.contains(word), "{help}");
    }
}

#[test]
fn invalid_options_leave_existing_output_untouched() {
    let dir = OutputDir::new();
    let path = dir.0.join("existing.html");
    std::fs::write(&path, "keep existing output").unwrap();
    let cases: &[(&[&str], &str)] = &[
        (&["--text-mode", "unknown"], "Unknown text mode"),
        (&["--text-mode", "--font"], "Missing value"),
        (&["--unknown"], "Unknown option"),
        (&["--text-mode", "outlines"], "at least one --font"),
        (&["--font", "font.ttf"], "require --text-mode outlines"),
        (&["--fallback-font", "font.ttf"], "require --text-mode outlines"),
        (&["--text-mode", "outlines", "--font", "font.ttc#bad"], "Invalid font index"),
    ];
    for (args, expected) in cases {
        let result = run(args, &path);
        assert!(!result.status.success(), "{args:?}");
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep existing output");
    }
}

#[test]
fn default_mode_still_exports_selectable_browser_text() {
    let dir = OutputDir::new();
    let path = dir.0.join("text.html");
    let result = run(&[], &path);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("<text "));
    assert!(html.contains("xml:space=\"preserve\""));
}

#[cfg(not(feature = "fontenv"))]
#[test]
fn unavailable_outline_feature_is_reported_before_reading_fonts() {
    let dir = OutputDir::new();
    let path = dir.0.join("outline.html");
    let result = run(&["--text-mode", "outlines", "--font", "nonexistent.ttf"], &path);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--features fontenv"));
    assert!(!path.exists());
}

#[cfg(feature = "fontenv")]
#[test]
fn outline_mode_renders_a_real_docx_using_explicit_fonts() {
    let dir = OutputDir::new();
    let path = dir.0.join("outline.html");
    let result = run(&[
        "--text-mode", "outlines",
        "--font", "fixtures/fonts/LiberationSans-Regular.ttf#0",
        "--fallback-font", "fixtures/fonts/DroidSansFallbackFull.ttf",
    ], &path);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("<path "));
    assert!(html.contains("matrix("));
    assert!(!html.contains("<text "), "text must not be reshaped by the browser");
    assert!(!html.contains("font-family="));
}

#[cfg(feature = "fontenv")]
#[test]
fn font_loading_errors_preserve_existing_output() {
    let dir = OutputDir::new();
    let path = dir.0.join("existing.html");
    std::fs::write(&path, "keep existing output").unwrap();
    for font in ["not-present.ttf", "fixtures/fonts/LiberationSans-Regular.ttf#999"] {
        let result = run(&["--text-mode", "outlines", "--font", font], &path);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(font.split('#').next().unwrap()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep existing output");
    }
}

#[test]
fn table_only_documents_render_and_keep_their_diagnostics() {
    // 只看 `paras.is_empty()` 时这份文档被拒；表格排得出，缺尾段的诊断照留。
    let dir = OutputDir::new();
    let input = table_only_docx(&dir.0);
    let path = dir.0.join("table.html");
    let result = run_on(&[], &input, &path);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stderr}");
    // 装载诊断先打、排版诊断后打，各一次。
    assert_eq!(stderr.matches("table has no following paragraph").count(), 1, "{stderr}");
    assert!(stderr.contains("近似度量"), "text mode must say it is approximate: {stderr}");
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("cell one") && html.contains("cell two"));
}

#[cfg(feature = "fontenv")]
#[test]
fn table_only_documents_render_as_outlines() {
    let dir = OutputDir::new();
    let input = table_only_docx(&dir.0);
    let path = dir.0.join("table.html");
    let result = run_on(&[
        "--text-mode", "outlines",
        "--font", "fixtures/fonts/LiberationSans-Regular.ttf#0",
        "--fallback-font", "fixtures/fonts/DroidSansFallbackFull.ttf",
    ], &input, &path);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("<path ") && !html.contains("<text "));
}

#[cfg(feature = "fontenv")]
#[test]
fn outline_mode_reports_characters_no_font_covers() {
    // 没有 CJK 字体：汉字照样占 1 em 画 .notdef，但要报出来，不能看着像排对了。
    let dir = OutputDir::new();
    let path = dir.0.join("outline.html");
    let result = run(&["--text-mode", "outlines", "--font", "fixtures/fonts/LiberationSans-Regular.ttf"], &path);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stderr}");
    assert!(stderr.contains("按名义 1 em 画 .notdef"), "{stderr}");
    assert!(stderr.contains("「Songti SC」"), "the declared eastAsia family is not installed: {stderr}");
}

#[test]
fn anchored_objects_still_wrap_text() {
    // render 按锚定对象绕排（会话的缺省是不绕排，所以要由入口显式要）。
    let dir = OutputDir::new();
    let path = dir.0.join("wrap.html");
    let result = run_on(&[], Path::new("fixtures/wrap.docx"), &path);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stderr}");
    assert!(String::from_utf8_lossy(&result.stdout).contains("环绕区 1 ·"));
    assert!(stderr.contains("按第 0 节版心定位"), "wrap is approximate and must say so: {stderr}");
}
