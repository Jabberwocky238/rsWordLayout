//! Document geometry and paragraph ranges for the shared page formatter.

use std::ops::Range;

use serde_json::{Value, json};

use crate::{DocumentGrid, Margins, PageSetup, Para, Rect, Twips, paras_from_document};

// Pinned rsword schema/props/section.toml, CT_Columns: at most 45 columns.
const MAX_COLUMNS: usize = 45;

/// One explicit column; the last column's trailing gap does not consume body width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    pub width: Twips,
    pub gap_after: Twips,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum ColumnKind {
    #[default]
    Single,
    Equal { count: usize, gap: Twips },
    Explicit(Vec<ColumnSpec>),
}

/// Left-to-right column geometry, independent of the current page's vertical flow.
/// Raw DOCX input is retained for trace provenance, including invalid declarations
/// that fell back to one column. Equality compares effective geometry settings.
#[derive(Debug, Clone, Default)]
pub struct ColumnLayout {
    kind: ColumnKind,
    declared: Option<Value>,
}

impl PartialEq for ColumnLayout {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for ColumnLayout {}

impl ColumnLayout {
    pub fn equal(count: usize, gap: Twips) -> Result<Self, String> {
        validate_column_count(count)?;
        if gap < 0 {
            return Err("column gap must be nonnegative".into());
        }
        Ok(Self {
            kind: if count == 1 { ColumnKind::Single } else { ColumnKind::Equal { count, gap } },
            declared: None,
        })
    }

    pub fn explicit(mut columns: Vec<ColumnSpec>) -> Result<Self, String> {
        validate_column_count(columns.len())?;
        if columns.iter().any(|column| column.width <= 0 || column.gap_after < 0) {
            return Err("explicit column widths must be positive and gaps nonnegative".into());
        }
        columns.last_mut().expect("validated nonempty columns").gap_after = 0;
        Ok(Self { kind: ColumnKind::Explicit(columns), declared: None })
    }

    pub fn count(&self) -> usize {
        match &self.kind {
            ColumnKind::Single => 1,
            ColumnKind::Equal { count, .. } => *count,
            ColumnKind::Explicit(columns) => columns.len(),
        }
    }

    /// Derive body-height column rectangles. Equal-column remainder twips are
    /// distributed left to right; this host rounding policy is not Word-measured.
    /// Explicit widths are never rescaled to fill unused space on the right.
    pub fn areas(&self, body: Rect) -> Result<Vec<Rect>, String> {
        let right = i64::from(body.x) + i64::from(body.width);
        let bottom = i64::from(body.y) + i64::from(body.height);
        if body.width <= 0 || body.height <= 0
            || Twips::try_from(right).is_err() || Twips::try_from(bottom).is_err()
        {
            return Err("columns require a positive, representable body rectangle".into());
        }
        let specs = match &self.kind {
            ColumnKind::Single => return Ok(vec![body]),
            ColumnKind::Equal { count, gap } => {
                let count = *count as i64;
                let available = i64::from(body.width) - (count - 1) * i64::from(*gap);
                if available < count {
                    return Err("column gaps leave no positive width for every column".into());
                }
                let base = available / count;
                let remainder = available % count;
                (0..count).map(|index| ColumnSpec {
                    width: (base + i64::from(index < remainder)) as Twips,
                    gap_after: if index + 1 < count { *gap } else { 0 },
                }).collect::<Vec<_>>()
            }
            ColumnKind::Explicit(columns) => columns.clone(),
        };
        let mut x = i64::from(body.x);
        let mut areas = Vec::with_capacity(specs.len());
        for column in specs {
            if x + i64::from(column.width) > right {
                return Err("explicit column widths and gaps exceed the body width".into());
            }
            areas.push(Rect::new(x as Twips, body.y, column.width, body.height));
            x += i64::from(column.width) + i64::from(column.gap_after);
        }
        Ok(areas)
    }

