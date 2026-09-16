//! 几何基元。
//!
//! 单位统一用 twips（1/1440 英寸），与 OOXML 的 `w:pgSz` / `w:ind` / `w:tblW` 同单位，
//! 避免在布局主干里反复换算。字号是半点（`w:sz`），行内度量换算见 `measure`。

/// 长度，twips。
pub type Twips = i32;

/// 每英寸 twips 数。
pub const TWIPS_PER_INCH: Twips = 1440;
/// 每点 twips 数。
pub const TWIPS_PER_POINT: Twips = 20;

/// 半点（`w:sz` 的单位）转 twips。
pub fn half_points_to_twips(half_points: u32) -> Twips {
    // 半点 → 点 → twips；用 i64 中转避免大字号溢出。
    ((i64::from(half_points) * i64::from(TWIPS_PER_POINT)) / 2) as Twips
}

/// 点转 twips。
pub fn points_to_twips(points: f64) -> Twips {
    (points * f64::from(TWIPS_PER_POINT)).round() as Twips
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Point {
    pub x: Twips,
    pub y: Twips,
}

impl Point {
    pub fn new(x: Twips, y: Twips) -> Point {
        Point { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Size {
    pub width: Twips,
    pub height: Twips,
}

impl Size {
    pub fn new(width: Twips, height: Twips) -> Size {
        Size { width, height }
    }
}

/// 矩形，左上角原点，y 向下增长（与 PDF 相反，落盘时由后端翻转）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Rect {
    pub x: Twips,
    pub y: Twips,
    pub width: Twips,
    pub height: Twips,
}

impl Rect {
    pub fn new(x: Twips, y: Twips, width: Twips, height: Twips) -> Rect {
        Rect { x, y, width, height }
    }

    pub fn right(&self) -> Twips {
        self.x + self.width
    }

    pub fn bottom(&self) -> Twips {
        self.y + self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// 相交部分；不相交时宽或高为 0。
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Rect::new(x, y, (right - x).max(0), (bottom - y).max(0))
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        !self.intersect(other).is_empty()
    }
}

/// 四边边距（页边距、单元格内边距）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Margins {
    pub top: Twips,
    pub right: Twips,
    pub bottom: Twips,
    pub left: Twips,
}

impl Margins {
    pub fn new(top: Twips, right: Twips, bottom: Twips, left: Twips) -> Margins {
        Margins { top, right, bottom, left }
    }

    pub fn uniform(v: Twips) -> Margins {
        Margins { top: v, right: v, bottom: v, left: v }
    }

    /// 从矩形内缩；内缩后为负则收敛到 0。
    pub fn shrink(&self, r: &Rect) -> Rect {
        Rect::new(
            r.x + self.left,
            r.y + self.top,
            (r.width - self.left - self.right).max(0),
            (r.height - self.top - self.bottom).max(0),
        )
    }
}
