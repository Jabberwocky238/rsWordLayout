//! GPU 光栅化后端的公共层。
//!
//! core 的产物 [`rsword_layout_core::PaintList`] 是**矢量**的：坐标是 twips，
//! 字形是字体内的编号，与分辨率无关。本 crate 负责把它变成像素——顶点、图集、纹理，
//! 这些都是设备空间的概念，不该出现在 core 里。
//!
//! ```text
//! PaintList (twips, 矢量)
//!        │  + Viewport (dpi)        ← 分辨率只在这里进入
//!        ▼
//! Frame { vertices, indices, batches }  (像素)
//!        │
//!   ┌────┴────┬─────────┐
//!   ▼         ▼         ▼
//! vulkan   opengl    webgl
//! ```
//!
//! 这个分层是照 dvipdfmx 的 `pdfdev.h` 定的：那里坐标一律在 user space，
//! device space 的换算系数 `unit_conv` 在设备初始化时设一次。
//!
//! SVG 与 PDF 后端**不经过本 crate**——它们能直接输出矢量，栅格化纯属多余。

pub mod atlas;
pub mod batch;
pub mod vertex;

#[cfg(feature = "raster")]
pub mod raster;

pub use atlas::{
    AtlasSource, DirtyRect, GlyphAtlas, GlyphKey, GlyphMetrics, RasterGlyph, Rasterizer,
};
pub use batch::{Batch, BatchKind, Frame, FrameBuilder};
pub use vertex::{Vertex, px_from_twips};

#[cfg(feature = "raster")]
pub use raster::SkrifaRasterizer;

use rsword_layout_core::{Rect, Twips};

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

/// 字形来源：查一个字形在图集里的位置，必要时按需填充。
pub trait GlyphSource {
    fn glyph(&self, key: &GlyphKey) -> Option<GlyphQuad>;
}

/// 渲染目标的像素参数。
///
/// **分辨率只从这里进入管线**。core 不知道它的存在。
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Viewport {
    pub width_px: f32,
    pub height_px: f32,
    /// 每英寸像素数。96 = CSS 像素，144 = 1.5x，192 = 2x HiDPI。
    pub dpi: f32,
}

impl Viewport {
    /// 按 DPI 从页面尺寸（twips）算出视口。
    pub fn from_page(width: Twips, height: Twips, dpi: f32) -> Viewport {
        Viewport {
            width_px: px_from_twips(width, dpi),
            height_px: px_from_twips(height, dpi),
            dpi,
        }
    }

    /// 正交投影矩阵：像素坐标（左上原点、y 向下）→ NDC。列主序。
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

/// 矩形转像素。
pub(crate) fn rect_px(r: Rect, dpi: f32) -> [f32; 4] {
    [
        px_from_twips(r.x, dpi),
        px_from_twips(r.y, dpi),
        px_from_twips(r.width, dpi),
        px_from_twips(r.height, dpi),
    ]
}

/// twips → 像素。
pub fn to_px(v: Twips, dpi: f32) -> f32 {
    px_from_twips(v, dpi)
}

/// 把一页绘制指令转成 GPU 数据。
///
/// 与旧的 `core::gpu::build_page` 的关键差别：输入是矢量的 [`PaintPage`]，
/// DPI 由 `viewport` 带入，core 侧不再有任何分辨率概念。
pub fn build_page(
    page: &rsword_layout_core::PaintPage,
    vp: &Viewport,
    glyphs: Option<&dyn GlyphSource>,
) -> Frame {
    use rsword_layout_core::{DrawCmd, PaintOp};

    let mut b = FrameBuilder::new(vp.dpi);
    for cmd in &page.cmds {
        match cmd {
            DrawCmd::DrawPath { path, op } => {
                // 路径的通用光栅化（曲线细分 + 扫描线填充）尚未实现；
                // 目前只处理矩形这一特例，其余跳过而不是画错。
                if let Some(rect) = path_as_rect(path) {
                    let color = match op {
                        PaintOp::Fill { paint, .. } => paint.color,
                        PaintOp::Stroke { paint, .. } => paint.color,
                        PaintOp::FillThenStroke { fill, .. } => fill.color,
                    };
                    b.push_rect(rect, color);
                }
            }
            DrawCmd::DrawImage { id, rect } => b.push_image(id, *rect),
            DrawCmd::DrawGlyphs { glyphs: gs, paint, font, .. } => {
                b.push_glyphs(gs, paint.color, font, glyphs);
            }
            // 图形状态与裁剪需要矩阵栈与模板缓冲，尚未实现。
            DrawCmd::Save | DrawCmd::Restore | DrawCmd::Transform(_) | DrawCmd::Clip { .. } => {}
        }
    }
    b.finish()
}

/// 认出「一个矩形」这个特例。
///
/// 通用路径光栅化未实现，而底纹/边框/下划线都是矩形，先把它们认出来。
/// 形状是 `MoveTo` + 三条 `LineTo` + `Close`，四角轴对齐。
fn path_as_rect(p: &rsword_layout_core::Path) -> Option<rsword_layout_core::Rect> {
    use rsword_layout_core::PathSeg;
    use rsword_layout_core::Rect;

    let mut pts: Vec<(i32, i32)> = Vec::new();
    for seg in &p.segs {
        match *seg {
            PathSeg::MoveTo { x, y } | PathSeg::LineTo { x, y } => pts.push((x, y)),
            PathSeg::Close => {}
            PathSeg::CurveTo { .. } => return None,
        }
    }
    if pts.len() != 4 {
        return None;
    }
    let xs: Vec<i32> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<i32> = pts.iter().map(|p| p.1).collect();
    let (x0, x1) = (*xs.iter().min()?, *xs.iter().max()?);
    let (y0, y1) = (*ys.iter().min()?, *ys.iter().max()?);
    // 轴对齐检查：每个点的坐标都必须落在两个极值上。
    if !xs.iter().all(|&x| x == x0 || x == x1) || !ys.iter().all(|&y| y == y0 || y == y1) {
        return None;
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// 整篇文档逐页转换。
pub fn build_document(
    list: &rsword_layout_core::PaintList,
    dpi: f32,
    glyphs: Option<&dyn GlyphSource>,
) -> Vec<(Frame, Viewport)> {
    list.pages
        .iter()
        .map(|p| {
            let vp = Viewport::from_page(p.width, p.height, dpi);
            (build_page(p, &vp, glyphs), vp)
        })
        .collect()
}
