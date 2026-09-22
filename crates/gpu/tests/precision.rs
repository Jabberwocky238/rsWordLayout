use std::cell::RefCell;

use rsword_layout_core::{Color, FontSpec, PositionedGlyph};
use rsword_layout_gpu::{FrameBuilder, GlyphKey, GlyphQuad, GlyphSource};

#[derive(Default)]
struct RecordingSource(RefCell<Vec<GlyphKey>>);

impl GlyphSource for RecordingSource {
    fn glyph(&self, key: &GlyphKey) -> Option<GlyphQuad> {
        self.0.borrow_mut().push(key.clone());
        Some(GlyphQuad {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0,
            v1: 1.0,
            left: 0.0,
            top: 0.0,
            width: 3.0,
            height: 4.0,
        })
    }
}

#[test]
fn batch_uses_precise_size_and_origin_at_each_dpi() {
    let glyph = PositionedGlyph {
        face: "test".into(),
        glyph_id: 4,
        x: 1440,
        y: 1600,
        x_pt: 72.0123,
        y_fine: 7992,
        advance_x: 60,
        advance_x_pt: 3.0,
        advance_y: 0,
        size_half_points: 16,
        size_centipoints: 792,
        source: Some((0, 1)),
    };
    for dpi in [72.0, 96.0, 144.0] {
        let source = RecordingSource::default();
        let mut builder = FrameBuilder::new(dpi);
        builder.push_glyphs(
            std::slice::from_ref(&glyph),
            Color::BLACK,
            &FontSpec::new("test", 16),
            Some(&source),
        );
        let frame = builder.finish();
        assert_eq!(
            *source.0.borrow(),
            [GlyphKey::from_centipoints("test", 4, 792)]
        );
        assert!((frame.vertices[0].x - 72.0123 * dpi / 72.0).abs() < 0.0001);
        assert!((frame.vertices[0].y - 79.92 * dpi / 72.0).abs() < 0.0001);
    }
}
