use rsword_layout_core::{FontMetrics, FontSpec, SimpleMetrics};

#[test]
fn exact_size_overrides_the_rounded_legacy_size() {
    let ordinary = FontSpec::new("Test", 24);
    assert_eq!(ordinary.size_centipoints, None);
    assert_eq!(ordinary.effective_size_centipoints(), 1200);

    let exact = ordinary.with_size_centipoints(792);
    assert_eq!(exact.size_half_points, 16);
    assert_eq!(exact.effective_size_centipoints(), 792);
    assert_eq!(exact.size_pt(), 7.92);

    let maximum = FontSpec::new("Test", u32::MAX);
    assert_eq!(
        maximum.effective_size_centipoints(),
        u64::from(u32::MAX) * 50
    );
    let beyond_legacy = maximum.with_size_centipoints(u64::MAX);
    assert_eq!(beyond_legacy.size_half_points, u32::MAX);
    assert_eq!(beyond_legacy.effective_size_centipoints(), u64::MAX);
}

#[test]
fn simple_metrics_keep_sub_twip_size_until_the_output_boundary() {
    let exact = FontSpec::new("Test", 24).with_size_centipoints(792);
    let measured = SimpleMetrics.measure("A\u{4e2d} ", &exact);
    assert_eq!(measured.advance, 277);
    assert_eq!(
        (measured.ascent, measured.descent, measured.line_gap),
        (126, 31, 23)
    );
    assert!((SimpleMetrics.advance_pt("A\u{4e2d} ", &exact) - 13.86).abs() < 1e-12);
    assert_eq!(SimpleMetrics.natural_height_fine("", &exact), 911);
    assert_eq!(
        SimpleMetrics.natural_height_fine("", &FontSpec::new("Test", 1)),
        58,
        "0.5pt * 1.15 is exactly 57.5 fine units and must round upward",
    );

    let mut spaced = exact;
    spaced.scale_pct = 125;
    spaced.letter_spacing = 3;
    assert!((SimpleMetrics.advance_pt("A\u{4e2d} ", &spaced) - 17.775).abs() < 1e-12);
}

#[test]
fn equivalent_size_representations_have_identical_metrics() {
    for half_points in [1, 17, 24, 51] {
        let legacy = FontSpec::new("Test", half_points);
        let exact = legacy
            .clone()
            .with_size_centipoints(u64::from(half_points) * 50);
        assert_eq!(
            SimpleMetrics.measure("A ", &legacy),
            SimpleMetrics.measure("A ", &exact)
        );
        assert_eq!(
            SimpleMetrics.advance_pt("A ", &legacy),
            SimpleMetrics.advance_pt("A ", &exact)
        );
        assert_eq!(
            SimpleMetrics.natural_height_fine("", &legacy),
            SimpleMetrics.natural_height_fine("", &exact),
        );
    }
}

#[cfg(feature = "shape")]
mod shape {
    use super::*;
    use rsword_layout_core::{RustybuzzShaper, TextShaper};

    const FONT: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");

    #[test]
    fn fractional_shaping_uses_font_units_at_the_exact_size() {
        let face = rustybuzz::Face::from_slice(FONT, 0).unwrap();
        let glyph_id = face.glyph_index('B').unwrap();
        let width = f64::from(face.glyph_hor_advance(glyph_id).unwrap());
        let mut shaper = RustybuzzShaper::new();
        shaper.add_face("Test", FONT.to_vec(), 0);

        for centipoints in [660, 792, 1188] {
            let font = FontSpec::new("Test", 24).with_size_centipoints(centipoints);
            let shaped = shaper.shape("BBBB", &font);
            let expected = width / f64::from(face.units_per_em()) * centipoints as f64 / 100.0;
            assert_eq!(shaped.len(), 4);
            assert!(
                shaped
                    .iter()
                    .all(|glyph| (glyph.x_advance_pt - expected).abs() < 1e-12)
            );
            assert_eq!(
                shaped.iter().map(|glyph| glyph.x_advance).sum::<i32>(),
                (expected * 80.0).round() as i32
            );
        }

        assert_eq!(
            shaper.shape_with_face(0, "BBBB", 24, false),
            shaper.shape_with_face_centipoints(0, "BBBB", 1200, false),
        );
    }
}

#[cfg(feature = "fontenv")]
mod real {
    use super::*;
    use rsword_layout_core::{FontSlots, RealMetrics, font::FontRegistry};
    use skrifa::{
        FontRef, MetadataProvider,
        instance::{LocationRef, Size},
    };

    const LATIN: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");
    const CJK: &[u8] = include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf");

