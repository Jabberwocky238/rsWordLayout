//! 探针：确认 rustybuzz 真的在做 shaping，而不只是逐字符映射。
//!
//! 判据是字形数与字符数**不相等**的那些情形——连字、组合符号、阿拉伯语形态。

use rsword_layout_core::RustybuzzShaper;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let candidates = [
        // 仓库自带的两份（任何平台都在），fixtures/fonts/README.md 说明来源。
        ("dejavu", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/fonts/DejaVuSans.ttf")),
        ("droid", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/fonts/DroidSansFallbackFull.ttf")),
    ];
    let mut sh = RustybuzzShaper::new();
    // `shape_with_face` 收的是 face **下标**，不是名字，所以注册时把下标记下来。
    let mut faces: Vec<(&str, usize)> = Vec::new();
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            faces.push((name, sh.add_face(name, bytes, 0)));
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
        ("droid", "中文排版", "CJK：等宽方块字"),
    ];

    for (face, text, note) in samples {
        let Some(&(_, index)) = faces.iter().find(|(name, _)| name == face) else {
            println!("\n[{face}] {text:?} —— 该字体没装上，跳过");
            continue;
        };
        let glyphs = sh.shape_with_face(index, text, 32, false); // 16pt，不做字距调整（Word 默认如此）
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
                g.glyph_id, g.x_advance, g.x_offset, g.y_offset
            );
        }
    }
    Ok(())
}
