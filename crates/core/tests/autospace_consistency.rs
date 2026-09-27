//! Synthetic invariants of the engine's existing DN spacing policy.
//! These do not establish Word's mixed-font or hidden-text behavior.

use rsword_layout_core::{
    Align, BreakOpportunity, Color, DrawCmd, Engine, FontMetrics, FontSpec, Fragment, LayoutRecord,
    LineRule, Margins, Page, PageSetup, Para, PlaceholderKind, PositionedGlyph, Run, ShapedRun,
    SimpleMetrics, Size, SpacingAdvance, TabAlign, TabStop, TextMetrics, TextShaper,
    paint_document,
};

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new("synthetic", 24),
        color: Color::BLACK,
        placeholders: vec![],
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn para(parts: &[&str]) -> Para {
    Para {
        runs: parts.iter().map(|text| run(text)).collect(),
        line_rule: LineRule::Exact,
        line_value: 240,
        ..Para::default()
    }
}

fn layout(metrics: &impl FontMetrics, p: &Para, width: i32) -> Vec<Page> {
    Engine::new(
        metrics,
        PageSetup {
            size: Size::new(width, 2400),
            margins: Margins::uniform(0),
        },
    )
    .layout(std::slice::from_ref(p))
}

fn ranges(pages: &[Page]) -> Vec<(u32, u32)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|page| &page.lines)
        .map(|line| {
            let s = line.source.unwrap();
            (s.start, s.end)
        })
        .collect()
}

struct ScalarShaper;

impl TextShaper for ScalarShaper {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        // Painting applies scale and tracking after shaping; return raw glyph
        // advances so tests do not apply either adjustment twice.
        let mut raw_font = font.clone();
        raw_font.scale_pct = 100;
        raw_font.letter_spacing = 0;
        raw_font.auto_space_dn = false;
        let mut cp = 0;
        text.chars()
            .map(|ch| {
                let end = cp + ch.len_utf16() as u32;
                let text = ch.to_string();
                let g = ShapedRun {
                    face_index: 0,
                    glyph_id: ch as u32,
                    source: Some((cp, end)),
                    x_advance: SimpleMetrics.measure(&text, &raw_font).advance,
                    x_advance_pt: SimpleMetrics.advance_pt(&text, &raw_font),
                    x_offset: 0,
                    y_offset: 0,
                    size_centipoints: None,
                };
                cp = end;
                g
            })
            .collect()
    }
}

fn glyphs(pages: &[Page], shaper: &dyn TextShaper) -> Vec<PositionedGlyph> {
    paint_document(pages, Some(shaper), &["synthetic".into()])
        .pages
        .into_iter()
        .flat_map(|page| page.cmds)
        .flat_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { glyphs, .. } => glyphs,
            _ => vec![],
        })
        .collect()
}

fn geometry(pages: &[Page]) -> Vec<((u32, u32), f64, f64)> {
    glyphs(pages, &ScalarShaper)
        .iter()
        .map(|g| (g.source.unwrap(), g.x_pt, g.advance_x_pt))
        .collect()
}

#[test]
fn run_boundaries_preserve_enabled_and_disabled_line_admission() {
    for enabled in [true, false] {
        let mut whole = para(&["\u{6c49}0"]);
        let mut split = para(&["\u{6c49}", "0"]);
        for p in [&mut whole, &mut split] {
            for r in &mut p.runs {
                r.font.auto_space_dn = enabled;
            }
        }
        let a = layout(&SimpleMetrics, &whole, 360);
        let b = layout(&SimpleMetrics, &split, 360);
        assert_eq!(ranges(&a), ranges(&b), "enabled={enabled}");
        assert_eq!(geometry(&a), geometry(&b));
    }
}

#[test]
fn negative_prefix_advance_can_cover_incoming_spacing_debt() {
    let mut whole = para(&["\u{6c49}0\u{6c49}"]);
    let mut split = para(&["\u{6c49}", "0\u{6c49}"]);
    for p in [&mut whole, &mut split] {
        for r in &mut p.runs {
            r.font.letter_spacing = -180;
        }
    }
    let a = layout(&SimpleMetrics, &whole, 100);
    let b = layout(&SimpleMetrics, &split, 100);
    assert_eq!(ranges(&a), [(0, 2), (2, 4)]);
    assert_eq!(ranges(&a), ranges(&b));
    assert_eq!(geometry(&a), geometry(&b));
    let g = glyphs(&a, &ScalarShaper);
    assert_eq!(g[0].advance_x_pt, 6.0);
    assert_eq!(g[1].x_pt, 6.0);
    assert_eq!(g[1].advance_x_pt, -3.0);
}

