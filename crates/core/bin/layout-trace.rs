//! `layout-trace [选项] <input.docx> [output.json]`
//!
//! 出具量具比较器要的引擎轨迹（schema `rsword-layout-trace/1`）。
//! 用法与限定见 `tools/measure/README.md`；契约见 [`rsword_layout_core::oracle`]。
//!
//! ```text
//! --font <path>          装这个字体文件，可重复。不给就只有桩度量，没有字形级记录
//! --require <family>     要求该族必须装上，可重复
//! --metrics simple|real  度量来源。默认 real（读字体）；simple 是近似桩
//! --vertical-grid mac|none  纵向量化，默认 none。**含回测规则，先读 VerticalGrid 的限定**
//! ```
//!
//! # 为什么 `--require` 不是可有可无的讲究
//!
//! 量具方法 §6.2：**度量兼容克隆的字体替换，在几何上完全不可见。**
//! Liberation Serif 是 Times New Roman 的度量兼容克隆（Sans↔Arial、Carlito↔Calibri），
//! 替换后 `glyphOrigin` 与 `advanceVector` 一字不差（2618 条记录 max |Δ| = 0.000000pt）。
//! **只有字体名能发现它。** 核不过就退出——不核就跑，跑出来的是废数据，
//! 而且废得看不出来。

use std::path::PathBuf;

use rsword::bind::native::SessionTable;
use rsword_layout_core::font::FontRegistry;
use rsword_layout_core::{
    Engine, LayoutRecord, PageSetup, RealMetrics, SimpleMetrics, TextShaper, TraceMeta,
    VerticalGrid, paint_document, paras_from_document, to_trace_json,
};

#[derive(Default)]
struct Args {
    input: Option<String>,
    output: Option<String>,
    fonts: Vec<PathBuf>,
    require: Vec<String>,
    metrics: Option<String>,
    grid: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} 缺参数"));
        match arg.as_str() {
            "--font" => args.fonts.push(PathBuf::from(value("--font")?)),
            "--require" => args.require.push(value("--require")?),
            "--metrics" => args.metrics = Some(value("--metrics")?),
            "--vertical-grid" => args.grid = Some(value("--vertical-grid")?),
            other if other.starts_with("--") => return Err(format!("未知选项 {other}")),
            other if args.input.is_none() => args.input = Some(other.to_string()),
            other if args.output.is_none() => args.output = Some(other.to_string()),
            other => return Err(format!("多余的参数 {other}")),
        }
    }
    Ok(args)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let input = args.input.ok_or("用法：layout-trace [选项] <input.docx> [output.json]")?;
    let output = args.output.unwrap_or_else(|| "trace.json".to_string());

    let bytes = std::fs::read(&input)?;
    let mut sessions = SessionTable::default();
    let id = sessions.open(&bytes, None)?;
    let doc: serde_json::Value = serde_json::from_str(&sessions.document(&id, None)?)?;
    sessions.close(&id);

    let (paras, skipped) = paras_from_document(&doc);
    if paras.is_empty() {
        return Err("没有可排版的段落".into());
    }

    // 装字体。没有整形器时 `paint_document` 不产字形序列，
    // 轨迹里就只有行、没有字形——那种轨迹过不了比较器的字形层，所以要说清楚。
    let mut registry = FontRegistry::new();
    let mut families: Vec<String> = Vec::new();
    for path in &args.fonts {
        let data = std::fs::read(path)?;
        let mut index = 0u32;
        loop {
            match registry.add(data.clone(), index) {
                Ok(_) => index += 1,
                Err(error) => {
                    if index == 0 {
                        eprintln!("装不进去 {}：{error}", path.display());
                    }
                    break;
                }
            }
        }
    }
    if !registry.is_empty() {
        families = registry.face_ids();
    }

    // §6.2 的核查。放在排版之后、写出之前都行，但**必须在写出之前**。
    if !args.require.is_empty() {
        let missing: Vec<&String> = args
            .require
            .iter()
            .filter(|want| !registry.covers_family(want))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "要求的字体族没装上：{}。字体替换在几何上完全不可见（方法 §6.2），所以这里必须停。",
                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")
            )
            .into());
        }
    }

    let grid = match args.grid.as_deref() {
        Some("mac") => VerticalGrid::MacWordThreeHundredthsInch,
        Some("none") | None => VerticalGrid::None,
        Some(other) => return Err(format!("--vertical-grid 只接受 mac / none，收到 {other}").into()),
    };
    let use_stub = args.metrics.as_deref() == Some("simple") || registry.is_empty();

    // 度量的**性质**要随数走：差值的来源常常就在这一栏里。
    let metrics_note = if use_stub {
        "SimpleMetrics (近似桩：按字符类别给固定宽度，不读字体文件)".to_string()
    } else {
        format!(
            "RealMetrics (读字体 + rustybuzz 整形；纵向栅格 {})",
            match grid {
                VerticalGrid::None => "无",
                VerticalGrid::MacWordThreeHundredthsInch => "Mac Word 1/300 英寸（含回测规则）",
            }
        )
    };

    // `Engine<M: FontMetrics>` 是泛型（度量在断行热路径上，不该走动态分发），
    // 所以这里分两支实例化，而不是传 trait object。
    let pages = if use_stub {
        Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&paras)
    } else {
        let real = RealMetrics::new(&registry).with_vertical_grid(grid);
        Engine::new(&real, PageSetup::a4()).layout(&paras)
    };

    let shaper: Option<&dyn TextShaper> =
        if registry.is_empty() { None } else { Some(&registry) };
    let record = LayoutRecord::from_paint(&paint_document(&pages, shaper, &families));

    let meta = TraceMeta {
        engine: format!("rsword-layout-core {}", env!("CARGO_PKG_VERSION")),
        metrics: metrics_note,
        glyph_origin_method: if registry.is_empty() {
            "none: no shaper registered, glyph sequences are empty".into()
        } else {
            "shaped: positions come from the shaper's own output (rustybuzz)".into()
        },
        source: input.clone(),
        font_fingerprint: registry.fingerprint().map(str::to_string),
    };

    std::fs::write(&output, to_trace_json(&record, &meta))?;

    let lines: usize = record.pages.iter().map(|p| p.lines.len()).sum();
    println!(
        "段落 {} · 页 {} · 行 {lines} · 字形 {}",
        paras.len(),
        record.page_count(),
        record.glyph_count()
    );
    if registry.is_empty() {
        println!("**没装字体**：轨迹里只有行、没有字形，过不了比较器的字形层（用 --font 指定）");
    }
    if skipped > 0 {
        // 跳过的块不会出现在轨迹里；比较器会把它读成行数不符，所以这里要明说。
        println!("跳过非文本块 {skipped}（表格 / 绘图等，本版未实现）——轨迹里没有它们");
    }
    println!("已写出 {output}");
    Ok(())
}
