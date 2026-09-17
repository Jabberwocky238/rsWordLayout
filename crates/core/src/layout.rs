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

use crate::font::{FontMetrics, FontSpec};


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
    /// 推进量，twips。整形器给出，比较器用它核对相邻字形的错位。
    pub advance_x: Twips,
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
        origin_y: Twips,
        text: String,
        font: crate::font::FontSpec,
        paint: Paint,
        /// 本行以什么结束。比较器按计数约定核对字形数，故须随指令带下来。
        terminator: crate::oracle::LineTerminator,
        /// 本片段的源字符区间（UTF-16，相对所在段落）。
        ///
        /// 独立于 `glyphs` 存在：能直接排文字的后端不需要字形序列，
        /// 此时 `glyphs` 为空，源区间不该跟着丢。
        source: Option<(u32, u32)>,
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
    /// 基线绝对 y（页内坐标）。
    pub baseline_y: Twips,
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    /// 来源段落的 `rsword` 节点 id，供调试与反查。
    pub source_node: Option<u32>,
    /// 本片段所在行的终止符。只有行末片段带真实值，行中片段是 `Wrapped`。
    pub terminator: crate::oracle::LineTerminator,
    /// 本片段覆盖的源字符区间（UTF-16 单位，相对所在段落）。
    ///
    /// 比较器按读序配对，需要它把字形对回源字符。`None` 表示引擎未能确定，
    /// 比较器据此报「判不了」而不是猜一个区间。
    pub source: Option<(u32, u32)>,
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
    text: String,
    font: FontSpec,
    color: Color,
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
    is_last: bool,
    /// 首行要额外吃 `indent_first_line`（可负，即悬挂缩进）。
    is_first: bool,
    /// 本行实际落在哪个横向区间。无环绕时就是整个正文宽度；
    /// 有环绕时可能是被图片劈开后的左段或右段。
    span: Span,
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
        let mut cursor = area.y;

        for (idx, para) in paras.iter().enumerate() {
            let lines = self.break_paragraph(para, area, cursor);
            let block_height: Twips = lines.iter().map(|l| l.height).sum();

            if para.page_break_before && !page.fragments.is_empty() {
                pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor = area.y;
            }

            cursor += para.space_before;

            // keepLines：整段放不下就先翻页（除非本页是空的，那样翻了也没用）。
            if para.keep_lines
                && cursor + block_height > area.bottom()
                && !page.fragments.is_empty()
            {
                pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor = area.y;
            }

            // keepNext：本段是最后一段时无意义；否则要保证下一段至少第一行同页。
            let next_first_line = if para.keep_next {
                paras.get(idx + 1).and_then(|n| {
                    self.break_paragraph(n, area, cursor).first().map(|l| l.height)
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
                if cursor + needed > area.bottom() && !page.fragments.is_empty() {
                    pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                    cursor = area.y;
                }
                self.place_line(&mut page, &line, para, cursor);
                cursor += line.height;
            }

            cursor += para.space_after;
        }

        pages.push(page);
        pages
    }

    /// 把一行放到页面上，处理水平对齐。
    fn place_line(&self, page: &mut Page, line: &PendingLine, para: &Para, top: Twips) {
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

        // 两端对齐：除最后一行外，把空隙按片段间隙均摊。
        let justify_gap = if para.align == Align::Justify
            && !line.is_last
            && line.pieces.len() > 1
        {
            slack / (line.pieces.len() as Twips - 1)
        } else {
            0
        };

        let baseline_y = top + line.baseline;
        let last = line.pieces.len().saturating_sub(1);
        for (i, p) in line.pieces.iter().enumerate() {
            let x = base_x + offset + p.dx + justify_gap * (i as Twips);
            // 只有段落最后一行的最后一个片段带真实终止符；其余是自动换行。
            // 计数约定按行核对字形数，把终止符记到行中片段会让核对偏移。
            let terminator = if line.is_last && i == last {
                para.terminator
            } else {
                crate::oracle::LineTerminator::Wrapped
            };
            page.fragments.push(Fragment::Text(TextFragment {
                x,
                baseline_y,
                text: p.text.clone(),
                font: p.font.clone(),
                color: p.color,
                source_node: para.source_node,
                source: Some(p.source),
                terminator,
            }));
        }
    }

    /// 段落断行。
    /// 段落断行。
    ///
    /// `y` 是本段起始的纵向位置——**有环绕时每行可用区间取决于它**，所以不能像
    /// 改造前那样只传一个标量宽度。无环绕时退化为整段用同一个满宽区间。
    fn break_paragraph(&self, para: &Para, area: Rect, y: Twips) -> Vec<PendingLine> {
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
                baseline: m.ascent,
                pieces: Vec::new(),
                width: 0,
                is_last: true,
                is_first: true,
                span,
            });
            return lines;
        }

        let mut cur: Vec<LinePiece> = Vec::new();
        // 段落内已消费的 UTF-16 字符数，作为各片段源区间的起点。
        let mut consumed: u32 = 0;
        let mut cur_w: Twips = 0;
        let mut cur_ascent: Twips = 0;
        let mut cur_descent: Twips = 0;
        let mut cur_natural: Twips = 0;
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
            let mut rest: &str = &run.text;
            while !rest.is_empty() {
                let remain = (line_avail - cur_w).max(0);
                let m_all = self.metrics.measure(rest, &run.font);

                if m_all.advance <= remain {
                    // 整段剩余放得下。
                    let n = rest.encode_utf16().count() as u32;
                    cur.push(LinePiece {
                        dx: cur_w,
                        text: rest.to_string(),
                        font: run.font.clone(),
                        color: run.color,
                        source: (consumed, consumed + n),
                    });
                    consumed += n;
                    cur_w += m_all.advance;
                    cur_ascent = cur_ascent.max(m_all.ascent);
                    cur_descent = cur_descent.max(m_all.descent);
                    cur_natural = cur_natural.max(m_all.natural_height());
                    break;
                }

                // 放不下：找能塞进去的最长前缀。
                match self.metrics.fit(rest, &run.font, remain) {
                    Some((cut, m)) if cut > 0 => {
                        let piece = &rest[..cut];
                        let n = piece.encode_utf16().count() as u32;
                        cur.push(LinePiece {
                            dx: cur_w,
                            text: piece.to_string(),
                            font: run.font.clone(),
                            color: run.color,
                            source: (consumed, consumed + n),
                        });
                        consumed += n;
                        cur_w += m.advance;
                        cur_ascent = cur_ascent.max(m.ascent);
                        cur_descent = cur_descent.max(m.descent);
                        cur_natural = cur_natural.max(m.natural_height());
                        rest = rest[cut..].trim_start_matches(' ');
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
                                text: rest[..n].to_string(),
                                font: run.font.clone(),
                                color: run.color,
                                source: (consumed, consumed + u16n),
                            });
                            consumed += u16n;
                            cur_w += m.advance;
                            cur_ascent = cur_ascent.max(m.ascent);
                            cur_descent = cur_descent.max(m.descent);
                            cur_natural = cur_natural.max(m.natural_height());
                            rest = &rest[n..];
                        }
                    }
                }

                // 收行。
                let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
                lines.push(PendingLine {
                    height: h,
                    baseline: cur_ascent,
                    pieces: std::mem::take(&mut cur),
                    width: cur_w,
                    is_last: false,
                    is_first: first_line,
                    span,
                });
                cur_w = 0;
                cur_ascent = 0;
                cur_descent = 0;
                cur_natural = 0;
                first_line = false;
                // 换行：y 推进一行高，可用区间随之可能变化。
                cur_y += h;
                span = self.pick_span(para, area, cur_y, h.max(probe_h));
                line_avail = span.width().max(1);
            }
        }

        // 末行。
        if !cur.is_empty() || lines.is_empty() {
            let h = self.line_height(para, cur_ascent + cur_descent, cur_natural);
            lines.push(PendingLine {
                height: h,
                baseline: cur_ascent,
                pieces: cur,
                width: cur_w,
                is_last: true,
                is_first: first_line,
                span,
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
                    origin_y: t.baseline_y,
                    text: t.text.clone(),
                    font: t.font.clone(),
                    paint: Paint::solid(t.color),
                    terminator: t.terminator,
                    source: t.source,
                });
            }
        }
    }
    out
}

/// 把整形结果摊成绝对坐标的字形。
fn position_glyphs(
    shaper: &dyn TextShaper,
    t: &TextFragment,
    faces: &[FaceId],
) -> Vec<PositionedGlyph> {
    let mut pen = t.x;
    let runs = shaper.shape(&t.text, &t.font);

    // 源区间按**读序**分配：整形器不报字符归属，所以只能按「字形序号 → 字符序号」
    // 对应。这与量具方法的配对前提一致（PDF 内容流顺序等于源字符顺序），
    // 但字形数与字符数不等时（连字、组合符号）无法逐一对应——
    // 那种情况下整段记同一个区间，让比较器能看出是聚合而非精确归属。
    let utf16_len: u32 = t.text.encode_utf16().count() as u32;
    let exact = runs.len() as u32 == utf16_len;
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
                advance_x: g.x_advance,
                advance_y: 0,
                size_half_points: t.font.size_half_points,
                source,
            };
            pen += g.x_advance;
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