#[test]
fn negative_budget_still_allows_qualified_punctuation_overflow() {
    let mut whole = para(&["\u{6c49}0\u{6c49}\u{3002}\u{6c49}"]);
    let mut split = para(&["\u{6c49}", "0\u{6c49}\u{3002}\u{6c49}"]);
    for p in [&mut whole, &mut split] {
        p.overflow_punct = true;
        for r in &mut p.runs {
            r.font.letter_spacing = -220;
        }
    }
    let a = layout(&SimpleMetrics, &whole, 60);
    let b = layout(&SimpleMetrics, &split, 60);
    assert_eq!(ranges(&a), [(0, 4), (4, 6)]);
    assert_eq!(ranges(&a), ranges(&b));
    assert_eq!(geometry(&a), geometry(&b));
    let g = glyphs(&a, &ScalarShaper);
    assert_eq!(g[3].x_pt + g[3].advance_x_pt, 4.0);
}

#[test]
fn internal_and_cross_run_glyph_advances_include_the_retained_gap() {
    let whole = layout(&SimpleMetrics, &para(&["\u{6c49}0\u{6c49}"]), 2000);
    let split = layout(&SimpleMetrics, &para(&["\u{6c49}", "0", "\u{6c49}"]), 2000);
    let g = glyphs(&whole, &ScalarShaper);
    assert_eq!(g[0].advance_x_pt, 15.0);
    assert_eq!(g[1].x_pt, 15.0);
    assert_eq!(g[1].advance_x_pt, 9.0);
    assert_eq!(g[2].x_pt, 24.0);
    assert_eq!(geometry(&whole), geometry(&split));
}

#[test]
fn rollback_removes_the_incoming_gap_and_wrap_does_not_keep_it() {
    let mut whole = para(&["\u{6c49}0ab"]);
    let mut split = para(&["\u{6c49}", "0", "ab"]);
    whole.align = Align::Right;
    split.align = Align::Right;
    let a = layout(&SimpleMetrics, &whole, 500);
    let b = layout(&SimpleMetrics, &split, 500);
    assert_eq!(ranges(&a), [(0, 1), (1, 5)]);
    assert_eq!(ranges(&a), ranges(&b));
    assert_eq!(geometry(&a), geometry(&b));
    let g = glyphs(&b, &ScalarShaper);
    assert_eq!(g[0].x_pt, 13.0);
    assert_eq!(g[0].advance_x_pt, 12.0);
    assert_eq!(g[1].x_pt, 7.0);
}

#[test]
fn hidden_source_gap_and_non_bmp_left_cluster_keep_visible_adjacency() {
    let mut p = para(&["A\u{20000}", "ZZ\u{1f600}", "0"]);
    p.runs[1].hidden = true;
    let g = glyphs(&layout(&SimpleMetrics, &p, 2000), &ScalarShaper);
    assert_eq!(g[1].source, Some((1, 3)));
    assert_eq!(g[2].source, Some((7, 8)));
    assert_eq!(g[1].advance_x_pt, 15.0);
    assert_eq!(g[2].x_pt, 21.0);
}

#[test]
fn aligned_tabs_measure_cross_run_boundaries_before_landing() {
    for align in [TabAlign::Right, TabAlign::Center, TabAlign::Decimal] {
        for width in [620, 1200, 2200] {
            let mut whole = para(&["A\t\u{6c49}0.0\u{6c49}"]);
            let mut split = para(&["A\t", "\u{6c49}", "0.0", "\u{6c49}"]);
            for p in [&mut whole, &mut split] {
                p.tabs.push(TabStop {
                    pos: 900,
                    align,
                    leader: Default::default(),
                });
            }
            let a = layout(&SimpleMetrics, &whole, width);
            let b = layout(&SimpleMetrics, &split, width);
            assert_eq!(ranges(&a), ranges(&b), "{align:?} width={width}");
            assert_eq!(geometry(&a), geometry(&b), "{align:?} width={width}");
        }
    }
}

