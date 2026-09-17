//! `layout-trace <input.docx> [output.json]`
//!
//! 出具验收量具要的引擎轨迹：每页、每行、每字形的原点与推进、所属源字符区间、行终止符类型。
//! 契约见 [`rsword_layout_core::trace`]，量具方法见 `tools/measure/README.md` §9.7。
//!
//! 与 `render` 用的是同一条布局主干（`Engine::layout_traced` 内部就是 `layout` 的主循环），
//! 所以量到的是这个引擎本身，不是为量具另算的一份。

use rsword::bind::native::SessionTable;
use rsword_layout_core::bridge::paras_from_document;
use rsword_layout_core::engine::{Engine, PageSetup};
use rsword_layout_core::simple_metrics::SimpleMetrics;
use rsword_layout_core::trace::TraceMeta;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("用法：layout-trace <input.docx> [output.json]")?;
    let output = args.next().unwrap_or_else(|| "trace.json".to_string());

    let bytes = std::fs::read(&input)?;
    let mut sessions = SessionTable::default();
    let id = sessions.open(&bytes, None)?;
    let doc: serde_json::Value = serde_json::from_str(&sessions.document(&id, None)?)?;
    sessions.close(&id);

    let (paras, skipped) = paras_from_document(&doc);
    if paras.is_empty() {
        return Err("没有可排版的段落".into());
    }

    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let (laid, trace) = engine.layout_traced(&paras);

    let meta = TraceMeta {
        engine: format!("rsword-layout-core {}", env!("CARGO_PKG_VERSION")),
        // 度量实现要记进轨迹：SimpleMetrics 是**近似度量桩**，按字符类别给固定宽度，
        // 不读字体文件、不做 shaping。拿它去和 Word 比，差值主要来自这里，不是来自断行。
        metrics: "SimpleMetrics (近似桩：按字符类别给固定宽度，不读字体文件、不做 shaping)".into(),
        source: input.clone(),
        source_sha256: sha256_hex(&bytes),
    };

    std::fs::write(&output, trace.to_json(&meta))?;

    println!(
        "段落 {} · 页 {} · 行 {} · 字形 {}",
        paras.len(),
        laid.page_count(),
        trace.pages.iter().map(|p| p.lines.len()).sum::<usize>(),
        trace.glyph_count()
    );
    if skipped > 0 {
        // 跳过的块不会出现在轨迹里；比较器会把它读成行数不符，所以这里要明说。
        println!("跳过非文本块 {skipped}（表格 / 绘图等，本版未实现）——轨迹里没有它们");
    }
    println!("已写出 {output}");
    Ok(())
}
