//! Document geometry and paragraph ranges for the shared page formatter.

use std::ops::Range;

use serde_json::{Value, json};

use crate::{Margins, PageSetup, Para, Twips, paras_from_document};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionStart {
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
    NextColumn,
}

impl SectionStart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NextPage => "nextPage",
            Self::Continuous => "continuous",
            Self::EvenPage => "evenPage",
            Self::OddPage => "oddPage",
            Self::NextColumn => "nextColumn",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutSection {
    /// Original main-story block indices, including unsupported blocks.
    pub block_range: Range<usize>,
    pub para_range: Range<usize>,
    pub setup: PageSetup,
    pub kind: SectionStart,
    /// Missing or invalid values use the host's A4 / one-inch defaults.
    pub fallback_fields: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PageOverrides {
    pub margin: Option<Twips>,
    pub page_width: Option<Twips>,
    pub content_width: Option<Twips>,
}

/// Document switches interpreted by the shared formatter, independently of the view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentCompatibility {
    /// `w:compat/w:splitPgBreakAndParaMark`. Absence and explicit false both keep
    /// the print-view default; mobile view still splits the paragraph mark.
    pub split_page_break_and_para_mark: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LayoutDocument {
    pub paras: Vec<Para>,
    pub sections: Vec<LayoutSection>,
    pub compatibility: DocumentCompatibility,
    pub skipped_blocks: usize,
    pub diagnostics: Vec<String>,
    overrides: PageOverrides,
}

impl LayoutDocument {
    /// Apply diagnostic overrides to every section, atomically. Unspecified
    /// fields retain earlier overrides; content width wins over side margins.
    pub fn apply_page_overrides(&mut self, overrides: PageOverrides) -> Result<(), String> {
        let overrides = PageOverrides {
            margin: overrides.margin.or(self.overrides.margin),
            page_width: overrides.page_width.or(self.overrides.page_width),
            content_width: overrides.content_width.or(self.overrides.content_width),
        };
        let setups = self
            .sections
            .iter()
            .map(|section| {
                let mut setup = section.setup;
                if let Some(margin) = overrides.margin {
                    if margin < 0 {
                        return Err("page margin must be nonnegative".into());
                    }
                    setup.margins = Margins::uniform(margin);
                }
                if let Some(width) = overrides.page_width {
                    setup.size.width = width;
                }
                if let Some(width) = overrides.content_width {
                    if width <= 0 || width > setup.size.width {
                        return Err(format!(
                            "content width {width} is outside page width {}",
                            setup.size.width
                        ));
                    }
                    let slack = setup.size.width - width;
                    setup.margins.left = slack / 2;
                    setup.margins.right = slack - setup.margins.left;
                }
                if !valid_setup(setup) {
                    return Err(
                        "page geometry must leave a positive, representable content area".into(),
                    );
                }
                Ok(setup)
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (section, setup) in self.sections.iter_mut().zip(setups) {
            section.setup = setup;
        }
        self.overrides = overrides;
        Ok(())
    }

    /// Input provenance for a trace; coordinates are twips, not trace points.
    pub fn trace_metadata(&self) -> Value {
        json!({
            "unit": "twips",
            "fallback": "host A4, 1440-twip margins; not inferred Word defaults",
            "sourceCoverage": if self.skipped_blocks == 0 { "projected main-story paragraphs" } else { "partial: unsupported blocks omitted; CP is projected text only" },
            "skippedBlocks": self.skipped_blocks,
            "diagnostics": self.diagnostics,
            "compatibility": {
                "splitPgBreakAndParaMark": self.compatibility.split_page_break_and_para_mark,
            },
            "overrides": {
                "margin": self.overrides.margin,
                "pageWidth": self.overrides.page_width,
                "contentWidth": self.overrides.content_width,
            },
            "sections": self.sections.iter().map(|s| json!({
                "blockRange": [s.block_range.start, s.block_range.end],
                "paraRange": [s.para_range.start, s.para_range.end],
                "kind": s.kind.as_str(),
                "pageSize": {"width": s.setup.size.width, "height": s.setup.size.height},
                "margins": {"top": s.setup.margins.top, "right": s.setup.margins.right,
                    "bottom": s.setup.margins.bottom, "left": s.setup.margins.left},
                "fallbackFields": s.fallback_fields,
            })).collect::<Vec<_>>(),
        })
    }
}

fn valid_setup(setup: PageSetup) -> bool {
    let m = setup.margins;
    setup.size.width > 0
        && setup.size.height > 0
        && [m.top, m.right, m.bottom, m.left].iter().all(|v| *v >= 0)
        && i64::from(m.left) + i64::from(m.right) < i64::from(setup.size.width)
        && i64::from(m.top) + i64::from(m.bottom) < i64::from(setup.size.height)
}

fn dimension(
    value: &Value,
    fallback: Twips,
    name: &'static str,
    positive: bool,
    fallback_fields: &mut Vec<&'static str>,
) -> Twips {
    if let Some(n) = value.as_i64().and_then(|n| Twips::try_from(n).ok())
        && if positive { n > 0 } else { n >= 0 }
    {
        return n;
    }
    fallback_fields.push(name);
    fallback
}

fn section_setup(props: &Value, settings: &Value) -> (PageSetup, Vec<&'static str>) {
    let mut setup = PageSetup::a4();
    let mut fallback = Vec::new();
    let size = &props["pageSize"];
    let margins = &props["pageMargins"];
    // Width and height already encode orientation; swapping again breaks landscape files.
    setup.size.width = dimension(&size["w"], setup.size.width, "width", true, &mut fallback);
    setup.size.height = dimension(&size["h"], setup.size.height, "height", true, &mut fallback);
    for (field, target) in [
        ("top", &mut setup.margins.top),
        ("right", &mut setup.margins.right),
        ("bottom", &mut setup.margins.bottom),
        ("left", &mut setup.margins.left),
    ] {
        // Negative top/bottom margins reserve an absolute distance for body text;
        // header/footer overlap is outside this text-only formatter.
        let value = if matches!(field, "top" | "bottom") {
            margins[field]
                .as_i64()
                .and_then(i64::checked_abs)
                .map(Value::from)
                .unwrap_or(Value::Null)
        } else {
            margins[field].clone()
        };
        *target = dimension(&value, *target, field, false, &mut fallback);
    }
    let gutter = dimension(&margins["gutter"], 0, "gutter", false, &mut fallback);
    let target = if settings["gutterAtTop"] == true {
        &mut setup.margins.top
    } else if props["rtlGutter"] == true {
        &mut setup.margins.right
    } else {
        &mut setup.margins.left
    };
    *target = target.saturating_add(gutter);
    (setup, fallback)
}

/// Project text and section geometry without losing original block ownership.
/// Unsupported blocks are counted; they do not have full-story CP coverage.
/// The pinned parser omits boolean compatibility flags from native JSON. Use
/// `LoadedDocument::layout_document` to retain them, or set `compatibility` explicitly.
pub fn document_from_json(doc: &Value) -> LayoutDocument {
    let (mut paras, _) = paras_from_document(doc);
    let main = doc["main"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let skipped_blocks = main
        .iter()
        .filter(|block| {
            block["kind"] != "text"
                && !(block["kind"] == "protected"
                    && block["protectedKind"]["kind"] == "sectionProps")
        })
        .count();
    let mut prefix = vec![0usize];
    for block in main {
        prefix.push(prefix.last().copied().unwrap_or(0) + usize::from(block["kind"] == "text"));
    }
    let mut diagnostics = Vec::new();
    let mut sections = Vec::new();
    let raw_sections = doc["sections"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut covered = 0usize;
    for raw in raw_sections {
        let range = &raw["blockRange"];
        let start = range[0].as_u64().and_then(|v| usize::try_from(v).ok());
        let end = range[1].as_u64().and_then(|v| usize::try_from(v).ok());
        let (Some(start), Some(end)) = (start, end) else {
            diagnostics.push("invalid section block range; using host fallback geometry".into());
            sections.clear();
            break;
        };
        if start != covered || end < start || end > main.len() {
            diagnostics
                .push("noncontiguous section block ranges; using host fallback geometry".into());
            sections.clear();
            break;
        }
        let props = &raw["props"];
        let kind = match props["kind"].as_str().unwrap_or("nextPage") {
            "continuous" => SectionStart::Continuous,
            "evenPage" => SectionStart::EvenPage,
            "oddPage" => SectionStart::OddPage,
            "nextColumn" => SectionStart::NextColumn,
            _ => SectionStart::NextPage,
        };
        let (mut setup, mut fallback_fields) = section_setup(props, &doc["settings"]);
        if !valid_setup(setup) {
            diagnostics.push(format!(
                "section {} has invalid content geometry; using host A4 defaults",
                sections.len()
            ));
            setup = PageSetup::a4();
            fallback_fields = vec![
                "width", "height", "top", "right", "bottom", "left", "gutter",
            ];
        }
        if props["columns"]["num"].as_i64().unwrap_or(1) > 1
            || props["columns"]["col"]
                .as_array()
                .is_some_and(|v| v.len() > 1)
        {
            diagnostics.push(format!(
                "section {}: multiple columns are not yet laid out",
                sections.len()
            ));
        }
        if kind == SectionStart::NextColumn {
            diagnostics.push(format!(
                "section {}: nextColumn is retained as continuous until column flow is implemented",
                sections.len()
            ));
        }
        sections.push(LayoutSection {
            block_range: start..end,
            para_range: prefix[start]..prefix[end],
            setup,
            kind,
            fallback_fields,
        });
        covered = end;
    }
    if sections.is_empty() || covered != main.len() {
        if !sections.is_empty() {
            diagnostics.push(
                "section ranges do not cover the main story; using host fallback geometry".into(),
            );
        }
        let (setup, fallback_fields) = section_setup(&Value::Null, &Value::Null);
        sections = vec![LayoutSection {
            block_range: 0..main.len(),
            para_range: 0..paras.len(),
            setup,
            kind: SectionStart::NextPage,
            fallback_fields,
        }];
    }
    if doc["settings"]["mirrorMargins"] == true {
        diagnostics
            .push("mirrorMargins: alternating inner/outer margins are not yet implemented".into());
    }
    for pair in sections.windows(2) {
        if matches!(
            pair[1].kind,
            SectionStart::Continuous | SectionStart::NextColumn
        ) && pair[0].setup != pair[1].setup
        {
            diagnostics.push(format!("section at block {} changes continuous geometry: current page retained; new geometry starts on the next page", pair[1].block_range.start));
        }
    }
    // The page formatter handles section starts separately; keep only the explicit
    // paragraph property here, including when a section begins with a skipped block.
    for (para, block) in paras
        .iter_mut()
        .zip(main.iter().filter(|b| b["kind"] == "text"))
    {
        para.page_break_before = block["props"]["pageBreakBefore"].as_bool().unwrap_or(false);
    }
    LayoutDocument {
        paras,
        sections,
        compatibility: DocumentCompatibility::default(),
        skipped_blocks,
        diagnostics,
        overrides: PageOverrides::default(),
    }
}