#[test]
fn tab_object_and_explicit_break_are_spacing_barriers() {
    for kind in [
        PlaceholderKind::Object,
        PlaceholderKind::LineBreak,
        PlaceholderKind::PageBreak,
    ] {
        let mut p = para(&["\u{6c49}\u{fffc}0"]);
        p.runs[0].placeholders = vec![kind];
        let pages = layout(&SimpleMetrics, &p, 2000);
        let g = glyphs(&pages, &ScalarShaper);
        assert_eq!(g[0].advance_x_pt, 12.0, "{kind:?}");
    }
    let pages = layout(&SimpleMetrics, &para(&["\u{6c49}\t0"]), 2000);
    assert_eq!(glyphs(&pages, &ScalarShaper)[0].advance_x_pt, 12.0);
}

struct OpaqueMetrics;

impl FontMetrics for OpaqueMetrics {
    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let mut font = font.clone();
        font.auto_space_dn = false;
        SimpleMetrics.measure(text, &font)
    }
}

#[test]
fn opaque_provider_does_not_acquire_builtin_spacing() {
    let whole = layout(&OpaqueMetrics, &para(&["\u{6c49}0"]), 360);
    let split = layout(&OpaqueMetrics, &para(&["\u{6c49}", "0"]), 360);
    assert_eq!(ranges(&whole), [(0, 3)]);
    assert_eq!(geometry(&whole), geometry(&split));
    assert_eq!(glyphs(&whole, &ScalarShaper)[0].advance_x_pt, 12.0);
}

#[test]
fn centipoint_size_retains_fine_gaps_separately_from_fit_twips() {
    let mut whole = para(&["\u{6c49}0\u{6c49}"]);
    let mut split = para(&["\u{6c49}", "0", "\u{6c49}"]);
    for p in [&mut whole, &mut split] {
        for r in &mut p.runs {
            r.font.size_centipoints = Some(792);
        }
    }
    let a = layout(&SimpleMetrics, &whole, 2000);
    let b = layout(&SimpleMetrics, &split, 2000);
    assert_eq!(geometry(&a), geometry(&b));
    let g = glyphs(&a, &ScalarShaper);
    assert!((g[0].advance_x_pt - 9.9).abs() < 1e-10);
    assert_eq!(g[0].advance_x, 198);
    let gap =
        SimpleMetrics.boundary_spacing('\u{6c49}', &whole.runs[0].font, '0', &whole.runs[0].font);
    assert_eq!(gap.fit_twips, 39);
    assert!((gap.paint_pt - 1.98).abs() < 1e-10);
}

#[test]
fn mixed_fonts_and_sizes_use_the_explicit_ideographic_side_policy() {
    for parts in [["\u{6c49}", "0"], ["0", "\u{6c49}"]] {
        let mut p = para(&parts);
        for r in &mut p.runs {
            if r.text == "0" {
                r.font = FontSpec::new("digits", 16);
                r.font.slots.east_asia = Some("unused slot".into());
            }
        }
        let g = glyphs(&layout(&SimpleMetrics, &p, 2000), &ScalarShaper);
        let base = SimpleMetrics.advance_pt(parts[0], &p.runs[0].font);
        assert_eq!(g[0].advance_x_pt, base + 3.0);
        assert_eq!(g[1].x_pt, base + 3.0);
    }
}

struct ClusterShaper(u8);

impl TextShaper for ClusterShaper {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        let mut glyphs = ScalarShaper.shape(text, font);
        match self.0 {
            0 => {
                let mut second = glyphs[0];
                glyphs[0].x_advance_pt = 6.0;
                glyphs[0].x_advance = 120;
                second.x_advance_pt = 6.0;
                second.x_advance = 120;
                glyphs.insert(1, second);
            }
            1 => {
                glyphs[0].source = Some((0, 2));
                glyphs[0].x_advance_pt = 18.0;
                glyphs[0].x_advance = 360;
                glyphs.remove(1);
            }
            2 => {
                for g in &mut glyphs {
                    g.source = None;
                }
            }
            3 => {
                glyphs.swap(0, 1);
            }
            4 => {
                let mut unknown = glyphs[0];
                unknown.source = None;
                unknown.x_advance_pt = 0.0;
                unknown.x_advance = 0;
                glyphs.insert(1, unknown);
            }
            _ => unreachable!(),
        }
        glyphs
    }
}

