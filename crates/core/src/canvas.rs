//! 输出后端抽象。
//!
//! 布局的产物是「什么东西画在哪」，落成 PDF / SVG / HTML / 测试快照是后端的事。
//! 接口刻意做成**绝对定位的绘图**，不含盒子、glue、断行等布局概念——布局在上游已经做完。
//!
//! 这与 dvipdfmx 的 `pdfdev.h` 是同一形状（`pdf_dev_set_string` / `set_rule` /
//! `put_image` / `bop` / `eop`），所以接一个 PDF 后端是直译，不需要再抽一层。
//!
//! 度量**不在**这里：见 `measure` 模块的说明，所有后端必须共用同一份度量。

use crate::geom::{Rect, Twips};
use crate::measure::FontSpec;

/// RGB 颜色，不带 alpha（OOXML 的透明走单独的效果属性）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    pub fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b }
    }
}

/// 一次文字绘制。
#[derive(Debug, Clone)]
pub struct TextRun<'a> {
    /// 基线左端。y 是**基线**位置，不是行顶——后端不需要知道 ascent。
    pub x: Twips,
    pub baseline_y: Twips,
    pub text: &'a str,
    pub font: &'a FontSpec,
    pub color: Color,
}

/// 后端。
///
/// 一次布局产出的所有绘制调用按页分组：每页 `begin_page` → 若干绘制 → `end_page`。
pub trait Canvas {
    type Error;

    /// 开新页。`size` 是页面物理尺寸（含页边距区域）。
    fn begin_page(&mut self, size: crate::geom::Size) -> Result<(), Self::Error>;

    fn end_page(&mut self) -> Result<(), Self::Error>;

    fn draw_text(&mut self, run: &TextRun<'_>) -> Result<(), Self::Error>;

    /// 实心矩形：用于表格底纹、段落边框、下划线、删除线。
    fn fill_rect(&mut self, rect: Rect, color: Color) -> Result<(), Self::Error>;

    /// 图片。`id` 是 `rsword` 媒体句柄，后端自行解析取字节。
    fn draw_image(&mut self, id: &str, rect: Rect) -> Result<(), Self::Error>;

    /// 全部页面画完。
    fn finish(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
