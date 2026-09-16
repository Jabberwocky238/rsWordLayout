//! 探针：确认 rustybuzz 真的在做 shaping，而不只是逐字符映射。
//!
//! 判据是字形数与字符数**不相等**的那些情形——连字、组合符号、阿拉伯语形态。

use rsword_layout_core::gpu::shape::RustybuzzShaper;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let candidates = [
        ("dejavu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        ("wqy", "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
    ];
    let mut sh = RustybuzzShaper::new();
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            sh.add_face(name, bytes, 0);
            println!("已载入 {name}");
        }
    }
    if sh.is_empty() {
        println!("没有可用字体，跳过");
        return Ok(());
    }

    let samples: &[(&str, &str, &str)] = &[
        ("dejavu", "fi", "连字候选：若 GSUB 合并则字形数 < 字符数"),
        ("dejavu", "AV", "kerning：看 x_advance 是否被 GPOS 调整"),
        ("dejavu", "Hello", "普通西文"),
        ("dejavu", "e\u{0301}", "组合符号：e + 锐音符，应附着"),
        ("dejavu", "مرحبا", "阿拉伯语：RTL + 字母形态变化"),
        ("wqy", "中文排版", "CJK：等宽方块字"),
    ];

    for (face, text, note) in samples {
        let glyphs = sh.shape_with_face(face, text, 32); // 16pt
        if glyphs.is_empty() {
            println!("\n[{face}] {text:?} —— 无输出（face 未注册或字体不可解析）");
            continue;
        }
        let chars = text.chars().count();
        println!(
            "\n[{face}] {text:?}  {chars} 字符 → {} 字形   {note}",
            glyphs.len()
        );
        for g in &glyphs {
            println!(
                "    gid={:<6} adv={:>6.2}  off=({:.2}, {:.2})",
                g.key.glyph_id, g.x_advance, g.x_offset, g.y_offset
            );
        }
    }
    Ok(())
}
