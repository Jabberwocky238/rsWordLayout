//! `render [--text-mode text|outlines] [--font path[#index]] <input.docx> [output.html]`
//!
//! 端到端链路：rsword 解析 → 桥接成段落 → 布局引擎排版 → **矢量**绘制指令 → SVG。
//! 解析、排版与绘制都经 [`rsword_layout_core::PreparedDocument`] 建的文档会话，与 `layout-trace` 同一条路。
//!
//! Text mode keeps the original selectable browser text and approximate metrics.
//! Outline mode uses explicit fonts for measurement, shaping and vector contours.

use std::path::PathBuf;

use rsword_layout_core::{LayoutOptions, PreparedDocument, WrapPolicy};
use rsword_layout_svg::render_html;

const USAGE: &str = "Usage: render [options] <input.docx> [output.html]\n\
    --text-mode text|outlines   text is selectable and approximate (default);\n\
                               outlines preserves positioned glyphs, without text selection\n\
    --font path[#index]         primary font for outlines; repeatable; index defaults to 0\n\
    --fallback-font path[#index]  explicit CJK fallback chain for outlines; repeatable\n\
    --                         remaining arguments are file paths\n\
    Outline mode requires a build with --features fontenv and at least one --font.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextMode {
    Text,
    Outlines,
}

#[cfg_attr(not(feature = "fontenv"), allow(dead_code))]
struct FontInput {
    path: PathBuf,
    index: u32,
}

impl FontInput {
    fn parse(value: String) -> Result<Self, String> {
        let (path, index) = match value.rsplit_once('#') {
            Some((path, index)) => (path, index.parse().map_err(|_| format!("Invalid font index: {value}"))?),
            None => (value.as_str(), 0),
        };
        if path.is_empty() {
            return Err("Font path must not be empty".into());
        }
        Ok(Self { path: path.into(), index })
    }
}

struct Args {
    mode: TextMode,
    #[cfg_attr(not(feature = "fontenv"), allow(dead_code))]
    fonts: Vec<FontInput>,
    #[cfg_attr(not(feature = "fontenv"), allow(dead_code))]
    fallback_fonts: Vec<FontInput>,
    input: PathBuf,
    output: PathBuf,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Self>, String> {
        let mut mode = TextMode::Text;
        let mut fonts = Vec::new();
        let mut fallback_fonts = Vec::new();
        let mut paths = Vec::new();
        let mut positional = false;
        while let Some(arg) = args.next() {
            if positional {
                paths.push(PathBuf::from(arg));
                continue;
            }
            let mut value = || args.next().filter(|s| !s.starts_with("--"))
                .ok_or_else(|| format!("Missing value for {arg}"));
            match arg.as_str() {
                "--help" | "-h" => return Ok(None),
                "--" => positional = true,
                "--text-mode" => mode = match value()?.as_str() {
                    "text" => TextMode::Text,
                    "outlines" => TextMode::Outlines,
                    other => return Err(format!("Unknown text mode: {other}")),
                },
                "--font" => fonts.push(FontInput::parse(value()?)?),
                "--fallback-font" => fallback_fonts.push(FontInput::parse(value()?)?),
                _ if arg.starts_with('-') => return Err(format!("Unknown option: {arg}")),
                _ => paths.push(PathBuf::from(arg)),
            }
        }
        if paths.is_empty() || paths.len() > 2 {
            return Err(USAGE.into());
        }
        match mode {
            TextMode::Text if !fonts.is_empty() || !fallback_fonts.is_empty() =>
                return Err("Font options require --text-mode outlines".into()),
            TextMode::Outlines if fonts.is_empty() =>
                return Err("Outline mode requires at least one --font".into()),
            _ => {}
        }
        let output = paths.get(1).cloned().unwrap_or_else(|| PathBuf::from("layout.html"));
        Ok(Some(Self { mode, fonts, fallback_fonts, input: paths.remove(0), output }))
    }
}

