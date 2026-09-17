//! 从磁盘装字体，建出 [`docx_layout::fontenv::FontEnvironment`]（feature `shape`）。
//!
//! 这一层只做「把文件读进来」，不做选择——选择是 `fontenv` 的事，按码位走。
//!
//! ## 为什么要记指纹
//!
//! 量具方法 §6.2：**度量兼容克隆的字体替换，在几何上完全不可见。**
//! Liberation Serif 是 Times New Roman 的度量兼容克隆（Sans↔Arial、Carlito↔Calibri），
//! 替换后 `glyphOrigin` 与 `advanceVector` 一字不差（2618 条记录 max |Δ| = 0.000000pt）。
//!
//! **后果：任何几何自检都发现不了字体被换过，只有字体名能。**
//! 所以 [`LoadReport`] 把装进来的族名逐个列出来，让上层能核「用的是不是申请的那一份」。

use std::path::{Path, PathBuf};

use docx_layout::fontenv::{FontEnvironment, FontEnvironmentBuilder};

/// macOS 的字体目录。与量具 `fingerprint.FONT_ROOTS` 同一套口径，
/// 两边算出的字体集才能对得上。
pub const MAC_FONT_ROOTS: &[&str] = &[
    "~/Library/Fonts",
    "/Library/Fonts",
    "/System/Library/Fonts",
    "/System/Library/Fonts/Supplemental",
    "/Network/Library/Fonts",
];

pub const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc", "dfont"];

/// 装载结果。**不要只看 `faces`**：`failed` 与 `families` 同样是结论的一部分。
#[derive(Debug, Clone, Default)]
pub struct LoadReport {
    /// 成功装入的 face 数。
    pub faces: usize,
    /// 扫到但装不进去的文件（连同原因）。位图字体、损坏文件、不支持的格式都会落在这里。
    pub failed: Vec<(PathBuf, String)>,
    /// 装进来的全部族名，排序去重。核字体替换就靠它（§6.2）。
    pub families: Vec<String>,
    /// 环境指纹：同一字体集给出同一串。
    pub fingerprint: String,
}

impl LoadReport {
    /// 申请的族是不是都在。**这是唯一能发现字体替换的检查。**
    pub fn missing(&self, required: &[&str]) -> Vec<String> {
        required
            .iter()
            .filter(|want| {
                let want = docx_layout::fontenv::normalize_family(want);
                !self.families.contains(&want)
            })
            .map(|s| s.to_string())
            .collect()
    }
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    // 按路径排序：`fontenv` 的指纹与 face 顺序都依赖装入顺序，
    // 而 `read_dir` 的顺序是文件系统说了算的。不排就不可复现。
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| FONT_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        {
            out.push(path);
        }
    }
}

/// 扫目录，把能装的都装进去。
///
/// TTC/OTC 是字体集合，一个文件里多个 face：逐个 index 试到失败为止。
pub fn load_from_dirs(roots: &[&str]) -> (FontEnvironment, LoadReport) {
    let mut builder = FontEnvironmentBuilder::new();
    let mut report = LoadReport::default();

    let mut files = Vec::new();
    for root in roots {
        collect(&expand(root), &mut files);
    }

    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            report.failed.push((path, "读不出来".into()));
            continue;
        };
        let mut index = 0u32;
        let mut any = false;
        loop {
            match builder.add(bytes.clone(), index) {
                Ok(_) => {
                    report.faces += 1;
                    any = true;
                    index += 1;
                }
                Err(error) => {
                    if !any {
                        report.failed.push((path.clone(), error.to_string()));
                    }
                    break;
                }
            }
        }
    }

    let env = builder.freeze();
    report.fingerprint = env.fingerprint().to_string();
    let mut families: Vec<String> = env
        .faces()
        .flat_map(|f| f.families().iter().cloned())
        .collect();
    families.sort();
    families.dedup();
    report.families = families;
    (env, report)
}

/// 只装指定的几个文件。测试与「只要这几份字体」的场景用。
pub fn load_files(paths: &[PathBuf]) -> (FontEnvironment, LoadReport) {
    let mut builder = FontEnvironmentBuilder::new();
    let mut report = LoadReport::default();
    for path in paths {
        match std::fs::read(path) {
            Ok(bytes) => match builder.add(bytes, 0) {
                Ok(_) => report.faces += 1,
                Err(error) => report.failed.push((path.clone(), error.to_string())),
            },
            Err(error) => report.failed.push((path.clone(), error.to_string())),
        }
    }
    let env = builder.freeze();
    report.fingerprint = env.fingerprint().to_string();
    let mut families: Vec<String> = env
        .faces()
        .flat_map(|f| f.families().iter().cloned())
        .collect();
    families.sort();
    families.dedup();
    report.families = families;
    (env, report)
}
