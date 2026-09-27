use std::{collections::HashMap, fmt, fmt::Write as _};

use rsword_layout_core::{
    DrawCmd, PaintList, PositionedGlyph, Twips, VectorCanvas, font::FontRegistry,
};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    raw::TableProvider,
};

use super::{SvgCanvas, alpha_attr, hex};

/// A positioned glyph could not be represented by its registered font outline.
/// Rendering never substitutes browser-shaped text when this occurs.
#[derive(Debug)]
pub enum OutlineError {
    MissingPositionedGlyphs {
        line: u32,
        source: Option<(u32, u32)>,
    },
    MissingFace {
        face: String,
    },
    InvalidFont {
        face: String,
        message: String,
    },
    InvalidGlyphId {
        face: String,
        glyph_id: u32,
    },
    UnsupportedOutlines {
        face: String,
    },
    GlyphOutline {
        face: String,
        glyph_id: u32,
        message: String,
    },
    InvalidGlyphGeometry {
        face: String,
        glyph_id: u32,
    },
    Formatting(fmt::Error),
}

impl fmt::Display for OutlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPositionedGlyphs { line, source } => write!(
                f,
                "nonempty text has no positioned glyphs (line {line}, source {source:?})"
            ),
            Self::MissingFace { face } => write!(f, "glyph face is not registered: {face}"),
            Self::InvalidFont { face, message } => write!(f, "invalid font {face}: {message}"),
            Self::InvalidGlyphId { face, glyph_id } => {
                write!(f, "invalid glyph {glyph_id} in font {face}")
            }
            Self::UnsupportedOutlines { face } => {
                write!(
                    f,
                    "font has no usable outline format (unsupported or unreadable): {face}"
                )
            }
            Self::GlyphOutline {
                face,
                glyph_id,
                message,
            } => write!(f, "cannot draw glyph {glyph_id} in font {face}: {message}"),
            Self::InvalidGlyphGeometry { face, glyph_id } => {
                write!(f, "invalid geometry for glyph {glyph_id} in font {face}")
            }
            Self::Formatting(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for OutlineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Formatting(error) => Some(error),
            _ => None,
        }
    }
}

impl From<fmt::Error> for OutlineError {
    fn from(error: fmt::Error) -> Self {
        Self::Formatting(error)
    }
}

/// Render the already positioned glyphs as unhinted vector outlines.
/// Uses registered face bytes, default variations and each glyph's precise
/// origin and size. Outlined text is not selectable; no shaping is performed.
pub fn render_outlined_html(
    list: &PaintList,
    fonts: &FontRegistry,
    title: &str,
) -> Result<String, OutlineError> {
    let mut canvas = OutlineCanvas {
        svg: SvgCanvas::new(),
        fonts,
        outlines: HashMap::new(),
    };
    list.replay(&mut canvas)?;
    Ok(canvas.svg.into_html_with_caption(
        title,
        "Vector SVG outlines: positioned glyphs, no text selection",
    ))
}

struct CachedOutline {
    data: String,
    units_per_em: u16,
}

struct OutlineCanvas<'a> {
    svg: SvgCanvas,
    fonts: &'a FontRegistry,
    outlines: HashMap<String, HashMap<u32, CachedOutline>>,
}

impl VectorCanvas for OutlineCanvas<'_> {
    type Error = OutlineError;

    fn begin_page(&mut self, width: Twips, height: Twips) -> Result<(), Self::Error> {
        self.svg.begin_page(width, height).map_err(Into::into)
    }

    fn end_page(&mut self) -> Result<(), Self::Error> {
        self.svg.end_page().map_err(Into::into)
    }

    fn draw(&mut self, cmd: &DrawCmd) -> Result<(), Self::Error> {
        let DrawCmd::DrawGlyphs {
            glyphs,
            text,
            font,
            paint,
            line,
            source,
            ..
        } = cmd
        else {
            return self.svg.draw(cmd).map_err(Into::into);
        };
        if !text.is_empty() && glyphs.is_empty() {
            return Err(OutlineError::MissingPositionedGlyphs {
                line: *line,
                source: *source,
            });
        }
        let scale = if font.scale_pct > 0 {
            f64::from(font.scale_pct) / 100.0
        } else {
            1.0
        };
        for glyph in glyphs {
            if !glyph.x_pt.is_finite() {
                return Err(invalid_geometry(glyph));
            }
            let face = self.outlines.entry(glyph.face.clone()).or_default();
            let outline = match face.entry(glyph.glyph_id) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(load_outline(self.fonts, glyph)?)
                }
            };
            let sy = glyph.size_centipoints as f64 / 100.0 / f64::from(outline.units_per_em);
            let sx = sy * scale;
            // Origins already contain shaping offsets, tracking and autospace.
            // Only the local contour receives the horizontal font scaling.
            write!(
                self.svg.cur,
                "<path d=\"{}\" transform=\"matrix({sx} 0 0 {} {} {})\" fill=\"{}\" fill-rule=\"nonzero\"{}/>",
                outline.data,
                -sy,
                glyph.x_pt,
                glyph.y_fine as f64 / 100.0,
                hex(paint.color),
                alpha_attr("fill-opacity", paint),
            )?;
        }
        Ok(())
    }
}

