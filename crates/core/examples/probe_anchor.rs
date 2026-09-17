//! 探针：确认能从 rsword 的 Rust 模型真的提取出环绕区。
//!
//! 判据不是「能跑」，而是：带环绕的语料必须产出非空的 WrapContext，
//! 且矩形坐标换算正确（EMU ÷ 635 = twips）。

use rsword::model::Document;
use rsword::package::Package;
use rsword_layout_core::AnchorScan;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("用法：probe_anchor <input.docx>")?;
    let bytes = std::fs::read(&path)?;
    let mut pkg = Package::open(&bytes)?;
    let doc = Document::rebuild(&mut pkg)?;

    let main = doc.main_part;
    let dom = pkg.dom(main)?.ok_or("主文档没有 DOM")?;

    // A4 正文区：页面 11906×16838，四周 1440 页边距。
    let content = rsword_layout_core::Rect::new(1440, 1440, 11906 - 2880, 16838 - 2880);
    let scan = AnchorScan::from_document(&doc, dom, content);

    println!("文件: {path}");
    println!("块数: {}", doc.main.len());
    println!("环绕区: {}", scan.wrap.len());
    println!("跳过（锚定方式未支持）: {}", scan.skipped);
    println!("不绕排（wrapNone / 随文）: {}", scan.not_wrapping);

    // 验证几何：区间求解是断行真正消费的东西。
    let full = rsword_layout_core::Span::new(content.x, content.x + content.width);
    for y in [1440, 3000, 6000] {
        let spans = scan.wrap.available(full, y, 240);
        let desc: Vec<String> = spans
            .iter()
            .map(|s| format!("[{}..{}] 宽{}", s.start, s.end, s.width()))
            .collect();
        println!(
            "  y={y} 可用区间: {}",
            if desc.is_empty() { "（整行被占）".to_string() } else { desc.join(" ") }
        );
    }
    Ok(())
}
