//! 探针：把比较器记录打出来，核对推进量、源区间与终止符是否真的填上了。

use rsword::bind::native::SessionTable;
use rsword_layout_core::{
    Engine, LayoutRecord, LineTerminator, PageSetup, SimpleMetrics, paint_document,
    paras_from_document,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("用法：probe_oracle <input.docx>")?;
    let mut sessions = SessionTable::default();
    let id = sessions.open(&std::fs::read(&path)?, None)?;
    let doc: serde_json::Value = serde_json::from_str(&sessions.document(&id, None)?)?;
    sessions.close(&id);

    let (paras, _) = paras_from_document(&doc);
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let pages = engine.layout(&paras);
    // 不传 shaper：SVG 类后端直接排文字，字形序列为空，
    // 故字形级字段只在接了 shaper 时才有值。
    let rec = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));

    println!("页 {} · 行 {} · 字形 {}", rec.page_count(),
        rec.pages.iter().map(|p| p.line_count()).sum::<usize>(), rec.glyph_count());

    let mut term = std::collections::BTreeMap::new();
    for p in &rec.pages {
        for l in &p.lines {
            *term.entry(format!("{:?}", l.terminator)).or_insert(0usize) += 1;
        }
    }
    println!("终止符分布: {term:?}");

    // 按计数约定，各行应当额外产出多少字形。
    let expected: usize = rec
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .map(|l| l.terminator.expected_glyphs())
        .sum();
    println!("终止符应产出字形合计: {expected}");

    println!("\n首页前 6 行：");
    if let Some(first) = rec.pages.first() {
        for (i, l) in first.lines.iter().enumerate().take(6) {
            println!(
                "  行{i} 字形{:<3} 源{:?} 终止符{:?}",
                l.glyph_count(),
                l.source.map(|s| (s.start, s.end)),
                l.terminator
            );
        }
    }
    // 自查：段落标记与分节符都应出现，否则说明穿透有漏。
    let has_mark = term.contains_key(&format!("{:?}", LineTerminator::ParagraphMark));
    println!("\n含段落标记: {has_mark}");
    Ok(())
}
