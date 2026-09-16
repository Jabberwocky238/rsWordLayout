//! GPU 后端的公共层（feature `gpu`）。
//!
//! 三个图形后端（Vulkan / OpenGL / WebGL）的差别只在**如何把缓冲交给驱动**，
//! 而「布局产物 → 顶点」这一步完全相同。所以这一层做完所有与图形 API 无关的工作：
//!
//! ```text
//! LaidOutDocument ──► FrameBuilder ──► Frame { vertices, indices, batches }
//!                                        │
//!                  ┌─────────────────────┼─────────────────────┐
//!                  ▼                     ▼                     ▼
//!              vulkan::*            opengl::*             webgl::*
//!          (vkCmdDrawIndexed)   (glDrawElements)   (WebGl2 drawElements)
//! ```
//!
//! 设计要点：
//!
//! - **顶点是 `#[repr(C)]` 的 POD**，可以整块 memcpy 进 GPU 缓冲，布局由 `tests/repr_c.rs` 钉死。
//! - **坐标在 CPU 侧就转成像素**：布局用 twips（整数、分辨率无关），这里按 DPI 一次性换算成
//!   像素浮点。GPU 不该知道 twips 的存在。
//! - **文字不在这层栅格化**：字形光栅化需要字体后端，本层只产出「哪个字形、画在哪个矩形、
//!   取图集哪一块」的批次，图集由调用方提供（见 [`GlyphSource`]）。

use crate::fragment::{Fragment, LaidOutDocument};
use crate::geom::{Rect, Twips};

pub mod atlas;
pub mod batch;
#[cfg(feature = "raster")]
pub mod raster;
#[cfg(feature = "shape")]
pub mod shape;
pub mod vertex;

pub use atlas::{
    AtlasSource, DirtyRect, GlyphAtlas, GlyphKey, GlyphMetrics, RasterGlyph, Rasterizer, Shaper,
};
pub use batch::{Batch, BatchKind, Frame, FrameBuilder};
#[cfg(feature = "raster")]
pub use raster::SkrifaRasterizer;
#[cfg(feature = "shape")]
pub use shape::RustybuzzShaper;
pub use vertex::{Vertex, px_from_twips};

/// 一段文字 shaping 之后的一个字形。
///
/// 全 Unicode 下「一字符一字形」不成立（连字、阿拉伯语形态、印度语重排），
/// 所以绘制的单位是 shaper 产出的字形而不是 `char`。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    pub key: atlas::GlyphKey,
    /// 相对该段起点的笔位推进，像素。
    pub x_advance: f32,
    /// 相对基线的偏移，像素（组合符号定位用）。
    pub x_offset: f32,
    pub y_offset: f32,
}

/// 字形来源：把一段文字变成已定位的字形，并给出各自在图集中的位置。
///
/// shaping 与栅格化都由调用方负责（HarfBuzz / skrifa / 浏览器 Canvas2D），
/// 本 crate 不引入字体依赖——与 [`crate::measure::FontMetrics`] 的分工理由相同。
pub trait GlyphSource {
    /// 把一段同字体的文字 shape 成字形序列。
    fn shape(&self, text: &str, font: &crate::measure::FontSpec) -> Vec<ShapedGlyph>;

    /// 查一个字形的图集位置。返回 `None` 表示该字形缺失，后端应跳过而不是画错。
    fn glyph(&self, key: &atlas::GlyphKey) -> Option<GlyphQuad>;
}

/// 一个字形在图集中的位置与它相对基线的摆放。
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GlyphQuad {
    /// 图集 UV，归一化到 [0,1]。
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    /// 相对笔位（基线左端）的偏移，像素。
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// 渲染目标的像素参数。
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Viewport {
    pub width_px: f32,
    pub height_px: f32,
    /// 每英寸像素数。96 = CSS 像素，144 = 1.5x，192 = 2x HiDPI。
    pub dpi: f32,
}

impl Viewport {
    /// 按 DPI 从页面尺寸算出视口。
    pub fn from_page(size: crate::geom::Size, dpi: f32) -> Viewport {
        Viewport {
            width_px: px_from_twips(size.width, dpi),
            height_px: px_from_twips(size.height, dpi),
            dpi,
        }
    }

    /// 正交投影矩阵：像素坐标（左上原点、y 向下）→ NDC。
    ///
    /// 列主序，可直接作为 `mat4` uniform 上传；三个后端的 NDC 约定在 y 轴上一致
    /// （Vulkan 的 y 向下由这里的符号处理，见 `vulkan` 模块说明）。
    pub fn ortho(&self) -> [f32; 16] {
        let w = self.width_px.max(1.0);
        let h = self.height_px.max(1.0);
        [
            2.0 / w, 0.0, 0.0, 0.0,
            0.0, -2.0 / h, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            -1.0, 1.0, 0.0, 1.0,
        ]
    }
}

/// 把一页布局产物转成一帧 GPU 数据。
pub fn build_page(
    page: &crate::fragment::Page,
    dpi: f32,
    glyphs: Option<&dyn GlyphSource>,
) -> Frame {
    let mut b = FrameBuilder::new(dpi);
    for frag in &page.fragments {
        match frag {
            Fragment::Rect { rect, color } => b.push_rect(*rect, *color),
            Fragment::Text(t) => b.push_text(t, glyphs),
            Fragment::Image { id, rect } => b.push_image(id, *rect),
        }
    }
    b.finish()
}

/// 整篇文档逐页转换。
pub fn build_document(
    doc: &LaidOutDocument,
    dpi: f32,
    glyphs: Option<&dyn GlyphSource>,
) -> Vec<Frame> {
    doc.pages.iter().map(|p| build_page(p, dpi, glyphs)).collect()
}

/// 矩形转像素。
pub(crate) fn rect_px(r: Rect, dpi: f32) -> [f32; 4] {
    [
        px_from_twips(r.x, dpi),
        px_from_twips(r.y, dpi),
        px_from_twips(r.width, dpi),
        px_from_twips(r.height, dpi),
    ]
}

/// twips → 像素，供外部换算单个长度。
pub fn to_px(v: Twips, dpi: f32) -> f32 {
    px_from_twips(v, dpi)
}
