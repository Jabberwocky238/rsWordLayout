//! 真度量下的字符间距与缩放：量宽与画字数同一个东西（feature `fontenv`）。
//!
//! 用仓库自带的 DejaVu Sans：它有组合用尖音符 U+0301，而 `x` 没有预组合形，
//! 于是 `x\u{301}` 整形成**两个字形、一个 cluster**——正好把「按字形」与「按 cluster」分开。
//! 按 cluster 是**假定**（`latinspace` 只覆盖一字形一字符的拉丁文，三种口径同值）；
//! 这里钉的是实现与这个假定一致，且度量与落位一致。

use std::path::PathBuf;

use rsword_layout_core::{
    Color, DrawCmd, Engine, FontMetrics, FontSpec, PageSetup, Para, RealMetrics, Run,
    SimpleMetrics, TabAlign, TabLeader, TabStop, TextShaper, font::FontRegistry, paint_document,
};

fn dejavu() -> FontRegistry {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/DejaVuSans.ttf");
    let mut r = FontRegistry::new();
    r.add(std::fs::read(path).expect("字体读得到"), 0).expect("字体装得进");
    r
}

fn spec(letter_spacing: i32, scale_pct: u32) -> FontSpec {
    let mut font = FontSpec::new("DejaVu Sans", 24);
    font.letter_spacing = letter_spacing;
    font.scale_pct = scale_pct;
    font
}

#[test]
fn spacing_is_added_once_per_cluster() {
    let r = dejavu();
    let m = RealMetrics::new(&r);
    let shaped = r.shape_text("x\u{301}", &spec(0, 100));
    assert_eq!(shaped.len(), 2, "前提：组合符号自成一个字形");
    assert_eq!(shaped[0].source, shaped[1].source, "前提：两个字形同属一个 cluster");

    let plain = m.measure("x\u{301}", &spec(0, 100)).advance;
    let spaced = m.measure("x\u{301}", &spec(20, 100)).advance;
    assert_eq!(spaced - plain, 20, "一个 cluster 只加一次间距");
    let plain_pt = m.advance_pt("x\u{301}", &spec(0, 100));
    let spaced_pt = m.advance_pt("x\u{301}", &spec(20, 100));
    assert!((spaced_pt - plain_pt - 1.0).abs() < 1e-9);

    // 一字形一字符的文字：每个字符一次，空格也算。
    let text = "ab c";
    let delta = m.measure(text, &spec(20, 100)).advance - m.measure(text, &spec(0, 100)).advance;
    assert_eq!(delta, 4 * 20);
}

#[test]
fn stub_and_real_metrics_count_the_same_spacing_slots() {
    // 两种度量对同一段文字必须数出同样多的间距位置，否则换度量就顺带换了断行。
    // 组合序列是两者最容易分岔的地方：整形器把 U+0301 并进 `x`，桩按源字符簇跳过它。
    let r = dejavu();
    let real = RealMetrics::new(&r);
    let delta = |m: &dyn FontMetrics, text: &str| {
        m.measure(text, &spec(20, 100)).advance - m.measure(text, &spec(0, 100)).advance
    };
    for text in ["x\u{301}", "x\u{301}y", "e\u{301}\u{302} a", "ab c"] {
        assert_eq!(delta(&SimpleMetrics, text), delta(&real, text), "{text:?}");
    }
    assert_eq!(delta(&real, "x\u{301}"), 20);
}

#[test]
fn spacing_is_not_scaled() {
    // 缩放只作用于字形推进量，间距照原值加（假定，未测）。
    let r = dejavu();
    let m = RealMetrics::new(&r);
    let text = "abcd";
    let base = m.advance_pt(text, &spec(0, 100));
    let got = m.advance_pt(text, &spec(20, 50));
    assert!((got - (base * 0.5 + 4.0)).abs() < 1e-9, "{got} vs {}", base * 0.5 + 4.0);
}

