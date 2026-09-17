//! 布局层的核心：坐标系、矢量画布、布局引擎、绘制指令。
//!
//! 五个原本分立的模块合并在此，因为它们是**同一套词汇**：坐标（twips）→ 路径 →
//! 绘制指令 → 布局产物。拆开只是让读者在文件间跳转，并不增加边界清晰度。
//!
//! 文件内按依赖顺序分节，节与节之间是真正的契约边界：
//!
//! | 节 | 内容 | 边界 |
//! | --- | --- | --- |
//! | 1 几何 | `Twips` / `Rect` / `Transform` | 单位一律 twips，无像素 |
//! | 2 画布 | `Path` / `DrawCmd` / `VectorCanvas` | 所有绘制归约到路径 |
//! | 3 环绕 | `WrapRegion` / `WrapContext` | 与绘制共用 `Path` |
//! | 4 引擎 | `Engine` / `Para` / `Page` | y 游标断行分页 |
//! | 5 指令 | `PaintList` / `paint_document` | 布局产物 → 画布指令 |
//!
//! **全层没有分辨率概念。** 栅格化属于后端（见 `rsword-layout-gpu`），
//! 这条线照 dvipdfmx 的 `pdfdev.h` 划：那里坐标在 user space，
//! device space 的换算系数在设备初始化时设一次。

use crate::font::{FINE_PER_TWIP, FontMetrics, FontSpec};


// ==========================================================================
// 1. 几何：坐标、尺寸、变换
// ==========================================================================

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

/// 每英寸 EMU 数（English Metric Units，OOXML 里绘图尺寸的单位）。
pub const EMU_PER_INCH: i64 = 914_400;

/// EMU 转 twips。1 英寸 = 914400 EMU = 1440 twips，故除以 635。
///
/// rsword 的 `Extent` 与 `Dist` 都以 EMU 计，接环绕时必须换算。
pub fn emu_to_twips(emu: i64) -> Twips {
    (emu / (EMU_PER_INCH / i64::from(TWIPS_PER_INCH))) as Twips
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

// ==========================================================================
// 2. 矢量画布：路径是唯一原语
// ==========================================================================

/// 颜色，RGB 不带 alpha（透明度走 [`Paint::alpha`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };

    pub fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b }
    }
}

/// 2×3 仿射变换，与 PDF / PostScript / SVG 的约定一致。
///
/// ```text
/// | a  b  0 |
/// | c  d  0 |
/// | e  f  1 |
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Transform {
    fn default() -> Transform {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform =
        Transform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(dx: f32, dy: f32) -> Transform {
        Transform { e: dx, f: dy, ..Transform::IDENTITY }
    }

    pub fn scale(sx: f32, sy: f32) -> Transform {
        Transform { a: sx, d: sy, ..Transform::IDENTITY }
    }

    /// `self` 之后再施加 `other`，等价于 dvipdfmx 的 `pdf_concatmatrix`。
    pub fn then(&self, other: &Transform) -> Transform {
        Transform {
            a: other.a * self.a + other.b * self.c,
            b: other.a * self.b + other.b * self.d,
            c: other.c * self.a + other.d * self.c,
            d: other.c * self.b + other.d * self.d,
            e: other.e * self.a + other.f * self.c + self.e,
            f: other.e * self.b + other.f * self.d + self.f,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
}

/// 一段路径。坐标 twips。
///
/// 只有直线与三次贝塞尔两种——二次贝塞尔与圆弧都能精确或足够近似地化为三次，
/// 后端因此只需实现两个算子。字体轮廓里的二次曲线由整形/栅格化层提升为三次。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSeg {
    MoveTo { x: Twips, y: Twips },
    LineTo { x: Twips, y: Twips },
    /// 三次贝塞尔：两个控制点 + 终点。
    CurveTo {
        c1x: Twips,
        c1y: Twips,
        c2x: Twips,
        c2y: Twips,
        x: Twips,
        y: Twips,
    },
    /// 闭合当前子路径。
    Close,
}

/// 路径：一串段。可含多个子路径（每个 `MoveTo` 起一个）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub segs: Vec<PathSeg>,
}

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn move_to(&mut self, x: Twips, y: Twips) -> &mut Path {
        self.segs.push(PathSeg::MoveTo { x, y });
        self
    }

    pub fn line_to(&mut self, x: Twips, y: Twips) -> &mut Path {
        self.segs.push(PathSeg::LineTo { x, y });
        self
    }

    pub fn curve_to(
        &mut self,
        c1x: Twips,
        c1y: Twips,
        c2x: Twips,
        c2y: Twips,
        x: Twips,
        y: Twips,
    ) -> &mut Path {
        self.segs.push(PathSeg::CurveTo { c1x, c1y, c2x, c2y, x, y });
        self
    }

    pub fn close(&mut self) -> &mut Path {
        self.segs.push(PathSeg::Close);
        self
    }

    /// 矩形：最常用的特例，等价于 dvipdfmx 的 `pdf_dev_rectfill` 那组便利函数。
    pub fn rect(r: Rect) -> Path {
        let mut p = Path::new();
        p.move_to(r.x, r.y)
            .line_to(r.x + r.width, r.y)
            .line_to(r.x + r.width, r.y + r.height)
            .line_to(r.x, r.y + r.height)
            .close();
        p
    }

    /// 任意多边形：文字环绕的排除区用它（`w:wrapPolygon`）。
    pub fn polygon(points: &[(Twips, Twips)]) -> Path {
        let mut p = Path::new();
        let mut it = points.iter();
        if let Some(&(x, y)) = it.next() {
            p.move_to(x, y);
            for &(x, y) in it {
                p.line_to(x, y);
            }
            p.close();
        }
        p
    }

    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// 轴对齐包围盒。空路径返回 `None`。
    ///
    /// 曲线按控制点算——包围盒因此偏大而非偏小，用于环绕时是保守的正确方向。
    pub fn bounds(&self) -> Option<Rect> {
        let mut min = (Twips::MAX, Twips::MAX);
        let mut max = (Twips::MIN, Twips::MIN);
        let mut seen = false;
        let mut note = |x: Twips, y: Twips| {
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
        };
        for seg in &self.segs {
            match *seg {
                PathSeg::MoveTo { x, y } | PathSeg::LineTo { x, y } => {
                    seen = true;
                    note(x, y);
                }
                PathSeg::CurveTo { c1x, c1y, c2x, c2y, x, y } => {
                    seen = true;
                    note(c1x, c1y);
                    note(c2x, c2y);
                    note(x, y);
                }
                PathSeg::Close => {}
            }
        }
        seen.then(|| Rect::new(min.0, min.1, max.0 - min.0, max.1 - min.1))
    }

    /// 认出「一个轴对齐矩形」这个特例。
    ///
    /// 底纹、边框、下划线都是矩形，后端可据此走快路径而不必做通用路径光栅化。
    pub fn as_rect(&self) -> Option<Rect> {
        let mut pts: Vec<(Twips, Twips)> = Vec::new();
        for seg in &self.segs {
            match *seg {
                PathSeg::MoveTo { x, y } | PathSeg::LineTo { x, y } => pts.push((x, y)),
                PathSeg::Close => {}
                PathSeg::CurveTo { .. } => return None,
            }
        }
        if pts.len() != 4 {
            return None;
        }
        let b = self.bounds()?;
        let (x0, x1) = (b.x, b.x + b.width);
        let (y0, y1) = (b.y, b.y + b.height);
        let ok = pts
            .iter()
            .all(|&(x, y)| (x == x0 || x == x1) && (y == y0 || y == y1));
        ok.then_some(b)
    }
}

/// 填充规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    /// 非零环绕。PDF 的 `f`、SVG 的 `nonzero`。
    #[default]
    NonZero,
    /// 奇偶。PDF 的 `f*`、SVG 的 `evenodd`。自交路径与挖孔靠它。
    EvenOdd,
}

