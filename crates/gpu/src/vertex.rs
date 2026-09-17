//! 顶点格式（feature `gpu`）。
//!
//! 只有一种顶点，文字和矩形共用：矩形把 UV 设成一个约定的「纯色」纹素，
//! 着色器因此不需要分支，一次 draw call 就能混画文字与色块。

use rsword_layout_core::geom::{TWIPS_PER_INCH, Twips};

/// twips → 像素。twips 是 1/1440 英寸，所以 px = twips / 1440 * dpi。
pub fn px_from_twips(v: Twips, dpi: f32) -> f32 {
    (v as f32) * dpi / (TWIPS_PER_INCH as f32)
}

/// 一个顶点：位置（像素）+ UV + 颜色（归一化 RGBA）。
///
/// 32 字节，4 字节对齐，无填充洞——可整块 memcpy 进 GPU 缓冲。
/// 布局由 `tests/repr_c.rs` 钉死，改字段会让测试失败。
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex {
    /// 像素坐标，左上原点、y 向下（与布局坐标系一致）。
    pub x: f32,
    pub y: f32,
    /// 图集 UV。纯色批次用 [`SOLID_UV`]。
    pub u: f32,
    pub v: f32,
    /// 归一化 RGBA。
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// 纯色顶点约定取图集左上角那一个纹素（应为不透明白），
/// 这样文字与色块能共用同一个着色器与同一次 draw call。
pub const SOLID_UV: (f32, f32) = (0.0, 0.0);

impl Vertex {
    pub fn new(x: f32, y: f32, u: f32, v: f32, color: rsword_layout_core::canvas::Color, alpha: f32) -> Vertex {
        Vertex {
            x,
            y,
            u,
            v,
            r: f32::from(color.r) / 255.0,
            g: f32::from(color.g) / 255.0,
            b: f32::from(color.b) / 255.0,
            a: alpha,
        }
    }

    /// 顶点属性在结构体里的字节偏移，供 `glVertexAttribPointer` /
    /// `VkVertexInputAttributeDescription` 使用。
    pub const OFFSET_POS: usize = 0;
    pub const OFFSET_UV: usize = 8;
    pub const OFFSET_COLOR: usize = 16;
    pub const STRIDE: usize = 32;
}
