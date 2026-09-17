//! 推进量只在落位时取一次整，不逐字形各取各的。
//!
//! 逐字形取整会**沿行累加**：实测 Word 给 `e` 5.3280pt、引擎给 5.3500pt（差 1 twip），
//! 到第 20 个字形就攒到 3 twips（0.15pt）。行首对得上、越往右越偏，
//! 而这种偏差看起来很像「度量不准」——其实度量是对的，是取整时机错了。
//!
//! 手法：未取整的累计推进量留在 f64 里，**推进量取相邻取整位置之差**。
//! 于是任意前缀和都恰好等于「精确累计值取整」，位置不漂；
//! 而单个推进量仍是整 twips，`ShapedRun` 的契约不变。
//!
//! 修掉后纯文本页 Δx max 从 0.1504pt 降到 **0.0244pt**——半个 twip，
//! 整数 twips 下的硬地板。

use std::path::PathBuf;

use rsword_layout_core::{FontMetrics, FontSpec, RealMetrics, Twips, font::FontRegistry};

fn serif() -> FontRegistry {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts/LiberationSerif-Regular.ttf");
    let mut r = FontRegistry::new();
    r.add(std::fs::read(path).expect("字体读得到"), 0).expect("字体装得进");
    r
}

fn spec() -> FontSpec {
    FontSpec::new("Liberation Serif", 24) // 12pt
}

/// 逐字形推进量的前缀和。
fn prefix_sums(r: &FontRegistry, text: &str) -> Vec<Twips> {
    let mut out = Vec::new();
    let mut acc = 0;
    for g in r.shape_text(text, &spec()) {
        acc += g.x_advance;
        out.push(acc);
    }
    out
}

/// 从字体表直接算前 k 个字符的**精确**累计推进量，twips，不取整。
///
/// **期望值必须独立于被测实现。** 第一版这条测试拿 `measure(前缀)` 当期望，
/// 结果两边共用同一条求和路径——逐字形取整的旧代码上照样全绿，等于自证
/// （量具方法 §7.2：设计案例不能兼作检验）。
fn exact_prefix(text: &str, k: usize, size_half_points: u32) -> f64 {
    use skrifa::{FontRef, MetadataProvider};
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts/LiberationSerif-Regular.ttf");
    let bytes = std::fs::read(path).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let size = skrifa::instance::Size::unscaled();
    let loc = skrifa::instance::LocationRef::default();
    let upem = f64::from(font.metrics(size, loc).units_per_em);
    let metrics = font.glyph_metrics(size, loc);
    let charmap = font.charmap();

    text.chars()
        .take(k)
        .map(|c| {
            let gid = charmap.map(c).expect("字体有这个字形");
            f64::from(metrics.advance_width(gid).unwrap())
        })
        .sum::<f64>()
        / upem
        * (f64::from(size_half_points) / 2.0)
        * 20.0
}

#[test]
fn accumulated_position_tracks_the_exact_value() {
    // 核心性质：走到第 k 个字形的位置，与**字体表算出的精确值**差不超过半个 twip，
    // 且这个差**不随长度增长**。
    //
    // 逐字形各取整时它会线性累加：实测到第 20 个字形攒到 3 twips。
    let r = serif();
    let text = "B01 break before mark";
    let sums = prefix_sums(&r, text);

    let mut worst = 0.0f64;
    for (k, &sum) in sums.iter().enumerate() {
        let exact = exact_prefix(text, k + 1, 24);
        worst = worst.max((f64::from(sum) - exact).abs());
    }
    assert!(
        worst <= 0.5,
        "累计位置与字体表的精确值差 {worst:.4} twips，超过半个 twip——中途取了整"
    );
}

#[test]
fn error_does_not_grow_with_length() {
    // 长串上更明显：逐字形取整的误差随字形数线性增长，正确实现则恒定在半个 twip 内。
    let r = serif();
    let text = "the quick brown fox jumps over the lazy dog".repeat(3);
    let sums = prefix_sums(&r, &text);

    let tail = (f64::from(*sums.last().unwrap()) - exact_prefix(&text, sums.len(), 24)).abs();
    assert!(tail <= 0.5, "串尾累计误差 {tail:.4} twips——误差随长度增长了");
}

#[test]
fn individual_advances_are_still_whole_twips() {
    // 契约没变：单个推进量仍是整 twips。改的只是**怎么分配舍入**。
    let r = serif();
    for g in r.shape_text("Wave", &spec()) {
        // Twips 本身是 i32，这里断言的是它没被改成别的表示。
        let _: Twips = g.x_advance;
    }
    // 非空文本应当产出字形。
    assert!(!r.shape_text("Wave", &spec()).is_empty());
}

#[test]
fn empty_and_single_glyph_are_unaffected() {
    let r = serif();
    assert!(r.shape_text("", &spec()).is_empty());
    let one = r.shape_text("A", &spec());
    assert_eq!(one.len(), 1);
    let m = RealMetrics::new(&r);
    assert_eq!(one[0].x_advance, m.measure("A", &spec()).advance);
}