/// 线端形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// 折角形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// 描边参数。
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// 线宽，twips。
    pub width: Twips,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    /// 虚线：实/虚交替长度，twips。空表示实线。
    pub dash: Vec<Twips>,
    pub dash_offset: Twips,
}

impl Default for Stroke {
    fn default() -> Stroke {
        Stroke {
            width: 20, // 1pt
            cap: LineCap::default(),
            join: LineJoin::default(),
            miter_limit: 10.0,
            dash: Vec::new(),
            dash_offset: 0,
        }
    }
}

/// 着色：颜色 + 不透明度。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub color: Color,
    /// 0.0 全透明，1.0 不透明。
    pub alpha: f32,
}

impl Default for Paint {
    fn default() -> Paint {
        Paint { color: Color::BLACK, alpha: 1.0 }
    }
}

impl Paint {
    pub fn solid(color: Color) -> Paint {
        Paint { color, alpha: 1.0 }
    }
}

/// 路径绘制方式。对应 dvipdfmx `pdf_dev_flushpath` 的 `p_op`。
#[derive(Debug, Clone, PartialEq)]
pub enum PaintOp {
    Fill { paint: Paint, rule: FillRule },
    Stroke { paint: Paint, stroke: Stroke },
    /// 先填后描，常见于表格单元格（底纹 + 边框）。
    FillThenStroke {
        fill: Paint,
        rule: FillRule,
        stroke_paint: Paint,
        stroke: Stroke,
    },
}

/// 一个已定位的字形。
///
/// 存 `glyph_id` 而非字符：连字与阿拉伯语形态没有对应的单个 `char`。
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedGlyph {
    /// 字体标识，与 `docx_layout::fontenv` 的 `FaceId::sha256()` 对齐。
    pub face: String,
    pub glyph_id: u32,
    /// 笔位，twips。
    pub x: Twips,
    pub y: Twips,
    /// 笔位横向的**精确值**，单位点。见 [`TextFragment::x_pt`]。
    pub x_pt: f64,
    /// 笔位纵向的**精确值**，单位 1/7200 英寸。见 [`TextFragment::baseline_fine`]。
    pub y_fine: i64,
    /// 推进量，twips。整形器给出，比较器用它核对相邻字形的错位。
    pub advance_x: Twips,
    /// 推进量的**精确值**，单位点。
    pub advance_x_pt: f64,
    pub advance_y: Twips,
    /// 字号，半点。
    pub size_half_points: u32,
    /// 本字形对应源文本的哪一段（UTF-16 单位，与 rsword 的坐标流一致）。
    ///
    /// 配对**不能靠 Unicode 身份**（PDF 字形可能没有 ToUnicode 映射），只能按读序，
    /// 所以引擎必须报出这个区间供校验。连字跨多个字符；自动编号标签在源文本里
    /// 没有对应字符，此时为 `None`。
    pub source: Option<(u32, u32)>,
}

/// 绘制指令。
///
/// 画布是**有状态**的：`Save`/`Restore` 与 `Transform`/`Clip` 按顺序作用于后续指令，
/// 这与 PDF 内容流、SVG 的 `<g>` 嵌套、GPU 的矩阵栈都能直接对应。
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    /// 压图形状态。
    Save,
    /// 弹图形状态。后端应当容忍多余的 `Restore`（忽略而非崩溃）。
    Restore,
    /// 叠加变换。
    Transform(Transform),
    /// 用路径裁剪后续绘制。
    Clip { path: Path, rule: FillRule },
    /// 画一条路径。
    DrawPath { path: Path, op: PaintOp },
    /// 画一串同字体同色的字形。
    ///
    /// 字形本身是路径，但不在这里展开成 [`Path`]：能直接排文字的后端
    /// （PDF / SVG）输出文字算子即可，产出可选中可搜索；要轮廓的后端
    /// 自己去字体里取。`text` 供前者使用。
    DrawGlyphs {
        glyphs: Vec<PositionedGlyph>,
        /// 段落起点（基线左端），twips。`glyphs` 为空时后端靠它定位。
        origin_x: Twips,
        /// 段落起点 x 的**精确值**，单位点。见 [`TextFragment::x_pt`]。
        origin_x_pt: f64,
        origin_y: Twips,
        /// 段落起点基线的**精确** y，单位 1/7200 英寸。见 [`TextFragment::baseline_fine`]。
        origin_y_fine: i64,
        text: String,
        font: crate::font::FontSpec,
        paint: Paint,
        /// 本行以什么结束。比较器按计数约定核对字形数，故须随指令带下来。
        terminator: crate::oracle::LineTerminator,
        /// 本片段的源字符区间（UTF-16，**全篇偏移**）。
        ///
        /// 独立于 `glyphs` 存在：能直接排文字的后端不需要字形序列，
        /// 此时 `glyphs` 为空，源区间不该跟着丢。
        source: Option<(u32, u32)>,
        /// 本片段属于本页第几行。**同一行的多条指令带同一个值**，
        /// 下游据此把它们合回一行（见 [`TextFragment::line`]）。
        line: u32,
    },
    /// 画图片。`id` 是媒体句柄，`rect` 是目标区域。
    ///
    /// 裁剪与非矩形边框用 [`DrawCmd::Clip`] 配合，不在此处重复表达。
    DrawImage { id: String, rect: Rect },
}

/// 矢量画布：后端实现它。
///
/// 只有一个必需方法——所有绘制都归约到指令流。提供 `begin_page`/`end_page`
/// 是因为多页文档的分页对每种后端都有实质意义（PDF 的页对象、SVG 的多个 `<svg>`）。
pub trait VectorCanvas {
    type Error;

    /// 开新页。`width`/`height` 单位 twips。
    fn begin_page(&mut self, width: Twips, height: Twips) -> Result<(), Self::Error>;

    fn end_page(&mut self) -> Result<(), Self::Error>;

    /// 执行一条指令。
    fn draw(&mut self, cmd: &DrawCmd) -> Result<(), Self::Error>;

    /// 全部画完。
    fn finish(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// 执行一串指令。默认逐条转调。
    fn draw_all(&mut self, cmds: &[DrawCmd]) -> Result<(), Self::Error> {
        for c in cmds {
            self.draw(c)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 文字环绕：基于同一套路径原语
// ---------------------------------------------------------------------------

/// 一维区间，twips。断行用它表示某一行的可用横向空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Span {
    pub start: Twips,
    pub end: Twips,
}

impl Span {
    pub fn new(start: Twips, end: Twips) -> Span {
        Span { start, end }
    }

    pub fn width(&self) -> Twips {
        (self.end - self.start).max(0)
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0
    }
}

/// 文字环绕的排除区。
///
/// Word 的 `w:wrapPolygon` 是**任意多边形**，矩形表达不了——这正是画布原语必须是
/// 路径的理由。与绘制共用 [`Path`]：布局器用它收窄行宽，后端可用同一条路径做裁剪。
#[derive(Debug, Clone, PartialEq)]
pub struct WrapRegion {
    /// 排除区形状。
    pub path: Path,
    pub rule: FillRule,
    /// 环绕时与形状保持的距离，twips。
    pub distance: Twips,
    /// 环绕方式。
    pub side: WrapSide,
}

/// 环绕方式，对应 OOXML 的 `wrapText`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapSide {
    /// 两侧都排文字。
    #[default]
    BothSides,
    /// 只在左侧排。
    Left,
    /// 只在右侧排。
    Right,
    /// 只在较宽的一侧排。
    Largest,
    /// 上下型：整行都让开。
    TopAndBottom,
}

impl WrapRegion {
    /// 矩形排除区：最常见的情形。
    pub fn rect(rect: Rect, distance: Twips) -> WrapRegion {
        WrapRegion {
            path: Path::rect(rect),
            rule: FillRule::NonZero,
            distance,
            side: WrapSide::default(),
        }
    }

    /// 多边形排除区，对应 `w:wrapPolygon`。
    pub fn polygon(points: &[(Twips, Twips)], distance: Twips) -> WrapRegion {
        WrapRegion {
            path: Path::polygon(points),
            rule: FillRule::NonZero,
            distance,
            side: WrapSide::default(),
        }
    }

    pub fn with_side(mut self, side: WrapSide) -> WrapRegion {
        self.side = side;
        self
    }

    /// 本区在 `[y, y + height)` 这条横带上的遮挡范围。
    ///
    /// 返回 `None` 表示不与该带相交。用路径的包围盒求解——多边形的精确逐行求交
    /// 要解边与扫描线的交点，那是后续的事；包围盒足以让机制跑通，且**偏保守**
    /// （遮挡略大而非略小），不会出现文字压到图上。
    pub fn occupied(&self, y: Twips, height: Twips) -> Option<Span> {
        let b = self.path.bounds()?;
        let top = b.y - self.distance;
        let bottom = b.y + b.height + self.distance;
        if y + height <= top || y >= bottom {
            return None;
        }
        Some(Span::new(b.x - self.distance, b.x + b.width + self.distance))
    }
}

/// 一组环绕区，供布局器查询每行可用区间。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WrapContext {
    regions: Vec<WrapRegion>,
}

impl WrapContext {
    pub fn new() -> WrapContext {
        WrapContext::default()
    }

