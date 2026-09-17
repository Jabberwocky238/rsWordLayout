//! `render <input.docx> [output.html]`
//!
//! 端到端链路：rsword 解析 → 桥接成段落 → 布局引擎排版 → **矢量**绘制指令 → SVG。
//!
//! 输出每页一个 `<svg>`，文字是 `<text>` 元素而非位图——由浏览器用真实字体渲染，
//! 因此放大无限清晰、可选中、可搜索。整条链路不经过任何栅格化。

use rsword::bind::native::SessionTable;
use rsword::model::Document;
use rsword::package::Package;
use rsword_layout_core::{
    AnchorScan, Engine, PageSetup, SimpleMetrics, paint_document, paras_from_document,
};
use rsword_layout_svg::render_html;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("用法：render <input.docx> [output.html]")?;
    let output = args.next().unwrap_or_else(|| "layout.html".to_string());

    let bytes = std::fs::read(&input)?;

    // 1. 解析。段落走 JSON 投影，环绕走 Rust 模型——锚定几何不在 JSON 里。
    let mut sessions = SessionTable::default();
    let id = sessions.open(&bytes, None)?;
    let doc: serde_json::Value = serde_json::from_str(&sessions.document(&id, None)?)?;
    sessions.close(&id);

    // 2. 桥接。
    let (paras, skipped) = paras_from_document(&doc);
    if paras.is_empty() {
        return Err("没有可排版的段落".into());
    }

    // 3. 提取文字环绕区。
    let setup = PageSetup::a4();
    let content = setup.content_area();
    let mut pkg = Package::open(&bytes)?;
    let model = Document::rebuild(&mut pkg)?;
    let scan = match pkg.dom(model.main_part)? {
        Some(dom) => AnchorScan::from_document(&model, dom, content),
        None => AnchorScan::default(),
    };

    // 4. 布局。有环绕时每行可用区间按 y 变化。
    let scan_len = scan.wrap.len();
    let metrics = SimpleMetrics;
    let engine = Engine::with_wrap(&metrics, setup, scan.wrap);
    let laid = engine.layout(&paras);

    // 5. 转矢量绘制指令。不传 shaper：SVG 后端直接排文字，不需要字形序列。
    let list = paint_document(&laid, None, &[]);

    let title = std::path::Path::new(&input)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.clone());
    std::fs::write(&output, render_html(&list, &title))?;

    let cmds: usize = list.pages.iter().map(|p| p.cmds.len()).sum();
    println!("段落 {} · 页 {} · 绘制指令 {}", paras.len(), list.page_count(), cmds);
    if skipped > 0 {
        println!("跳过非文本块 {skipped}（表格 / 绘图等，本版未实现）");
    }
    println!(
        "环绕区 {} · 锚定未支持 {} · 不绕排 {}",
        scan_len, scan.skipped, scan.not_wrapping
    );
    println!("已写出 {output}（矢量 SVG，放大不失真）");
    Ok(())
}
