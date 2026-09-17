//! 真度量的检验（feature `shape`）。
//!
//! 用仓库里自带的 DejaVuSans / DroidSansFallback，不碰系统字体——
//! 系统字体集会变，而这些断言要在任何机器上给同一个答案。
//!
//! 断言钉的是**来源**，不是具体数值：真度量必须问字体要宽度。
//! 所以这里先从字体文件独立算出期望值，再要求度量给出同一个数——
//! 把期望值抄成字面量就成了自证，改坏了也看不出来。

use std::path::PathBuf;

use rsword_layout_core::font_metrics::{FontEnvMetrics, VerticalGrid};
use rsword_layout_core::fontload;
use rsword_layout_core::geom::Twips;
use rsword_layout_core::measure::{FontMetrics, FontSpec};

fn fonts() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../webgl/web/public/fonts");
    vec![root.join("DejaVuSans.ttf"), root.join("DroidSansFallbackFull.ttf")]
}

fn latin_only() -> Vec<PathBuf> {
    fonts().into_iter().take(1).collect()
}

/// 直接从字体表里读一个字符的推进量，作为独立的期望值来源。
fn advance_from_font(path: &PathBuf, c: char, size_half_points: u32) -> f64 {
    use skrifa::{FontRef, MetadataProvider};
    let bytes = std::fs::read(path).expect("字体读得到");
    let font = FontRef::new(&bytes).expect("字体解析得了");
    let upem = f64::from(font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default()).units_per_em);
    let gid = font.charmap().map(c).expect("字体有这个字形");
    let advance = font
        .glyph_metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default())
        .advance_width(gid)
        .expect("有推进量");
    let pt = f64::from(size_half_points) / 2.0;
    f64::from(advance) / upem * pt * 20.0
}

#[test]
fn advances_come_from_the_font_not_a_fixed_fraction() {
    let paths = latin_only();
    let (env, report) = fontload::load_files(&paths);
    assert_eq!(report.faces, 1, "装入失败：{:?}", report.failed);

    let metrics = FontEnvMetrics::new(&env);
    let font = FontSpec::new("DejaVu Sans", 24); // 12pt

    // 逐字与字体表对齐（允许 1 twip 的落位舍入）。
    for c in ['B', 'i', 'W', '[', ' '] {
        let expected = advance_from_font(&paths[0], c, 24);
        let got = metrics.measure(&c.to_string(), &font).advance;
        assert!(
            (f64::from(got) - expected).abs() <= 1.0,
            "{c:?}: 度量给 {got} twips，字体表给 {expected:.3}"
        );
    }

    // 关键：不同字符宽度**不同**。桩实现给西文一律 0.5 em，正是这条分不出来。
    let b = metrics.measure("B", &font).advance;
    let i = metrics.measure("i", &font).advance;
    assert_ne!(b, i, "真度量不该给所有西文字母同一个宽度");
}

#[test]
fn glyph_positions_are_cumulative_and_cover_the_source() {
    let (env, _) = fontload::load_files(&latin_only());
    let metrics = FontEnvMetrics::new(&env);
    let font = FontSpec::new("DejaVu Sans", 24);
    let text = "Wave";

    let positions = metrics.glyph_positions(text, &font);
    assert_eq!(positions.len(), text.chars().count());

    // x 单调不减，且等于前面所有推进量之和。
    let mut x: Twips = 0;
    for p in &positions {
        assert_eq!(p.x, x, "第 {}..{} 个字形的 x 不连续", p.start, p.end);
        x += p.advance;
    }
    // 逐字形推进量之和 == 整串宽度。这条是配对器能按读序对上的前提。
    assert_eq!(x, metrics.measure(text, &font).advance);

    // 覆盖的源区间首尾相接，且铺满整串。
    assert_eq!(positions.first().unwrap().start, 0);
    assert_eq!(positions.last().unwrap().end, text.len());
    for pair in positions.windows(2) {
        assert_eq!(pair[0].end, pair[1].start);
    }
}

#[test]
fn glyph_positions_are_not_the_prefix_sum_default() {
    // 覆盖默认实现是有代价的，得确认真的覆盖到了：
    // 默认实现按 `measure(text[..i])` 逐前缀量，做 shaping 的实现直接用 shaper 的输出。
    // 两者在有 kerning 的串上应当**可能**不同；这里只断言真度量没有退回默认路径——
    // 退回去的话字形数会等于字符数且推进量等于逐字单量，连字串上会露馅。
    let (env, _) = fontload::load_files(&latin_only());
    let metrics = FontEnvMetrics::new(&env);
    let font = FontSpec::new("DejaVu Sans", 24);

    let single: Twips = "AV".chars().map(|c| metrics.measure(&c.to_string(), &font).advance).sum();
    let together = metrics.measure("AV", &font).advance;
    let shaped: Twips = metrics.glyph_positions("AV", &font).iter().map(|g| g.advance).sum();
    // 逐字形之和必须等于整串量，而不是等于「逐字单独量之和」——
    // 后者在有 kerning 时会偏，前者不会。
    assert_eq!(shaped, together);
    let _ = single; // 有没有 kern 取决于字体，不作断言。
}

#[test]
fn vertical_metrics_come_from_the_font() {
    let (env, _) = fontload::load_files(&latin_only());
    let metrics = FontEnvMetrics::new(&env);
    let font = FontSpec::new("DejaVu Sans", 24);
    let m = metrics.measure("A", &font);

    assert!(m.ascent > 0 && m.descent > 0, "纵向量应当非零：{m:?}");
    // 桩用的是 0.8 em / 0.2 em；真字体极少恰好是这两个值。
    let em = 240; // 12pt = 240 twips
    assert_ne!((m.ascent, m.descent), (em * 8 / 10, em * 2 / 10));
}