    pub fn add(&mut self, r: WrapRegion) -> &mut WrapContext {
        self.regions.push(r);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// 求 `[y, y+height)` 这条横带上、在 `full` 范围内的可用区间。
    ///
    /// 返回多个不连续区间：图片落在段落中间时会把行劈成左右两段。
    /// 空结果表示整行被占满（上下型环绕，或图片横跨整个行宽）。
    pub fn available(&self, full: Span, y: Twips, height: Twips) -> Vec<Span> {
        let mut spans = vec![full];
        for r in &self.regions {
            let Some(occ) = r.occupied(y, height) else { continue };
            // TopAndBottom：整行让开。
            if r.side == WrapSide::TopAndBottom {
                return Vec::new();
            }
            spans = spans
                .into_iter()
                .flat_map(|s| Self::subtract(s, occ, r.side))
                .filter(|s| !s.is_empty())
                .collect();
            if spans.is_empty() {
                break;
            }
        }
        spans
    }

    /// 从一个区间里减去遮挡，按环绕方式决定留哪边。
    fn subtract(s: Span, occ: Span, side: WrapSide) -> Vec<Span> {
        // 不相交：原样保留。
        if occ.end <= s.start || occ.start >= s.end {
            return vec![s];
        }
        let left = Span::new(s.start, occ.start.min(s.end));
        let right = Span::new(occ.end.max(s.start), s.end);
        match side {
            WrapSide::Left => vec![left],
            WrapSide::Right => vec![right],
            WrapSide::Largest => {
                if left.width() >= right.width() {
                    vec![left]
                } else {
                    vec![right]
                }
            }
            WrapSide::BothSides => vec![left, right],
            // 上面已提前返回，这里不可达；保守起见整行让开。
            WrapSide::TopAndBottom => Vec::new(),
        }
    }
}

// ==========================================================================
// 3. 布局产物：页面与片段
// ==========================================================================

/// 页内一个已定位的绘制项。
#[derive(Debug, Clone)]
pub enum Fragment {
    Text(TextFragment),
    /// 实心矩形：底纹、边框、下划线、删除线都归一到它。
    Rect { rect: Rect, color: Color },
    Image { id: String, rect: Rect },
}

#[derive(Debug, Clone)]
pub struct TextFragment {
    pub x: Twips,
    /// 片段起点 x 的**精确值**，单位点。
    ///
    /// `x` 是它落到整 twips 的结果。横向不设定点单位的理由见
    /// [`crate::font::FontMetrics::advance_pt`]：Word 的推进量落在 1/10000 em 上，
    /// 随字号变，没有固定的绝对单位能整除它。
    pub x_pt: f64,
    /// 基线绝对 y（页内坐标），twips。**有残差**：栅格点落不到整 twips 上。
    pub baseline_y: Twips,
    /// 基线绝对 y 的**精确值**，单位 1/7200 英寸。
    ///
    /// `baseline_y` 是它落到整 twips 的结果，而 Word 的栅格是 0.24pt = 4.8 twips，
    /// 取整必然有残差、且沿页累加。要与 Word 逐位相比，就得读这一个。
    pub baseline_fine: i64,
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    /// 来源段落的 `rsword` 节点 id，供调试与反查。
    pub source_node: Option<u32>,
    /// 本片段所在行的终止符。只有行末片段带真实值，行中片段是 `Wrapped`。
    pub terminator: crate::oracle::LineTerminator,
    /// 本片段覆盖的源字符区间（UTF-16 单位，**全篇偏移**）。
    ///
    /// 比较器按读序配对，需要它把字形对回源字符。`None` 表示引擎未能确定，
    /// 比较器据此报「判不了」而不是猜一个区间。
    pub source: Option<(u32, u32)>,
    /// 基线抬升，twips，正值向上。`baseline_y` 已经减去过它；
    /// 单独留着是为了让下游能分辨「这行基线在这里」与「这段被抬高了」。
    pub rise: Twips,
    /// 本片段属于本页第几行，从 0 计。
    ///
    /// **一行可能有多个片段**（换字体、上标、分页符都会把行切开），而绘制指令是
    /// 按片段出的。没有这个字段，下游就只能把「一条指令」当「一行」，于是行这一层
    /// 被 run 切碎——比较器的行层配对必然对不上，且失败**看起来像「引擎少排了行」，
    /// 其实是记账粒度错了**。
    pub line: u32,
}

/// 一行。保留行信息而不直接摊平成 Fragment，是因为对齐、两端对齐的空白分配、
/// 以及垂直居中都需要「整行」这个单位。
#[derive(Debug, Clone, Default)]
pub struct Line {
    /// 行顶 y（页内）。
    pub top: Twips,
    pub height: Twips,
    /// 基线相对行顶的偏移。
    pub baseline: Twips,
    pub fragments: Vec<Fragment>,
}

#[derive(Debug, Clone)]
pub struct Page {
    /// 页面物理尺寸。
    pub size: Size,
    /// 正文可用区（已扣页边距），供调试与页眉页脚定位参考。
    pub content_area: Rect,
    pub fragments: Vec<Fragment>,
}

impl Page {
    pub fn new(size: Size, content_area: Rect) -> Page {
        Page { size, content_area, fragments: Vec::new() }
    }

    /// 把一行摊平进页面。
    pub fn push_line(&mut self, line: Line) {
        self.fragments.extend(line.fragments);
    }
}

// ==========================================================================
// 4. 布局引擎：y 游标断行与分页
// ==========================================================================

/// 段落对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    /// 两端对齐：最后一行不拉伸。
    Justify,
}

/// 行距规则（`w:spacing/@w:lineRule`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineRule {
    /// `auto`：倍数行距，`value` 是 240 分之一倍（240 = 单倍）。
    #[default]
    Auto,
    /// `atLeast`：至少这么高，twips。
    AtLeast,
    /// `exact`：精确高度，twips。
    Exact,
}

/// 布局引擎接受的段落。
///
/// 这是 `rsword::resolve` 的 `EffectiveParaProps` / `EffectiveRunProps` 投影到布局所需字段的结果。
/// 刻意不直接吃 rsword 类型，保持引擎可单独测试。
#[derive(Debug, Clone)]
pub struct Para {
    pub runs: Vec<Run>,
    pub align: Align,
    /// 左缩进。
    pub indent_left: Twips,
    pub indent_right: Twips,
    /// 首行缩进，可负（悬挂缩进）。
    pub indent_first_line: Twips,
    pub space_before: Twips,
    pub space_after: Twips,
    pub line_rule: LineRule,
    /// 配合 `line_rule` 的值。
    pub line_value: Twips,
    /// `w:keepNext`：与下一段同页。
    pub keep_next: bool,
    /// `w:keepLines`：段内不跨页。
    pub keep_lines: bool,
    /// `w:pageBreakBefore`。
    pub page_break_before: bool,
    pub source_node: Option<u32>,
    /// 本段以什么结束。计数约定要区分：段落标记画 1 个空格，软回车画 1 个，
    /// 分节符画 0 个，手动分页符视位置画 0 或 1 个。
    pub terminator: crate::oracle::LineTerminator,
}

