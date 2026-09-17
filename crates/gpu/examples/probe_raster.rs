//! 探针：确认 skrifa + zeno 真的能栅格化出字形。
//!
//! 编译通过不等于画得对，所以这里把覆盖率位图用 ASCII 打出来肉眼可验。

use rsword_layout_gpu::atlas::GlyphKey;
use rsword_layout_gpu::raster::SkrifaRasterizer;
use rsword_layout_gpu::{GlyphAtlas, Rasterizer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let candidates = [
        ("dejavu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        ("wqy", "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
    ];

    let mut r = SkrifaRasterizer::new();
    let mut loaded = Vec::new();
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            r.add_face(name, bytes, 0);
            loaded.push(name);
            println!("已载入 {name}: {path}");
        }
    }
    if loaded.is_empty() {
        println!("没有可用字体，跳过");
        return Ok(());
    }

    for (face, ch) in [("dejavu", 'A'), ("dejavu", 'g'), ("wqy", '中')] {
        if !r.has_face(face) {
            continue;
        }
        let Some(gid) = r.glyph_id(face, ch) else {
            println!("\n{face} 没有 {ch:?} 的字形");
            continue;
        };
        let key = GlyphKey::new(face, gid, 32); // 16pt
        let Some(g) = r.rasterize(&key) else {
            println!("\n{face} {ch:?} 栅格化失败");
            continue;
        };

        println!(
            "\n=== {face} {ch:?} gid={gid} ===\n{}×{} left={} top={} advance={:.1}",
            g.metrics.width, g.metrics.height, g.metrics.left, g.metrics.top, g.metrics.advance
        );

        // 覆盖率转 ASCII，肉眼确认形状。
        let ramp = [' ', '.', ':', '*', '#', '@'];
        for y in 0..g.metrics.height {
            let mut line = String::new();
            for x in 0..g.metrics.width {
                let v = g.coverage[(y * g.metrics.width + x) as usize];
                line.push(ramp[(v as usize * (ramp.len() - 1)) / 255]);
            }
            println!("{line}");
        }
        assert_eq!(
            g.coverage.len(),
            (g.metrics.width * g.metrics.height) as usize,
            "覆盖率字节数必须等于 width*height"
        );
    }

    // 接进图集，确认整条链路通。
    let mut atlas = GlyphAtlas::new(512, 512);
    let face = loaded[0];
    let mut placed = 0;
    for ch in "Hello 世界".chars() {
        if let Some(gid) = r.glyph_id(face, ch)
            && atlas.get(&GlyphKey::new(face, gid, 32), &mut r).is_some()
        {
            placed += 1;
        }
    }
    println!("\n图集：放入 {placed} 个字形，缓存 {} 条", atlas.len());
    Ok(())
}
