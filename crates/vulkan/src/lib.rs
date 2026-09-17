//! Vulkan 后端描述（feature `vulkan`）。
//!
//! 本模块**不链接 Vulkan**，也不引入 `ash` / `vulkano` 依赖——那会把图形栈的选择权
//! 从调用方手里拿走，也违反本 crate 的零图形依赖原则。它提供的是接 Vulkan 所需的
//! 全部**描述信息**：顶点输入描述、缓冲尺寸、绘制序列、以及 NDC 约定的差异说明。
//!
//! 调用方拿 [`rsword_layout_gpu::Frame`] 与这里的常量，几十行就能填完 `VkGraphicsPipelineCreateInfo`。
//!
//! # 与其他后端的唯一实质差异：NDC 的 y 轴
//!
//! OpenGL / WebGL 的 NDC y 轴向上，Vulkan 向下。[`Viewport::ortho`] 产出的矩阵
//! 已经把像素坐标（y 向下）映射为 y 向上的 NDC，所以 **Vulkan 需要额外翻转一次**：
//! 要么用负高度的 viewport（`VK_KHR_maintenance1`，推荐），要么用 [`ortho_vk`]。

use rsword_layout_gpu::Viewport;
use rsword_layout_gpu::vertex::Vertex;

/// `VkVertexInputBindingDescription` 的取值。
pub const BINDING: u32 = 0;
pub const STRIDE: u32 = Vertex::STRIDE as u32;

/// `VkVertexInputAttributeDescription`：(location, offset, 格式说明)。
///
/// 格式依次是 `R32G32_SFLOAT`（位置）、`R32G32_SFLOAT`（UV）、`R32G32B32A32_SFLOAT`（颜色）。
pub const ATTRIBUTES: [(u32, u32); 3] = [
    (0, Vertex::OFFSET_POS as u32),
    (1, Vertex::OFFSET_UV as u32),
    (2, Vertex::OFFSET_COLOR as u32),
];

/// 索引类型是 `VK_INDEX_TYPE_UINT32`（`Frame::indices` 是 `u32`）。
pub const INDEX_TYPE_IS_U32: bool = true;

/// Vulkan 用的正交矩阵：在 [`Viewport::ortho`] 基础上翻转 y。
///
/// 若已经用 `VK_KHR_maintenance1` 的负高度 viewport，就该用 [`Viewport::ortho`]，
/// 不要再翻一次。
pub fn ortho_vk(vp: &Viewport) -> [f32; 16] {
    let mut m = vp.ortho();
    m[5] = -m[5];
    m[13] = -m[13];
    m
}
