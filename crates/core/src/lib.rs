//! `rsword-layout-core`：DOCX 布局层的核心。
//!
//! 只做一件事：**回答文档如何占据二维空间**。不解析 docx（归 `rsword`），
//! 不栅格化（归 `rsword-layout-gpu`），不输出具体格式（归 `rsword-layout-svg` 等）。
//!
//! 模块一律私有，对外只经 `pub use` 暴露具体项——这样内部怎么分文件与调用方无关，
//! 重新组织文件不会破坏下游。

mod anchor;
mod bridge;
mod document;
mod grid;
mod layout;
mod load;
pub mod font;
mod oracle;
mod oracle_json;
mod paragraph_mark;
mod session;
mod table;

// ---- 几何：坐标一律 twips，没有像素 ----
pub use layout::{
    Margins, Point, Rect, Size, TWIPS_PER_INCH, TWIPS_PER_POINT, Transform, Twips,
    emu_to_twips, half_points_to_twips, points_to_twips, EMU_PER_INCH,
};

// ---- 矢量画布：路径是唯一原语 ----
pub use layout::{
    Color, DrawCmd, FillRule, LineCap, LineJoin, Paint, PaintOp, Path, PathSeg, PositionedGlyph,
    Stroke, VectorCanvas,
};

// ---- 文字环绕：与绘制共用 Path ----
pub use layout::{Span, WrapContext, WrapRegion, WrapSide};

// ---- 布局引擎 ----
pub use layout::{
    Align, Engine, Fragment, Line, LinePlacement, LineRule, Page, PageSetup, Para, PlaceholderKind, Platform, Run,
    TableCellBox, TableRowBox, TabAlign, TabLeader, TabStop, TextFragment, SpacingEvent, View,
};

// ---- 绘制指令：布局产物 → 画布 ----
pub use layout::{
    FaceId, OBJECT_PLACEHOLDER, PaintList, PaintPage, ShapedRun, TextShaper, paint_document,
    paint_page,
};

// ---- 比较器输入契约：与真实 Word 逐字形比对 ----
pub use oracle::{
    CompareState, GlyphRecord, LayoutRecord, LineRecord, LineTerminator, MismatchLevel,
    PageBreakPosition, PageRecord, SourceRange,
};
pub use oracle_json::{TraceMeta, to_trace_json};

// ---- 度量契约 ----
pub use font::{
    BreakOpportunity, Caps, FontHint, FontMetrics, FontSlots, FontSpec, SimpleMetrics, SlotKind,
    LineFontMetrics, MeasuredFontSpan, SpacingAdvance, TextMetrics,
};

// ---- rsword 桥接 ----
pub use anchor::AnchorScan;
pub use bridge::paras_from_document;
pub use grid::{DocumentGrid, GridKind};
pub use paragraph_mark::ParagraphMarkProperties;
pub use table::{CellMargins, LayoutTable, LayoutTableCell, LayoutTableRow, TableWidth};
pub use document::{
    ColumnLayout, ColumnSpec, DocumentCompatibility, LayoutDocument, LayoutSection, PageOverrides, SectionStart,
    document_from_json,
};
pub use load::{LoadedDocument, load_document, merge_sibling_run_props};

// ---- 文档会话：各入口共用的装载 → 排版 → 绘制 ----
pub use session::{
    AnchorReport, DiagnosticCode, DocumentSession, LayoutOptions, PreparedDocument,
    SessionDiagnostic, SessionError, SessionErrorKind, WrapPolicy,
};
#[cfg(feature = "fontenv")]
pub use session::FontSources;

// ---- 近似度量桩，**不可用于真实排版** ----

// ---- rustybuzz 整形 ----
#[cfg(feature = "shape")]
pub use font::RustybuzzShaper;

// ---- 真度量：读字体文件（feature `fontenv`）----
#[cfg(feature = "fontenv")]
pub use font::{HorizontalGrid, RealMetrics, VerticalGrid};