#[test]
fn painting_owns_the_last_glyph_of_an_exact_source_cluster_once() {
    let pages = layout(&SimpleMetrics, &para(&["\u{6c49}0"]), 2000);
    let g = glyphs(&pages, &ClusterShaper(0));
    assert_eq!(g[0].advance_x_pt, 6.0);
    assert_eq!(g[1].advance_x_pt, 9.0);
    assert_eq!(g[2].x_pt, 15.0);
}

#[test]
fn missing_merged_and_reordered_cluster_sources_do_not_guess_event_positions() {
    let pages = layout(&SimpleMetrics, &para(&["\u{6c49}0"]), 2000);
    let fragment = pages[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Text(t) if t.text.starts_with('\u{6c49}') => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(fragment.spacing.len(), 1);
    assert_eq!(fragment.spacing[0].source_end, 1);
    for mode in [1, 2, 3, 4] {
        let shaper = ClusterShaper(mode);
        let raw = shaper.shape(&fragment.text, &fragment.font);
        let painted = glyphs(&pages, &shaper);
        assert_eq!(
            raw.iter().map(|g| g.x_advance_pt).collect::<Vec<_>>(),
            painted.iter().map(|g| g.advance_x_pt).collect::<Vec<_>>()
        );
        assert_eq!(
            raw.iter().map(|g| g.x_advance).collect::<Vec<_>>(),
            painted.iter().map(|g| g.advance_x).collect::<Vec<_>>()
        );
    }
}

struct DebtMetrics;

impl FontMetrics for DebtMetrics {
    fn measure(&self, text: &str, _: &FontSpec) -> TextMetrics {
        TextMetrics {
            advance: text.matches('\u{6c49}').count() as i32 * 100
                + text.matches("\u{6c49}0").count() as i32 * 30,
            ascent: 180,
            descent: 60,
            line_gap: 0,
        }
    }
    fn boundary_spacing(
        &self,
        left: char,
        _: &FontSpec,
        right: char,
        _: &FontSpec,
    ) -> SpacingAdvance {
        if left == '\u{6c49}' && right == '0' {
            SpacingAdvance {
                fit_twips: 30,
                paint_pt: 1.5,
            }
        } else {
            SpacingAdvance::default()
        }
    }
    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        vec![BreakOpportunity {
            offset: text.len(),
            hyphen: false,
        }]
    }
}

#[test]
fn incoming_gap_debt_cannot_admit_a_zero_width_prefix() {
    let pages = layout(&DebtMetrics, &para(&["\u{6c49}", "0"]), 100);
    assert_eq!(ranges(&pages), [(0, 1), (1, 3)]);
    let g = glyphs(&pages, &ScalarShaper);
    assert_eq!(g[0].advance_x_pt, 12.0);
    assert_eq!(g[1].x_pt, 0.0);
}

