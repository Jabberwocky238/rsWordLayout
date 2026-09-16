//! POD 类型的内存布局断言。
//!
//! 这些类型标了 `#[repr(C)]` 是为了将来直接喂给 GPU 后端（Vulkan / OpenGL 的顶点与
//! uniform 缓冲）与 C FFI。`repr(C)` 只保证字段顺序与 C 一致，**不保证**尺寸符合预期——
//! 加了字段、换了 `Twips` 的底层类型都会悄悄改变布局。这里把尺寸与偏移钉死，改动时会失败。

use std::mem::{align_of, offset_of, size_of};

use rsword_layout_core::canvas::Color;
use rsword_layout_core::geom::{Margins, Point, Rect, Size, Twips};
use rsword_layout_core::measure::{BreakOpportunity, TextMetrics};

#[test]
fn twips_is_i32() {
    assert_eq!(size_of::<Twips>(), 4);
    assert_eq!(align_of::<Twips>(), 4);
}

#[test]
fn point_layout() {
    assert_eq!(size_of::<Point>(), 8);
    assert_eq!(offset_of!(Point, x), 0);
    assert_eq!(offset_of!(Point, y), 4);
}

#[test]
fn size_layout() {
    assert_eq!(size_of::<Size>(), 8);
    assert_eq!(offset_of!(Size, width), 0);
    assert_eq!(offset_of!(Size, height), 4);
}

#[test]
fn rect_layout() {
    assert_eq!(size_of::<Rect>(), 16);
    assert_eq!(offset_of!(Rect, x), 0);
    assert_eq!(offset_of!(Rect, y), 4);
    assert_eq!(offset_of!(Rect, width), 8);
    assert_eq!(offset_of!(Rect, height), 12);
}

#[test]
fn margins_layout() {
    // 顺序与 CSS 一致：top / right / bottom / left。
    assert_eq!(size_of::<Margins>(), 16);
    assert_eq!(offset_of!(Margins, top), 0);
    assert_eq!(offset_of!(Margins, right), 4);
    assert_eq!(offset_of!(Margins, bottom), 8);
    assert_eq!(offset_of!(Margins, left), 12);
}

#[test]
fn color_is_three_bytes() {
    // 没有 alpha、没有填充：可以直接当 RGB8 顶点属性用。
    assert_eq!(size_of::<Color>(), 3);
    assert_eq!(align_of::<Color>(), 1);
    assert_eq!(offset_of!(Color, r), 0);
    assert_eq!(offset_of!(Color, g), 1);
    assert_eq!(offset_of!(Color, b), 2);
}

#[test]
fn text_metrics_layout() {
    assert_eq!(size_of::<TextMetrics>(), 16);
    assert_eq!(offset_of!(TextMetrics, advance), 0);
    assert_eq!(offset_of!(TextMetrics, ascent), 4);
    assert_eq!(offset_of!(TextMetrics, descent), 8);
    assert_eq!(offset_of!(TextMetrics, line_gap), 12);
}

#[test]
fn break_opportunity_layout() {
    // usize + bool：有尾部填充，断言尺寸只是为了改动时有感知。
    assert_eq!(offset_of!(BreakOpportunity, offset), 0);
    assert_eq!(size_of::<BreakOpportunity>(), size_of::<usize>() * 2);
}

/// 数组连续、无洞——GPU 缓冲按 stride 读取的前提。
#[test]
fn arrays_are_tightly_packed() {
    let pts = [Point::new(1, 2), Point::new(3, 4)];
    let base = pts.as_ptr() as usize;
    let second = &pts[1] as *const Point as usize;
    assert_eq!(second - base, size_of::<Point>());

    let cols = [Color::rgb(1, 2, 3), Color::rgb(4, 5, 6)];
    let cbase = cols.as_ptr() as usize;
    let csecond = &cols[1] as *const Color as usize;
    assert_eq!(csecond - cbase, 3, "Color 数组必须紧密排列，不能有填充");
}
