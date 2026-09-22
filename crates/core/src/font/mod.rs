//! 字体：加载、选择、整形、栅格化。
//!
//! 与字体有关的一切集中在此，调用方不必知道内部怎么分文件。分层照 dvipdfmx
//! 的思路划——**契约与与分辨率无关的计算在这里，设备空间的缓存在后端**：
//!
//! | 这里 | 后端（`rsword-layout-gpu`） |
//! | --- | --- |
//! | `FontMetrics` / `TextShaper` / `Rasterizer` 契约 | 字形位图图集 |
//! | 整形（glyph id 与 twips 推进量，与 DPI 无关） | 图集的货架摆放与淘汰 |
//! | 栅格化实现（给定 DPI 产出覆盖率位图） | 纹理上传与脏区域管理 |
//!
//! 栅格化本身需要知道目标分辨率，但它是**无状态的纯函数**（同一 key 给同一结果），
//! 所以放在这里；有状态的缓存才属于后端。
//!
//! # feature
//!
//! 只做矢量输出的调用方（SVG / PDF 后端）不需要字体库，故按需 gated：
//!
//! - `shape` —— rustybuzz 整形
//! - `raster` —— skrifa 读轮廓 + zeno 栅格化
//! - `fontenv` —— 按码位选字体与 fallback（依赖 docx-layout）

mod spec;

pub use spec::{
    BreakOpportunity, FINE_PER_TWIP, FontHint, FontMetrics, FontSlots, FontSpec,
    OverflowPunctuationContext, SlotKind, TextMetrics,
};

mod simple;
mod linebreak;
pub use simple::SimpleMetrics;

#[cfg(feature = "shape")]
mod shape;
#[cfg(feature = "shape")]
pub use shape::RustybuzzShaper;

#[cfg(feature = "raster")]
mod raster;
#[cfg(feature = "raster")]
pub use raster::{
    FaceData, GlyphKey, GlyphMetrics, HintingMode, RasterFormat, RasterGlyph, Rasterizer,
    SkrifaRasterizer,
};

#[cfg(feature = "fontenv")]
mod registry;
#[cfg(feature = "fontenv")]
pub use registry::FontRegistry;

#[cfg(feature = "fontenv")]
mod real;
#[cfg(feature = "fontenv")]
pub use real::{RealMetrics, VerticalGrid};
