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

use crate::font::{FINE_PER_TWIP, FontMetrics, FontSpec, OverflowPunctuationContext};


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
    /// 有效字号，0.01pt；半点字段仅为兼容近似值。
    pub size_centipoints: u64,
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
// Text is the common case; keep it inline to avoid an allocation per fragment.
#[allow(clippy::large_enum_variant)]
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
    /// 兼容抬升近似值，twips，正值向上。实际落位使用 `rise_fine`；
    /// 单独留着是为了让下游能分辨「这行基线在这里」与「这段被抬高了」。
    pub rise: Twips,
    /// 精确基线抬升，1/7200 英寸，正值向上；`baseline_fine` 已扣除。
    pub rise_fine: i64,
    /// 本片段属于本页第几行，从 0 计。
    ///
    /// **一行可能有多个片段**（换字体、上标、分页符都会把行切开），而绘制指令是
    /// 按片段出的。没有这个字段，下游就只能把「一条指令」当「一行」，于是行这一层
    /// 被 run 切碎——比较器的行层配对必然对不上，且失败**看起来像「引擎少排了行」，
    /// 其实是记账粒度错了**。
    pub line: u32,
    /// 本片段以一个制表符开头时，该制表符的推进量，单位点。
    ///
    /// 制表符的宽度由制表位定，不是字体里 U+0009 的推进量（Calibri 没有 U+0009 的 cmap，
    /// 落到 glyph 0，12pt 下 121.6 twips）。绘制时画成**一个空格字形**，推进量取这个值——
    /// 「制表符画 1 个空格」是量具方法 §4 的 Windows 计数约定，Mac 与 Android 上**未测**。
    pub tab_advance_pt: Option<f64>,
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
    /// `w:overflowPunct`: allow a supported closing CJK punctuation glyph
    /// outside the text boundary before applying line-start restrictions.
    /// 只在 [`Platform::Desktop`] 下起作用；[`Platform::Android`] 从不挂出（见 [`Platform`]）。
    pub overflow_punct: bool,
    /// 自定义制表位（`w:tabs`）：样式链已合并、`clear` 已剔除，按位置升序。
    ///
    /// 位置相对**左页边距**（正文区左缘），不是相对缩进——ECMA-376 §17.3.1.37
    /// 「with respect to the current page margins」（照规范，**未实测**：实测夹具都没有缩进）。
    pub tabs: Vec<TabStop>,
    /// 默认制表位间距（`w:defaultTabStop`），twips。`None` 表示文档没写。
    ///
    /// 没写时用多少**因平台而异**，由引擎按 [`Platform`] 补（见 [`Engine`] 的
    /// `default_tab_stop`）：桥接层不知道在模拟谁，所以这里不填一个数冒充「文档写了」。
    /// 默认制表位只出现在**最后一个自定义制表位之后**（§17.15.1.25）；实测
    /// `tab-stop-720` 行首制表符停在 720，没有先停在更近的默认档上。
    pub default_tab_stop: Option<Twips>,
    pub source_node: Option<u32>,
    /// 本段的终止符，通常为段落标记或分节符。
    /// 段内软回车和分页符由 `Run::placeholders` 在实际断行处处理。
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
            overflow_punct: true,
            tabs: Vec::new(),
            default_tab_stop: None,
            source_node: None,
            terminator: crate::oracle::LineTerminator::ParagraphMark,
        }
    }
}

/// 制表位的对齐方式（`w:tab/@w:val`）。
///
/// `start`/`end` 在从左到右的段落里就是 `left`/`right`；`num`（旧式列表制表位）
/// 按左对齐处理。`clear` 不进这张表——桥接层合并样式链时就把它用掉了。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabAlign {
    /// 文字从制表位起排。**已实测**（Android，`tab-stop-720` / `tab-stop-1440`）。
    #[default]
    Left,
    /// 后面那段文字以制表位为中心。**未实测**，照规范。
    Center,
    /// 后面那段文字收在制表位上；放不下时制表符不占宽度。
    /// **放不下的一支已实测**（Android，`tab-right-1440`）；放得下时收在哪只有 `tab-right-fit`
    /// 的行界可对照，行内落位未测。
    Right,
    /// 后面那段文字的第一个 `.` 对齐制表位；没有 `.` 时同右对齐。**未实测**，照规范。
    Decimal,
    /// 竖线制表位：只画一条竖线，**不是**制表符的停靠点。竖线本版不画。
    Bar,
}

/// 制表符的前导符（`w:tab/@w:leader`）。
///
/// 只影响绘制，不影响断行与落位。**本版不画前导符**：Windows 侧量到填充字形的
/// 步距就是填充字符的推进量，但个数公式已被证否（量具方法 §4），不猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabLeader {
    #[default]
    None,
    Dot,
    Hyphen,
    Underscore,
    Heavy,
    MiddleDot,
}

/// 一个自定义制表位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TabStop {
    /// 相对左页边距，twips，可负（落进左边距）。
    pub pos: Twips,
    pub align: TabAlign,
    pub leader: TabLeader,
}

/// 文档没写 `w:defaultTabStop` 时桌面 Word 的默认制表位间距：720，ECMA-376 §17.15.1.25
/// 的缺省（rsword 解析器的 `default_tab_stop_or_default` 也给 720）。
///
/// **规范值，Mac 上未实测**：`tools/measure` 的采集与本仓库的夹具里一个制表符都没有。
const DESKTOP_MISSING_DEFAULT_TAB_STOP: Twips = 720;

/// 文档没写 `w:defaultTabStop` 时 Android Word 的默认制表位间距，twips。
///
/// **这是拟合值，不是测量值**：它是「用 rsword 现在的窄路径字宽，按整 twips 算术复现实测
/// 行起点」挑出来的数，绑在本引擎当前的度量上。
///
/// 实测（Android Word 16.0.20513，夹具没有 settings.xml，`word_analyse/reports/rsword-diff/tab.md`）：
/// 行首一个制表符之后，窄路径（5329）放 41 个 `0`（`tab-zeros`）、92 个 `i`（`tab-i`），
/// 纸页路径（10466）放 84 个 `0`（`tab-paper`）。规范的 720 差得远。
///
/// Word 这一档（记作 T）**稳得住的区间是 (123.9, 222.04)**，只用窄路径的数据：
/// 下界 W/43——`zero-plain` 一行 43 个 `0`、`tab-zeros` 制表符后 41 个；
/// 上界 W/24——`i-plain` 一行放不下 96 个 `i`（Word 窄路径的 `i` 大于 W/96），
/// 而 `tab-i` 制表符后放得下 92 个。
/// `tab.md` 写的 217.7–222.0 把纸页路径的 `0` 宽（`zero-paper`）借到了窄路径上，而两条路径的
/// 字宽并不相同：窄路径 `i-plain` 每行 95 个 `i`（Word 的 `i` 大于 5329/96 = 55.51），纸页路径
/// 每行 190 个（不大于 10466/190 = 55.08，`indent.md`）。那个区间不成立。
///
/// 为什么是 221：本引擎的 `0` 是 121.64（Calibri 1038/2048 em），要复现 `tab-zeros` 的 41 个，
/// 这一档必须 ≥ 221（220 + 42 × 121.64 = 5328.9，放得下第 42 个）。往上一直到 248，
/// 五个依赖它的实测夹具（`tab-zeros`、`tab-i`、`tab-after-a`、`tab-paper` 两个宽度）全都照样
/// 复现（249 起 `tab-paper@10466` 变成 0、84）——**记分器分不出 221 与 248**，把它钉在 221 的
/// 只有单元测试。Word 一侧的上界（≈222）来自 `tab-i` 配 Word 自己的窄路径 `i`，
/// 而本引擎的 `i`（55.08）还复现不了那个宽度（`i-plain` 给 96 而不是 95）。
///
/// 若窄路径的推进量将来按像素（1440/778 twips）量化——它能解释 `i-plain` 的 95——
/// Word 的区间变成 (198.3, 220.5]，这一档**必须挪进去**（比如 220 或 214），不能留在 221：
/// 那时 221 + 92 × 55.527 = 5329.48 > 5329，`tab-i` 会少一个 `i`。
///
/// 不是 Calibri 的 U+0009（落到 glyph 0，121.6），不是 720 按视图比例（×5329/10466）缩成的 366，
/// 也不是 zh-CN 缺省 420 缩成的 213.9——后者在量化假说下也复现全部窄路径起点，排除不了。
///
/// 还没量、能分辨它的夹具（`findings` 的测量队列）：
/// - 同一份 `tab-zeros` 写上 `w:defaultTabStop` 720 与 1440：照用给 38 / 32，按视图缩放给
///   41 / 38，不认给 42；
/// - `0<TAB>` + 90 个 `i`、不带尾巴：默认档是栅格给一行，是固定约 221 宽给 0、1；
/// - `tab-zeros` 改成 24pt 放在纸页路径（10466）：固定 221 twips 给下一行起点 43，
///   随字号（约 0.92 em）给 42——现有夹具全是 12pt，分不出它是固定 twips 还是随字号变。
///
/// 两条路径（窄路径、纸页路径）用同一个值是**假设**：`tab-paper@paper` 只把 T 限在
/// (126.6, 248.2]，而纸页路径的横向几何随当前视图宽度变（`indent.md` 的 `wm size` 那一段）。
const ANDROID_MISSING_DEFAULT_TAB_STOP: Twips = 221;

/// 制表符从 `x`（相对左页边距，twips，精确值）起，停到哪个制表位。
///
/// 规则（ECMA-376 §17.3.1.37、§17.15.1.25，加实测）：
///
/// 1. 取位置**严格大于** `x` 的最近一个自定义制表位（竖线制表位不算停靠点）。
///    「最近的自定义制表位」已实测（`tab-stop-720`、`tab-stop-1440`、`tab-right-fit`）；
///    **严格大于**是假设——没有夹具让笔位恰好落在制表位上而结果有别；
/// 2. 首行悬挂缩进时，左缩进处另有一个隐含的左对齐制表位——Word 的列表编号靠它。
///    **未实测**，照 Word 桌面版的已知行为；
/// 3. 都没有时落到默认制表位：从左页边距起每 `step` 一档（`step` 由调用方按平台补好）。
///    这一档只可能在所有自定义制表位之后，因为比 `x` 大的自定义制表位已经在第 1 步用掉了。
///    栅格锚在左页边距是规范的说法，实测只有行首（`x` = 0）一种情形。
fn next_tab_stop(para: &Para, x: f64, first_line: bool, step: Twips) -> (f64, TabAlign) {
    // 与制表位恰好重合时去下一个：笔位已经在那里，停在原地等于没有制表符。
    const EPS: f64 = 1e-6;
    let hanging = (first_line && para.indent_first_line < 0).then_some(TabStop {
        pos: para.indent_left,
        ..TabStop::default()
    });
    let custom = para
        .tabs
        .iter()
        .chain(hanging.iter())
        .filter(|t| t.align != TabAlign::Bar && f64::from(t.pos) > x + EPS)
        .min_by_key(|t| t.pos);
    if let Some(t) = custom {
        return (f64::from(t.pos), t.align);
    }
    let step = f64::from(step.max(1));
    let mut stop = ((x / step).floor() + 1.0) * step;
    if stop <= x + EPS {
        stop += step;
    }
    (stop, TabAlign::Left)
}

#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    /// Omit this run from layout and painting while retaining its UTF-16 source length.
    /// This models hidden text with display of hidden text disabled, on every platform.
    pub hidden: bool,
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
    /// 基线抬升，twips，**正值向上**。`rise_fine` 指定时优先使用精细值。
    ///
    /// 两个来源：`w:vertAlign`（上下标，同时缩小字号——那部分反映在
    /// [`FontSpec::size_half_points`] 里）与 `w:position`（只抬升，不改字号）。
    ///
    /// 挂在 run 上而不是 `FontSpec` 上，是因为它**不影响度量**：
    /// 抬升不改变推进量，只改变落笔的 y。放进 FontSpec 会让度量缓存按它分桶，
    /// 白白多出一倍的键。
    pub rise: Twips,
    /// 精确抬升，1/7200 英寸。未指定时沿用 `rise` 的 twips 值。
    pub rise_fine: Option<i64>,
}

