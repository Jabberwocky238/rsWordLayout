//! 真度量：问字体要宽度与纵向量（feature `fontenv`）。
//!
//! 用仓库自带的 `fixtures/fonts`，不碰系统字体——系统字体集会变，
//! 而这些断言要在任何机器上给同一个答案。
//!
//! 断言钉的是**来源**，不是具体数值：真度量必须问字体要。所以期望值从字体表
//! 独立算出来，再要求度量给出同一个数——把数字抄成字面量就成了自证。

use std::path::PathBuf;

use rsword_layout_core::{
    FontMetrics, FontSpec, RealMetrics, SimpleMetrics, Twips, VerticalGrid, font::FontRegistry,
};

fn font_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(name)
}

fn registry(names: &[&str]) -> FontRegistry {
    let mut r = FontRegistry::new();
    for name in names {
        let bytes = std::fs::read(font_path(name)).expect("字体读得到");
        r.add(bytes, 0).expect("字体装得进");
    }
    r
}

fn serif() -> FontRegistry {
    registry(&["LiberationSerif-Regular.ttf"])
}

fn spec() -> FontSpec {
    FontSpec::new("Liberation Serif", 24) // 12pt
}

/// 直接从字体表读一个字符的推进量，作为独立的期望值来源。
fn advance_from_font(name: &str, c: char, size_half_points: u32) -> f64 {
    use skrifa::{FontRef, MetadataProvider};
    let bytes = std::fs::read(font_path(name)).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let size = skrifa::instance::Size::unscaled();
    let loc = skrifa::instance::LocationRef::default();
    let upem = f64::from(font.metrics(size, loc).units_per_em);
    let gid = font.charmap().map(c).expect("字体有这个字形");
    let adv = font.glyph_metrics(size, loc).advance_width(gid).unwrap();
    f64::from(adv) / upem * (f64::from(size_half_points) / 2.0) * 20.0
}

#[test]
fn advances_come_from_the_font_not_a_fixed_fraction() {
    let r = serif();
    let m = RealMetrics::new(&r);
    let font = spec();

    for c in ['B', 'i', 'W', '[', ' '] {
        let expected = advance_from_font("LiberationSerif-Regular.ttf", c, 24);
        let got = m.measure(&c.to_string(), &font).advance;
        assert!(
            (f64::from(got) - expected).abs() <= 1.0,
            "{c:?}: 度量给 {got} twips，字体表给 {expected:.3}"
        );
    }

    // 关键：不同字符宽度**不同**。桩给西文一律 0.5 em，正是这条分不出来。
    assert_ne!(
        m.measure("B", &font).advance,
        m.measure("i", &font).advance,
        "真度量不该给所有西文字母同一个宽度"
    );
}

#[test]
fn vertical_metrics_come_from_the_font() {
    let r = serif();
    let m = RealMetrics::new(&r);
    let got = m.measure("A", &spec());

    assert!(got.ascent > 0 && got.descent > 0, "纵向量应当非零：{got:?}");
    // 桩用 0.8 em / 0.2 em；真字体极少恰好是这两个值。
    let em = 240; // 12pt = 240 twips
    assert_ne!(
        (got.ascent, got.descent),
        (em * 8 / 10, em * 2 / 10),
        "纵向量看着像桩的经验值，没真读字体"
    );
}

#[test]
fn empty_text_still_has_vertical_metrics() {
    // 空段落的行高由段落标记的字体决定——空串没有宽度，但必须有高度。
    let r = serif();
    let m = RealMetrics::new(&r);
    let got = m.measure("", &spec());
    assert_eq!(got.advance, 0);
    assert!(got.natural_height() > 0, "空串没有行高，空段落会塌掉");
}

#[test]
fn kerning_is_off_unless_asked_for() {
    // OOXML 的 `w:kern` 给的是「字号大到多少才启用字距调整」，不写就是**不调整**。
    // rustybuzz 不给 feature 时默认**开**，所以必须显式关掉——不能靠不传。
    //
    // 逼出这条的读数：Liberation Serif 的 `B11`，两个 `1` 之间有一对 kern。
    // rustybuzz 默认用上了，Word 没有，于是该行从第二个字形起整体偏 0.45pt。
    assert!(!FontSpec::new("Liberation Serif", 24).kerning, "默认必须是关的");

    let r = serif();
    let m = RealMetrics::new(&r);
    let mut on = spec();
    on.kerning = true;

    let off_width = m.measure("11", &spec()).advance;
    let on_width = m.measure("11", &on).advance;
    assert!(on_width <= off_width, "开字距调整后不该变宽：开={on_width} 关={off_width}");

    // 关掉时，整串宽度应当等于逐字单独量之和（没有跨边界调整）。
    let sum: Twips = "11".chars().map(|c| m.measure(&c.to_string(), &spec()).advance).sum();
    assert_eq!(off_width, sum, "关掉字距调整后，整串宽度应等于逐字之和");
}

