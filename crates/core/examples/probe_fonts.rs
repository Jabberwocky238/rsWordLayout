use rsword_layout_core::font::{GlyphKey, HintingMode, RasterFormat, Rasterizer, SkrifaRasterizer};

fn main() {
    let dir = std::path::Path::new("fixtures/fonts");
    let mut names: Vec<_> = std::fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ttf") || n.ends_with(".otf") || n.ends_with(".ttc"))
        .collect();
    names.sort();
    println!("{:<30} {:>6} {:>7} {:>9} {:>11}", "文件", "字符", "gid", "位图", "格式");
    println!("{}", "-".repeat(70));
    for n in &names {
        let bytes = std::fs::read(dir.join(n)).unwrap();
        let mut r = SkrifaRasterizer::new();
        r.set_hinting(HintingMode::Smooth).set_format(RasterFormat::Subpixel);
        r.add_face("f", bytes, 0);
        // 逐个试：CJK 回退字体可能不含拉丁字形，用 'A' 判会误报失败。
        let (ch, gid) = ['A', '中', 'あ', '가']
            .iter()
            .find_map(|&c| r.glyph_id("f", c).map(|g| (c, Some(g))))
            .unwrap_or((' ', None));
        let out = gid.and_then(|g| r.rasterize(&GlyphKey::new("f", g, 32)));
        match out {
            Some(g) => println!("{:<30} {:>6} {:>7} {:>9} {:>11}",
                n, ch, gid.unwrap(),
                format!("{}x{}", g.metrics.width, g.metrics.height),
                format!("{:?}", g.format)),
            None => println!("{:<30} {:>6} {:>7} {:>9} {:>11}", n, "-", "-", "失败", "-"),
        }
    }
}