#[cfg(feature = "fontenv")]
#[test]
fn real_glyph_positions_include_unscaled_spacing_and_match_measured_total() {
    use rsword_layout_core::{RealMetrics, font::FontRegistry};
    let mut registry = FontRegistry::new();
    registry
        .add(
            include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf").to_vec(),
            0,
        )
        .unwrap();
    registry
        .add(
            include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf").to_vec(),
            0,
        )
        .unwrap();
    let metrics = RealMetrics::new(&registry);
    for scale in [100, 150] {
        let mut p = para(&["\u{6c49}0\u{6c49}"]);
        let font = &mut p.runs[0].font;
        font.family = "Liberation Sans".into();
        font.slots.ascii = Some("Liberation Sans".into());
        font.slots.east_asia = Some("Droid Sans Fallback".into());
        font.size_centipoints = Some(792);
        font.scale_pct = scale;
        font.letter_spacing = 7;
        let font = &p.runs[0].font;
        let raw = registry.shape(&p.runs[0].text, font);
        let expected = metrics.advance_pt(&p.runs[0].text, font);
        let pages = layout(&metrics, &p, 2000);
        let painted = paint_document(&pages, Some(&registry), &registry.face_ids());
        let g: Vec<_> = painted
            .pages
            .iter()
            .flat_map(|page| &page.cmds)
            .filter_map(|cmd| match cmd {
                DrawCmd::DrawGlyphs { glyphs, .. } => Some(glyphs),
                _ => None,
            })
            .flatten()
            .collect();
        let first = raw[0].x_advance_pt * f64::from(scale) / 100.0 + 0.35 + 1.98;
        assert!((g[0].advance_x_pt - first).abs() < 1e-10);
        assert!((g[1].x_pt - first).abs() < 1e-10);
        assert!((g[2].x_pt + g[2].advance_x_pt - expected).abs() < 1e-10);
        assert_eq!(
            g[..3].iter().map(|g| g.advance_x).sum::<i32>(),
            (expected * 20.0).round() as i32
        );
    }
}

#[test]
fn scalar_run_splits_preserve_actual_paint_with_numeric_cjk_tabs_and_controls() {
    let texts = [
        "\u{6c49}0\u{ff09}ab\t0\u{6c49}",
        "ab \u{6c49}0ab\t\u{6c49}0",
        "\u{6c49}0\u{6c49} 0\u{6c49}",
        "A\u{6c49}\u{fffc}0\t\u{6c49}0",
    ];
    for text in texts {
        let parts: Vec<_> = text
            .char_indices()
            .map(|(at, ch)| &text[at..at + ch.len_utf8()])
            .collect();
        for width in [300, 420, 620, 900, 1440] {
            for align in [
                TabAlign::Left,
                TabAlign::Right,
                TabAlign::Center,
                TabAlign::Decimal,
            ] {
                for kind in [
                    PlaceholderKind::Object,
                    PlaceholderKind::LineBreak,
                    PlaceholderKind::PageBreak,
                ] {
                    let mut a = para(&[text]);
                    let mut b = para(&parts);
                    for p in [&mut a, &mut b] {
                        p.tabs.push(TabStop {
                            pos: 600,
                            align,
                            leader: Default::default(),
                        });
                        for r in &mut p.runs {
                            r.placeholders = r.text.matches('\u{fffc}').map(|_| kind).collect();
                        }
                    }
                    let a = layout(&SimpleMetrics, &a, width);
                    let b = layout(&SimpleMetrics, &b, width);
                    assert_eq!(
                        ranges(&a),
                        ranges(&b),
                        "{text:?} width={width} {align:?} {kind:?}"
                    );
                    assert_eq!(
                        geometry(&a),
                        geometry(&b),
                        "{text:?} width={width} {align:?} {kind:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn later_paragraphs_use_global_source_endpoints_and_repainting_is_idempotent() {
    for width in [360, 2000] {
        let engine = Engine::new(
            &SimpleMetrics,
            PageSetup {
                size: Size::new(width, 2400),
                margins: Margins::uniform(0),
            },
        );
        let whole = engine.layout(&[
            para(&["A\u{1f600}"]),
            para(&["\u{6c49}0"]),
            para(&["\u{6c49}0"]),
        ]);
        let split = engine.layout(&[
            para(&["A\u{1f600}"]),
            para(&["\u{6c49}", "0"]),
            para(&["\u{6c49}", "0"]),
        ]);
        assert_eq!(ranges(&whole), ranges(&split));
        let expected = geometry(&whole);
        assert_eq!(expected, geometry(&split));
        assert_eq!(expected, geometry(&whole));
        if width == 2000 {
            let endpoints: Vec<_> = split
                .iter()
                .flat_map(|p| &p.fragments)
                .filter_map(|f| match f {
                    Fragment::Text(t) => Some(&t.spacing),
                    _ => None,
                })
                .flatten()
                .map(|e| e.source_end)
                .collect();
            assert_eq!(endpoints, [5, 8]);
            for (source, x, _) in expected {
                if source == (5, 6) || source == (8, 9) {
                    assert_eq!(x, 15.0);
                }
            }
        }
    }
}