    fn trace_metadata(&self, body: Rect) -> Value {
        let effective = match &self.kind {
            ColumnKind::Single => json!({"kind": "single"}),
            ColumnKind::Equal { count, gap } => json!({"kind": "equal", "count": count, "gap": gap}),
            ColumnKind::Explicit(columns) => json!({
                "kind": "explicit",
                "columns": columns.iter().map(|column| json!({
                    "width": column.width, "gapAfter": column.gap_after,
                })).collect::<Vec<_>>(),
            }),
        };
        let (areas, error) = match self.areas(body) {
            Ok(areas) => (areas, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let unused_right = areas.last().map(|last| body.right() - last.right());
        json!({
            "declared": self.declared,
            "effective": effective,
            "flow": "left-to-right sequential; continuous-section balancing not implemented",
            "rounding": "equal widths distribute remainder twips from left to right (host policy)",
            "areas": areas.iter().map(|area| json!({
                "x": area.x, "y": area.y, "width": area.width, "height": area.height,
            })).collect::<Vec<_>>(),
            "areaError": error,
            "unusedRightTwips": unused_right,
        })
    }
}

fn validate_column_count(count: usize) -> Result<(), String> {
    if !(1..=MAX_COLUMNS).contains(&count) {
        return Err(format!("column count must be between 1 and {MAX_COLUMNS}"));
    }
    Ok(())
}

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
    pub columns: ColumnLayout,
    /// Declared section grid; retained independently of geometry overrides.
    pub grid: DocumentGrid,
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
    /// `w:compat/w:noColumnBalance`, preserved separately from layout policy.
    pub no_column_balance: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LayoutDocument {
    pub paras: Vec<Para>,
    pub sections: Vec<LayoutSection>,
    pub compatibility: DocumentCompatibility,
    pub skipped_blocks: usize,
    pub diagnostics: Vec<String>,
    /// Original parser diagnostics, separate from layout diagnostics and unchanged.
    pub source_warnings: Vec<Value>,
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
                section.columns.areas(setup.content_area())?;
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
            "sourceWarnings": self.source_warnings,
            "grid": {
                "applied": false,
                "status": "inputs retained; grid layout and absent defaults are unresolved",
                "paragraphSnapToGrid": self.paras.iter().map(|para| para.snap_to_grid).collect::<Vec<_>>(),
            },
            "compatibility": {
                "splitPgBreakAndParaMark": self.compatibility.split_page_break_and_para_mark,
                "noColumnBalance": self.compatibility.no_column_balance,
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
                "columns": s.columns.trace_metadata(s.setup.content_area()),
                "grid": s.grid.trace_metadata(),
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

fn column_integer(value: &Value, field: &str, fallback: Option<Twips>) -> Result<Twips, String> {
    match value.get(field) {
        None => fallback.ok_or_else(|| format!("column {field} is missing")),
        Some(value) => value.as_i64().and_then(|n| Twips::try_from(n).ok())
            .ok_or_else(|| format!("column {field} is not a representable integer: {value}")),
    }
}

fn column_boolean(value: &Value, field: &str, fallback: bool) -> Result<bool, String> {
    match value.get(field) {
        None => Ok(fallback),
        Some(value) => value.as_bool()
            .ok_or_else(|| format!("column {field} is not a boolean: {value}")),
    }
}

fn section_columns(props: &Value, body: Rect, section: usize, diagnostics: &mut Vec<String>) -> ColumnLayout {
    let Some(raw) = props.get("columns") else { return ColumnLayout::default() };
    let parse = |notes: &mut Vec<String>| -> Result<ColumnLayout, String> {
        if !raw.is_object() {
            return Err("columns must be an object".into());
        }
        let equal = column_boolean(raw, "equalWidth", true)?;
        let separator = column_boolean(raw, "sep", false)?;
        let explicit = match raw.get("col") {
            None => &[][..],
            Some(value) => value.as_array().map(Vec::as_slice)
                .ok_or_else(|| "columns.col must be an array".to_owned())?,
        };
        let columns = if equal {
            if !explicit.is_empty() {
                notes.push("explicit column entries are ignored because equalWidth is true or absent".into());
            }
            let count = column_integer(raw, "num", Some(1))?;
            let count = usize::try_from(count).map_err(|_| "column count must be positive".to_owned())?;
            ColumnLayout::equal(count, column_integer(raw, "space", Some(720))?)?
        } else {
            validate_column_count(explicit.len())?;
            if let Some(count) = raw.get("num")
                && count.as_u64() != Some(explicit.len() as u64)
            {
                notes.push(format!("column num={count} ignored for explicit columns; using {} entries", explicit.len()));
            }
            let columns = explicit.iter().map(|entry| {
                if !entry.is_object() {
                    return Err("each explicit column must be an object".into());
                }
                Ok(ColumnSpec {
                    width: column_integer(entry, "w", None)?,
                    // Container space belongs to equal columns. An omitted
                    // explicit gap is zero; no gap follows the final column.
                    gap_after: column_integer(entry, "space", Some(0))?,
                })
            }).collect::<Result<Vec<_>, String>>()?;
            ColumnLayout::explicit(columns)?
        };
        let areas = columns.areas(body)?;
        if let ColumnKind::Explicit(_) = &columns.kind
            && let Some(last) = areas.last()
            && last.right() < body.right()
        {
            notes.push(format!("explicit columns leave {} twips unused in the declared body; widths are not rescaled by page overrides", body.right() - last.right()));
        }
        if separator {
            notes.push("column separator lines (sep) are not rendered".into());
        }
        if columns.count() > 1 {
            notes.push("sequential column flow: continuous-section column balancing is not implemented".into());
            if let Some(bidi) = props.get("bidi") {
                match bidi.as_bool() {
                    Some(true) => notes.push("right-to-left column order (bidi) is not implemented; using left-to-right order".into()),
                    Some(false) => {},
                    None => notes.push(format!("invalid column direction bidi={bidi}; using left-to-right order")),
                }
            }
            if let Some(direction) = props.get("textDirection")
                && direction != "lrTb"
            {
                notes.push(format!("column textDirection={direction} is not implemented; using horizontal left-to-right flow"));
            }
        }
        Ok(columns)
    };
    let mut notes = Vec::new();
    let mut columns = match parse(&mut notes) {
        Ok(columns) => columns,
        Err(error) => {
            notes.push(format!("invalid columns: {error}; using a single body column"));
            ColumnLayout::default()
        }
    };
    columns.declared = Some(raw.clone());
    diagnostics.extend(notes.into_iter().map(|note| format!("section {section}: {note}")));
    columns
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
        let columns = section_columns(props, setup.content_area(), sections.len(), &mut diagnostics);
        let grid = DocumentGrid::from_json(props.get("docGrid"));
        diagnostics.extend(grid.diagnostics().into_iter()
            .map(|note| format!("section {}: {note}", sections.len())));
        sections.push(LayoutSection {
            block_range: start..end,
            para_range: prefix[start]..prefix[end],
            setup,
            kind,
            columns,
            grid,
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
            columns: ColumnLayout::default(),
            grid: DocumentGrid::default(),
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
        if matches!(pair[1].kind, SectionStart::Continuous | SectionStart::NextColumn)
            && pair[0].columns != pair[1].columns
        {
            diagnostics.push(format!("section at block {} changes continuous columns: current page retains its columns; new columns start on the next page; balancing and mixed column regions are not implemented", pair[1].block_range.start));
        }
        if matches!(pair[1].kind, SectionStart::Continuous | SectionStart::NextColumn)
            && pair[0].grid != pair[1].grid
        {
            diagnostics.push(format!("section at block {} changes continuous docGrid declarations: each section input is retained; grid switch timing is unresolved and neither grid is applied", pair[1].block_range.start));
        }
    }
    // The page formatter handles section starts separately; keep only the explicit
    // paragraph property here, including when a section begins with a skipped block.
    for (para, (block_index, block)) in paras
        .iter_mut()
        .zip(main.iter().enumerate().filter(|(_, block)| block["kind"] == "text"))
    {
        para.page_break_before = block["props"]["pageBreakBefore"].as_bool().unwrap_or(false);
        if let Some(value) = block["props"].get("snapToGrid")
            && !value.is_boolean()
        {
            diagnostics.push(format!("block {block_index}: snapToGrid requires a boolean; cannot interpret {value}"));
        }
    }
    LayoutDocument {
        paras,
        sections,
        compatibility: DocumentCompatibility::default(),
        skipped_blocks,
        diagnostics,
        source_warnings: doc["warnings"].as_array().cloned().unwrap_or_default(),
        overrides: PageOverrides::default(),
    }
}