    #[test]
    fn registry_metrics_preserve_exact_size_across_face_boundaries_and_vertical_cache() {
        let mut registry = FontRegistry::new();
        registry.add(LATIN.to_vec(), 0).unwrap();
        registry.add(CJK.to_vec(), 0).unwrap();
        let metrics = RealMetrics::new(&registry);
        let mut font = FontSpec::new("Liberation Serif", 24).with_size_centipoints(792);
        font.slots = FontSlots {
            ascii: Some("Liberation Serif".into()),
            east_asia: Some("Droid Sans Fallback".into()),
            ..FontSlots::default()
        };
        let expected: f64 = [('A', LATIN), ('\u{4e2d}', CJK), ('B', LATIN)]
            .into_iter()
            .map(|(ch, bytes)| {
                let face = FontRef::new(bytes).unwrap();
                let gid = face.charmap().map(ch).unwrap();
                f64::from(
                    face.glyph_metrics(Size::unscaled(), LocationRef::default())
                        .advance_width(gid)
                        .unwrap(),
                ) / f64::from(
                    face.metrics(Size::unscaled(), LocationRef::default())
                        .units_per_em,
                ) * 7.92
            })
            .sum();
        assert!((metrics.advance_pt("A\u{4e2d}B", &font) - expected).abs() < 1e-12);

        let face = FontRef::new(LATIN).unwrap();
        let vertical = face.metrics(Size::unscaled(), LocationRef::default());
        for centipoints in [1200, 792, 1188, 792] {
            let size = font.clone().with_size_centipoints(centipoints);
            let scale = centipoints as f64 / 100.0 * 20.0 / f64::from(vertical.units_per_em);
            let actual = metrics.measure("A", &size);
            assert_eq!(
                actual.ascent,
                (f64::from(vertical.ascent) * scale).round() as i32
            );
            assert_eq!(
                actual.descent,
                (f64::from(vertical.descent).abs() * scale).round() as i32
            );
            let natural = (f64::from(vertical.ascent)
                + f64::from(vertical.descent).abs()
                + f64::from(vertical.leading))
                * scale;
            assert_eq!(
                metrics.natural_height_fine("", &size),
                (natural * 5.0).round() as i64
            );
        }
    }
}

#[cfg(feature = "raster")]
mod raster {
    use rsword_layout_core::font::{GlyphKey, Rasterizer, SkrifaRasterizer};
    use std::collections::HashSet;

    const FONT: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");

    fn rasterizer() -> SkrifaRasterizer {
        let mut rasterizer = SkrifaRasterizer::new();
        rasterizer.add_face("Test", FONT.to_vec(), 0);
        rasterizer
    }

    #[test]
    fn atlas_keys_canonicalize_legacy_sizes_without_merging_fractional_sizes() {
        let keys = HashSet::from([
            GlyphKey::new("Test", 1, 16),
            GlyphKey::from_centipoints("Test", 1, 800),
            GlyphKey::from_centipoints("Test", 1, 792),
        ]);
        assert_eq!(keys.len(), 2);
        assert_eq!(
            GlyphKey::new("Test", 1, u32::MAX).size_centipoints,
            u64::from(u32::MAX) * 50
        );
    }

    #[test]
    fn raster_size_and_hinting_are_independent_of_previously_cached_sizes() {
        let mut cached = rasterizer();
        let glyph_id = cached.glyph_id("Test", 'B').unwrap();
        for centipoints in [800, 792, 776, 824, 1188, 792] {
            let key = GlyphKey::from_centipoints("Test", glyph_id, centipoints);
            let actual = cached.rasterize(&key).unwrap();
            let expected = rasterizer().rasterize(&key).unwrap();
            assert_eq!(actual.metrics, expected.metrics, "{centipoints}");
            assert_eq!(actual.coverage, expected.coverage, "{centipoints}");
        }
        let fractional = cached
            .rasterize(&GlyphKey::from_centipoints("Test", glyph_id, 792))
            .unwrap();
        let legacy = cached
            .rasterize(&GlyphKey::new("Test", glyph_id, 16))
            .unwrap();
        // Skrifa uses FreeType's 26.6 ppem scale before its 16.16 advance.
        // Raster resolution is separate from the f64 layout advances above.
        let expected = f64::from(legacy.metrics.advance) * ((7.92_f64 * 64.0).trunc() / 64.0) / 8.0;
        assert!((f64::from(fractional.metrics.advance) - expected).abs() < 1.0 / 65536.0);
        assert_ne!(fractional.metrics.advance, legacy.metrics.advance);
    }
}
