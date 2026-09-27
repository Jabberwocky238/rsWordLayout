//! `render [--text-mode text|outlines] [--font path[#index]] <input.docx> [output.html]`
//!
//! 端到端链路：rsword 解析 → 桥接成段落 → 布局引擎排版 → **矢量**绘制指令 → SVG。
//!
//! Text mode keeps the original selectable browser text and approximate metrics.
//! Outline mode uses explicit fonts for measurement, shaping and vector contours.

use std::path::PathBuf;

use rsword::model::Document;
use rsword::package::Package;
use rsword_layout_core::{
    AnchorScan, Engine, SimpleMetrics, load_document, paint_document,
};
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

#[cfg(feature = "fontenv")]
fn render_outlines(
    args: &Args,
    document: &rsword_layout_core::LayoutDocument,
    wrap: rsword_layout_core::WrapContext,
    title: &str,
) -> Result<(rsword_layout_core::PaintList, String), Box<dyn std::error::Error>> {
    use rsword_layout_core::font::{FontRegistry, RealMetrics};
    let mut fonts = FontRegistry::new();
    for (sources, fallback) in [(&args.fonts, false), (&args.fallback_fonts, true)] {
        for source in sources {
            let bytes = std::fs::read(&source.path)
                .map_err(|error| format!("{}: {error}", source.path.display()))?;
            let result = if fallback { fonts.add_fallback(bytes, source.index) }
                else { fonts.add(bytes, source.index) };
            result.map_err(|error| format!("{}#{}: {error}", source.path.display(), source.index))?;
        }
    }
    let metrics = RealMetrics::new(&fonts);
    let engine = Engine::with_wrap(&metrics, document.sections[0].setup, wrap);
    let pages = engine.layout_document(document);
    let list = paint_document(&pages, Some(&fonts), &fonts.face_ids());
    let html = rsword_layout_svg::render_outlined_html(&list, &fonts, title)?;
    Ok((list, html))
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

    // 1. 解析。段落走 JSON 投影，环绕走 Rust 模型——锚定几何不在 JSON 里。
    // 并排的 `w:rPr` 解析器只留最后一个，`load_document` 先把它们并起来。
    let loaded = load_document(&bytes)?;
    if let Some(error) = &loaded.merge_error {
        eprintln!("并排 w:rPr 的合并失败，按解析器原样的 JSON 排：{error}");
    }
    // 2. 桥接。
    let document = loaded.layout_document();
    for diagnostic in &document.diagnostics {
        eprintln!("layout: {diagnostic}");
    }
    if document.paras.is_empty() {
        return Err("没有可排版的段落".into());
    }

    // 3. 提取文字环绕区。
    let setup = document.sections[0].setup;
    let content = setup.content_area();
    let mut pkg = Package::open(&bytes)?;
    let model = Document::rebuild(&mut pkg)?;
    let scan = match pkg.dom(model.main_part)? {
        Some(dom) => AnchorScan::from_document(&model, dom, content),
        None => AnchorScan::default(),
    };

    let title = args.input.file_name().unwrap_or(args.input.as_os_str()).to_string_lossy();
    // Measurement, shaping and contours share the explicit font registry.
    let scan_len = scan.wrap.len();
    let (list, html) = match args.mode {
        TextMode::Text => {
            let metrics = SimpleMetrics;
            let engine = Engine::with_wrap(&metrics, setup, scan.wrap);
            let pages = engine.layout_document(&document);
            let list = paint_document(&pages, None, &[]);
            let html = render_html(&list, &title);
            (list, html)
        }
        #[cfg(feature = "fontenv")]
        TextMode::Outlines => render_outlines(&args, &document, scan.wrap, &title)?,
        #[cfg(not(feature = "fontenv"))]
        TextMode::Outlines => unreachable!("feature availability was checked before loading"),
    };
    std::fs::write(&args.output, html)?;

    let cmds: usize = list.pages.iter().map(|p| p.cmds.len()).sum();
    println!("段落 {} · 页 {} · 绘制指令 {}", document.paras.len(), list.page_count(), cmds);
    let skipped = document.skipped_blocks;
    if skipped > 0 {
        println!("跳过非文本块 {skipped}（表格 / 绘图等，本版未实现）");
    }
    println!(
        "环绕区 {} · 锚定未支持 {} · 不绕排 {}",
        scan_len, scan.skipped, scan.not_wrapping
    );
    let mode = match args.mode {
        TextMode::Text => "近似文本，可选择文字",
        TextMode::Outlines => "定位字形轮廓，不含可选择文字",
    };
    println!("已写出 {}（矢量 SVG，{mode}）", args.output.display());
    Ok(())
}
