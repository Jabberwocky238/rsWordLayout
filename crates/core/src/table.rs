//! Main-story tables in the first supported shape.
//!
//! Only the `table32-tail.docx` shape is laid out: no table style, single-cell
//! rows, exact row heights, explicit zero cell margins, no visible borders and
//! paragraph-only cells. Anything else stays an omitted block with a diagnostic;
//! it is never approximated by a fitted height. Declared JSON is the input; the
//! pinned parser's table projection is not a resolved table layout.
//!
//! Source projection: each cell paragraph consumes its text plus one mark unit,
//! and each cell one further end unit. For single-cell, single-paragraph rows
//! this reproduces the `tablepage` print FormatLine starts 0, 35, 70; it is not
//! a measured rule for multiple cells, row marks or nested tables.

use serde_json::{Value, json};

use crate::bridge::{EffectiveProperties, project_cell_paragraphs};
use crate::layout::utf16_len;
use crate::{Para, Twips};

/// `w:tblW` in the supported units. Percentages are fiftieths of a percent of
/// the current flow region width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableWidth {
    Pct(u32),
    Dxa(Twips),
}

impl TableWidth {
    pub fn resolve(self, region_width: Twips) -> Twips {
        match self {
            Self::Pct(pct) => (i64::from(region_width) * i64::from(pct) / 5000) as Twips,
            Self::Dxa(width) => width,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutTableCell {
    pub source_node: Option<u32>,
    pub paras: Vec<Para>,
}

impl LayoutTableCell {
    /// Paragraph texts and marks plus the cell end unit.
    pub fn source_len(&self) -> u32 {
        self.paras
            .iter()
            .map(|para| para.runs.iter().map(|run| utf16_len(&run.text)).sum::<u32>() + 1)
            .sum::<u32>()
            + 1
    }
}

#[derive(Debug, Clone)]
pub struct LayoutTableRow {
    pub source_node: Option<u32>,
    /// Exact `w:trHeight`; content neither grows nor is clipped to it. `None`
    /// when the row declares no height: the row is as tall as its tallest cell's
    /// content (paragraph lines and spacing).
    pub height: Option<Twips>,
    /// Width of the visible border above this row (`top` for the first row,
    /// `insideH` after it), added to a content-height row. Zero for nil/none.
    pub border_above: Twips,
    pub cells: Vec<LayoutTableCell>,
}

/// Cell margins in twips. Undeclared sides take Word's table defaults: 108 at
/// the left and right, 0 at the top and bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellMargins {
    pub left: Twips,
    pub right: Twips,
    pub top: Twips,
    pub bottom: Twips,
}

#[derive(Debug, Clone)]
pub struct LayoutTable {
    /// Original main-story block index.
    pub block_index: usize,
    /// Number of main-story paragraphs before the table.
    pub before_para: usize,
    pub source_node: Option<u32>,
    pub width: TableWidth,
    pub margins: CellMargins,
    /// Whether any border is visible. Visible borders take vertical space
    /// (see [`LayoutTableRow::border_above`]) but are not painted.
    pub visible_borders: bool,
    pub rows: Vec<LayoutTableRow>,
}

impl LayoutTable {
    pub fn source_len(&self) -> u32 {
        self.rows.iter().flat_map(|row| &row.cells).map(LayoutTableCell::source_len).sum()
    }

    pub(crate) fn trace_metadata(&self) -> Value {
        json!({
            "blockIndex": self.block_index,
            "beforeParagraph": self.before_para,
            "sourceNode": self.source_node,
            "width": match self.width {
                TableWidth::Pct(pct) => json!({"type": "pct", "w": pct}),
                TableWidth::Dxa(width) => json!({"type": "dxa", "w": width}),
            },
            "rows": self.rows.iter().map(|row| json!({
                "sourceNode": row.source_node,
                "exactHeight": row.height,
                "borderAbove": row.border_above,
                "heightRule": if row.height.is_some() { "exact" } else { "content" },
                "cells": row.cells.iter().map(|cell| json!({
                    "sourceNode": cell.source_node,
                    "paragraphs": cell.paras.len(),
                    "sourceLength": cell.source_len(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

pub(crate) const TABLE_POLICY: &str = "supported shape only: no table style, single-cell rows, exact or undeclared (content-height) row heights, cell margins (undeclared: 108 left/right, 0 top/bottom), nil/none borders or single borders on content-height rows (border width adds to the row; borders are not painted), tblW auto from a dxa tcW, paragraph-only cells; table width from tblW (pct of the flow region or dxa) at the region's left edge; rows do not split and move whole to the next region; cell content taller than the row is painted unclipped and reported as overflow";
pub(crate) const SOURCE_POLICY: &str = "each cell paragraph consumes text plus one mark unit and each cell one end unit; matches the tablepage print FormatLine starts only for single-cell single-paragraph rows";

fn node(value: &Value) -> Option<u32> {
    value.get("node").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok())
}

fn keys_within(value: &Value, what: &str, allowed: &[&str]) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return if value.is_null() { Ok(()) } else { Err(format!("{what} is not an object")) };
    };
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(format!("{what} declares unsupported `{key}`")),
        None => Ok(()),
    }
}

fn no_revisions(value: &Value, what: &str) -> Result<(), String> {
    match value.get("revisions") {
        None => Ok(()),
        Some(Value::Array(items)) if items.is_empty() => Ok(()),
        Some(_) => Err(format!("{what} carries revisions")),
    }
}

fn number(value: &Value) -> Option<i64> {
    value.get("w").and_then(|w| w.get("number")).and_then(Value::as_i64)
}

fn table_width(props: &Value, first_cell: &Value) -> Result<TableWidth, String> {
    let width = &props["width"];
    // `tblW auto` with a fixed-width cell: the single cell's `tcW` sets the width.
    if width["kind"] == "auto" {
        let cell = &first_cell["props"]["width"];
        return match (cell["kind"].as_str(), number(cell)) {
            (Some("dxa"), Some(w)) if w > 0 => Ok(TableWidth::Dxa(w as Twips)),
            _ => Err("tblW auto without a positive dxa tcW needs content sizing".into()),
        };
    }
    let value = number(width).ok_or("tblW is missing or not numeric (auto width needs content sizing)")?;
    match width["kind"].as_str() {
        Some("pct") => u32::try_from(value).ok().filter(|v| *v > 0).map(TableWidth::Pct)
            .ok_or_else(|| format!("tblW pct {value} is not positive")),
        Some("dxa") => Twips::try_from(value).ok().filter(|v| *v > 0).map(TableWidth::Dxa)
            .ok_or_else(|| format!("tblW dxa {value} is not positive")),
        other => Err(format!("tblW type {other:?} is not supported")),
    }
}

/// Undeclared sides take TableNormal's built-in margins (108 left/right, 0
/// top/bottom). `talltable-080` (no `tblCellMar`) and its copy with all four
/// margins written as zero lay out the same number of rows per page.
fn read_margins(props: &Value) -> Result<CellMargins, String> {
    let margins = &props["cellMargins"];
    let side = |sides: &[&str], default: Twips| -> Result<Twips, String> {
        match sides.iter().find_map(|side| margins.get(*side)) {
            None => Ok(default),
            Some(margin) if margin["kind"] == "dxa" => number(margin)
                .and_then(|v| Twips::try_from(v).ok()).filter(|v| *v >= 0)
                .ok_or_else(|| format!("cell margin {} is not a nonnegative dxa", sides[0])),
            Some(_) => Err(format!("cell margin {} is not dxa", sides[0])),
        }
    };
    Ok(CellMargins {
        left: side(&["start", "left"], 108)?,
        right: side(&["end", "right"], 108)?,
        top: side(&["top"], 0)?,
        bottom: side(&["bottom"], 0)?,
    })
}

/// Visible borders: only `single` is measured. Returns the widths in twips of
/// the `top` and `insideH` borders (`sz` is in eighths of a point) and whether
/// any border is visible.
fn read_borders(props: &Value) -> Result<(Twips, Twips, bool), String> {
    let Some(borders) = props.get("borders") else { return Ok((0, 0, false)) };
    let Some(borders) = borders.as_object() else { return Err("borders is not an object".into()) };
    let mut visible = false;
    let mut width = |side: &str| -> Result<Twips, String> {
        let Some(border) = borders.get(side) else { return Ok(0) };
        match border["val"].as_str() {
            Some("nil" | "none") => Ok(0),
            Some("single") => {
                visible = true;
                let sz = border["sz"].as_i64().unwrap_or(2).clamp(2, 96);
                Ok((sz * 20 / 8) as Twips)
            }
            other => Err(format!("{side} border {other:?} is not supported")),
        }
    };
    let top = width("top")?;
    let inside = width("insideH")?;
    for side in ["bottom", "start", "left", "end", "right", "insideV"] {
        width(side)?;
    }
    Ok((top, inside, visible))
}

fn project(
    doc: &Value,
    block: &Value,
    effective: Option<&EffectiveProperties>,
) -> Result<(TableWidth, CellMargins, bool, Vec<LayoutTableRow>), String> {
    keys_within(block, "table", &["grid", "kind", "node", "props", "revisions", "rows"])?;
    no_revisions(block, "table")?;
    let props = &block["props"];
    keys_within(props, "tblPr", &["width", "borders", "cellMargins"])?;
    let rows = block["rows"].as_array().filter(|rows| !rows.is_empty())
        .ok_or("table has no rows")?;
    let width = table_width(props, &rows[0]["cells"][0])?;
    let margins = read_margins(props)?;
    let (border_top, border_inside, visible_borders) = read_borders(props)?;
    rows.iter().enumerate().map(|(ri, row)| {
        let what = format!("row {ri}");
        keys_within(row, &what, &["cells", "node", "props", "revisions"])?;
        no_revisions(row, &what)?;
        keys_within(&row["props"], &format!("{what} trPr"), &["height"])?;
        let height = &row["props"]["height"];
        // 没写 `w:trHeight` 的行随内容高。Android 打印视图实测（`findings/pagination-path.md`）：
        // 每格一段精确 480 的单列表，24、28、30、31 行一页，32 行两页（31 × 480 加表后补的空段放得下，
        // 32 × 480 = 15360 再加空段放不下）。`atLeast` / `auto` 带值的行没有读数，照旧不排。
        let height = if height.is_null() {
            None
        } else {
            if height["hRule"] != "exact" {
                return Err(format!("{what} height rule is not exact"));
            }
            Some(height["val"].as_i64().and_then(|v| Twips::try_from(v).ok()).filter(|v| *v > 0)
                .ok_or_else(|| format!("{what} exact height is not positive"))?)
        };
        let cells = row["cells"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        if cells.len() != 1 {
            return Err(format!("{what} has {} cells; only single-cell rows are supported", cells.len()));
        }
        let cells = cells.iter().map(|cell| {
            keys_within(cell, &format!("{what} cell"), &["blocks", "node", "props", "revisions"])?;
            no_revisions(cell, &format!("{what} cell"))?;
            // With one cell per row the table width governs; tcW is not used.
            keys_within(&cell["props"], &format!("{what} tcPr"), &["width"])?;
            let blocks = cell["blocks"].as_array().filter(|blocks| !blocks.is_empty())
                .ok_or_else(|| format!("{what} cell has no blocks"))?;
            let paras = project_cell_paragraphs(doc, blocks, effective)
                .ok_or_else(|| format!("{what} cell contains a non-paragraph block"))?;
            Ok(LayoutTableCell { source_node: node(cell), paras })
        }).collect::<Result<Vec<_>, String>>()?;
        // 可见边框只在随内容高的行上有读数（`talltable-080` 一组）；写了行高的行边框占不占行高没测。
        if visible_borders && height.is_some() {
            return Err(format!("{what} has an exact height and visible borders"));
        }
        let border_above = if ri == 0 { border_top } else { border_inside };
        Ok(LayoutTableRow { source_node: node(row), height, border_above, cells })
    }).collect::<Result<Vec<_>, String>>().map(|rows| (width, margins, visible_borders, rows))
}

/// Project one main-story table block, or explain why it stays omitted.
pub(crate) fn project_table(
    doc: &Value,
    block: &Value,
    block_index: usize,
    before_para: usize,
    effective: Option<&EffectiveProperties>,
) -> Result<LayoutTable, String> {
    if let Some(style) = block.get("styleId") {
        return Err(format!("table style {style} is not applied"));
    }
    let (width, margins, visible_borders, rows) = project(doc, block, effective)?;
    Ok(LayoutTable { block_index, before_para, source_node: node(block), width, margins, visible_borders, rows })
}