/// 排一段、画出来，返回（片段文字，片段起点 x_pt，各字形 x_pt）。
fn painted(text: &str, font: FontSpec) -> Vec<(String, f64, Vec<f64>)> {
    let r = dejavu();
    let m = RealMetrics::new(&r);
    let para = Para {
        runs: vec![Run {
            text: text.into(),
            font,
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        ..Para::default()
    };
    let pages = Engine::new(&m, PageSetup::a4()).layout(&[para]);
    let faces = r.face_ids();
    let shaper: &dyn TextShaper = &r;
    let list = paint_document(&pages, Some(shaper), &faces);
    let mut out = Vec::new();
    for cmd in &list.pages[0].cmds {
        if let DrawCmd::DrawGlyphs { glyphs, text, origin_x_pt, .. } = cmd {
            out.push((text.clone(), *origin_x_pt, glyphs.iter().map(|g| g.x_pt).collect()));
        }
    }
    out
}

#[test]
fn painted_glyphs_follow_the_measured_advances() {
    // 断行量宽用的是 `advance_pt`（含缩放与间距），画字若不含，行宽对了、
    // 行内字形却挤在一起。逐字形核：第 k 个字形的 x = 起点 + 前 k 个字符的 advance_pt。
    let r = dejavu();
    let m = RealMetrics::new(&r);
    let text = "alpha beta";
    for (spacing, scale) in [(0, 100), (20, 100), (-10, 100), (0, 80), (30, 150)] {
        let font = spec(spacing, scale);
        for (frag, origin, xs) in painted(text, font.clone()) {
            assert!(frag.starts_with(text), "{frag:?}");
            for (k, x) in xs.iter().enumerate().take(text.chars().count()) {
                let prefix: String = text.chars().take(k).collect();
                let want = origin + m.advance_pt(&prefix, &font);
                assert!(
                    (x - want).abs() < 1e-6,
                    "spacing={spacing} scale={scale} 第 {k} 个字形 x={x:.6}，度量给 {want:.6}"
                );
            }
        }
    }
}

/// 排一段带制表位的、画出来，返回各字形（源位置 UTF-16，x_pt，推进量 pt）。
fn painted_with_tab(text: &str, font: FontSpec, stop: TabStop) -> Vec<(u32, f64, f64)> {
    let r = dejavu();
    let m = RealMetrics::new(&r);
    let para = Para {
        runs: vec![Run {
            text: text.into(),
            font,
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        tabs: vec![stop],
        ..Para::default()
    };
    let pages = Engine::new(&m, PageSetup::a4()).layout(&[para]);
    let faces = r.face_ids();
    let shaper: &dyn TextShaper = &r;
    let list = paint_document(&pages, Some(shaper), &faces);
    let mut out = Vec::new();
    for cmd in &list.pages[0].cmds {
        if let DrawCmd::DrawGlyphs { glyphs, .. } = cmd {
            for g in glyphs {
                let (at, _) = g.source.expect("一字形一字符，归属精确");
                out.push((at, g.x_pt, g.advance_x_pt));
            }
        }
    }
    out
}

#[test]
fn a_tab_keeps_its_stop_width_under_spacing_and_scale() {
    // 画字的顺序：制表符换成空格 → 整形 → 缩放与间距 → 制表符那个字形的推进量换成制表位定下的宽度。
    // 缩放与间距若排在改宽之后，制表符会多出一份间距，后面的字全错开一格——断行量的制表符宽
    // 不含它们（制表符宽只由制表位定），两边就对不上。制表符不吃间距本身是**假定**（Word 未测）。
    // A4 左边距 1440 twips = 72pt，左对齐制表位 2880 twips = 144pt，`c` 正好从 216pt 起画。
    let left = TabStop { pos: 2880, align: TabAlign::Left, leader: TabLeader::None };
    for (spacing, scale) in [(0, 100), (40, 100), (-10, 100), (0, 200), (40, 50)] {
        let font = spec(spacing, scale);
        let glyphs = painted_with_tab("ab\tcd", font, left);
        let x = |at: u32| glyphs.iter().find(|g| g.0 == at).map(|g| g.1).unwrap();
        let tab = glyphs.iter().find(|g| g.0 == 2).unwrap();
        assert!((x(3) - 216.0).abs() < 1e-9, "spacing={spacing} scale={scale}: c 在 {}", x(3));
        assert!((tab.1 + tab.2 - 216.0).abs() < 1e-9, "制表符画到制表位为止：{tab:?}");
        // 制表符之后的字照样吃缩放与间距。
        let want = x(3) + RealMetrics::new(&dejavu()).advance_pt("c", &spec(spacing, scale));
        assert!((x(4) - want).abs() < 1e-9, "spacing={spacing} scale={scale}: d 在 {}，该在 {want}", x(4));
    }
    // 右对齐：`cd`（含间距与缩放）的右端落在制表位上。制表位落位按整 twips 量段宽，差在 1 twip 内。
    let right = TabStop { pos: 4320, align: TabAlign::Right, leader: TabLeader::None };
    let r = dejavu();
    let m = RealMetrics::new(&r);
    for (spacing, scale) in [(0, 100), (40, 100), (40, 50)] {
        let font = spec(spacing, scale);
        let glyphs = painted_with_tab("ab\tcd", font.clone(), right);
        let c = glyphs.iter().find(|g| g.0 == 3).unwrap().1;
        let end = c + m.advance_pt("cd", &font);
        assert!((end - 288.0).abs() <= 0.05, "spacing={spacing} scale={scale}: cd 右端 {end}");
    }
}