fn invalid_geometry(glyph: &PositionedGlyph) -> OutlineError {
    OutlineError::InvalidGlyphGeometry {
        face: glyph.face.clone(),
        glyph_id: glyph.glyph_id,
    }
}

fn load_outline(
    fonts: &FontRegistry,
    glyph: &PositionedGlyph,
) -> Result<CachedOutline, OutlineError> {
    let (bytes, index) = fonts
        .face_data(&glyph.face)
        .ok_or_else(|| OutlineError::MissingFace {
            face: glyph.face.clone(),
        })?;
    let bad_font = |message: String| OutlineError::InvalidFont {
        face: glyph.face.clone(),
        message,
    };
    let font = FontRef::from_index(bytes, index).map_err(|e| bad_font(e.to_string()))?;
    let units_per_em = font
        .head()
        .map_err(|e| bad_font(e.to_string()))?
        .units_per_em();
    if !(16..=16384).contains(&units_per_em) {
        return Err(bad_font(format!(
            "unitsPerEm is out of range: {units_per_em}"
        )));
    }
    let count = font
        .maxp()
        .map_err(|e| bad_font(e.to_string()))?
        .num_glyphs();
    if glyph.glyph_id >= u32::from(count) {
        return Err(OutlineError::InvalidGlyphId {
            face: glyph.face.clone(),
            glyph_id: glyph.glyph_id,
        });
    }
    let outlines = font.outline_glyphs();
    if outlines.format().is_none() {
        return Err(OutlineError::UnsupportedOutlines {
            face: glyph.face.clone(),
        });
    }
    let bad_outline = |message: String| OutlineError::GlyphOutline {
        face: glyph.face.clone(),
        glyph_id: glyph.glyph_id,
        message,
    };
    let outline = outlines
        .get(GlyphId::new(glyph.glyph_id))
        .ok_or_else(|| bad_outline("outline is missing or unreadable".into()))?;
    let mut pen = SvgOutlinePen {
        data: String::new(),
        finite: true,
    };
    outline
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
            &mut pen,
        )
        .map_err(|e| bad_outline(e.to_string()))?;
    if !pen.finite {
        return Err(invalid_geometry(glyph));
    }
    // A valid space has a successfully drawn empty path. Missing/unreadable
    // outlines returned above are errors, even when their output would be empty.
    Ok(CachedOutline {
        data: pen.data.trim_end().to_owned(),
        units_per_em,
    })
}

struct SvgOutlinePen {
    data: String,
    finite: bool,
}

impl SvgOutlinePen {
    fn check(&mut self, values: &[f32]) {
        self.finite &= values.iter().all(|v| v.is_finite());
    }
}

impl OutlinePen for SvgOutlinePen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.check(&[x, y]);
        let _ = write!(self.data, "M{x} {y} ");
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.check(&[x, y]);
        let _ = write!(self.data, "L{x} {y} ");
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.check(&[cx, cy, x, y]);
        let _ = write!(self.data, "Q{cx} {cy} {x} {y} ");
    }

    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.check(&[c1x, c1y, c2x, c2y, x, y]);
        let _ = write!(self.data, "C{c1x} {c1y} {c2x} {c2y} {x} {y} ");
    }

    fn close(&mut self) {
        self.data.push_str("Z ");
    }
}