impl Run {
    /// 精确抬升，1/7200 英寸；旧调用方仍可只设置 `rise`。
    pub fn effective_rise_fine(&self) -> i64 {
        self.rise_fine.unwrap_or_else(|| i64::from(self.rise) * FINE_PER_TWIP)
    }
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

/// 段落拍平之后的一截：断行游标在截上往前走，跨 run 回退时退回去。
///
/// 每个 run 按制表符与 [`OBJECT_PLACEHOLDER`] 切开：制表符、占位符各自成一截，其余是文字截
/// （空的不收）。截按文档顺序排，run 边界不另占位置——**run 边界不是断点**，断不断由两侧字符
/// 决定（实测 `webhidden` / `specvanish`，`vanish.md`），所以断行只看截的种类与字符，不看 run。
#[derive(Debug, Clone, Copy)]
struct Segment {
    run_index: usize,
    /// 在 run 文本里的字节区间。
    start: usize,
    end: usize,
    /// 起点在全篇 UTF-16 偏移空间里的下标。
    source: u32,
    kind: SegmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    Text,
    Tab,
    /// 一个 [`OBJECT_PLACEHOLDER`]，种类由桥接层带进来（[`Run::placeholders`]）。
    Placeholder(PlaceholderKind),
}

/// 把段落拍平成截。`source_base` 是本段首字符在全篇 UTF-16 偏移空间里的下标。
///
/// 占位符的种类按 run 内的出现顺序取 [`Run::placeholders`]；对不上的按
/// [`PlaceholderKind::Object`] 处理——保守方向，不凭空造出分页。
fn segments(para: &Para, source_base: u32) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut source = source_base;
    for (run_index, run) in para.runs.iter().enumerate() {
        if run.hidden {
            source += utf16_len(&run.text);
            continue;
        }
        let mut kinds = run.placeholders.iter().copied();
        let (mut text_start, mut text_source) = (0, source);
        for (i, c) in run.text.char_indices() {
            let kind = match c {
                '\t' => SegmentKind::Tab,
                OBJECT_PLACEHOLDER => {
                    SegmentKind::Placeholder(kinds.next().unwrap_or(PlaceholderKind::Object))
                }
                _ => {
                    source += c.len_utf16() as u32;
                    continue;
                }
            };
            if i > text_start {
                out.push(Segment {
                    run_index,
                    start: text_start,
                    end: i,
                    source: text_source,
                    kind: SegmentKind::Text,
                });
            }
            out.push(Segment { run_index, start: i, end: i + c.len_utf8(), source, kind });
            // 制表符与 U+FFFC 都在 BMP 里，各占 1 个 UTF-16 单位。
            source += 1;
            (text_start, text_source) = (i + c.len_utf8(), source);
        }
        if run.text.len() > text_start {
            out.push(Segment {
                run_index,
                start: text_start,
                end: run.text.len(),
                source: text_source,
                kind: SegmentKind::Text,
            });
        }
    }
    out
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// 游标走过第 `at` 截之后停在哪：下一截的起点。
fn next_segment(segs: &[Segment], at: usize) -> (usize, usize) {
    (at + 1, segs.get(at + 1).map_or(0, |s| s.start))
}

/// 游标 (`at`, `byte`) 在全篇 UTF-16 偏移空间里的下标；截走完了就是段末 `end`（不含段落标记）。
fn cursor_source(para: &Para, segs: &[Segment], at: usize, byte: usize, end: u32) -> u32 {
    segs.get(at)
        .map_or(end, |s| s.source + utf16_len(&para.runs[s.run_index].text[s.start..byte]))
}

/// 从游标起吃掉换行处的空格，跨 run 吃，遇到别的字、制表符、占位符或段末就停。返回停下的游标。
fn skip_spaces(para: &Para, segs: &[Segment], mut at: usize, mut byte: usize) -> (usize, usize) {
    while let Some(s) = segs.get(at).filter(|s| s.kind == SegmentKind::Text) {
        let text = &para.runs[s.run_index].text[byte..s.end];
        let after = text.trim_start_matches(' ');
        if !after.is_empty() {
            return (at, s.end - after.len());
        }
        (at, byte) = next_segment(segs, at);
    }
    (at, byte)
}

/// 一截字连一个断点都塞不下时，这一行怎么收（见 `Engine::shortfall`）。
enum Shortfall {
    /// 把这一截的前若干字节放上本行（`fit` 切在断点上，或紧急断行按字符切），然后收行。
    Place(usize, crate::font::TextMetrics),
    /// 这一截不上本行，就此收行。`eat`：吃掉游标处的空格（见 [`skip_spaces`]）。
    Close { eat: bool },
    /// 本行截到第 `piece` 个片段里的 `offset` 字节处（`offset` 为 0 即该片段左侧的交界；
    /// `piece` 等于片段数就是不截），游标退回那里，再收行。
    Truncate { piece: usize, offset: usize, eat: bool },
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
    /// 基线抬升，1/7200 英寸，正值向上。
    rise_fine: i64,
    /// 源字符区间，UTF-16 单位、相对所在段落。
    ///
    /// 断行是唯一知道「切在第几个字符」的地方，所以必须在这里记下；
    /// 事后从文字反推会在重复文本上出错。
    source: (u32, u32),
    /// 本片段与同一行上的前一片段之间隔着行内对象占位符（[`PlaceholderKind::Object`]）。
    ///
    /// 对象两侧可断（UAX #14 的 CB；**假设**，沿用此前的行为，Word 未测），所以这条交界
    /// 是断点。显式记下来，而不是拿「源区间不相接」去推：源区间的缺口将来还会有别的来源
    /// （`w:vanish` 的隐藏文字、删除修订），那些缺口**不是**断点——实测隐藏文字两侧照样
    /// 连着填（`vanish.md`：`vanish` 窄路径 0、73、116）。
    after_object: bool,
    /// 本片段是一个制表符时的来历与推进量。制表符**自成一片**（`text` 是 `"\t"`）：
    /// 它的宽度取决于落在行里哪里、停到哪个制表位，不能和相邻文字一起问度量要。
    tab: Option<TabPiece>,
    /// 本片段起点在断行游标里的位置：第几截（[`Segment`]），以及所在 run 文本里的字节偏移。
    /// 跨 run 回退要把游标退回到某个片段里，靠它。
    seg: usize,
    byte: usize,
}

/// 行内一个制表符。
#[derive(Debug, Clone, Copy)]
struct TabPiece {
    /// 制表符所在 run，以及它之后第一个字节在该 run 文本里的偏移——
    /// 右／居中／小数点制表位要从这里往后量那段文字。
    run_index: usize,
    after: usize,
    /// 落定后的推进量，单位点。
    advance_pt: f64,
    /// 左对齐（含默认档）的停靠点在行尾或行尾之外：制表符只推到行尾，后面的字另起一行。
    /// 断点紧挨在它之前时制表符**留在本行**、不跟下去；前面是粘着的字（`（<TAB>`）时照样试排，
    /// 到下一行落得下就跟下去（见 `Engine::shortfall` 的（丙））。
    past_line_end: bool,
}

/// 右／居中／小数点制表位后面那段文字的宽度（见 `Engine::tab_segment`）。
struct TabSegment {
    /// 整段，twips 精确值。
    whole: f64,
    /// 整段，逐片 `measure` 的整 twips 之和——断行侧的口径。
    whole_twips: Twips,
    /// 第一个 `.` 之前，twips 精确值。没有 `.` 时同 `whole`。
    before_point: f64,
}

/// 排好的一行，尚未定位到页面。
struct PendingLine {
    baseline: Twips,
    pieces: Vec<LinePiece>,
    width: Twips,
    /// 行宽的**精确值**，单位点。对齐（居中 / 右对齐 / 两端对齐）算的是
    /// 「可用宽减行宽」，走整 twips 的 `width` 会把残差搬到每个字形上。
    width_pt: f64,
    is_last: bool,
    /// 本段最后一条排着内容的行：两端对齐不拉伸它。
    ///
    /// 平常与 `is_last` 相同。只有移动视图把段末分页符与段落标记拆成两行时
    /// （[`Engine::splits_page_break_and_mark`]）分页符那行是它、却不是 `is_last`：
    /// 段落标记那行没有内容，拉伸不着。分页符那行照分页视图不拉伸，落位与分页视图逐点相同。
    /// 这是**假设**：Android 的窄路径读数只有码元区间，没有横向位置。
    last_content: bool,
    terminator: crate::oracle::LineTerminator,
    /// Visible spaces for the control characters at this line's end.
    /// A trailing page break and paragraph mark can contribute two together.
    trailing_glyphs: usize,
    end_font: FontSpec,
    end_color: Color,
    end_rise_fine: i64,
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
    /// 游标推进、整段保留与分页预留都使用同一个值，避免逐行取整后
    /// 「放得下」的判断与实际推进相矛盾。
    height_fine: i64,
    /// 本行起点在全篇 UTF-16 偏移空间里的下标。
    ///
    /// 有片段时可以从片段推，但**空行没有片段**——而空行同样要有源位置，
    /// 否则它的行记录就只能给 `None`，下游会读成「判不了」。
    source_start: u32,
    /// 本行**消费到**哪个下标（不含），包括终止符字符。
    ///
    /// 不能拿「最后一个片段的终点 +1」代替：片段之间可能有**不产生片段**的源字符
    /// （对象占位符就是），那时片段终点比行的真实终点小，终止符会被记到错的位置上。
    source_end: u32,
}

/// 模拟哪个平台上的 Word。
///
/// 同一份文档在 Mac Word 与 Android Word 上有几条规则实测相反，而两边的夹具形状一样
/// （最小 docx，没有 settings / styles，也没有 `w:lang`），**文档里没有任何属性能区分**，
/// 只能由调用方说明在模拟谁。所以平台是引擎的一个显式输入，一处设定、各条规则去读，
/// 而不是每条规则各开一个开关。
///
/// 默认 [`Platform::Desktop`]：本仓库既有的测试与 `tools/measure` 的全部采集都是 Mac Word，
/// 不说明平台的调用方（svg / wasm / cffi）行为不变。
///
/// 读它的规则要在自己的注释里写明哪一边是实测（引夹具 / 报告）、哪一边是假设。目前读它的：
///
/// - 文档没写 `w:defaultTabStop` 时的默认制表位（[`Para::default_tab_stop`] 为 `None`）：
///   桌面 720 是规范值，Android 221 是拟合值（见 `ANDROID_MISSING_DEFAULT_TAB_STOP`）。
/// - 行末标点挂出（[`Para::overflow_punct`]）：桌面照段落属性挂出单个越界的 `。，）、`
///   （Mac Word 实测，`docs/PREREG-2026-09-18-kinsoku.md`）；Android 从不挂出，退回前一个
///   合法断点（`kinsoku.md` 实测 `）`、`。`；`，`、`、`、纸页路径与显式开是假设）。
///   行首 / 行尾禁则本身两个平台共用一套（`font::linebreak`）。
///
/// 禁则不读平台，所以它扩充的表（大多是 Android 的断行类读数）与「西文进 CJK 的边界也查禁则」
/// 在桌面上同样生效。库的默认因此有几处 **Mac 上未测**的变化，都没有进 Mac 回放与仓库夹具：
/// 挂出的候选前后两个字读的是同一张表，`22汉。〉10汉` 由挂出的 23 退到 21、`21汉％。10汉`
/// 由 23 退到 20（见 `font::linebreak` 里 `overflow_punctuation_candidates` 的说明）；
/// `21汉…。10汉` 原先断在 `…|。`、把 `。` 放到下一行行首（22），现在退到 20。
/// `tests/kinsoku_android.rs` 的 `desktop_no_longer_hangs_…` 钉着。
///
/// 同样的序列拆在两个 run 里、run 边界挨着那处被挡住的挂出时，跨 run 回退（`Engine::shortfall`）
/// 退回行里更早的断点，与一个 run 里相同：`[21汉％][。10汉]`、`[21汉～][，10汉]` 20，
/// `[22汉][。〉10汉]`、`[22汉][，…10汉]` 21，`[21汉][％。10汉]` 20（**假设**，Word 未测；
/// `tests/kinsoku_android.rs` 的 `assumed_…_on_desktop` 钉着）。跨 run 回退落地之前 run 边界
/// 成了断点，前四种是 22、越界的 `。`、`，` 开下一行，与 Mac 实测的行首禁则相反
/// （`docs/PREREG-2026-09-18-kinsoku2.md` J-a 4/4）。
///
/// 不含制表符（或写了 `w:defaultTabStop`）、也没有越界的收尾标点的文档，两个取值排出来的
/// 结果逐字节相同。
///
/// CJK 回退字体**不在这里读平台**：装不装、装哪个由调用方定（`layout-trace` 在
/// `--platform android` 下默认装仓库里的 Droid，`mac` 不装），库只按已装的字体选字；
/// 谁都画不出的 CJK 字符按名义 1 em 画 `.notdef`，两个平台都一样（见 `FontRegistry`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Platform {
    /// 桌面 Word。**实测基础只有 Word for Mac 16.112**（`tools/measure` 的采集）；
    /// Windows Word 未测，按量具方法 §6.6 两者不能互相替代，这里只是沿用同一套规则（假设）。
    #[default]
    Desktop,
    /// Android Word 16.0.20513.20014（word_analyse `findings/android-word-layout.md`；
    /// 读数在 `reports/diff/*.word.narrow.jsonl` 与 `reports/rsword-diff/*.md`）。
    Android,
}

