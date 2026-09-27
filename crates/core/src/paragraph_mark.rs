//! Independent paragraph-mark inputs from the native parser projection.

use serde_json::Value;

/// Painting only: these values do not contribute to line metrics or visibility.
pub(crate) struct ParagraphMarkStyle {
    pub font: crate::FontSpec,
    pub rise_fine: i64,
}

/// Paragraph-mark run properties, separate from the paragraph's text runs.
///
/// `declared` is the native JSON `props.rpr` value; `effective` is the result of
/// the pinned parser's style resolver, including resolved theme font slots.
/// Neither is inferred from the last text run. `None` means absent declaration
/// or unavailable resolution, respectively; empty objects and explicit nulls
/// remain distinguishable from `None`.
///
/// These values preserve the parser's JSON projection, not original XML bytes.
/// Layout can use independently resolved font/rise for the paragraph-end glyph
/// when a Latin font, a valid size and valid position/vertical alignment are
/// available. Missing or invalid required fields retain the legacy paint
/// fallback; other font fields use the existing run projection. Mark metrics
/// and visibility remain unresolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphMarkProperties {
    declared: Option<Value>,
    effective: Option<Value>,
}

impl ParagraphMarkProperties {
    /// Preserve declaration and resolution independently, without adding defaults.
    pub fn from_json(declared: Option<&Value>, effective: Option<&Value>) -> Self {
        Self {
            declared: declared.cloned(),
            effective: effective.cloned(),
        }
    }

    pub fn declared(&self) -> Option<&Value> {
        self.declared.as_ref()
    }

    pub fn effective(&self) -> Option<&Value> {
        self.effective.as_ref()
    }

    pub(crate) fn paint_style(&self) -> Option<ParagraphMarkStyle> {
        crate::bridge::paragraph_mark_paint_style(self.effective.as_ref()?)
    }
}