impl Default for Para {
    fn default() -> Para {
        Para {
            runs: Vec::new(),
            align: Align::Left,
            indent_left: 0,
            indent_right: 0,
            indent_first_line: 0,
            space_before: 0,
            space_after: 0,
            line_rule: LineRule::Auto,
            line_value: 240,
            keep_next: false,
            keep_lines: false,
            page_break_before: false,
            source_node: None,
            terminator: crate::oracle::LineTerminator::ParagraphMark,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    /// `text` 里每个 [`OBJECT_PLACEHOLDER`] 各是什么，按文档顺序，一一对应。
    ///
    /// 占位符本身**不区分种类**——分页符、软回车、行内图在 run 文本里都是同一个
    /// U+FFFC。种类只在 rsword 的 `segments[].kind` 里，所以必须由桥接层带进来，
    /// 否则排版分不出「这里要翻页」和「这里有张图」。
    ///
    /// 长度与 `text` 里的占位符个数对不上时，桥接层应当整体退回 [`PlaceholderKind::Object`]
    /// ——那是保守方向：不会凭空造出分页。
    pub placeholders: Vec<PlaceholderKind>,
    /// 基线抬升，twips，**正值向上**。
    ///
    /// 两个来源：`w:vertAlign`（上下标，同时缩小字号——那部分反映在
    /// [`FontSpec::size_half_points`] 里）与 `w:position`（只抬升，不改字号）。
    ///
    /// 挂在 run 上而不是 `FontSpec` 上，是因为它**不影响度量**：
    /// 抬升不改变推进量，只改变落笔的 y。放进 FontSpec 会让度量缓存按它分桶，
    /// 白白多出一倍的键。
    pub rise: Twips,
}

/// run 文本里一个 [`OBJECT_PLACEHOLDER`] 代表什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderKind {
    /// `w:br w:type="page"`：断行并翻页。
    PageBreak,
    /// `w:br w:type="column"`：分栏符。**未测**——本版不实现分栏，
    /// 按软回车同级处理（断行不翻页），留待实测。
    ColumnBreak,
    /// `w:br`、`w:br w:type="textWrapping"`：断行不翻页。
    LineBreak,
    /// 行内对象（图、OLE、pict）。**不是断开**：不断行也不翻页。
    Object,
}

impl PlaceholderKind {
    /// 是否要在此处收行。
    fn breaks_line(self) -> bool {
        !matches!(self, PlaceholderKind::Object)
    }

    /// 是否要在收行之后翻页。
    fn breaks_page(self) -> bool {
        matches!(self, PlaceholderKind::PageBreak)
    }
}

/// 页面设置（来自 `rsword::resolve::section::SectionGeom`）。
#[derive(Debug, Clone, Copy)]
pub struct PageSetup {
    pub size: Size,
    pub margins: Margins,
}

impl PageSetup {
    /// A4 纵向 + 1 英寸页边距。
    pub fn a4() -> PageSetup {
        PageSetup {
            size: Size::new(11906, 16838),
            margins: Margins::uniform(1440),
        }
    }

    pub fn content_area(&self) -> Rect {
        self.margins.shrink(&Rect::new(0, 0, self.size.width, self.size.height))
    }
}

/// 行内一段同字体同色的文字。
struct LinePiece {
    /// 相对行首的 x，twips。
    dx: Twips,
    /// 相对行首的 x 的**精确值**，单位点。见 [`crate::font::FontMetrics::advance_pt`]。
    dx_pt: f64,
    text: String,
    font: FontSpec,
    color: Color,
    /// 基线抬升，twips，正值向上。见 [`Run::rise`]。
    rise: Twips,
    /// 源字符区间，UTF-16 单位、相对所在段落。
    ///
    /// 断行是唯一知道「切在第几个字符」的地方，所以必须在这里记下；
    /// 事后从文字反推会在重复文本上出错。
    source: (u32, u32),
}

/// 排好的一行，尚未定位到页面。
struct PendingLine {
    height: Twips,
    baseline: Twips,
    pieces: Vec<LinePiece>,
    width: Twips,
    /// 行宽的**精确值**，单位点。对齐（居中 / 右对齐 / 两端对齐）算的是
    /// 「可用宽减行宽」，走整 twips 的 `width` 会把残差搬到每个字形上。
    width_pt: f64,
    is_last: bool,
    /// 首行要额外吃 `indent_first_line`（可负，即悬挂缩进）。
    is_first: bool,
    /// 本行实际落在哪个横向区间。无环绕时就是整个正文宽度；
    /// 有环绕时可能是被图片劈开后的左段或右段。
    span: Span,
    /// 本行之后要翻页（段内的 `w:br w:type="page"`）。
    ///
    /// 与 `Para::page_break_before` 不同：那条是段落属性（`w:pageBreakBefore`），
    /// 这条是段**内**任意位置的手动分页符，所以必须挂在行上而不是段上。
    page_break_after: bool,
    /// 本行高度的**精确值**，单位 1/7200 英寸。
    ///
    /// `height` 是它落到整 twips 的结果；游标按 `height` 累加会逐行漂移
    /// （栅格上的 273.6 twips 落成 274，每行多 0.4 twip，一页 40 行攒 0.8pt）。
    /// 所以纵向游标走这一个，`height` 只用于「放不放得下」这类整 twips 的判断。
    height_fine: i64,
    /// 本行起点在全篇 UTF-16 偏移空间里的下标。
    ///
    /// 有片段时可以从片段推，但**空行没有片段**——而空行同样要有源位置，
    /// 否则它的行记录就只能给 `None`，下游会读成「判不了」。
    source_start: u32,
    /// 本行**消费到**哪个下标（不含）。终止符字符就紧跟在这里。
    ///
    /// 不能拿「最后一个片段的终点 +1」代替：片段之间可能有**不产生片段**的源字符
    /// （对象占位符就是），那时片段终点比行的真实终点小，终止符会被记到错的位置上。
    source_end: u32,
}

pub struct Engine<'m, M: FontMetrics> {
    metrics: &'m M,
    setup: PageSetup,
    /// 文字环绕的排除区。空则每行可用区间恒为整个正文宽度。
    wrap: WrapContext,
}

