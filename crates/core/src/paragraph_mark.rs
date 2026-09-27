//! Independent paragraph-mark inputs from the native parser projection.

use serde_json::Value;

/// Paragraph-mark run properties, separate from the paragraph's text runs.
///
/// `declared` is the native JSON `props.rpr` value; `effective` is the result of
/// the pinned parser's style resolver, including resolved theme font slots.
/// Neither is inferred from the last text run. `None` means absent declaration
/// or unavailable resolution, respectively; empty objects and explicit nulls
/// remain distinguishable from `None`.
///
/// These values preserve the parser's JSON projection, not original XML bytes.
/// Existing layout still uses effective font/rise only for empty DOCX paragraphs;
/// independent mark metrics, visibility and painting for other paragraphs remain
/// unresolved. Retaining a property does not imply it affects layout.
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
}
