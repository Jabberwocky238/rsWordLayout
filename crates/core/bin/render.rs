//! `render <input.docx> [output.html]`
//!
//! 端到端链路：rsword 解析 → 桥接成段落 → 布局引擎排版 → SVG 后端 → HTML。
//!
//! 输出的 HTML 每页一个 `<svg>`，坐标全部由布局引擎算出，浏览器只负责画字形，
//! 不参与任何排版决策——这正是要验证的东西。

use rsword::bind::native::SessionTable;
use rsword_layout_core::bridge::paras_from_document;
use rsword_layout_core::canvas::{Canvas, TextRun};
use rsword_layout_core::engine::{Engine, PageSetup};
use rsword_layout_core::fragment::Fragment;
use rsword_layout_core::simple_metrics::SimpleMetrics;
use rsword_layout_core::svg::SvgCanvas;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("用法：render <input.docx> [output.html]")?;
    let output = args.next().unwrap_or_else(|| "layout.html".to_string());

    // 1. 解析。
    let mut sessions = SessionTable::default();
    let id = sessions.open(&std::fs::read(&input)?, None)?;
    let doc: serde_json::Value = serde_json::from_str(&sessions.document(&id, None)?)?;
    sessions.close(&id);

    // 2. 桥接。
    let (paras, skipped) = paras_from_document(&doc);
    if paras.is_empty() {
        return Err("没有可排版的段落".into());
    }

    // 3. 布局。
    let metrics = SimpleMetrics;
    let setup = PageSetup::a4();
    let engine = Engine::new(&metrics, setup);
    let laid = engine.layout(&paras);

    // 4. 渲染。
    let mut canvas = SvgCanvas::new();
    for page in &laid.pages {
        canvas.begin_page(page.size)?;
        for frag in &page.fragments {
            match frag {
                Fragment::Text(t) => canvas.draw_text(&TextRun {
                    x: t.x,
                    baseline_y: t.baseline_y,
                    text: &t.text,
                    font: &t.font,
                    color: t.color,
                })?,
                Fragment::Rect { rect, color } => canvas.fill_rect(*rect, *color)?,
                Fragment::Image { id, rect } => canvas.draw_image(id, *rect)?,
            }
        }
        canvas.end_page()?;
    }
    canvas.finish()?;

    let title = std::path::Path::new(&input)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.clone());
    std::fs::write(&output, canvas.into_html(&title))?;

    let frags: usize = laid.pages.iter().map(|p| p.fragments.len()).sum();
    println!("段落 {} · 页 {} · 片段 {}", paras.len(), laid.page_count(), frags);
    if skipped > 0 {
        // 表格与绘图尚未接入：明确报出来，不假装已经排完。
        println!("跳过非文本块 {skipped}（表格 / 绘图等，本版未实现）");
    }
    println!("已写出 {output}");
    Ok(())
}