/// 模拟哪种视图。
///
/// 视图与平台分开：同一平台的两种视图可以排得不一样，而平台差异不该借视图来表达。
/// 默认 [`View::Print`]。
///
/// 目前读它的规则只有一条：**段末手动分页符与段落标记拆不拆成两行**。
/// 分页符之后段内再无内容、段落以普通段落标记结束（不是分节符）时：
///
/// - [`View::Print`]：段落标记收进分页符那一行，留在翻页前那一页。Mac Word **实测**：
///   `breaks-sections` 3 组、`vmisc2` 1 组「只有分页符的段」，分页符与段落标记逐字符报
///   同页同行；`breaks-sections` 里 `'hy'` + 分页符 + 段落标记 3 组也同行。PDF 里两者各画
///   一个空格、同一基线、都在翻页前那一页——前一个是分页符（Times New Roman），后一个是
///   段落标记（run 的字体），相隔 144pt（`captures/breaks-sections-2026-09-17/glyphs.json`
///   页下标 5 的 x=72 / x=216；`vmisc2` 页下标 7、8 同样，后者分页符前有 `'jbefore'`）。
///   `docs/PREREG-2026-09-17-vmisc2.md` §5.1 把 Times 那个空格读成「无可见 run 的段落标记
///   用默认字体」，与它自己的采集不符：Times 那个在分页符的位置上，`'jbefore'` 那段也有可见 run。
///   Android 打印视图是**推断**：`br-page` 第一页 11 条裁剪带、第二页 5 条，与
///   「10 行 + 合并的一行 | 5 行」相符（拆成两行无论落哪页都会多一条带，`br-column-one` 里
///   段落标记独占的一行是有带的）；裁剪带不带码元，这一步没有直测。
/// - [`View::Mobile`]：分页符收行（行终点含分页符），段落标记另起一行。Android Word **实测**：
///   窄路径 `br-page.word.narrow.jsonl` 排成 `…(306,340),(340,341),(341,342),(342,376)…`，
///   一份夹具、一处。分页符之前有文字（`'ab'` + 分页符 + 段落标记）、连续几个段末分页符、
///   以及拆开后段落标记那行落到下一页顶，都是**假设**：Android 只量到「只有分页符的段」，
///   移动视图又没有页。
///
/// 这与 OOXML 兼容项 `w:splitPgBreakAndParaMark`（`w:compat` 下）打开时**行形状相同**，
/// 但不是它：`br-page.docx` 没有 settings.xml，那一项是关的，移动视图照样拆。那一项是文档设置、
/// 两种视图都该认；以后桥接层读到它时另作一个输入，生效条件是「移动视图 **或** 兼容项」，
/// 不要拿它顶替视图（见 `Engine::splits_page_break_and_mark`）。
///
/// 分页符后面段内还有文字或对象时两种视图相同：都在分页符处收行，其余内容从下一页起。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// 分页视图（打印布局）。`tools/measure` 的 Mac 采集与 word_analyse 纸页路径
    /// （`w3=10466`）都是它。
    #[default]
    Print,
    /// 移动视图：Android Word 的连续重排。word_analyse 的窄路径（`w3=5329`）就是它——
    /// 这条重排不走打印分页函数 `DoPsLayoutPage`，页记录只剩一条
    /// （`findings/pagination-path.md`、`findings/page-frame-selector.md`）。
    ///
    /// 只在 Android 上量过；与 [`Platform::Desktop`] 组合不拒绝，但没有任何实测依据。
    /// 移动视图没有页，引擎照样分页，所以这时的页下标只是排版的副产品，不对应 Word。
    Mobile,
}

pub struct Engine<'m, M: FontMetrics> {
    metrics: &'m M,
    setup: PageSetup,
    /// 文字环绕的排除区。空则每行可用区间恒为整个正文宽度。
    wrap: WrapContext,
    /// 模拟哪个平台。见 [`Platform`]。
    platform: Platform,
    /// 模拟哪种视图。见 [`View`]。
    view: View,
}

