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
    Command::new(env!("CARGO_BIN_EXE_render"))
        .current_dir(root())
        .args(args)
        .arg("fixtures/cjk-plain.docx")
        .arg(output)
        .output()
        .unwrap()
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
