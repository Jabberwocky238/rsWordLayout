//! `rsword-layout`：DOCX 布局层。
//!
//! `rsword` 负责解析与有效属性求值（它的冻结架构明确不做布局，见其 `docs/03` §1.2），
//! 本 crate 接在它的 `resolve` 输出之后，回答它不回答的问题：
//! **一行放得下几个字、一页放得下几行、每个字形落在哪一页的哪个坐标。**
//!
//! ```text
//! rsword::resolve                本 crate                  后端
//!   EffectiveParaProps  ──┐
//!   EffectiveRunProps   ──┤
//!   EffectiveSection    ──┼──►  布局引擎  ──►  LaidOutDocument  ──►  impl Canvas
//!   ColumnView          ──┤     (y 游标)       (已定位 Fragment)      PDF / SVG / 快照
//!   Numbering           ──┘
//!                            ▲
//!                            └── impl FontMetrics（HarfBuzz 等，由调用方提供）
//! ```
//!
//! 两条设计约束：
//!
//! 1. **度量与后端解耦**：度量走 [`measure::FontMetrics`]，所有后端共用同一份，
//!    否则同一文档在不同后端会分出不同页数。
//! 2. **布局产物是数据**：[`fragment::LaidOutDocument`] 是纯数据，能直接做快照回归，
//!    不必靠渲染结果截图比对。

pub mod bridge;
pub mod canvas;
pub mod engine;
pub mod fragment;
pub mod geom;
pub mod measure;
pub mod simple_metrics;
pub mod svg;

pub use canvas::{Canvas, Color};
pub use fragment::{Fragment, LaidOutDocument, Line, Page, TextFragment};
pub use geom::{Margins, Point, Rect, Size, Twips};
pub use engine::{Align, Engine, LineRule, Para, PageSetup, Run};
pub use measure::{BreakOpportunity, FontMetrics, FontSpec, TextMetrics};
pub use simple_metrics::SimpleMetrics;
pub use svg::SvgCanvas;