impl<'m, M: FontMetrics> Engine<'m, M> {
    pub fn new(metrics: &'m M, setup: PageSetup) -> Engine<'m, M> {
        Engine::with_wrap(metrics, setup, WrapContext::new())
    }

    /// 带环绕区构造。
    ///
    /// 有环绕时每行的可用区间由 `y` 决定，而且可能被劈成多段（图片落在段落中间）。
    pub fn with_wrap(metrics: &'m M, setup: PageSetup, wrap: WrapContext) -> Engine<'m, M> {
        Engine {
            metrics,
            setup,
            wrap,
            platform: Platform::default(),
            view: View::default(),
        }
    }

    /// 换一个平台与视图。不调用就是 [`Platform::Desktop`] + [`View::Print`]。
    ///
    /// 两者一起给：会读它们的规则（行末标点挂不挂出、缺省制表位、段末分页符拆不拆……）
    /// 有的看平台、有的看视图，分两次设很容易只设了一半。
    pub fn with_platform(mut self, platform: Platform, view: View) -> Engine<'m, M> {
        self.platform = platform;
        self.view = view;
        self
    }

    /// 当前模拟的平台。
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// 当前模拟的视图。
    pub fn view(&self) -> View {
        self.view
    }

    /// 段末手动分页符之后，段落标记是否另起一行（见 [`View`] 的说明）。
    ///
    /// 只看视图：[`View::Mobile`] 拆，[`View::Print`] 不拆。以后读到文档的兼容项
    /// `w:splitPgBreakAndParaMark` 时，在这里与视图取「或」。
    ///
    /// 段落以分节符结束（段内 `w:sectPr`）时不拆，分节符照分页视图收进分页符那一行。
    /// 这一条是**假设**：Android 只量到以段落标记结束的 `br-page`，OOXML 那个兼容项也只说
    /// 段落标记；拆开的话分节符那行落到下一页、分节再翻一页，平白多出一页。
    fn splits_page_break_and_mark(&self, para: &Para) -> bool {
        self.view == View::Mobile
            && para.terminator == crate::oracle::LineTerminator::ParagraphMark
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
            let block_height_fine: i64 = lines.iter().map(|l| l.height_fine).sum();

            if para.page_break_before && !page.fragments.is_empty() {
                pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor_fine = fine(area.y);
                line_index = 0;
            }

            cursor_fine += fine(para.space_before);

            // keepLines：整段放不下就先翻页（除非本页是空的，那样翻了也没用）。
            if para.keep_lines
                && cursor_fine + block_height_fine > fine(area.bottom())
                && !page.fragments.is_empty()
            {
                pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor_fine = fine(area.y);
                line_index = 0;
            }

            // keepNext：本段是最后一段时无意义；否则要保证下一段至少第一行同页。
            let next_first_line_fine = if para.keep_next {
                paras.get(idx + 1).and_then(|n| {
                    // 只取高度，源区间用不上；给下一段的正确基点，免得读代码时费解。
                    self.break_paragraph(n, area, coarse(cursor_fine), source_cursor)
                        .first()
                        .map(|l| l.height_fine)
                }).unwrap_or(0)
            } else {
                0
            };

            let total_lines = lines.len();
            for (li, line) in lines.into_iter().enumerate() {
                let mut needed_fine = line.height_fine;
                // 最后一行还要替下一段的首行占位。
                if para.keep_next && li + 1 == total_lines {
                    needed_fine += next_first_line_fine;
                }
                if cursor_fine + needed_fine > fine(area.bottom()) && !page.fragments.is_empty() {
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

        // 两端对齐：除最后一行外，把空隙按**词间**的片段交界均摊。
        //
        // 按片段交界摊是既有近似（Word 按空格摊，本引擎未实现，也未测）。只摊在词间交界上：
        // 交界是断点（[`Self::join_breaks`]），或右侧片段以空白开头——UAX #14 的断点在空格
        // 之后，但空格之前那条交界同样在两词之间。词内的 run 边界不摊：紧急断行把一个词
        // 跨 run 填进一行之后，空隙会落在词中间（20 + 30 + 80 个 `0` 分三个 run、5329 宽，
        // 两端对齐时第 20、21 个 `0` 之间会凭空多出约 98 twips；同样的字排在一个 run 里没有）。
        //
        // 也只摊在**最后一个制表符之后**：制表符之前的文字、以及紧跟制表符的那一片，
        // 已经按制表位定死了位置，再拉开就离开了制表位。没有制表符时 `justify_from` 是 0，
        // 与原来一致。这条照 Word 的已知行为，**未实测**；而且眼下几乎不起作用——
        // 空隙只摊在片段交界上，单 run 的行根本不拉伸，只有制表符之后还有多个 run 时才看得出。
        let justify_from = line
            .pieces
            .iter()
            .rposition(|p| p.tab.is_some())
            .map_or(0, |i| i + 1);
        let word_join: Vec<bool> = line
            .pieces
            .iter()
            .enumerate()
            .map(|(i, p)| {
                i > justify_from
                    && (self.join_breaks(&line.pieces[i - 1], p)
                        || p.text.starts_with([' ', '\t']))
            })
            .collect();
        let gaps = word_join.iter().filter(|&&w| w).count();
        let justify = para.align == Align::Justify && !line.last_content && gaps > 0;
        let justify_gap = if justify { slack / gaps as Twips } else { 0 };
        let justify_gap_pt = if justify { slack_pt / gaps as f64 } else { 0.0 };

        // 基线落位：在 1/7200 英寸上加好再交给度量量化到它的栅格。
        // 先取整到 twips 再量化是不行的——0.24pt = 4.8 twips，取整就把栅格点碾碎了。
        let fine = |t: Twips| i64::from(t) * FINE_PER_TWIP;
        let coarse = |f: i64| ((f as f64) / FINE_PER_TWIP as f64).round() as Twips;
        let baseline_fine = self
            .metrics
            .quantize_baseline_fine(top_fine + fine(line.baseline));
        let baseline_y = coarse(baseline_fine);
        let last = line.pieces.len().saturating_sub(1);
        let suffix_start = line.source_end - line.trailing_glyphs as u32;
        let append_suffix = line.pieces.last().is_some_and(|p| {
            p.source.1 == suffix_start
                && p.font == line.end_font
                && p.color == line.end_color
                && p.rise_fine == line.end_rise_fine
        });
        if let Some(first_piece) = line.pieces.first()
            && line.source_start < first_piece.source.0
        {
            page.fragments.push(Fragment::Text(TextFragment {
                x: base_x + offset,
                x_pt: base_x_pt + offset_pt,
                baseline_y,
                baseline_fine,
                text: String::new(),
                font: first_piece.font.clone(),
                color: first_piece.color,
                source_node: para.source_node,
                source: Some((line.source_start, first_piece.source.0)),
                terminator: crate::oracle::LineTerminator::Wrapped,
                rise: 0,
                rise_fine: 0,
                line: line_index,
                tab_advance_pt: None,
            }));
        }
        // 本片段之前（含本片段左侧那条交界）摊到了几份空隙。
        let mut gaps_before = 0usize;
        for (i, p) in line.pieces.iter().enumerate() {
            if word_join[i] {
                gaps_before += 1;
            }
            let x = base_x + offset + p.dx + justify_gap * (gaps_before as Twips);
            let x_pt = base_x_pt + offset_pt + p.dx_pt + justify_gap_pt * (gaps_before as f64);
            // Only the final fragment carries the line's terminator.
            let terminator = if i == last && append_suffix {
                line.terminator
            } else {
                crate::oracle::LineTerminator::Wrapped
            };
            let (text, source) = if i == last && append_suffix {
                with_terminator_glyphs(&p.text, p.source, line.source_end, line.trailing_glyphs)
            } else {
                (p.text.clone(), p.source)
            };
            page.fragments.push(Fragment::Text(TextFragment {
                x,
                x_pt,
                // 抬升是**向上**的，而页内 y 向下增长，所以要减。
                baseline_y: coarse(baseline_fine - p.rise_fine),
                baseline_fine: baseline_fine - p.rise_fine,
                text,
                font: p.font.clone(),
                color: p.color,
                source_node: para.source_node,
                source: Some(source),
                terminator,
                rise: (p.rise_fine / FINE_PER_TWIP) as Twips,
                rise_fine: p.rise_fine,
                line: line_index,
                tab_advance_pt: p.tab.map(|t| t.advance_pt),
            }));
        }

        // 空行也要产出一个片段。
        //
        // 没有它，空行就**没有任何绘制指令**，于是下游根本看不到这一行——
        // 而 Word 是给空行一条行记录的（实测：独占一行的分页符、空段落都有）。
        // 那种缺失在比较器里表现为「引擎少排了行」，查起来像分页错，其实是这里漏了。
        if !append_suffix {
            let terminator = line.terminator;
            let tail_start = line.pieces.last().map_or(line.source_start, |p| p.source.1);
            // Keep nonpainting controls separate, so glyph source spans do not
            // absorb a page/section break or an omitted object placeholder.
            let mut tails = Vec::new();
            if tail_start < suffix_start && line.trailing_glyphs > 0 {
                tails.push((
                    String::new(),
                    (tail_start, suffix_start),
                    crate::oracle::LineTerminator::Wrapped,
                ));
            }
            tails.push((
                " ".repeat(line.trailing_glyphs),
                (
                    if line.trailing_glyphs > 0 { suffix_start } else { tail_start },
                    line.source_end,
                ),
                terminator,
            ));
            for (text, source, terminator) in tails {
                page.fragments.push(Fragment::Text(TextFragment {
                    x: base_x + offset + line.width + justify_gap * gaps as Twips,
                    x_pt: base_x_pt + offset_pt + line.width_pt + justify_gap_pt * gaps as f64,
                    baseline_y: coarse(baseline_fine - line.end_rise_fine),
                    baseline_fine: baseline_fine - line.end_rise_fine,
                    text,
                    font: line.end_font.clone(),
                    color: line.end_color,
                    source_node: para.source_node,
                    source: Some(source),
                    terminator,
                    rise: (line.end_rise_fine / FINE_PER_TWIP) as Twips,
                    rise_fine: line.end_rise_fine,
                    line: line_index,
                    tab_advance_pt: None,
                }));
            }
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
        use crate::oracle::{LineTerminator as T, PageBreakPosition as B};
        let mut lines: Vec<PendingLine> = Vec::new();
        let paragraph_end = source_base
            + para.runs.iter().map(|r| r.text.encode_utf16().count() as u32).sum::<u32>();
        let segs = segments(para, source_base);

        // Hidden text still owns source positions. Its paragraph mark remains
        // visible; without a separate mark style, keep the existing font
        // fallback for an entirely hidden paragraph.
        if segs.is_empty() {
            let mark_run = para
                .runs
                .iter()
                .find(|run| !run.hidden)
                .or_else(|| para.runs.last());
            let font = mark_run
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
                last_content: true,
                terminator: para.terminator,
                trailing_glyphs: para.terminator.expected_glyphs(),
                end_font: font,
                end_color: mark_run.map_or(Color::BLACK, |r| r.color),
                end_rise_fine: mark_run.map_or(0, Run::effective_rise_fine),
                is_first: true,
                span,
                page_break_after: false,
                source_start: source_base,
                source_end: paragraph_end + 1,
            });
            return lines;
        }

        // 段落拍平成截（见 [`Segment`]）：断行游标 (`at`, `byte`) 在截上往前走，
        // 跨 run 回退时退回本行更早的断点（见 [`Self::shortfall`]）。
        // 游标：第几截，以及该截所在 run 文本里的字节偏移。
        let mut at: usize = 0;
        let mut byte: usize = segs.first().map_or(0, |s| s.start);
        let mut cur: Vec<LinePiece> = Vec::new();
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
        // 本行上一个片段（或行首）之后刚跨过一个行内对象占位符：下一个片段与它之间的交界
        // 是断点。见 `LinePiece::after_object`。每推一个片段、每收一行都清掉。
        let mut object_join = false;
        // 行高未知时用来试探区间的估值：取正文字号的自然行高。
        let probe_h = self
            .metrics
            .measure("", &para.runs[segs[0].run_index].font)
            .natural_height()
            .max(1);
        let mut cur_y = y;

        // 本行可用区间：有环绕时按 y 查，可能被劈成多段，这里取最宽的一段。
        // 取最宽而非逐段填充，是因为把一行拆到不连续区间里需要把行再切分，
        // 那是后续的事；取最宽段保证不与图片重叠，且是保守的正确方向。
        let mut span = self.pick_span(para, area, cur_y, probe_h);
        // 首行缩进吃掉的宽度。
        let mut line_avail = (span.width() - if first_line { para.indent_first_line } else { 0 }).max(1);

        while let Some(&seg) = segs.get(at) {
            let run = &para.runs[seg.run_index];
            match seg.kind {
                SegmentKind::Placeholder(kind) => {
                    // 每个 `w:br` / `w:cr` 在 run 文本里是一个 U+FFFC（对象替换符）：分页、分栏是
                    // 解析器放的，软回车是桥接层把解析器的 `'\n'` 换成的（`bridge::run_text_and_placeholders`）。
                    // 它是**控制字符，不是文字**：占 1 个源字符位（Word 的 `Range` 数它），
                    // 但既不成字形也不占宽度——Word 导出的 PDF 里一个都没有。
                    //
                    // 不切掉的后果是实测过的：它会被整形器当普通字符画出来，
                    // 12pt 字号下 advance 12.0pt，其后整行字形集体右移；
                    // 一份 11 页夹具上 8 次共凭空占掉 96pt。
                    //
                    // 在这里切而不是在绘制层滤，是因为**断行也不能算它的宽度**：
                    // 只在绘制层滤，行宽照样是错的，而那种错在轨迹里看不出来。
                    // 占位符可能夹在文字中间（实测 `'分页符之前￼分页符之后'`），所以它自成一截。
                    //
                    // 源游标走 1 个 UTF-16 单位，但不产生片段、不占宽度。
                    let consumed = seg.source + 1;
                    (at, byte) = next_segment(&segs, at);

                    // 软回车紧跟在一条刚按宽度收下的行后面、中间什么都没有（Desktop 把 `。`、`）`
                    // 挂出行末后当场收行，或末尾空格溢出时）：它收进那一行，不自成一条空行——
                    // 同一位置的段落标记也是收进那一行的（见末行那一支的条件）。**推断**：
                    // Word 没有这一形状的读数；拆成两行会凭空多一行，比收进去离 Word 的行数更远。
                    if kind == PlaceholderKind::LineBreak
                        && cur.is_empty()
                        && cur_w == 0
                        && let Some(last) = lines.last_mut()
                        && last.terminator == T::Wrapped
                        && last.source_end == seg.source
                    {
                        last.terminator = T::LineBreak;
                        last.trailing_glyphs += T::LineBreak.expected_glyphs();
                        last.source_end = consumed;
                        last.end_font = run.font.clone();
                        last.end_color = run.color;
                        last.end_rise_fine = run.effective_rise_fine();
                        line_start = consumed;
                        object_join = false;
                        continue;
                    }

                    // 占位符是什么，决定要不要在此处收行／翻页。种类由桥接层带进来；
                    // 拿不到就按 `Object` 处理——保守方向，不凭空造出分页（见 [`segments`]）。
                    if kind.breaks_line() {
                        let at_end = consumed == paragraph_end;
                        // 分页符紧跟段落标记：分页视图把段落标记收进本行（Mac `breaks-sections`
                        // / `vmisc2` 实测）；移动视图不收，段落标记由末行那一支另起一行
                        // （Android 窄路径 `br-page` 实测）。见 [`View`]。
                        let keeps_mark = at_end && !self.splits_page_break_and_mark(para);
                        let terminator = if kind.breaks_page() {
                            T::PageBreak(if keeps_mark && para.terminator == T::ParagraphMark {
                                B::BeforeMark
                            } else if cur.is_empty() {
                                B::OwnLine
                            } else {
                                B::MidParagraph
                            })
                        } else {
                            T::LineBreak
                        };
                        let ends_paragraph = kind.breaks_page() && keeps_mark;
                        // 收行。空行也要收：`'\u{FFFC}文字'` 这种分页符在段首的情形，
                        // 前面确实是一条空行（Word 也给它一条独立的行记录）。
                        // 显式换行收下的这一行此后再不回退：退回只在本行之内找断点。
                        let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
                        let h = if cur.is_empty() && cur_w == 0 {
                            // 空行高度由该 run 的字体定，不能取 0——否则后面的行会叠上来。
                            let m = self.metrics.empty_line_metrics(&run.font);
                            cur_ascent = m.ascent;
                            cur_descent = m.descent;
                            cur_natural_fine = self.metrics.natural_height_fine("", &run.font);
                            self.line_height(para, m.ascent + m.descent, m.natural_height())
                        } else {
                            h
                        };
                        lines.push(PendingLine {
                            height_fine: self.line_height_fine(para, cur_ascent + cur_descent, cur_natural_fine),
                            baseline: cur_ascent,
                            pieces: std::mem::take(&mut cur),
                            width: cur_w,
                            width_pt: cur_w_pt,
                            is_last: ends_paragraph,
                            // 拆开时这行仍是本段最后一行内容（见 `PendingLine::last_content`）。
                            last_content: kind.breaks_page() && at_end,
                            terminator,
                            trailing_glyphs: terminator.expected_glyphs()
                                + if ends_paragraph { para.terminator.expected_glyphs() } else { 0 },
                            end_font: run.font.clone(),
                            end_color: run.color,
                            end_rise_fine: run.effective_rise_fine(),
                            is_first: first_line,
                            span,
                            page_break_after: kind.breaks_page(),
                            source_start: line_start,
                            source_end: consumed + u32::from(ends_paragraph),
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
                        object_join = false;
                    } else {
                        // 行内对象：不收行，但它与后一片段之间的交界是断点。
                        object_join = true;
                    }
                }
                SegmentKind::Tab => {
                    // 制表符自成一片：它的宽度取决于落在行里哪里、停到哪个制表位，
                    // 不能和相邻文字一起问度量要。退回到它之前时游标会再走过它，
                    // 到新行上重新落位——所以这里不必记「挪下去的制表符」。
                    let info = TabPiece {
                        run_index: seg.run_index,
                        after: seg.end,
                        advance_pt: 0.0,
                        past_line_end: false,
                    };
                    // 制表位相对左页边距；行首相对它的位置 = 区间起点 + 首行缩进。
                    let origin = f64::from(
                        span.start - area.x + if first_line { para.indent_first_line } else { 0 },
                    );
                    let (advance, past_line_end, landed_w) =
                        self.land_tab(para, info, origin, cur_w_pt * 20.0, line_avail, first_line);
                    let m = self.metrics.measure(" ", &run.font);
                    cur.push(LinePiece {
                        dx: cur_w,
                        dx_pt: cur_w_pt,
                        text: "\t".to_string(),
                        font: run.font.clone(),
                        color: run.color,
                        rise_fine: run.effective_rise_fine(),
                        source: (seg.source, seg.source + 1),
                        after_object: std::mem::take(&mut object_join),
                        tab: Some(TabPiece { advance_pt: advance / 20.0, past_line_end, ..info }),
                        seg: at,
                        byte: seg.start,
                    });
                    // 行宽随制表位对齐到精确位置：左对齐制表位落在整 twips 上，
                    // 不带上前面各片段逐个取整的残差；右／居中／小数点见 `land_tab`。
                    cur_w = cur_w.max(landed_w);
                    cur_w_pt += advance / 20.0;
                    cur_ascent = cur_ascent.max(m.ascent);
                    cur_descent = cur_descent.max(m.descent);
                    cur_natural = cur_natural.max(m.natural_height());
                    cur_natural_fine =
                        cur_natural_fine.max(self.metrics.natural_height_fine(" ", &run.font));
                    (at, byte) = next_segment(&segs, at);
                }
                SegmentKind::Text => {
                    // 游标所在这一截剩下的文字（到下一个制表符、占位符或 run 末为止）。
                    let rest = &run.text[byte..seg.end];
                    // 游标在全篇 UTF-16 偏移空间里的下标，作为片段源区间的起点
                    // （从本段基点起算，不从 0）。退回时跟着游标退（[`cursor_source`]）。
                    let mut consumed = seg.source + utf16_len(&run.text[seg.start..byte]);
                    if rest.is_empty() {
                        (at, byte) = next_segment(&segs, at);
                        continue;
                    }
                    let remain = (line_avail - cur_w).max(0);
                    let m_all = self.metrics.measure(rest, &run.font);

                    if m_all.advance <= remain {
                        // 整截放得下。
                        cur.push(LinePiece {
                            dx: cur_w,
                            dx_pt: cur_w_pt,
                            text: rest.to_string(),
                            font: run.font.clone(),
                            color: run.color,
                            rise_fine: run.effective_rise_fine(),
                            source: (consumed, consumed + utf16_len(rest)),
                            after_object: std::mem::take(&mut object_join),
                            tab: None,
                            seg: at,
                            byte,
                        });
                        cur_w += m_all.advance;
                        cur_w_pt += self.metrics.advance_pt(rest, &run.font);
                        cur_ascent = cur_ascent.max(m_all.ascent);
                        cur_descent = cur_descent.max(m_all.descent);
                        cur_natural = cur_natural.max(m_all.natural_height());
                        cur_natural_fine =
                            cur_natural_fine.max(self.metrics.natural_height_fine(rest, &run.font));
                        (at, byte) = next_segment(&segs, at);
                        continue;
                    }

                    // 放不下：先在这一截里找能塞进去的最长前缀。
                    let context = OverflowPunctuationContext {
                        previous: cur.last()
                            .filter(|piece| !object_join && piece.tab.is_none())
                            .and_then(|piece| piece.text.chars().last()),
                        // 紧跟着的制表符与占位符一样是屏障；run 边界不是。
                        next: segs
                            .get(at + 1)
                            .filter(|s| s.kind == SegmentKind::Text)
                            .and_then(|s| para.runs[s.run_index].text[s.start..].chars().next()),
                    };
                    // 行末标点挂出只在桌面上做：段落 `w:overflowPunct`（桥接层没写时给开）只在
                    // [`Platform::Desktop`] 下起作用。
                    // - 桌面：Mac Word 实测挂出单个越界的 `。，）、`（`docs/PREREG-2026-09-18-kinsoku.md`，
                    //   K-a～K-d 4/4，第一行 38 字）；显式关掉才轮到行首禁则（`kinsoku2`，36 字）。
                    // - Android：同样没写 `w:overflowPunct` 的最小 docx，越界的 `）`、`。` 都**不**挂出，
                    //   退回前一个合法断点（word_analyse `reports/rsword-diff/kinsoku.md`，移动视图
                    //   窄路径 5329 twips：`kinsoku`、`kinsoku-period` 都是 21，不是 23）。
                    //   以下三条是**假设**，没有夹具：`，`、`、` 同样不挂出（只量了 `）`、`。`）；
                    //   纸页路径（10466）与移动视图一样不挂出；显式写了 `w:overflowPunct w:val="1"`
                    //   也不挂出。所以这里只看平台、不看视图。
                    let hang = para.overflow_punct && self.platform == Platform::Desktop;
                    let decision = match self.metrics.fit_with_overflow_punctuation_context(
                        rest, &run.font, remain, hang,
                        context,
                    ) {
                        Some((cut, m)) if cut > 0 => Shortfall::Place(cut, m),
                        // 一个断点都塞不下。
                        _ => self.shortfall(para, area, span, remain, &cur, object_join, &segs, at, byte),
                    };
                    let (eat, end_run) = match decision {
                        Shortfall::Place(cut, m) => {
                            let piece = &rest[..cut];
                            cur.push(LinePiece {
                                dx: cur_w,
                                dx_pt: cur_w_pt,
                                text: piece.to_string(),
                                font: run.font.clone(),
                                color: run.color,
                                rise_fine: run.effective_rise_fine(),
                                source: (consumed, consumed + utf16_len(piece)),
                                after_object: std::mem::take(&mut object_join),
                                tab: None,
                                seg: at,
                                byte,
                            });
                            cur_w += m.advance;
                            cur_w_pt += self.metrics.advance_pt(piece, &run.font);
                            cur_ascent = cur_ascent.max(m.ascent);
                            cur_descent = cur_descent.max(m.descent);
                            cur_natural = cur_natural.max(m.natural_height());
                            cur_natural_fine = cur_natural_fine
                                .max(self.metrics.natural_height_fine(piece, &run.font));
                            byte += cut;
                            (true, seg.run_index)
                        }
                        Shortfall::Close { eat } => (eat, seg.run_index),
                        Shortfall::Truncate { piece, offset, eat } => {
                            if let Some(p) = cur.get_mut(piece) {
                                // 退回：游标回到切口，切口之后的片段（含制表符）到下一行重排。
                                (at, byte) = (p.seg, p.byte + offset);
                                if offset > 0 {
                                    p.text.truncate(offset);
                                    p.source.1 = p.source.0 + utf16_len(&p.text);
                                    cur_w = p.dx + self.metrics.measure(&p.text, &p.font).advance;
                                    cur_w_pt = p.dx_pt + self.metrics.advance_pt(&p.text, &p.font);
                                    cur.truncate(piece + 1);
                                } else {
                                    cur_w = p.dx;
                                    cur_w_pt = p.dx_pt;
                                    cur.truncate(piece);
                                }
                                // 行高按剩下的片段重算：切走的那部分字体未必与前文相同。
                                (cur_ascent, cur_descent, cur_natural, cur_natural_fine) =
                                    self.pieces_vertical(&cur);
                            }
                            (eat, segs[at].run_index)
                        }
                    };
                    // 换行处吃掉的空格在源侧**仍然占位**，记进本行。不记进游标的话，
                    // 本段后续所有片段的源区间会整体前移，而这种错在几何上
                    // 看不出来，只会让配对悄悄错位。空格跨 run 也照吃，拆不拆 run 下一行
                    // 都从同一个字起（**假设**：Word 在 run 边界上的行尾空格未测；原先只吃到
                    // 本 run 末尾，下一行可能以空格开头）。
                    if eat {
                        (at, byte) = skip_spaces(para, &segs, at, byte);
                    }
                    consumed = cursor_source(para, &segs, at, byte, paragraph_end);

                    // 收行。行尾字体取切口所在的 run（与原先「本截所在 run」同一口径）。
                    let run = &para.runs[end_run];
                    if cur.is_empty() && cur_w == 0 {
                        // 只有行内对象的一行（对象在行首，后面的长串一个字也不上这一行）。
                        // 对象本身不占宽也不占高，行高按该 run 的字体取，与空行同一口径。
                        let m = self.metrics.empty_line_metrics(&run.font);
                        cur_ascent = m.ascent;
                        cur_descent = m.descent;
                        cur_natural = m.natural_height();
                        cur_natural_fine = self.metrics.natural_height_fine("", &run.font);
                    }
                    let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
                    lines.push(PendingLine {
                        height_fine: self.line_height_fine(para, cur_ascent + cur_descent, cur_natural_fine),
                        baseline: cur_ascent,
                        pieces: std::mem::take(&mut cur),
                        width: cur_w,
                        width_pt: cur_w_pt,
                        is_last: false,
                        last_content: false,
                        terminator: T::Wrapped,
                        trailing_glyphs: 0,
                        end_font: run.font.clone(),
                        end_color: run.color,
                        end_rise_fine: run.effective_rise_fine(),
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
                    object_join = false;
                    // 换行：y 推进一行高，可用区间随之可能变化。
                    cur_y += h;
                    span = self.pick_span(para, area, cur_y, h.max(probe_h));
                    line_avail = span.width().max(1);
                }
            }
        }

        // 末行。
        //
        // 分页符收行而没收下段落标记（只在移动视图拆开段末分页符时发生）：段落标记自成一行，
        // 与段末软回车之后那条空行同理；它排在翻页之后，所以落到下一页。分页视图下分页符那行
        // 要么已收下段落标记（`is_last`），要么其后还有内容（`paragraph_end > source_end`），
        // 这一条不会多出行来。
        if !cur.is_empty()
            || lines.is_empty()
            || lines.last().is_some_and(|l| l.terminator == T::LineBreak)
            || lines.last().is_some_and(|l| paragraph_end > l.source_end)
            || lines.last().is_some_and(|l| l.page_break_after && !l.is_last)
        {
            let mark_run = para.runs.iter().rev().find(|run| !run.hidden)
                .expect("nonempty visible paragraph");
            if cur.is_empty() {
                let font = &mark_run.font;
                let m = self.metrics.empty_line_metrics(font);
                cur_ascent = m.ascent;
                cur_descent = m.descent;
                cur_natural_fine = self.metrics.natural_height_fine("", font);
            }
            lines.push(PendingLine {
                height_fine: self.line_height_fine(para, cur_ascent + cur_descent, cur_natural_fine),
                baseline: cur_ascent,
                pieces: cur,
                width: cur_w,
                width_pt: cur_w_pt,
                is_last: true,
                last_content: true,
                terminator: para.terminator,
                trailing_glyphs: para.terminator.expected_glyphs(),
                end_font: mark_run.font.clone(),
                end_color: mark_run.color,
                end_rise_fine: mark_run.effective_rise_fine(),
                is_first: first_line,
                span,
                page_break_after: false,
                source_start: line_start,
                source_end: paragraph_end + 1,
            });
        } else if let Some(last) = lines.last_mut()
            && !last.is_last
        {
            last.is_last = true;
            last.last_content = true;
            last.terminator = para.terminator;
            last.trailing_glyphs = para.terminator.expected_glyphs();
            last.source_end += 1;
        }

        lines
    }

    /// 游标处这一截字（`segs[at]` 从 `byte` 起）连一个断点都塞不下——普通断行与桌面的挂出都不成——
    /// 时，这一行怎么收。`remain` 是本行剩下的宽度，`cur` 是本行已排的片段。
    ///
    /// 先看行尾与这一截之间的交界。交界是断点（隔着行内对象，或两侧字符照 `break_opportunities`
    /// 可断）就此收行，这一截整个挪到下一行。否则找本行**行首之后最后一个断点**
    /// （[`Self::last_break_candidate`]），分三种：
    ///
    /// **（甲）有断点、这一截不粘在制表符上：退回那里**（跨 run 回退）。本行截到那个断点（必要时把
    /// 一个片段切开），游标退回去，切口之后的片段（含制表符）到下一行重排、重新落位。排在一个 run
    /// 里时 `fit` 本来就退到那里；拆成几个 run 不该换一种断法——**run 边界不是断点**是实测
    /// （`webhidden`、`specvanish` 20 + 30 + 80 个 `0` 仍是 0、86，`vanish.md`），而断点落在更早的
    /// run 里、要退回去的情形 Word **未测**（待测：`hello wor` | `ld` + 60 个 `0`、`[22汉][）10汉]`、
    /// `[22汉][。10汉]`、`[21汉（][10汉]` @5329），这里照「拆不拆 run 断法都一样」推。
    ///
    /// **（乙）一个断点都没有、这一截不粘在制表符上：紧急断行**。这一行从行首起就没有断点，
    /// 挪到下一行也不会有，只能按字符切，切在最后一个放得下的字符之后（切法见
    /// `FontMetrics::fit_clusters`）。实测 Android Word（`word_analyse/reports/rsword-diff`）：
    /// 一串 `0` 在 5329 上每行 43 个、10466 上 86 个（`tab.md`、`char-scale.md`）；
    /// 20 + 30 + 80 个 `0` 分在三个 run 里仍是 86 个（`vanish.md`）——已有内容的行也照样
    /// 按剩余宽度切。第一个字符就放不下：空行仍收它，避免死循环（**假设**：不出空行）；
    /// 已有内容的行就此收行。
    ///
    /// 「行首之后有更早的断点就断在那里，长串整个挪到下一行」只在制表符上量过：
    /// `tab-right-1440`（0、1、45：`A` 加 43 个 `0` 超出 5329，只能分两行）与
    /// `tab-right-fit`（0、4、48：1440 之后空着约 3.9k twips 也没有紧急填满）。
    /// `tab-after-a`（0、1、43）分不出是不是视觉上的两行，不作依据。
    /// 空格与 CJK 交界上的同一条规则是**假设**，没有夹具。
    ///
    /// 制表符与后面的字之间不断、断点在制表符之前（实测，`tab.md`），所以紧跟在制表符后面、
    /// 中间一个断点都没有的这个词（「粘在制表符上」，可以跨 run）另有两条。粘着的那截字分两种，
    /// 决定用不用紧急断行：
    /// - 空行上也放不下（到第一个断点的那截去掉词后的空格，仍比下一行的整宽宽，
    ///   比如一长串 `0`）。
    ///   实测过的制表符夹具全是这一种（`tab.md` 的八份，放不下的那截都是 50～120 个
    ///   `0`／`i`，比一行长），紧急断行按剩余宽度切（`tab-zeros`：制表符后 41 个 `0`）；
    /// - 空行上放得下的词（`Sincerely,`、`Date: today`）。**假设**：它不在词中间被切开，
    ///   也不被推出行尾，而是整个挪到能从头放下它的地方。没有夹具——照上面实测的
    ///   几条规则硬推（制表符之前可断、之后不可断，行首之后没有断点就按剩余宽度切），
    ///   Word 会把它切开（行首制表符停在 5040 的 `\tSincerely,` @5329 给 `\tSi` | `ncerely,`）；
    ///   这里认为那不像 Word，按下面的办法不切。待测：`\t` + 94 个 `i` @5329（照这里 0、1，
    ///   硬推 0、93），与左对齐 5040 的 `Name:<TAB>Date: today` @5329（照这里 0、6，硬推 0、5、8）。
    ///
    /// **（丙）粘在制表符上、制表符之前有断点：退回那里，制表符跟着下去**，到下一行重新落位。
    /// 实测 `tab-right-1440`（0、1、45）：第二行那 43 个 `0` 得从 x ≤ 98.5 twips 起，
    /// 比 `A` 的宽（138.87）还小，所以制表符之前确有一次换行，制表符跟着 `0` 下去了；
    /// `tab-after-a`（0、1、43）、`tab-stop-720`（0、1、39）、`tab-stop-1440`
    /// （0、1、33、76）的行起点与之一致（`tab.md`）。
    ///
    /// 制表符之前那一处交界照 `chars_break(前一个字, Some('\t'))` 判断，不一律当作可断：
    /// 行尾禁则字后面不记「制表符之前」（`（<TAB>` 之间不断，**假设**，见 `font::linebreak`），
    /// 那时退到更早的断点，`（` 跟着制表符下去（`20汉（<TAB>Sincerely` @5329 退到 `汉|（`，20；
    /// 默认档 221 与 720 都是，例外见下面「够不着」一段的末尾）。
    ///
    /// 退之前先在下一行试排一次：切口到制表符那一截重新落下之后，制表符停到哪、后面的字上不上得来
    /// （放得下的词要上来到第一个断点，放不下的长串至少上来一个字）。上不来就不退——退下去只会排出
    /// 一条只有制表符的行、或把词切开——制表符**留在本行**，字另起一行、前面不带制表符
    /// （`Name:<TAB>` | `Date: today`、`A<TAB>` | `B C`）。**假设**，未实测。
    /// 试排按本行的区间估下一行；有环绕时下一行的区间可能不同，那时落到（丁）。
    ///
    /// 左对齐停靠点在行尾或行尾之外（`TabPiece::past_line_end`，制表符只推到行尾）分两种：
    /// - 断点紧挨在制表符之前：不试、不退，制表符留在本行，字另起一行、从行首排
    ///   （默认档 720 上的 `21汉<TAB>Sincerely` @5329：`21汉<TAB>` | `Sincerely`）。**假设**，未实测
    ///   （待测：左对齐 6480 的 `A<TAB>000` @5329）。
    /// - 断点在制表符前那截粘着的字之前（`（<TAB>`）：不退的话 `（` 留在行尾，违反禁则，所以照样
    ///   试排。本行的停靠点够不着，不等于下一行也够不着：默认档从下一行较前的位置起落得下
    ///   （默认档 720 的 `20汉（<TAB>Sincerely` @5329：第二行 `（` 240、制表符停到 720，退到
    ///   `汉|（`，20；两个平台一样）。试排里制表符到下一行仍然够不着（左对齐停靠点在行外，比如
    ///   6480）就不退，`（` 与制表符留在行尾——**已知偏差**：退下去 `（` 照样停在下一行行尾，
    ///   还多出一行，禁则在这里让步。**假设**，Word 未测（待测：默认档 720 与左对齐 6480 的
    ///   `20汉（<TAB>Sincerely` @5329 各一份）。修正之前两种都不退（G6 已记下默认档 720 这条偏差），
    ///   `20汉（<TAB>Sincerely` 在默认档 720 上给 0、22。
    ///
    /// **（丁）粘在制表符上、行首之后一个断点都没有**：制表符前面只有制表符（它们在行首），或只有
    /// 同样粘在一起的字（`（<TAB>`）。放不下的长串照紧急断行，在制表符之后按剩余宽度切
    /// （`tab-zeros`，与 `tab-after-a` 第二行一样；`（<TAB>` + 60 个 `0` 一行填满）；放得下的词、
    /// 或一个字都塞不进剩余宽度时，这一行**收在制表符之后**，字另起一行（左对齐 5760 的
    /// `\tSincerely,` @5329：`\t` | `Sincerely,`）。这是「除了段末与显式换行之前，没有一行只有
    /// 制表符」这条**假设**的不变式唯一的例外：只有制表符的行只出现在行首的制表符后面一个字都
    /// 上不来的时候，（丙）退下去的制表符按上面的试排总有字跟着（环绕区除外）。**假设**，未实测
    /// （原先当作空行至少收一个字，于是 `S` 画在行尾之外、`Sincerely,` 被切开）。
    ///
    /// 「空行上放不放得下」「上不上得来」量的都是**整个词**：已排在制表符之后的片段加上游标起到
    /// 第一个断点为止的字，跨 run 累加（[`Self::word_ahead`]）。只量这一截的话，词拆在两个 run 里
    /// 就换一种断法。拆不拆 run 断法都一样是**假设**，Word 未测。
    ///
    /// 已知未测的副作用：
    /// - 本引擎不在 `-`、`/` 后断（UAX #14 的 BA/HY 未实现，Word 在此断不断也未测）。
    ///   原先 run 边界恰在 `-` 之后时会碰巧收行，现在同一行没有别的断点就紧急填满。
    /// - `fit` 的前缀宽度含词后的空格，所以行尾余量不足一个空格宽时，「词 + 空格」这个前缀
    ///   放不下：有更早的断点就退回那里，词挪下去；没有就落到紧急断行，词照收、空格随后吃掉
    ///   （原先是一行一个字符）。上面判断粘着的字「空行上放不放得下」照的就是这个结果，不是 `fit`。
    ///   拆成 run 之后同样如此：`…词` | ` 下一词` 退回、挪下去的是 `词`（原先 `词` 留在本行、
    ///   下一行以那个空格开头）。Word 让行尾空格挂在版心之外，这一条与 Word 未必一致，未测。
    /// - 下一行更宽（首行缩进、环绕区）时，挪下去本可以放得下整个词；这里照样
    ///   在本行切。
    ///
    /// 原先这里只硬塞一个字符就收行，没有断点的长串于是一行一个码元，
    /// 直到剩下的尾巴整段放得下（`x`×200 在 5329 上排成 149 行单字加一行 51 个）。
    #[allow(clippy::too_many_arguments)]
    fn shortfall(
        &self,
        para: &Para,
        area: Rect,
        span: Span,
        remain: Twips,
        cur: &[LinePiece],
        object_join: bool,
        segs: &[Segment],
        at: usize,
        byte: usize,
    ) -> Shortfall {
        let run = &para.runs[segs[at].run_index];
        let chunk = &run.text[byte..segs[at].end];
        // 行尾与这一截之间的交界。隔着行内对象时不吃对象之后的空格：沿用此前的行为，
        // Word 未测（照 UAX #14 空格之前不断，吃掉才对，但没有夹具）。
        if object_join {
            return Shortfall::Close { eat: false };
        }
        if cur
            .last()
            .is_some_and(|last| self.chars_break(last.text.chars().last(), chunk.chars().next()))
        {
            return Shortfall::Close { eat: true };
        }
        let candidate = self.last_break_candidate(cur);
        // 粘着的制表符：本行最后一个制表符，它之后再没有断点。
        let glued = cur
            .iter()
            .rposition(|p| p.tab.is_some())
            .filter(|&t| candidate.is_none_or(|c| c <= (t, 0)));
        let Some(t) = glued else {
            if let Some((piece, offset)) = candidate {
                // （甲）
                return Shortfall::Truncate { piece, offset, eat: true };
            }
            // （乙）
            let (cut, m) = self.metrics.fit_clusters(chunk, &run.font, remain);
            return if cut > 0 && (cur.is_empty() || m.advance <= remain) {
                Shortfall::Place(cut, m)
            } else {
                Shortfall::Close { eat: true }
            };
        };

        // 粘在制表符上的整个词。下一行不是首行，没有首行缩进；区间按本行的估。
        let next_avail = span.width().max(1);
        let mut word: Vec<(&str, &FontSpec)> =
            cur[t + 1..].iter().map(|p| (p.text.as_str(), &p.font)).collect();
        word.extend(self.word_ahead(para, segs, at, byte));
        let width = |parts: &[(&str, &FontSpec)]| -> Twips {
            parts.iter().map(|&(text, font)| self.metrics.measure(text, font).advance).sum()
        };
        let spaced = width(&word);
        // 「空行上放得下」照空行实际的排法判断：空行上 `fit` 放不下时走紧急断行，`fit_clusters`
        // 收下整个词、词后的空格随后吃掉（见「已知未测的副作用」第二条）。所以比的是到第一个断点的
        // 那截**去掉词后空格**的宽，不能直接问 `fit`——它的前缀含那个空格，行宽只比词宽出不到一个
        // 空格时，会把空行上不切开的词当成放不下，在制表符之后切开（`\tSincerely, x` 与
        // `\tSincerely,` 断法不同）。
        while let Some(last) = word.last_mut() {
            last.0 = last.0.trim_end_matches(' ');
            if !last.0.is_empty() {
                break;
            }
            word.pop();
        }
        let fits_alone = width(&word) <= next_avail;
        // 这个词在制表符之后的 `room` 里上不上得来：放得下的词要上来到第一个断点（不切开），
        // 放不下的长串按紧急断行至少上来一个字。前者照有内容的行的口径，含词后的空格——制表符
        // 之后不是空行，与 `A Sincerely, x` 里 `Sincerely,` 的空格放不下就整词换行一样。
        let starts_in = |room: Twips| {
            if fits_alone {
                spaced <= room
            } else {
                let (text, font) = cur.get(t + 1).map_or((chunk, &run.font), |p| (p.text.as_str(), &p.font));
                let (cut, m) = self.metrics.fit_clusters(text, font, room);
                cut > 0 && m.advance <= room
            }
        };
        let after_tab = Shortfall::Truncate { piece: t + 1, offset: 0, eat: false };
        let Some((piece, offset)) = candidate else {
            // （丁）
            if !fits_alone {
                let (cut, m) = self.metrics.fit_clusters(chunk, &run.font, remain);
                if cut > 0 && m.advance <= remain {
                    return Shortfall::Place(cut, m);
                }
                // 制表符之后已经上来了字：这就是紧急断行切到的地方。
                if t + 1 < cur.len() {
                    return Shortfall::Close { eat: true };
                }
            }
            return after_tab;
        };
        // （丙）试排：切口到制表符那一截在下一行上重新落一次。本行停靠点够不着
        // （`past_line_end`）而断点紧挨在制表符之前时不试、制表符留在本行；断点在制表符前那截
        // 粘着的字之前（`（<TAB>`）时照试，下一行上仍够不着由试排里的 `past` 拦下。
        let tab = cur[t].tab.expect("粘着的片段是制表符");
        let carries = (!tab.past_line_end || (piece, offset) < (t, 0)) && {
            let origin = f64::from(span.start - area.x);
            let (mut w, mut w_pt, mut past) = (0, 0.0, false);
            for (index, p) in cur.iter().enumerate().take(t + 1).skip(piece) {
                match p.tab {
                    Some(info) => {
                        let (advance, past_here, landed_w) =
                            self.land_tab(para, info, origin, w_pt * 20.0, next_avail, false);
                        w = w.max(landed_w);
                        w_pt += advance / 20.0;
                        past = past_here;
                    }
                    None => {
                        let text = if index == piece { &p.text[offset..] } else { p.text.as_str() };
                        w += self.metrics.measure(text, &p.font).advance;
                        w_pt += self.metrics.advance_pt(text, &p.font);
                    }
                }
            }
            !past && starts_in(next_avail - w.max(0))
        };
        if carries { Shortfall::Truncate { piece, offset, eat: true } } else { after_tab }
    }

    /// 本行**行首之后**最后一个断点：(片段下标, 片段内字节偏移)，偏移 0 表示该片段左侧的交界。
    ///
    /// 行首那个位置不算——断在那里就是一条空行。片段内部的断点照 `break_opportunities`，
    /// 片段之间的交界按两侧字符查同一套断点（[`Self::join_breaks`]），**run 边界本身不是断点**：
    /// 同一个词拆成几个 run 不该换一种断法（实测 `webhidden`）。制表符之前那一处交界也照两侧字符
    /// 查（`（<TAB>` 之间不断，见 `font::linebreak`），制表符之后不断。
    ///
    /// 隔着行内对象的交界是断点（`LinePiece::after_object`）。对象在行首时也一样：对象占着行首
    /// 那个源位置，它后面的交界已在行首之后，所以退到那里这一行只收下对象，长串挪到下一行——与
    /// 对象在行中时同一条规则（**假设**；Word 未测，本引擎的对象不占宽）。
    ///
    /// 退回之后留在本行的部分要有字：只剩制表符的一行不算（「没有一行只有制表符」这条**假设**的
    /// 不变式，见 [`Self::shortfall`] 的（丁））。只剩行内对象、而对象后面是字的那处交界照上一段算。
    fn last_break_candidate(&self, cur: &[LinePiece]) -> Option<(usize, usize)> {
        let first_text = cur.iter().position(|p| p.tab.is_none());
        for (index, piece) in cur.iter().enumerate().rev() {
            if piece.tab.is_none()
                && let Some(offset) = self
                    .metrics
                    .break_opportunities(&piece.text)
                    .iter()
                    .rev()
                    .map(|b| b.offset)
                    .find(|&o| o > 0 && o < piece.text.len())
            {
                // 串尾那个是 `break_opportunities` 总会给的，不算；交界由下面按两侧字符另查。
                return Some((index, offset));
            }
            let join = match index.checked_sub(1) {
                Some(left) => self.join_breaks(&cur[left], piece),
                None => piece.after_object,
            };
            let keeps_text = first_text.is_some_and(|f| f < index)
                || (piece.tab.is_none() && piece.after_object);
            if join && keeps_text {
                return Some((index, 0));
            }
        }
        None
    }

    /// 从游标起、到第一个断点为止的那截字，可以跨 run：逐 run 给出 (文字, 字体)。
    ///
    /// 截内照 `break_opportunities`，run 交界照两侧字符（[`Self::chars_break`]）；制表符、占位符与
    /// 段末都是尽头。断点在空格之后，所以这截字带着词后的空格，与 `fit` 的前缀同一口径。
    fn word_ahead<'p>(
        &self,
        para: &'p Para,
        segs: &[Segment],
        mut at: usize,
        mut byte: usize,
    ) -> Vec<(&'p str, &'p FontSpec)> {
        let mut word: Vec<(&'p str, &'p FontSpec)> = Vec::new();
        while let Some(s) = segs.get(at).filter(|s| s.kind == SegmentKind::Text) {
            let run = &para.runs[s.run_index];
            let text = &run.text[byte..s.end];
            if word
                .last()
                .is_some_and(|&(prev, _)| self.chars_break(prev.chars().last(), text.chars().next()))
            {
                break;
            }
            if let Some(end) = self
                .metrics
                .break_opportunities(text)
                .iter()
                .map(|b| b.offset)
                .find(|&o| o > 0 && o < text.len())
            {
                word.push((&text[..end], &run.font));
                break;
            }
            word.push((text, &run.font));
            (at, byte) = next_segment(segs, at);
        }
        word
    }

    /// 同一行上相邻两个片段之间的交界是不是断点。
    ///
    /// 隔着行内对象就是（`LinePiece::after_object`）；否则按两侧字符查，与同一 run 内
    /// 的断法完全一致。源区间不相接**不**算——那不是断点的证据。
    fn join_breaks(&self, left: &LinePiece, right: &LinePiece) -> bool {
        right.after_object
            || self.chars_break(left.text.chars().last(), right.text.chars().next())
    }

    /// 两个相邻字符之间能不能断：拼成两个字符的串问 `break_opportunities`。
    ///
    /// 现有规则只看断点两侧各一个字符，所以这样查与整串一起查给出同一个答案。
    fn chars_break(&self, prev: Option<char>, following: Option<char>) -> bool {
        let (Some(prev), Some(following)) = (prev, following) else {
            return false;
        };
        let mut pair = String::with_capacity(8);
        pair.push(prev);
        pair.push(following);
        self.metrics
            .break_opportunities(&pair)
            .iter()
            .any(|b| b.offset == prev.len_utf8())
    }

    /// 本段的默认制表位间距：文档写了 `w:defaultTabStop` 就照用，没写按平台补。
    ///
    /// 桌面 720 是规范值、Mac 上未测；Android 221 是拟合值（见 `ANDROID_MISSING_DEFAULT_TAB_STOP`）。
    fn default_tab_stop(&self, para: &Para) -> Twips {
        para.default_tab_stop.unwrap_or(match self.platform {
            Platform::Desktop => DESKTOP_MISSING_DEFAULT_TAB_STOP,
            Platform::Android => ANDROID_MISSING_DEFAULT_TAB_STOP,
        })
    }

    /// 一个制表符落定：(推进量 twips 精确值, 停靠点是否在行尾或行尾之外, 后面那段的宽度)。
    /// 断行侧用的整 twips 行宽另由 [`Self::land_tab`] 算。
    ///
    /// `x` 是它的起点、`line_end` 是本行右缘，都相对左页边距。
    ///
    /// - 左对齐（含默认档）：推进到停靠点。停靠点在行尾或行尾之外时只推到行尾，并如实报告，
    ///   断行据此让它留在本行（**假设**，未实测）。
    /// - 右／居中／小数点：停靠点先**夹到行尾**，再减去后面那段（或其一半、或小数点之前的部分），
    ///   不小于 0；居中与小数点另外不让那段越过行尾（越过就改成收在行尾）。两处夹都是**假设**：
    ///   Android 上自定义制表位不随视图缩放（`tab-stop-720` / `tab-stop-1440`），所以文档里写在
    ///   纸页右边距上的右对齐制表位（目录、页眉页脚的「标题<TAB>页码」）在窄路径上全在行外；
    ///   不夹的话，后面那段怎么也放不下，一行会拆成三行、中间一行只有制表符。夹了之后，
    ///   放得下的那段收在行尾（待测：右对齐 10466 的 `Title<TAB>12` @5329）；居中那段若照停靠点
    ///   摆会越出行尾，下一行还是同一个停靠点、同样越出，只能被紧急断行从词中间切开。
    ///
    /// 放不下时右对齐制表符不占宽度：实测 `tab-right-1440` 下一行起点 45，
    /// 即制表符加 43 个 `0`，与没有制表符时每行 43 个相同。
    ///
    /// 第三项：右／居中／小数点制表符后面那段的宽度 (精确 twips, 整 twips)；左对齐给 `None`。
    fn tab_advance(
        &self,
        para: &Para,
        tab: TabPiece,
        x: f64,
        line_end: f64,
        first_line: bool,
    ) -> (f64, bool, Option<(f64, Twips)>) {
        let (stop, align) = next_tab_stop(para, x, first_line, self.default_tab_stop(para));
        let aligned = |share: fn(&TabSegment) -> f64| {
            let segment = self.tab_segment(para, tab.run_index, tab.after);
            let room = stop.min(line_end) - x - share(&segment);
            (
                room.min(line_end - x - segment.whole).max(0.0),
                false,
                Some((segment.whole, segment.whole_twips)),
            )
        };
        match align {
            TabAlign::Left | TabAlign::Bar if stop >= line_end => ((line_end - x).max(0.0), true, None),
            TabAlign::Left | TabAlign::Bar => (stop - x, false, None),
            TabAlign::Right => aligned(|s| s.whole),
            TabAlign::Center => aligned(|s| s.whole / 2.0),
            TabAlign::Decimal => aligned(|s| s.before_point),
        }
    }

    /// 在行内 `x`（相对行首，twips 精确值）处落一个制表符：
    /// (推进量 twips 精确值, 停靠点是否在行尾或行尾之外, 落定后的整 twips 行宽)。
    /// 调用方再与落位前的整 twips 行宽取大：制表符不会往回走。
    ///
    /// `origin` 是行首相对左页边距的位置。
    ///
    /// 整 twips 行宽是断行判断用的：后面的字逐片用 `measure` 的整 twips 去比「行宽 − 已占」。
    /// 右／居中／小数点制表符的推进量却是按那段的**精确**宽度倒推的，而整 twips 宽按整形段、
    /// 按 face 段各自取整，加起来可能比精确值多出 1～2 twips。停靠点在行尾（或夹到行尾）时，
    /// 只取 `round(x + 推进量)` 会让最后一片差这 1～2 twips 放不下，一行变两行——那段文字
    /// 跨 run 或跨 face 就会碰上（`Name<TAB>` `John ` `Smith`，右对齐停在行尾）。
    /// 所以落定的行宽不超过「那段精确终点取整 − 那段整 twips 宽」：两边的量法各自自洽，
    /// 那段在整 twips 上正好收在它精确收住的地方。落位（`dx_pt`）照旧走精确值。
    /// 左对齐制表位落在整 twips 上，没有这个问题。
    fn land_tab(
        &self,
        para: &Para,
        tab: TabPiece,
        origin: f64,
        x: f64,
        line_avail: Twips,
        first_line: bool,
    ) -> (f64, bool, Twips) {
        let (advance, past_line_end, segment) =
            self.tab_advance(para, tab, origin + x, origin + f64::from(line_avail), first_line);
        let mut end = (x + advance).round() as Twips;
        if let Some((exact, twips)) = segment {
            end = end.min((x + advance + exact).round() as Twips - twips);
        }
        (advance, past_line_end, end)
    }

    /// 右／居中／小数点制表位后面那段文字的宽度。
    ///
    /// 「那段」从制表符之后起，到下一个制表符、占位符或段末为止，**跨 run 累加**、
    /// 各用各的字体量（**假设**，照规范）。不看断行：那段比空当宽时制表符本来就不占宽度，
    /// 它落不落在本行不改变结论。只在那段在制表位之前就换行时才有差别——未实测，
    /// 这里不做 Line Services 那种行末回填。小数点只认 `.`，不看语言的小数分隔符。
    ///
    /// 各 run 的切片与断行循环里逐片问度量的那些片段是同一批（都切在制表符与占位符上），
    /// 所以 `whole_twips` 与断行侧逐片 `measure` 的整 twips 之和相等。
    ///
    /// 代价：每落位一次（被挪到下一行时再落一次，判断挪不挪时再试一次）就把那段整形一次，
    /// 不缓存；左对齐制表符不走这里。
    fn tab_segment(&self, para: &Para, run_index: usize, after: usize) -> TabSegment {
        let mut whole = 0.0;
        let mut whole_twips: Twips = 0;
        let mut before_point = None;
        for (index, run) in para.runs.iter().enumerate().skip(run_index) {
            if run.hidden {
                continue;
            }
            let text = if index == run_index { &run.text[after..] } else { run.text.as_str() };
            let end = text.find(['\t', OBJECT_PLACEHOLDER]).unwrap_or(text.len());
            let slice = &text[..end];
            if before_point.is_none()
                && let Some(point) = slice.find('.')
            {
                before_point =
                    Some(whole + self.metrics.advance_pt(&slice[..point], &run.font) * 20.0);
            }
            whole += self.metrics.advance_pt(slice, &run.font) * 20.0;
            whole_twips += self.metrics.measure(slice, &run.font).advance;
            if end < text.len() {
                break;
            }
        }
        TabSegment { whole, whole_twips, before_point: before_point.unwrap_or(whole) }
    }

    /// 按片段重算一行的纵向量：(ascent, descent, natural, natural_fine)。
    fn pieces_vertical(&self, pieces: &[LinePiece]) -> (Twips, Twips, Twips, i64) {
        pieces.iter().fold((0, 0, 0, 0), |(a, d, n, nf), p| {
            // 制表符按空格量纵向：U+0009 在不少字体里没有字形，选不到 face。
            let text = if p.tab.is_some() { " " } else { p.text.as_str() };
            let m = self.metrics.measure(text, &p.font);
            (
                a.max(m.ascent),
                d.max(m.descent),
                n.max(m.natural_height()),
                nf.max(self.metrics.natural_height_fine(text, &p.font)),
            )
        })
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
            // 分页与 keepLines/keepNext 必须预留精细游标实际推进的高度；
            // exact 在两条路径里都不受字体内容下限约束。
            LineRule::Exact => return para.line_value.max(1),
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


/// 字体内容标识；TTC 的非零 face index 以 `:index` 后缀区分。
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
    /// Source cluster in UTF-16 units, relative to the text passed to the shaper.
    /// `None` is reserved for shapers that cannot report cluster provenance.
    pub source: Option<(u32, u32)>,
    /// 推进量与偏移，twips。
    pub x_advance: Twips,
    /// 推进量的**精确值**，单位点。见 [`crate::font::FontMetrics::advance_pt`]。
    ///
    /// `x_advance` 是它落到整 twips 的结果。整形器给的推进量本来就落不到
    /// 整 twips 上（实测 Word 的 0/1340 个落在整 twips 上），取整的残差沿行累加。
    pub x_advance_pt: f64,
    pub x_offset: Twips,
    pub y_offset: Twips,
    /// 整形时实际用的字号，0.01pt。`None` 表示就是片段字体的字号。
    ///
    /// 小型大写（`w:smallCaps`）让同一个片段里出现两个字号：小写字母换成大写字形、
    /// 按缩小的字号排。推进量已经按那个字号算了，字形记录与栅格化也得用它，
    /// 否则画出来是全尺寸的大写。
    pub size_centipoints: Option<u64>,
}

/// 字形 `i` 之后要不要加字符间距：它是所在 cluster（源字符组）的最后一个字形。
///
/// 间距按 cluster 加，不按字形、也不按 UTF-16 单位：组合符号不该与基字拉开，
/// 一个代理对也只是一个字符。**这一条是假定**——实测只覆盖了一字形一字符的拉丁文
/// （`latinspace`），那里三种口径给同一个数。整形器报不出 cluster 时退回逐字形。
fn is_cluster_end(runs: &[ShapedRun], i: usize) -> bool {
    match (runs[i].source, runs.get(i + 1).map(|g| g.source)) {
        (Some(this), Some(Some(next))) => this != next,
        _ => true,
    }
}

/// 一段整形结果里加字符间距的位置数（[`is_cluster_end`] 为真的字形数）。
///
/// 度量与落位必须数同一个东西，否则断行按一个宽度、画字按另一个宽度。
/// 只有 `RealMetrics`（feature `fontenv`）用它；桩度量不整形，按源字符簇数
/// （`linebreak::cluster_boundaries`），组合序列上与这里给同一个数。
#[cfg_attr(not(feature = "fontenv"), allow(dead_code))]
pub(crate) fn spacing_slots(runs: &[ShapedRun]) -> usize {
    (0..runs.len()).filter(|&i| is_cluster_end(runs, i)).count()
}

/// 把 `w:w`（横向缩放）与 `w:spacing`（字符间距）落到一段整形结果的推进量上。
///
/// 口径与 [`crate::font::FontMetrics::advance_pt`] 相同：字形推进量乘缩放比例，
/// 每个 cluster 末尾再加间距；**间距不随缩放**。整 twips 的 `x_advance` 按累计精确值
/// 取整之差重算，与整形器的做法一致，前缀和不漂。两者都是默认值时原样返回。
///
/// 整形器本身不做这一步：`RealMetrics::measure` 是在整形结果之上加的，
/// 整形器也加就会算两遍。
pub(crate) fn apply_char_spacing(runs: &mut [ShapedRun], font: &FontSpec) {
    let scaled = font.scale_pct != 100 && font.scale_pct > 0;
    if !scaled && font.letter_spacing == 0 {
        return;
    }
    let scale = if scaled { f64::from(font.scale_pct) / 100.0 } else { 1.0 };
    let spacing_pt = f64::from(font.letter_spacing) / f64::from(TWIPS_PER_POINT);
    let ends: Vec<bool> = (0..runs.len()).map(|i| is_cluster_end(runs, i)).collect();
    let mut acc_pt = 0.0f64;
    let mut acc_twips: Twips = 0;
    for (g, end) in runs.iter_mut().zip(ends) {
        g.x_advance_pt = g.x_advance_pt * scale + if end { spacing_pt } else { 0.0 };
        g.x_offset = (f64::from(g.x_offset) * scale).round() as Twips;
        acc_pt += g.x_advance_pt;
        let next = (acc_pt * f64::from(TWIPS_PER_POINT)).round() as Twips;
        g.x_advance = next - acc_twips;
        acc_twips = next;
    }
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
    extra: usize,
) -> (String, (u32, u32)) {
    let mut out = String::with_capacity(text.len() + extra);
    out.push_str(text);
    for _ in 0..extra {
        out.push(' ');
    }
    // The line range already includes every control character it consumed.
    let end = line_source_end.max(source.1);
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
    // 制表符画成一个空格字形，推进量取制表位定下的宽度（见 `TextFragment::tab_advance_pt`）。
    // 换字符不换长度，源区间的一一对应不受影响。
    let tab_text;
    let text = if t.tab_advance_pt.is_some() {
        tab_text = t.text.replacen('\t', " ", 1);
        tab_text.as_str()
    } else {
        t.text.as_str()
    };
    let mut runs = shaper.shape(text, &t.font);
    // 断行量宽时已含缩放与字符间距（`FontMetrics::measure`），画字也得含，
    // 否则行宽对了、行内字形却挤在一起。片段末尾补上的终止符字形（段落标记画的空格）
    // 也照加：它不撑开行宽，只改它自己的推进量。Word 给不给段落标记加间距没测（假定）。
    // 顺序在制表符改宽之前：制表符那个空格字形的推进量随后整个换成制表位定下的宽度，
    // 缩放与间距都不作用在它身上（断行里的制表符宽也不含它们，两边一致）。
    apply_char_spacing(&mut runs, &t.font);

    // Shaper clusters remain meaningful for ligatures, combining marks and RTL.
    // A fragment containing omitted source characters still needs a conservative
    // range: its text offsets no longer map directly onto the source stream.
    let utf16_len: u32 = t.text.encode_utf16().count() as u32;
    // 精确归属要**两边都对得上**：字形数 == 字符数，且源区间长度 == 字符数。
    // 后一条不能省：行里有不产生片段的源字符（对象占位符）时，区间会比文本长，
    // 这时 `base + i` 指到的就是别的字符，配对会悄悄错位而几何上看不出来。
    let span_len = t.source.map(|(a, b)| b - a).unwrap_or(0);
    let exact = runs.len() as u32 == utf16_len && span_len == utf16_len;
    let base = t.source.map(|(s, _)| s).unwrap_or(0);

    runs.into_iter()
        .enumerate()
        .filter_map(|(i, mut g)| {
            // 片段以制表符开头：它是第一个字形，推进量换成制表位定下的宽度，
            // 这样后面的字形正好从制表位起画。
            if i == 0
                && let Some(advance_pt) = t.tab_advance_pt
            {
                g.x_advance = (advance_pt * 20.0).round() as Twips;
                g.x_advance_pt = advance_pt;
            }
            let face = faces.get(g.face_index)?.clone();
            let source = t.source.map(|(_, end)| {
                if span_len == utf16_len
                    && let Some((start, end)) = g.source
                    && start < end
                    && end <= utf16_len
                {
                    (base + start, base + end)
                } else if g.source.is_none() && exact {
                    // 一字形一字符：精确归属。
                    let at = base + i as u32;
                    (at, at + 1)
                } else {
                    // 字形数 ≠ 字符数：给整段区间，不猜。
                    (base, end)
                }
            });
            // 整形器报了别的字号（小型大写）才换；否则沿用片段字体，旧的精确字号不动。
            let (size_half_points, size_centipoints) = match g.size_centipoints {
                Some(cp) if cp != t.font.effective_size_centipoints() => {
                    let half = cp / 50 + u64::from(cp % 50 >= 25);
                    (half.min(u64::from(u32::MAX)) as u32, cp)
                }
                _ => (t.font.size_half_points, t.font.effective_size_centipoints()),
            };
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
                size_half_points,
                size_centipoints,
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