impl<'m, M: FontMetrics> Engine<'m, M> {
    pub fn new(metrics: &'m M, setup: PageSetup) -> Engine<'m, M> {
        Engine { metrics, setup, wrap: WrapContext::new() }
    }

    /// 带环绕区构造。
    ///
    /// 有环绕时每行的可用区间由 `y` 决定，而且可能被劈成多段（图片落在段落中间）。
    pub fn with_wrap(metrics: &'m M, setup: PageSetup, wrap: WrapContext) -> Engine<'m, M> {
        Engine { metrics, setup, wrap }
    }

    /// 某一行可用的横向区间。
    ///
    /// 这是断行从「固定宽度」变成「查询当前位置」的关键：无环绕时退化为单个满宽区间，
    /// 与改造前行为一致。
    fn line_spans(&self, para: &Para, area: Rect, y: Twips, height: Twips) -> Vec<Span> {
        let full = Span::new(area.x + para.indent_left, area.right() - para.indent_right);
        if self.wrap.is_empty() {
            return vec![full];
        }
        self.wrap.available(full, y, height)
    }

    /// 把段落序列排成页面。
    pub fn layout(&self, paras: &[Para]) -> Vec<Page> {
        let area = self.setup.content_area();
        let mut pages: Vec<Page> = Vec::new();
        let mut page = Page::new(self.setup.size, area);
        // 纵向游标走**精细单位**（1/7200 英寸），只在落位与判断时换回 twips。
        // 按整 twips 累加会逐行漂移：栅格上的 273.6 twips 落成 274，每行多 0.4 twip。
        let fine = |t: Twips| i64::from(t) * FINE_PER_TWIP;
        let coarse = |f: i64| ((f as f64) / FINE_PER_TWIP as f64).round() as Twips;
        let mut cursor_fine: i64 = fine(area.y);

        // 本页内的行号。一行可能出多个片段，下游要靠它把它们合回一行；
        // 翻页时归零。
        let mut line_index: u32 = 0;

        // 全篇 UTF-16 偏移游标。与 Word 的 `Range.Start/End` 同一套数法：
        // 各段文本依次拼接，**每段末尾算一个终止符**（`\r`，带 `w:sectPr` 的段是 `\x0c`，
        // 都占 1 个 UTF-16 单位）。段内的软回车与分页符已经以 U+FFFC 占位符
        // 落在 run 文本里，所以逐 run 数就够，不必另加。
        let mut source_cursor: u32 = 0;

        for (idx, para) in paras.iter().enumerate() {
            let para_base = source_cursor;
            let para_units: u32 = para
                .runs
                .iter()
                .map(|r| r.text.encode_utf16().count() as u32)
                .sum();
            // +1 是段落终止符本身。
            source_cursor = para_base + para_units + 1;

            let lines = self.break_paragraph(para, area, coarse(cursor_fine), para_base);
            let block_height: Twips = lines.iter().map(|l| l.height).sum();

            if para.page_break_before && !page.fragments.is_empty() {
                pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor_fine = fine(area.y);
                line_index = 0;
            }

            cursor_fine += fine(para.space_before);

            // keepLines：整段放不下就先翻页（除非本页是空的，那样翻了也没用）。
            if para.keep_lines
                && coarse(cursor_fine) + block_height > area.bottom()
                && !page.fragments.is_empty()
            {
                pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor_fine = fine(area.y);
                line_index = 0;
            }

            // keepNext：本段是最后一段时无意义；否则要保证下一段至少第一行同页。
            let next_first_line = if para.keep_next {
                paras.get(idx + 1).and_then(|n| {
                    // 只取高度，源区间用不上；给下一段的正确基点，免得读代码时费解。
                    self.break_paragraph(n, area, coarse(cursor_fine), source_cursor)
                        .first()
                        .map(|l| l.height)
                }).unwrap_or(0)
            } else {
                0
            };

            let total_lines = lines.len();
            for (li, line) in lines.into_iter().enumerate() {
                let mut needed = line.height;
                // 最后一行还要替下一段的首行占位。
                if para.keep_next && li + 1 == total_lines {
                    needed += next_first_line;
                }
                if coarse(cursor_fine) + needed > area.bottom() && !page.fragments.is_empty() {
                    pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                    cursor_fine = fine(area.y);
                    line_index = 0;
                }
                self.place_line(&mut page, &line, para, cursor_fine, line_index);
                line_index += 1;
                // **精确累加**：用 height_fine 而不是取整后的 height。
                cursor_fine += line.height_fine;

                // 段内手动分页符：本行之后翻页。
                //
                // 与「放不下就翻页」不同，这一条**不看还剩多少空间**——源里写了分页就是分页。
                // 实测夹具里有连续两个分页符的情形，那确实产生一张只有一条行记录的页。
                if line.page_break_after {
                    pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                    cursor_fine = fine(area.y);
                    line_index = 0;
                }
            }

            cursor_fine += fine(para.space_after);
        }

        pages.push(page);
        pages
    }

    /// 把一行放到页面上，处理水平对齐。
    /// 把一行放到页面上。
    ///
    /// `line_index` 是本页内的行号，随每个片段带下去——一行可能有多个片段，
    /// 下游要靠它把它们合回一行。
    fn place_line(
        &self,
        page: &mut Page,
        line: &PendingLine,
        para: &Para,
        top_fine: i64,
        line_index: u32,
    ) {
        // 用本行实际落到的区间，而不是整个正文宽度——有环绕时两者不同。
        let first = if line.is_first { para.indent_first_line } else { 0 };
        let avail = (line.span.width() - first).max(0);
        let base_x = line.span.start + first;
        let slack = (avail - line.width).max(0);

        let offset = match para.align {
            Align::Left | Align::Justify => 0,
            Align::Center => slack / 2,
            Align::Right => slack,
        };

        // 横向落位的精确支路，单位点。可用宽本来就是整 twips（页宽与缩进都是），
        // 只有行宽不是——所以只有它换精确值，其余照换算。
        let pt = |t: Twips| f64::from(t) / 20.0;
        let base_x_pt = pt(base_x);
        let slack_pt = (pt(avail) - line.width_pt).max(0.0);
        let offset_pt = match para.align {
            Align::Left | Align::Justify => 0.0,
            Align::Center => slack_pt / 2.0,
            Align::Right => slack_pt,
        };

        // 两端对齐：除最后一行外，把空隙按片段间隙均摊。
        let justify_gap = if para.align == Align::Justify
            && !line.is_last
            && line.pieces.len() > 1
        {
            slack / (line.pieces.len() as Twips - 1)
        } else {
            0
        };
        let justify_gap_pt = if para.align == Align::Justify
            && !line.is_last
            && line.pieces.len() > 1
        {
            slack_pt / (line.pieces.len() - 1) as f64
        } else {
            0.0
        };

        // 基线落位：在 1/7200 英寸上加好再交给度量量化到它的栅格。
        // 先取整到 twips 再量化是不行的——0.24pt = 4.8 twips，取整就把栅格点碾碎了。
        let fine = |t: Twips| i64::from(t) * FINE_PER_TWIP;
        let coarse = |f: i64| ((f as f64) / FINE_PER_TWIP as f64).round() as Twips;
        let baseline_fine = self
            .metrics
            .quantize_baseline_fine(top_fine + fine(line.baseline));
        let baseline_y = coarse(baseline_fine);
        let last = line.pieces.len().saturating_sub(1);
        for (i, p) in line.pieces.iter().enumerate() {
            let x = base_x + offset + p.dx + justify_gap * (i as Twips);
            let x_pt = base_x_pt + offset_pt + p.dx_pt + justify_gap_pt * (i as f64);
            // 只有段落最后一行的最后一个片段带真实终止符；其余是自动换行。
            // 计数约定按行核对字形数，把终止符记到行中片段会让核对偏移。
            let terminator = if line.is_last && i == last {
                para.terminator
            } else {
                crate::oracle::LineTerminator::Wrapped
            };
            let (text, source) =
                with_terminator_glyphs(&p.text, p.source, line.source_end, terminator);
            page.fragments.push(Fragment::Text(TextFragment {
                x,
                x_pt,
                // 抬升是**向上**的，而页内 y 向下增长，所以要减。
                baseline_y: coarse(baseline_fine - fine(p.rise)),
                baseline_fine: baseline_fine - fine(p.rise),
                text,
                font: p.font.clone(),
                color: p.color,
                source_node: para.source_node,
                source: Some(source),
                terminator,
                rise: p.rise,
                line: line_index,
            }));
        }

        // 空行也要产出一个片段。
        //
        // 没有它，空行就**没有任何绘制指令**，于是下游根本看不到这一行——
        // 而 Word 是给空行一条行记录的（实测：独占一行的分页符、空段落都有）。
        // 那种缺失在比较器里表现为「引擎少排了行」，查起来像分页错，其实是这里漏了。
        if line.pieces.is_empty() {
            let terminator = if line.is_last {
                para.terminator
            } else {
                crate::oracle::LineTerminator::Wrapped
            };
            let (text, source) = with_terminator_glyphs(
                "",
                (line.source_start, line.source_end),
                line.source_end,
                terminator,
            );
            page.fragments.push(Fragment::Text(TextFragment {
                x: base_x + offset,
                x_pt: base_x_pt + offset_pt,
                baseline_y,
                baseline_fine,
                text,
                font: para
                    .runs
                    .first()
                    .map(|r| r.font.clone())
                    .unwrap_or_else(|| FontSpec::new("Times New Roman", 24)),
                color: para.runs.first().map(|r| r.color).unwrap_or(Color::BLACK),
                source_node: para.source_node,
                source: Some(source),
                terminator,
                rise: 0,
                line: line_index,
            }));
        }
    }

    /// 段落断行。
    ///
    /// `y` 是本段起始的纵向位置——**有环绕时每行可用区间取决于它**，所以不能像
    /// 改造前那样只传一个标量宽度。无环绕时退化为整段用同一个满宽区间。
    ///
    /// `source_base` 是本段首字符在**全篇** UTF-16 偏移空间里的下标。
    /// 片段的源区间要落在这个空间里，不是段内偏移——Word 的 `Range.Start/End`
    /// 是全篇连续的，段内偏移与它长得几乎一样（都是小整数、都单调）却对不上，
    /// 而按读序配对的校验全靠这个区间。
    fn break_paragraph(
        &self,
        para: &Para,
        area: Rect,
        y: Twips,
        source_base: u32,
    ) -> Vec<PendingLine> {
        let mut lines: Vec<PendingLine> = Vec::new();

        // 空段落：高度由段落标记字体决定。
        if para.runs.iter().all(|r| r.text.is_empty()) {
            let font = para
                .runs
                .first()
                .map(|r| r.font.clone())
                .unwrap_or_else(|| FontSpec::new("Times New Roman", 24));
            let m = self.metrics.empty_line_metrics(&font);
            let h = self.line_height(para, m.ascent + m.descent, m.natural_height());
            let span = self
                .line_spans(para, area, y, h)
                .into_iter()
                .max_by_key(|s| s.width())
                .unwrap_or_else(|| Span::new(area.x, area.right()));
            lines.push(PendingLine {
                height: h,
                height_fine: self.line_height_fine(
                    para,
                    m.ascent + m.descent,
                    self.metrics.natural_height_fine("", &font),
                ),
                baseline: m.ascent,
                pieces: Vec::new(),
                width: 0,
                width_pt: 0.0,
                is_last: true,
                is_first: true,
                span,
                page_break_after: false,
                source_start: source_base,
                source_end: source_base,
            });
            return lines;
        }

        let mut cur: Vec<LinePiece> = Vec::new();
        // 全篇 UTF-16 偏移游标，作为各片段源区间的起点。从本段基点起算，不从 0。
        let mut consumed: u32 = source_base;
        // 当前行起点，供空行用（空行没有片段可推）。
        let mut line_start: u32 = source_base;
        let mut cur_w: Twips = 0;
        // 与 `cur_w` 并行的精确值，单位点。断行判断仍走 `cur_w`（整 twips 够用），
        // 只有**落位**读这一个——横向取整的残差同样沿行累加。
        let mut cur_w_pt: f64 = 0.0;
        let mut cur_ascent: Twips = 0;
        let mut cur_descent: Twips = 0;
        let mut cur_natural: Twips = 0;
        // 与 `cur_natural` 并行的精确值，单位 1/7200 英寸。
        let mut cur_natural_fine: i64 = 0;
        let mut first_line = true;
        // 行高未知时用来试探区间的估值：取正文字号的自然行高。
        let probe_h = self
            .metrics
            .measure("", para.runs.first().map(|r| &r.font).unwrap_or(&FontSpec::new("Times New Roman", 24)))
            .natural_height()
            .max(1);
        let mut cur_y = y;

        // 本行可用区间：有环绕时按 y 查，可能被劈成多段，这里取最宽的一段。
        // 取最宽而非逐段填充，是因为把一行拆到不连续区间里需要把行再切分，
        // 那是后续的事；取最宽段保证不与图片重叠，且是保守的正确方向。
        let mut span = self.pick_span(para, area, cur_y, probe_h);
        // 首行缩进吃掉的宽度。
        let mut line_avail = (span.width() - if first_line { para.indent_first_line } else { 0 }).max(1);

        for run in &para.runs {
            // rsword 为每个 `w:br` 在 run 文本里放一个 U+FFFC（对象替换符）。
            // 它是**控制字符，不是文字**：占 1 个源字符位（Word 的 `Range` 数它），
            // 但既不成字形也不占宽度——Word 导出的 PDF 里一个都没有。
            //
            // 不切掉的后果是实测过的：它会被整形器当普通字符画出来，
            // 12pt 字号下 advance 12.0pt，其后整行字形集体右移；
            // 一份 11 页夹具上 8 次共凭空占掉 96pt。
            //
            // 在这里切而不是在绘制层滤，是因为**断行也不能算它的宽度**：
            // 只在绘制层滤，行宽照样是错的，而那种错在轨迹里看不出来。
            // 占位符可能夹在文字中间（实测 `'分页符之前￼分页符之后'`），所以按它切段。
            for (part_index, part) in run.text.split(OBJECT_PLACEHOLDER).enumerate() {
                if part_index > 0 {
                    // 不是第一段 ⇒ 前面刚跨过一个占位符：源游标要走 1 个 UTF-16 单位，
                    // 但不产生片段、不占宽度。
                    consumed += 1;

                    // 占位符是什么，决定要不要在此处收行／翻页。种类由桥接层带进来；
                    // 拿不到就按 `Object` 处理——保守方向，不凭空造出分页。
                    let kind = run
                        .placeholders
                        .get(part_index - 1)
                        .copied()
                        .unwrap_or(PlaceholderKind::Object);

                    if kind.breaks_line() {
                        // 收行。空行也要收：`'\u{FFFC}文字'` 这种分页符在段首的情形，
                        // 前面确实是一条空行（Word 也给它一条独立的行记录）。
                        let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
                        let h = if cur.is_empty() && cur_w == 0 {
                            // 空行高度由该 run 的字体定，不能取 0——否则后面的行会叠上来。
                            let m = self.metrics.empty_line_metrics(&run.font);
                            cur_ascent = m.ascent;
                            self.line_height(para, m.ascent + m.descent, m.natural_height())
                        } else {
                            h
                        };
                        lines.push(PendingLine {
                            height: h,
                            height_fine: self.line_height_fine(para, cur_ascent + cur_descent, cur_natural_fine),
                            baseline: cur_ascent,
                            pieces: std::mem::take(&mut cur),
                            width: cur_w,
                            width_pt: cur_w_pt,
                            is_last: false,
                            is_first: first_line,
                            span,
                            page_break_after: kind.breaks_page(),
                            source_start: line_start,
                            source_end: consumed,
                        });
                        line_start = consumed;
                        cur_w = 0;
                        cur_w_pt = 0.0;
                        cur_ascent = 0;
                        cur_descent = 0;
                        cur_natural = 0;
                        cur_natural_fine = 0;
                        first_line = false;
                        cur_y += h;
                        span = self.pick_span(para, area, cur_y, h.max(probe_h));
                        line_avail = span.width().max(1);
                    }
                }
            let mut rest: &str = part;
            while !rest.is_empty() {
                let remain = (line_avail - cur_w).max(0);
                let m_all = self.metrics.measure(rest, &run.font);

                if m_all.advance <= remain {
                    // 整段剩余放得下。
                    let n = rest.encode_utf16().count() as u32;
                    cur.push(LinePiece {
                        dx: cur_w,
                        dx_pt: cur_w_pt,
                        text: rest.to_string(),
                        font: run.font.clone(),
                        color: run.color,
                        rise: run.rise,
                        source: (consumed, consumed + n),
                    });
                    consumed += n;
                    cur_w += m_all.advance;
                    cur_w_pt += self.metrics.advance_pt(rest, &run.font);
                    cur_ascent = cur_ascent.max(m_all.ascent);
                    cur_descent = cur_descent.max(m_all.descent);
                    cur_natural = cur_natural.max(m_all.natural_height());
                    cur_natural_fine =
                        cur_natural_fine.max(self.metrics.natural_height_fine(rest, &run.font));
                    break;
                }

                // 放不下：找能塞进去的最长前缀。
                match self.metrics.fit(rest, &run.font, remain) {
                    Some((cut, m)) if cut > 0 => {
                        let piece = &rest[..cut];
                        let n = piece.encode_utf16().count() as u32;
                        cur.push(LinePiece {
                            dx: cur_w,
                            dx_pt: cur_w_pt,
                            text: piece.to_string(),
                            font: run.font.clone(),
                            color: run.color,
                            rise: run.rise,
                            source: (consumed, consumed + n),
                        });
                        consumed += n;
                        cur_w += m.advance;
                        cur_w_pt += self.metrics.advance_pt(piece, &run.font);
                        cur_ascent = cur_ascent.max(m.ascent);
                        cur_descent = cur_descent.max(m.descent);
                        cur_natural = cur_natural.max(m.natural_height());
                        // 换行处吃掉的空格在源侧**仍然占位**。不记进游标的话，
                        // 本段后续所有片段的源区间会整体前移，而这种错在几何上
                        // 看不出来，只会让配对悄悄错位。
                        let after = rest[cut..].trim_start_matches(' ');
                        consumed += (rest[cut..].encode_utf16().count()
                            - after.encode_utf16().count()) as u32;
                        rest = after;
                    }
                    _ => {
                        // 一个断点都塞不下：若本行已有内容就换行重试，否则硬塞一个字符避免死循环。
                        if cur.is_empty() && cur_w == 0 {
                            let c = rest.chars().next().expect("rest 非空");
                            let n = c.len_utf8();
                            let m = self.metrics.measure(&rest[..n], &run.font);
                            let u16n = rest[..n].encode_utf16().count() as u32;
                            cur.push(LinePiece {
                                dx: cur_w,
                                dx_pt: cur_w_pt,
                                text: rest[..n].to_string(),
                                font: run.font.clone(),
                                color: run.color,
                                rise: run.rise,
                                source: (consumed, consumed + u16n),
                            });
                            consumed += u16n;
                            cur_w += m.advance;
                            cur_w_pt += self.metrics.advance_pt(&rest[..n], &run.font);
                            cur_ascent = cur_ascent.max(m.ascent);
                            cur_descent = cur_descent.max(m.descent);
                            cur_natural = cur_natural.max(m.natural_height());
                            cur_natural_fine = cur_natural_fine
                                .max(self.metrics.natural_height_fine(&rest[..n], &run.font));
                            rest = &rest[n..];
                        }
                    }
                }

                // 收行。
                let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
                lines.push(PendingLine {
                    height: h,
                    height_fine: self.line_height_fine(para, cur_ascent + cur_descent, cur_natural_fine),
                    baseline: cur_ascent,
                    pieces: std::mem::take(&mut cur),
                    width: cur_w,
                    width_pt: cur_w_pt,
                    is_last: false,
                    is_first: first_line,
                    span,
                    page_break_after: false,
                    source_start: line_start,
                    source_end: consumed,
                });
                line_start = consumed;
                cur_w = 0;
                cur_w_pt = 0.0;
                cur_ascent = 0;
                cur_descent = 0;
                cur_natural = 0;
                cur_natural_fine = 0;
                first_line = false;
                // 换行：y 推进一行高，可用区间随之可能变化。
                cur_y += h;
                span = self.pick_span(para, area, cur_y, h.max(probe_h));
                line_avail = span.width().max(1);
            }
            }
        }

        // 末行。
        if !cur.is_empty() || lines.is_empty() {
            let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
            lines.push(PendingLine {
                height: h,
                height_fine: self.line_height_fine(para, cur_ascent + cur_descent, cur_natural_fine),
                baseline: cur_ascent,
                pieces: cur,
                width: cur_w,
                width_pt: cur_w_pt,
                is_last: true,
                is_first: first_line,
                span,
                page_break_after: false,
                source_start: line_start,
                source_end: consumed,
            });
        } else if let Some(last) = lines.last_mut() {
            last.is_last = true;
        }

        lines
    }

    /// 取本行用哪个区间：最宽的那段。整行被占满时退回满宽，
    /// 让内容照样排出来而不是消失——宁可重叠也不丢字。
    fn pick_span(&self, para: &Para, area: Rect, y: Twips, height: Twips) -> Span {
        let full = Span::new(area.x + para.indent_left, area.right() - para.indent_right);
        self.line_spans(para, area, y, height)
            .into_iter()
            .max_by_key(|s| s.width())
            .filter(|s| !s.is_empty())
            .unwrap_or(full)
    }

    /// 按 `w:spacing` 规则算行高的**精确值**，单位 1/7200 英寸。
    ///
    /// 与 [`Engine::line_height`] 同一套规则，只是全程不落到整 twips——
    /// 纵向游标靠它避免逐行漂移。
    fn line_height_fine(&self, para: &Para, content: Twips, natural_fine: i64) -> i64 {
        let fine = |t: Twips| i64::from(t) * FINE_PER_TWIP;
        let height = match para.line_rule {
            LineRule::Exact => fine(para.line_value.max(1)),
            LineRule::AtLeast => natural_fine.max(fine(para.line_value)),
            LineRule::Auto => {
                let mult = if para.line_value <= 0 { 240 } else { para.line_value };
                natural_fine * i64::from(mult) / 240
            }
        };

        // `exact` **不受内容下限约束**：行高就是 `w:line`，装不下就装不下。
        //
        // 这是量出来的。font-free 那一批（`docs/PREREG-2026-09-17-font-free.md`）
        // 在 `exact` 行距下换六个度量差 2.4 倍的字体族，基线**逐格相同**，40/40——
        // 其中 Zapfino 在 13pt 下 ascent 有 **24.4pt**，比那一批最大的行距 14.6pt
        // 还高，Word 照样把基线放在同一格上。**字体一点都没参与。**
        //
        // 加了内容下限就做不到：那会让行高被 ascent + descent 撑开，
        // 于是 Zapfino 那一组的行距变成 24.4pt 而不是 11.1pt。
        if para.line_rule == LineRule::Exact {
            return height;
        }

        // 其余规则才有内容下限（行至少要装得下文字），**在整 twips 的粒度上比较**。
        //
        // `content` 是 ascent + descent，两者都已经取整过：栅格上 273.6 会落成
        // 221 + 53 = 274，比精确值大 0.4 twip。拿它直接 `max` 精确值，
        // 就把刚刚避开的舍入误差又灌了回来——**每行灌一次，沿页累加**。
        // 这条是测试抓出来的：合成输入第 2 行就漂了 0.8 twip。
        let rounded = ((height as f64) / FINE_PER_TWIP as f64).round() as Twips;
        if rounded < content { fine(content) } else { height }
    }

    /// 按 `w:spacing` 规则算行高。
    fn line_height(&self, para: &Para, content: Twips, natural: Twips) -> Twips {
        match para.line_rule {
            LineRule::Exact => para.line_value.max(1),
            LineRule::AtLeast => natural.max(para.line_value),
            // auto：line_value 以 240 为单倍。
            LineRule::Auto => {
                let mult = if para.line_value <= 0 { 240 } else { para.line_value };
                ((i64::from(natural) * i64::from(mult)) / 240) as Twips
            }
        }
        .max(content)
    }
}

// ==========================================================================
// 5. 绘制指令：布局产物 → 画布
// ==========================================================================


/// 字体标识，与 `docx_layout::fontenv` 的 `FaceId::sha256()` 对齐。
pub type FaceId = String;

/// 对象替换符 U+FFFC。
///
/// rsword 用它在 run 文本里为 `w:br` 与行内对象占位。**它是控制字符，不是文字**：
/// 占 1 个源字符位（Word 的 `Range` 数它），但不成字形、不占宽度——
/// 量具方法 §4 对行内对象的实测也是「`Range.Text` 里 1 个占位字符，PDF 里 0 个字形」。
pub const OBJECT_PLACEHOLDER: char = '\u{FFFC}';

/// 文字整形：把一段文字变成字形序列。
///
/// 留在 core 是因为产物**与分辨率无关**——glyph id 与 advance 都由字体的
/// GSUB/GPOS 表决定，是字体单位而非像素。栅格化则必须知道目标分辨率，故归后端。
pub trait TextShaper {
    /// 整形一段文字。位置相对该段起点，单位 twips。
    ///
    /// 返回空表示无法整形（字体缺失等）；调用方不应把它当作「这段文字不占宽度」。
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun>;
}

/// 整形结果里的一个字形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedRun {
    /// 字体在 `faces` 列表里的下标。
    pub face_index: usize,
    pub glyph_id: u32,
    /// 推进量与偏移，twips。
    pub x_advance: Twips,
    /// 推进量的**精确值**，单位点。见 [`crate::font::FontMetrics::advance_pt`]。
    ///
    /// `x_advance` 是它落到整 twips 的结果。整形器给的推进量本来就落不到
    /// 整 twips 上（实测 Word 的 0/1340 个落在整 twips 上），取整的残差沿行累加。
    pub x_advance_pt: f64,
    pub x_offset: Twips,
    pub y_offset: Twips,
}

/// 一页的绘制指令。
#[derive(Debug, Clone, PartialEq)]
pub struct PaintPage {
    /// 页面尺寸，twips。
    pub width: Twips,
    pub height: Twips,
    /// 正文区（已扣页边距），供页眉页脚定位与调试参考。
    pub content_area: Rect,
    pub cmds: Vec<DrawCmd>,
}

impl PaintPage {
    pub fn new(width: Twips, height: Twips, content_area: Rect) -> PaintPage {
        PaintPage { width, height, content_area, cmds: Vec::new() }
    }

