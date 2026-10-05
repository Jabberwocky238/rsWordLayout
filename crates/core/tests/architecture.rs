//! 依赖边界：写成测试，越界就红。
//!
//! 1. core 不依赖任何后端或绑定 crate（`rsword-layout-*`）：后端只消费 core 的输出。
//! 2. 只有装载与会话入口直接读解析器的数据结构（`rsword::`）；排版、断行、分页、表格、字体
//!    只看 core 自己的投影（`LayoutDocument` / `Para` / `FontSpec`），解析器换版本时改动集中在入口。
//! 3. 纯后端（gpu / vulkan / opengl / webgl）不直接依赖解析器，只经 core 拿绘制列表。

use std::fs;
use std::path::{Path, PathBuf};

const CORE: &str = env!("CARGO_MANIFEST_DIR");

/// 允许直接写 `rsword::` 的 core 源文件（相对 `src/`）。加一项要在提交里说明为什么排版层需要它。
const PARSER_ENTRY_FILES: &[&str] = &["load.rs", "session.rs", "anchor.rs"];

const BACKENDS: &[&str] = &["gpu", "vulkan", "opengl", "webgl"];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `[dependencies]` 及各 `[target.*.dependencies]` 段里的依赖名。
fn dependency_names(manifest: &Path) -> Vec<String> {
    let text = fs::read_to_string(manifest).unwrap();
    let mut names = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let header = line.trim_matches(|c| c == '[' || c == ']');
            in_deps = header == "dependencies" || header.ends_with(".dependencies");
            // `[target.'cfg(..)'.dependencies.web-sys]` 这种表头本身就是一个依赖
            if let Some(name) = header.split(".dependencies.").nth(1) {
                names.push(name.to_string());
            }
            continue;
        }
        if in_deps
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            names.push(name.trim().to_string());
        }
    }
    names
}

fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.split("//").next().unwrap_or("")))
}

#[test]
fn core_depends_on_no_backend_or_binding_crate() {
    let deps = dependency_names(&Path::new(CORE).join("Cargo.toml"));
    let bad: Vec<_> = deps.iter().filter(|d| d.starts_with("rsword-layout-")).collect();
    assert!(bad.is_empty(), "core must not depend on {bad:?}");
}

#[test]
fn only_the_entry_modules_name_parser_types() {
    let src = Path::new(CORE).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut hits = Vec::new();
    for file in &files {
        let rel = file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if PARSER_ENTRY_FILES.contains(&rel.as_str()) {
            continue;
        }
        let text = fs::read_to_string(file).unwrap();
        for (n, code) in code_lines(&text) {
            if code.contains("rsword::") || code.trim_start().starts_with("use rsword") {
                hits.push(format!("src/{rel}:{n}: {}", code.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "layout code reads parser structures directly; go through load/bridge instead:\n{}",
        hits.join("\n")
    );
}

#[test]
fn every_parser_entry_file_still_exists() {
    // 防止白名单在改名后悄悄失效
    for f in PARSER_ENTRY_FILES {
        assert!(Path::new(CORE).join("src").join(f).is_file(), "src/{f} is gone; update PARSER_ENTRY_FILES");
    }
}

#[test]
fn render_backends_do_not_depend_on_the_parser() {
    let crates = Path::new(CORE).parent().unwrap();
    for backend in BACKENDS {
        let manifest = crates.join(backend).join("Cargo.toml");
        let deps = dependency_names(&manifest);
        assert!(!deps.is_empty(), "{backend}: no dependencies parsed from {manifest:?}");
        assert!(!deps.iter().any(|d| d == "rsword"), "{backend} must get pages from core, not the parser");
        let mut files = Vec::new();
        rust_files(&crates.join(backend).join("src"), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            for (n, code) in code_lines(&text) {
                assert!(!code.contains("rsword::"), "{}:{n} names a parser type", file.display());
            }
        }
    }
}