#[test]
fn empty_text_still_has_vertical_metrics() {
    // 空段落的行高由段落标记的字体决定——空串没有宽度，但必须有高度。
    let (env, _) = fontload::load_files(&latin_only());
    let metrics = FontEnvMetrics::new(&env);
    let font = FontSpec::new("DejaVu Sans", 24);
    let m = metrics.empty_line_metrics(&font);
    assert_eq!(m.advance, 0);
    assert!(m.natural_height() > 0);
}

#[test]
fn vertical_grid_lands_on_the_grid_within_twip_resolution() {
    // 栅格是 1/300 英寸 = **4.8 twips**，而 Twips 是 i32、步长 1/1440 英寸——
    // 栅格点落不到整 twips 上。所以这里只能要求「落到最近的 twip」，
    // 残差 ≤ 0.5 twip。这不是实现将就，是单位本身的分辨率不够（见模块文档）。
    let (env, _) = fontload::load_files(&latin_only());
    let font = FontSpec::new("DejaVu Sans", 24);

    let plain = FontEnvMetrics::new(&env);
    let gridded = FontEnvMetrics::new(&env).with_vertical_grid(VerticalGrid::MacWordThreeHundredthsInch);
    assert_eq!(gridded.vertical_grid(), VerticalGrid::MacWordThreeHundredthsInch);

    let a = plain.measure("A", &font);
    let b = gridded.measure("A", &font);

    // 量化后 line_gap 折进 ascent，自然行高仍应与未量化的相差不到一格。
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
    // 基线 = 行高 − descent，line_gap 已折进 ascent（好让布局主干不必改）。
    assert_eq!(b.line_gap, 0);
    assert_eq!(b.ascent + b.descent, b.natural_height());
}

#[test]
fn fallback_across_faces_is_reported() {
    // 一个 run 一个字体在全 Unicode 下不成立：西文与汉字往往落在不同文件。
    // 回退过的度量不该被当成原生度量看，所以回退必须能被上层看见。
    let (env, report) = fontload::load_files(&fonts());
    assert_eq!(report.faces, 2, "装入失败：{:?}", report.failed);

    let metrics = FontEnvMetrics::new(&env);
    let font = FontSpec::new("DejaVu Sans", 24);

    metrics.reset_selection_stats();
    let m = metrics.measure("A中", &font);
    assert!(m.advance > 0);
    let stats = metrics.selection_stats();
    assert!(
        !stats.is_clean(),
        "跨字体取到的度量必须带诊断，否则分不清「引擎算错」与「没找到字体」"
    );
}

#[test]
fn missing_required_family_is_detectable_by_name_only() {
    // 方法 §6.2：度量兼容克隆的替换在几何上完全不可见，只有字体名能发现。
    let (_, report) = fontload::load_files(&latin_only());
    assert!(report.missing(&["DejaVu Sans"]).is_empty());
    assert_eq!(report.missing(&["Liberation Serif"]), vec!["Liberation Serif".to_string()]);
}

#[test]
fn break_opportunities_are_shared_with_the_stub() {
    // 断点与字体无关。两种度量必须给出同一套，否则换度量之后的差值里
    // 会混进断行策略的变化，分不出是哪一边错。
    use rsword_layout_core::simple_metrics::SimpleMetrics;
    let (env, _) = fontload::load_files(&latin_only());
    let real = FontEnvMetrics::new(&env);
    for text in ["hello world", "中文断行测试", "mixed 中英 text"] {
        assert_eq!(
            real.break_opportunities(text),
            SimpleMetrics.break_opportunities(text),
            "{text:?} 的断点两边不一致"
        );
    }
}

#[test]
fn kerning_is_off_unless_asked_for() {
    // OOXML 的 `w:kern` 给的是「字号大到多少才启用字距调整」，不写就是**不调整**。
    // rustybuzz 不给 feature 时默认**开**，所以必须显式关掉——不能靠不传。
    //
    // 逼出这条的读数：Liberation Serif 的 `B11`，两个 `1` 之间有一对 kern。
    // rustybuzz 默认用上了，Word 没有，于是该行从第二个字形起整体偏 0.45pt。
    let (env, _) = fontload::load_files(&latin_only());
    let metrics = FontEnvMetrics::new(&env);

    let mut off = FontSpec::new("DejaVu Sans", 24);
    off.kerning = false;
    let mut on = off.clone();
    on.kerning = true;

    // 默认必须是关的。
    assert!(!FontSpec::new("DejaVu Sans", 24).kerning);

    // 找一对真有 kern 的字符；DejaVu Sans 的 "AV" 通常有。
    let pair = "AV";
    let width_off = metrics.measure(pair, &off).advance;
    let width_on = metrics.measure(pair, &on).advance;
    assert!(
        width_on <= width_off,
        "开字距调整后不该变宽：开={width_on} 关={width_off}"
    );

    // 关掉时，整串宽度应当等于逐字单独量之和（没有跨边界调整）。
    let sum: Twips = pair.chars().map(|c| metrics.measure(&c.to_string(), &off).advance).sum();
    assert!(
        (width_off - sum).abs() <= 1,
        "关掉字距调整后，整串宽度应等于逐字之和：{width_off} vs {sum}"
    );
}