    /// 把本页交给一个画布。
    pub fn replay<C: VectorCanvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        canvas.begin_page(self.width, self.height)?;
        canvas.draw_all(&self.cmds)?;
        canvas.end_page()
    }
}

/// 整篇文档的绘制指令。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintList {
    pub pages: Vec<PaintPage>,
}

impl PaintList {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// 把整篇文档交给一个画布。
    pub fn replay<C: VectorCanvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        for p in &self.pages {
            p.replay(canvas)?;
        }
        canvas.finish()
    }
}

/// 把一页布局产物转成绘制指令。
pub fn paint_page(page: &Page, shaper: Option<&dyn TextShaper>, faces: &[FaceId]) -> PaintPage {
    let mut out = PaintPage::new(page.size.width, page.size.height, page.content_area);
    for frag in &page.fragments {
        match frag {
            Fragment::Rect { rect, color } => {
                // 矩形只是路径的特例，走同一条绘制路径。
                out.cmds.push(DrawCmd::DrawPath {
                    path: Path::rect(*rect),
                    op: PaintOp::Fill {
                        paint: Paint::solid(*color),
                        rule: FillRule::NonZero,
                    },
                });
            }
            Fragment::Image { id, rect } => {
                out.cmds.push(DrawCmd::DrawImage { id: id.clone(), rect: *rect });
            }
            Fragment::Text(t) => {
                let glyphs = match shaper {
                    Some(s) => position_glyphs(s, t, faces),
                    None => Vec::new(),
                };
                out.cmds.push(DrawCmd::DrawGlyphs {
                    glyphs,
                    origin_x: t.x,
                    origin_x_pt: t.x_pt,
                    origin_y: t.baseline_y,
                    origin_y_fine: t.baseline_fine,
                    text: t.text.clone(),
                    font: t.font.clone(),
                    paint: Paint::solid(t.color),
                    terminator: t.terminator,
                    source: t.source,
                    line: t.line,
                });
            }
        }
    }
    out
}