/// 按命令行装字体：正文字体全部先装，回退字体按声明顺序装（全装，不看文档缺不缺字）。
/// 任何一个读不到、装不进都停：宁可不出结果，也不悄悄换字体。
#[cfg(feature = "fontenv")]
fn load_fonts(args: &Args) -> Result<rsword_layout_core::font::FontRegistry, String> {
    let mut fonts = rsword_layout_core::font::FontRegistry::new();
    for (sources, fallback) in [(&args.fonts, false), (&args.fallback_fonts, true)] {
        for source in sources {
            let bytes = std::fs::read(&source.path)
                .map_err(|error| format!("{}: {error}", source.path.display()))?;
            let result = if fallback { fonts.add_fallback(bytes, source.index) }
                else { fonts.add(bytes, source.index) };
            result.map_err(|error| format!("{}#{}: {error}", source.path.display(), source.index))?;
        }
    }
    Ok(fonts)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(args) = Args::parse(std::env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    #[cfg(not(feature = "fontenv"))]
    if args.mode == TextMode::Outlines {
        return Err("Outline mode requires rebuilding with --features fontenv".into());
    }

    let bytes = std::fs::read(&args.input)?;

    // 1. 解析与桥接。并排的 `w:rPr` 解析器只留最后一个，`PreparedDocument::load` 先把它们并起来。
    let prepared = PreparedDocument::load(&bytes)?;
    for diagnostic in prepared.diagnostics() {
        eprintln!("{diagnostic}");
    }
    if !prepared.has_layout_content() {
        return Err("没有可排版的段落".into());
    }

    // 2. 排版。环绕区在会话里扫（锚定几何走 rsword 的 Rust 模型，不在 JSON 里）；
    // 轮廓模式的量宽、整形与轮廓共用会话持有的同一个字体注册表。
    let options = LayoutOptions { wrap: WrapPolicy::Anchors, ..LayoutOptions::default() };
    let session = match args.mode {
        TextMode::Text => prepared.layout_approximate(&options).map_err(|e| e.to_string())?,
        #[cfg(feature = "fontenv")]
        TextMode::Outlines => {
            let fonts = load_fonts(&args)?;
            prepared
                .layout_with_fonts(fonts, rsword_layout_core::VerticalGrid::None, &options)
                .map_err(|e| e.to_string())?
        }
        #[cfg(not(feature = "fontenv"))]
        TextMode::Outlines => unreachable!("feature availability was checked before loading"),
    };
    for diagnostic in session.layout_diagnostics() {
        eprintln!("{diagnostic}");
    }

    // 3. 绘制。
    let title = args.input.file_name().unwrap_or(args.input.as_os_str()).to_string_lossy();
    let list = session.paint();
    let html = match args.mode {
        TextMode::Text => render_html(&list, &title),
        #[cfg(feature = "fontenv")]
        TextMode::Outlines => {
            let fonts = session.fonts().expect("outline sessions carry their fonts");
            rsword_layout_svg::render_outlined_html(&list, fonts, &title)?
        }
        #[cfg(not(feature = "fontenv"))]
        TextMode::Outlines => unreachable!("feature availability was checked before loading"),
    };
    std::fs::write(&args.output, html)?;

    let document = session.document();
    let cmds: usize = list.pages.iter().map(|p| p.cmds.len()).sum();
    println!(
        "段落 {}（表格 {}）· 页 {} · 绘制指令 {}",
        document.paras.len(),
        document.tables.len(),
        list.page_count(),
        cmds
    );
    let skipped = document.skipped_blocks;
    if skipped > 0 {
        println!("跳过非文本块 {skipped}（不支持的表格 / 绘图等，见 layout 诊断）");
    }
    if let Some(scan) = session.anchors() {
        println!(
            "环绕区 {} · 锚定未支持 {} · 不绕排 {}",
            scan.regions, scan.skipped, scan.not_wrapping
        );
    }
    let mode = match args.mode {
        TextMode::Text => "近似文本，可选择文字",
        TextMode::Outlines => "定位字形轮廓，不含可选择文字",
    };
    println!("已写出 {}（矢量 SVG，{mode}）", args.output.display());
    Ok(())
}
