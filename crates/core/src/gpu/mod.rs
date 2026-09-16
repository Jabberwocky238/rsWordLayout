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
pub mod vertex;

pub use atlas::{AtlasSource, DirtyRect, GlyphAtlas, RasterGlyph, Rasterizer};
pub use batch::{Batch, BatchKind, Frame, FrameBuilder};
pub use vertex::{Vertex, px_from_twips};

/// 字形图集查询。
///
/// GPU 后端需要知道「这个字符用这个字体画出来，在图集的哪个 UV 区域、占多大」。
/// 光栅化与图集管理由调用方负责（通常接 FreeType / swash / fontdue），
/// 本 crate 不引入字体依赖——与 [`crate::measure::FontMetrics`] 的分工理由相同。
pub trait GlyphSource {
    /// 查一个字形的图集位置。返回 `None` 表示该字形缺失，后端应跳过而不是画错。
    fn glyph(&self, ch: char, font: &crate::measure::FontSpec) -> Option<GlyphQuad>;
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