/// 终止符自己也要画出来——把它的字符接到片段末尾，并把源区间覆盖到它。
///
/// Word **为段落标记画一个空格**（量具方法 §4，`LineTerminator::expected_glyphs`
/// 就是那张表）。引擎原来只在契约里报「应画 1 个」，实际一个也没画：
/// 每行的字形数比 Word 少 1，比较器一上来就 `GLYPH_COUNT_MISMATCH`，
/// **结构对不上，几何一条都比不了**。
///
/// 接在末尾而不是参与断行，是因为它本来就不参与：行宽、对齐用的都是
/// `line.width`，那是断行时算好的，这里是落位之后再补。Word 也是这样——
/// 行尾那个空格不撑开行，也不影响居中与右对齐。
fn with_terminator_glyphs(
    text: &str,
    source: (u32, u32),
    line_source_end: u32,
    terminator: crate::oracle::LineTerminator,
) -> (String, (u32, u32)) {
    let extra = terminator.expected_glyphs();
    if extra == 0 {
        return (text.to_string(), source);
    }
    let mut out = String::with_capacity(text.len() + extra);
    out.push_str(text);
    for _ in 0..extra {
        out.push(' ');
    }
    // 终止符字符紧跟**整行**消费到的位置，不是紧跟本片段——两者在行里有
    // 不产生片段的字符（对象占位符）时并不相等。区间取到行末，
    // 顺带把那些被跳过的字符也盖住：行记录本来就取并集，覆盖全行才是对的。
    let end = line_source_end.max(source.1) + extra as u32;
    (out, (source.0, end))
}

