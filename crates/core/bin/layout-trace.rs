//! `layout-trace [选项] <input.docx> [output.json]`
//!
//! 出具验收量具要的引擎轨迹：每页、每行、每字形的原点与推进、所属源字符区间、行终止符类型。
//! 契约见 [`rsword_layout_core::trace`]，量具方法见 `tools/measure/README.md` §9.7。
//!
//! 与 `render` 用的是同一条布局主干（`Engine::layout_traced` 内部就是 `layout` 的主循环），
//! 所以量到的是这个引擎本身，不是为量具另算的一份。
//!
//! 选项：
//!
//! ```text
//! --metrics simple|real   度量来源。默认 real（读字体文件 + 整形）；simple 是近似桩
//! --font <path>           只装这些字体文件，可重复。不给就扫系统字体目录
//! --require <family>      要求该族必须装上，可重复。装不上就退出——
//!                         字体替换在几何上完全不可见，只有字体名能发现（方法 §6.2）
//! --vertical-grid mac|none  纵向量化。默认 none
//! ```

use std::path::PathBuf;

use rsword::bind::native::SessionTable;
use rsword_layout_core::bridge::paras_from_document;
use rsword_layout_core::engine::{Engine, PageSetup};
use rsword_layout_core::font_metrics::{FontEnvMetrics, VerticalGrid};
use rsword_layout_core::fontload;
use rsword_layout_core::measure::FontMetrics;
use rsword_layout_core::simple_metrics::SimpleMetrics;
use rsword_layout_core::trace::{ORIGIN_PREFIX_ADVANCE, ORIGIN_SHAPED, TraceMeta};

fn sha256_hex(bytes: &[u8]) -> String {
    // 只为溯源记个指纹，不值得引依赖：这里用 FNV-1a 的 64 位变体并标明它不是 SHA-256。
    // 量具侧的 `fingerprint.docx_identity` 才是权威的 sha256。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

#[derive(Default)]
struct Args {
    input: Option<String>,
    output: Option<String>,
    metrics: Option<String>,
    fonts: Vec<PathBuf>,
    require: Vec<String>,
    grid: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} 缺参数"));
        match arg.as_str() {
            "--metrics" => args.metrics = Some(value("--metrics")?),
            "--font" => args.fonts.push(PathBuf::from(value("--font")?)),
            "--require" => args.require.push(value("--require")?),
            "--vertical-grid" => args.grid = Some(value("--vertical-grid")?),
            other if other.starts_with("--") => return Err(format!("未知选项 {other}")),
            other if args.input.is_none() => args.input = Some(other.to_string()),
            other if args.output.is_none() => args.output = Some(other.to_string()),
            other => return Err(format!("多余的参数 {other}")),
        }
    }
    Ok(args)
}

fn run<M: FontMetrics>(
    metrics: &M,
    paras: &[rsword_layout_core::engine::Para],
    meta: TraceMeta,
    output: &str,
) -> Result<(usize, usize, usize), Box<dyn std::error::Error>> {
    let engine = Engine::new(metrics, PageSetup::a4());
    let (laid, trace) = engine.layout_traced(paras);
    std::fs::write(output, trace.to_json(&meta))?;
    Ok((
        laid.page_count(),
        trace.pages.iter().map(|p| p.lines.len()).sum(),
        trace.glyph_count(),
    ))
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

    let base = TraceMeta {
        engine: format!("rsword-layout-core {}", env!("CARGO_PKG_VERSION")),
        source: input.clone(),
        source_sha256: sha256_hex(&bytes),
        ..TraceMeta::default()
    };

    let (pages, lines, glyphs) = if args.metrics.as_deref() == Some("simple") {
        let meta = TraceMeta {
            // 桩的性质要随数走：差值主要来自这里，不是来自断行。
            metrics: "SimpleMetrics (近似桩：按字符类别给固定宽度，不读字体文件、不做 shaping)".into(),
            glyph_origin_method: ORIGIN_PREFIX_ADVANCE.into(),
            ..base
        };
        run(&SimpleMetrics, &paras, meta, &output)?
    } else {
        let (env, report) = if args.fonts.is_empty() {
            fontload::load_from_dirs(fontload::MAC_FONT_ROOTS)
        } else {
            fontload::load_files(&args.fonts)
        };
        eprintln!(
            "字体：装入 {} 个 face / {} 个族，失败 {} 个文件；指纹 {}",
            report.faces,
            report.families.len(),
            report.failed.len(),
            &report.fingerprint[..report.fingerprint.len().min(16)]
        );
        if report.faces == 0 {
            return Err("一个字体都没装上；真度量跑不了（可用 --metrics simple 退回桩）".into());
        }

        // 方法 §6.2：度量兼容克隆的替换在几何上完全不可见，**只有字体名能发现**。
        // 所以申请的族必须当场核，核不过就停——不核就采，采到的是废数据，而且废得看不出来。
        let required: Vec<&str> = args.require.iter().map(String::as_str).collect();
        let missing = report.missing(&required);
        if !missing.is_empty() {
            return Err(format!(
                "要求的字体族没装上：{}。\
                 字体替换在几何上完全不可见（方法 §6.2），所以这里必须停。",
                missing.join("、")
            )
            .into());
        }

        let grid = match args.grid.as_deref() {
            Some("mac") => VerticalGrid::MacWordThreeHundredthsInch,
            Some("none") | None => VerticalGrid::None,
            Some(other) => return Err(format!("--vertical-grid 只接受 mac / none，收到 {other}").into()),
        };
        let metrics = FontEnvMetrics::new(&env).with_vertical_grid(grid);

        let meta = TraceMeta {
            metrics: format!(
                "FontEnvMetrics (rustybuzz 整形 + 字体声明的纵向量；{} face，指纹 {}；纵向栅格 {})",
                report.faces,
                &report.fingerprint[..report.fingerprint.len().min(16)],
                match grid {
                    VerticalGrid::None => "无",
                    VerticalGrid::MacWordThreeHundredthsInch => "Mac Word 1/300 英寸（含回测规则）",
                }
            ),
            glyph_origin_method: ORIGIN_SHAPED.into(),
            ..base
        };
        let counts = run(&metrics, &paras, meta, &output)?;

        // 回退过的度量不该被当成原生度量看：有诊断就报出来。
        let stats = metrics.selection_stats();
        if !stats.is_clean() {
            eprintln!(
                "字体选择诊断：替代 {} 次 / 回退 {} 次 / 无覆盖 {} 次——\
                 这些字符的度量不是首选族给的",
                stats.substituted, stats.fallback, stats.missing
            );
        }
        counts
    };

    println!("段落 {} · 页 {pages} · 行 {lines} · 字形 {glyphs}", paras.len());
    if skipped > 0 {
        // 跳过的块不会出现在轨迹里；比较器会把它读成行数不符，所以这里要明说。
        println!("跳过非文本块 {skipped}（表格 / 绘图等，本版未实现）——轨迹里没有它们");
    }
    println!("已写出 {output}");
    Ok(())
}