#[test]
fn vertical_grid_lands_on_the_grid_within_twip_resolution() {
    // 栅格是 1/300 英寸 = **4.8 twips**，而 Twips 是 i32、步长 1/1440 英寸——
    // 栅格点落不到整 twips 上。所以只能要求「落到最近的 twip」，残差 ≤ 0.5 twip。
    // 这不是实现将就，是**单位本身的分辨率不够**（见 docs 的 G-8）。
    let r = serif();
    let plain = RealMetrics::new(&r);
    let gridded = RealMetrics::new(&r).with_vertical_grid(VerticalGrid::MacWordThreeHundredthsInch);
    assert_eq!(gridded.vertical_grid(), VerticalGrid::MacWordThreeHundredthsInch);

    let a = plain.measure("A", &spec());
    let b = gridded.measure("A", &spec());

    let step = 4.8_f64;
    for value in [b.natural_height(), b.descent] {
        let units = f64::from(value) / step;
        assert!(
            (units - units.round()).abs() * step <= 0.5,
            "{value} twips 离 4.8 twips 栅格超过半个 twip"
        );
    }
    assert!(
        (f64::from(b.natural_height()) - f64::from(a.natural_height())).abs() <= step,
        "量化不该把行高挪动超过一格：{} vs {}",
        b.natural_height(),
        a.natural_height()
    );
    // line_gap 折进 ascent，好让布局主干现有的 baseline = ascent 直接成立。
    assert_eq!(b.line_gap, 0);
    assert_eq!(b.ascent + b.descent, b.natural_height());
}

#[test]
fn break_opportunities_are_shared_with_the_stub() {
    // 断点与字体无关。两种度量必须给出同一套，否则换度量之后的差值里
    // 会混进断行策略的变化，分不出是哪一边错。
    let r = serif();
    let real = RealMetrics::new(&r);
    for text in ["hello world", "中文断行测试", "mixed 中英 text"] {
        assert_eq!(
            real.break_opportunities(text),
            SimpleMetrics.break_opportunities(text),
            "{text:?} 的断点两边不一致"
        );
    }
}

#[test]
fn missing_font_yields_no_metrics_rather_than_invented_ones() {
    // 一个 face 都没装时给零，让上层看到「没度量」，而不是一个编出来的高度。
    let empty = FontRegistry::new();
    let m = RealMetrics::new(&empty);
    let got = m.measure("A", &spec());
    assert_eq!(got.advance, 0);
    assert_eq!(got.natural_height(), 0, "没字体却给出了行高");
}

/// 开栅格**不改变步长**——量化只发生在落位那一次。
///
/// 这条是量出来的。cursor-unit 那一批（`docs/PREREG-2026-09-17-cursor-unit.md`
/// 的 R2）把游标步长分别取整到 1/600、1/1200、twip、半 twip、1/7200 英寸，
/// 得分随单位变粗**单调下降**，不取整的 654/752 最高，最粗的 1/600 英寸只有 26/752。
///
/// 这里原来把步长量化到 **0.24pt**，比那张表最粗的一档还粗。改掉之后，
/// 引擎在 accumulation 采集上与 Word 逐位相等的基线从 41/240 升到 113/240。
#[test]
fn the_grid_does_not_quantise_the_step() {
    let reg = serif();
    let plain = RealMetrics::new(&reg);
    let gridded = RealMetrics::new(&reg).with_vertical_grid(VerticalGrid::MacWordThreeHundredthsInch);

    for size in [16u32, 20, 24, 26, 36, 48] {
        let font = FontSpec::new("Liberation Serif", size);
        assert_eq!(
            plain.natural_height_fine("x", &font),
            gridded.natural_height_fine("x", &font),
            "{size} 半点：开栅格把步长也量化了——量化该只在落位时做一次"
        );
    }
}

/// 自证上面那条有鉴别力：这些字号的自然行高**本来就不在栅格上**。
///
/// 若它们恰好都落在栅格上，量化与否无从区分，上面那条会白白全绿。
#[test]
fn those_sizes_really_are_off_grid() {
    let reg = serif();
    let m = RealMetrics::new(&reg);
    // 0.24pt = 4.8 twips = 24 个 1/7200 英寸。
    let off = [16u32, 20, 24, 26, 36, 48]
        .iter()
        .filter(|&&size| {
            m.natural_height_fine("x", &FontSpec::new("Liberation Serif", size)) % 24 != 0
        })
        .count();
    assert!(off > 0, "这批字号的自然行高全在栅格上，区分不出步长有没有被量化");
}