/// 把整形结果摊成绝对坐标的字形。
fn position_glyphs(
    shaper: &dyn TextShaper,
    t: &TextFragment,
    faces: &[FaceId],
) -> Vec<PositionedGlyph> {
    let mut pen = t.x;
    // 精确笔位与 `pen` 并行推进。整形器的分段（按码位换 face）会让每段各自从零
    // 重新取整，走 twips 时那个断点也会漏进来；精确支路没有断点。
    let mut pen_pt = t.x_pt;
    let runs = shaper.shape(&t.text, &t.font);

    // 源区间按**读序**分配：整形器不报字符归属，所以只能按「字形序号 → 字符序号」
    // 对应。这与量具方法的配对前提一致（PDF 内容流顺序等于源字符顺序），
    // 但字形数与字符数不等时（连字、组合符号）无法逐一对应——
    // 那种情况下整段记同一个区间，让比较器能看出是聚合而非精确归属。
    let utf16_len: u32 = t.text.encode_utf16().count() as u32;
    // 精确归属要**两边都对得上**：字形数 == 字符数，且源区间长度 == 字符数。
    // 后一条不能省：行里有不产生片段的源字符（对象占位符）时，区间会比文本长，
    // 这时 `base + i` 指到的就是别的字符，配对会悄悄错位而几何上看不出来。
    let span_len = t.source.map(|(a, b)| b - a).unwrap_or(0);
    let exact = runs.len() as u32 == utf16_len && span_len == utf16_len;
    let base = t.source.map(|(s, _)| s).unwrap_or(0);

    runs.into_iter()
        .enumerate()
        .filter_map(|(i, g)| {
            let face = faces.get(g.face_index)?.clone();
            let source = t.source.map(|(_, end)| {
                if exact {
                    // 一字形一字符：精确归属。
                    let at = base + i as u32;
                    (at, at + 1)
                } else {
                    // 字形数 ≠ 字符数：给整段区间，不猜。
                    (base, end)
                }
            });
            let out = PositionedGlyph {
                face,
                glyph_id: g.glyph_id,
                x: pen + g.x_offset,
                y: t.baseline_y - g.y_offset,
                x_pt: pen_pt + f64::from(g.x_offset) / 20.0,
                y_fine: t.baseline_fine - i64::from(g.y_offset) * FINE_PER_TWIP,
                advance_x: g.x_advance,
                advance_x_pt: g.x_advance_pt,
                advance_y: 0,
                size_half_points: t.font.size_half_points,
                source,
            };
            pen += g.x_advance;
            pen_pt += g.x_advance_pt;
            Some(out)
        })
        .collect()
}

/// 整篇文档。
pub fn paint_document(
    pages: &[Page],
    shaper: Option<&dyn TextShaper>,
    faces: &[FaceId],
) -> PaintList {
    PaintList {
        pages: pages.iter().map(|p| paint_page(p, shaper, faces)).collect(),
    }
}
