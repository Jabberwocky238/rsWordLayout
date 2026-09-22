//! 比较器输入契约：把布局结果摊成可与真实 Word 逐字形比对的形式。
//!
//! 量具方法的核心决策是**不测行基线与行盒**（那条路在现有通道上不可测），
//! 改测每个字形被画在哪里——字形原点在 Word 导出的 PDF 里直接可读。
//! 同条件重复采集的噪声底是 0.0000pt（34 对采集、38845 对字形逐位相同），
//! 所以引擎与 Word 之间任何非零差都是信号，**不需要给测量噪声留容差**。
//!
//! 本模块只定义引擎该吐什么，不含采集与比对——那两步要 Windows 桌面 Word
//! （COM 对象模型 + `ExportAsFixedFormat`），本机跑不了。
//!
//! # 三层配对
//!
//! 比较器按 页 → 行 → 字形 三层走，**任何一层数目对不上即结构失败，
//! 不修配对再比**。所以本契约也按这三层组织，且每层都带计数。
//!
//! # 为什么要源字符区间
//!
//! PDF 里的字形可能没有 ToUnicode 映射（合成字体、CJK 子集），所以配对
//! **不能靠 Unicode 身份**，只能按读序：同一行内 PDF 字形按内容流顺序编号，
//! 源字符按顺序编号，同序号相配。引擎因此必须报出每个字形对应源文本的哪一段，
//! 否则配对无从校验。

use crate::layout::{PositionedGlyph, Twips};

/// 行终止符类型。
///
/// 计数约定要求区分它们：段落标记画 1 个空格，软回车画 1 个，
/// 分节符画 0 个，手动分页符视位置画 0 或 1 个。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineTerminator {
    /// 段落标记（`\r`）。Word 为它画 **1 个空格**。
    ParagraphMark,
    /// 软回车（`w:br type="textWrapping"`，`\x0b`）。画 **1 个**字形。
    LineBreak,
    /// 手动分页符（`w:br type="page"`）。画几个取决于它在行里的位置，
    /// 见 [`PageBreakPosition`]。
    PageBreak(PageBreakPosition),
    /// 分节符（段内 `w:sectPr`）。画 **0 个**字形。
    SectionBreak,
    /// 行在段落中间断开（自动换行），没有终止符字符。
    #[default]
    Wrapped,
}

/// 手动分页符在行里的位置——决定它画几个字形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageBreakPosition {
    /// 分页符自身独占一条行记录（不含段落标记）：画 **0 个**。
    OwnLine,
    /// 紧跟段落标记：画 **1 个空格**（该行连同段落标记共 +2）。
    BeforeMark,
    /// 段中，两侧都有文字：画 **0 个**。
    MidParagraph,
}

impl LineTerminator {
    /// 本终止符按计数约定应当产生几个字形。
    ///
    /// 这些数字来自实测而非规范推导，各自的检验范围见量具方法 §4。
    /// **仍在范围外**：制表符、跨页表格行、自动编号、行内对象、合成字体与 CJK。
    pub fn expected_glyphs(&self) -> usize {
        match self {
            LineTerminator::ParagraphMark | LineTerminator::LineBreak => 1,
            LineTerminator::PageBreak(PageBreakPosition::BeforeMark) => 1,
            LineTerminator::PageBreak(_) | LineTerminator::SectionBreak => 0,
            LineTerminator::Wrapped => 0,
        }
    }
}

/// 源文本里的一段字符区间，UTF-16 单位（与 rsword 的坐标流一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SourceRange {
    pub start: u32,
    pub end: u32,
}

impl SourceRange {
    pub fn new(start: u32, end: u32) -> SourceRange {
        SourceRange { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// 一个字形的可比对记录。
///
/// 与 [`PositionedGlyph`] 的差别：这里额外带**推进量**与**源字符区间**——
/// 前者用于核对相邻字形的错位，后者是按读序配对的校验依据。
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRecord {
    /// 字形原点（笔位），twips。
    pub origin_x: Twips,
    pub origin_y: Twips,
    /// 原点横向的**精确值**，单位点。
    ///
    /// 与 Word 逐位相比必须读这一个，理由与 `origin_y_fine` 同源、量级不同：
    /// `origin_x` 取整到 twips（0.05pt），而 Word 的推进量一个都不落在整 twips 上
    /// （实测 0/1340），残差沿**行**累加（实测一行攒到 0.04pt）。
    /// 横向不设定点单位的理由见 [`crate::font::FontMetrics::advance_pt`]。
    pub origin_x_pt: f64,
    /// 原点纵向的**精确值**，单位 1/7200 英寸。
    ///
    /// 与 Word 逐位相比必须读这一个：Word 的基线落在 0.24pt = **4.8 twips** 的栅格上，
    /// `origin_y` 取整到 twips 的残差会沿页累加（实测一页 20 行攒到 2.4pt）。
    pub origin_y_fine: i64,
    /// 推进向量，twips。
    pub advance_x: Twips,
    /// 推进量的**精确值**，单位点。
    pub advance_x_pt: f64,
    pub advance_y: Twips,
    pub face: String,
    pub glyph_id: u32,
    /// 字号，半点。
    ///
    /// Mac 通道的 PDF 里 `fontSize` 逐条恒为 1（字号在文本矩阵里），
    /// 故那一侧不可作字号读数；本字段只与 Windows 侧比对。
    pub size_half_points: u32,
    /// Effective font size in 1/100 point, without the legacy half-point rounding.
    pub size_centipoints: u64,
    /// 本字形对应的源字符区间。多字符合成一个字形（连字）时跨多个字符；
    /// 自动编号标签在源文本里没有对应字符，此时为 `None`。
    pub source: Option<SourceRange>,
}

/// 一行的可比对记录。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineRecord {
    /// 行内字形，按读序——**配对依赖这个顺序**。
    ///
    /// 「PDF 内容流的绘制顺序等于源字符顺序」是量具方法显式引入的前提，
    /// 不是默认成立的，故引擎这一侧必须保证同序输出。
    pub glyphs: Vec<GlyphRecord>,
    /// 本行覆盖的源字符区间。
    pub source: Option<SourceRange>,
    /// 行终止符。
    pub terminator: LineTerminator,
    /// 行盒。**仅供诊断，不参与验收**——行盒与行基线在现有通道上不可测
    /// （Word COM 的 `Line` 不报基线；PDF 字形基线不能唯一确定行基线：
    /// 同一文档两种排布下五个字形的基线全是 300.0，而真实行基线分别是 300 与 304）。
    pub box_top: Twips,
    pub box_height: Twips,
}

impl LineRecord {
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }
}

/// 一页的可比对记录。
#[derive(Debug, Clone, PartialEq)]
pub struct PageRecord {
    /// 页号，从 0 计。
    pub index: usize,
    /// 页面尺寸，twips。
    pub width: Twips,
    pub height: Twips,
    pub lines: Vec<LineRecord>,
}

impl PageRecord {
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn glyph_count(&self) -> usize {
        self.lines.iter().map(LineRecord::glyph_count).sum()
    }
}

/// 整篇文档的可比对记录：比较器的输入。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutRecord {
    pub pages: Vec<PageRecord>,
    /// 未能归入任何行的字形数。
    ///
    /// 实测 Windows 总包有 2988/35087 = 8.5% 的字形不进行划分，集中在抬升 run、
    /// 脚注、表格页。目前的处理是**显式写进验收定义里排除**，而不是给它们归属——
    /// 给归属要靠表格与脚注的行级约定，那两件都只在有限范围内成立。
    pub unassigned_glyphs: usize,
}

impl LayoutRecord {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn glyph_count(&self) -> usize {
        self.pages.iter().map(PageRecord::glyph_count).sum()
    }

    /// 从绘制指令摊出记录。
    ///
    /// **按 `DrawGlyphs` 的 `line` 合并，而不是一条指令一行。** 绘制指令是按**片段**出的，
    /// 一行里换字体、上标、分页符都会把它切开；把一条指令当一行，行这一层就被 run 切碎，
    /// 比较器的行层配对必然对不上——而且失败**看起来像「引擎少排了行」，
    /// 其实是记账粒度错了**，查起来会往分页的方向白跑一趟。
    ///
    /// 实测：MR1 夹具 Word 排 20 行，按指令数则是 29 条；差额正是一行里的多个 run。
    pub fn from_paint(list: &crate::layout::PaintList) -> LayoutRecord {
        let mut out = LayoutRecord::default();
        for (index, page) in list.pages.iter().enumerate() {
            let mut rec = PageRecord {
                index,
                width: page.width,
                height: page.height,
                lines: Vec::new(),
            };
            let mut current: Option<u32> = None;
            for cmd in &page.cmds {
                let crate::layout::DrawCmd::DrawGlyphs {
                    glyphs,
                    terminator,
                    source,
                    line,
                    ..
                } = cmd
                else {
                    continue;
                };

                let piece_source = source.map(|(a, b)| SourceRange::new(a, b));
                if current == Some(*line) {
                    // 同一行的后续片段：并进上一条记录。
                    let last = rec.lines.last_mut().expect("current 非空时必有记录");
                    last.glyphs.extend(glyphs.iter().map(GlyphRecord::from_positioned));
                    // 行的源区间是各片段的并集；缺一个就整条给 None，
                    // 让比较器报「判不了」而不是拿半截区间去配对。
                    last.source = match (last.source, piece_source) {
                        (Some(a), Some(b)) => Some(SourceRange::new(a.start.min(b.start), a.end.max(b.end))),
                        _ => None,
                    };
                    // 终止符只有行末片段带真实值，所以后来的覆盖先前的。
                    if *terminator != LineTerminator::Wrapped {
                        last.terminator = *terminator;
                    }
                } else {
                    rec.lines.push(LineRecord {
                        glyphs: glyphs.iter().map(GlyphRecord::from_positioned).collect(),
                        // 取片段自己的区间，而不是从字形序列反推——后者在没接
                        // shaper 时会丢，而源区间与是否栅格化无关。
                        source: piece_source,
                        // 终止符由 paint 层随指令带下来；缺失时保持 Wrapped
                        // （自动换行），不猜。
                        terminator: *terminator,
                        box_top: 0,
                        box_height: 0,
                    });
                    current = Some(*line);
                }
            }
            out.pages.push(rec);
        }
        out
    }

    /// 重复性自检：同一输入排两次应当逐位相同。
    ///
    /// 采集链的噪声底是 0.0000pt，引擎这一侧同样不该有抖动。
    /// **不相同就先别往下走**——那说明产出不确定，后面的比对全无意义。
    pub fn is_bit_identical(&self, other: &LayoutRecord) -> bool {
        self == other
    }
}

impl GlyphRecord {
    /// 从 [`PositionedGlyph`] 构造。
    ///
    /// 源区间由引擎在断行时记下（那是唯一知道「切在第几个字符」的地方）；
    /// 未能确定时保持 `None`，比较器据此报「判不了」而不是把缺失读成 0 差值。
    pub fn from_positioned(g: &PositionedGlyph) -> GlyphRecord {
        GlyphRecord {
            origin_x: g.x,
            origin_y: g.y,
            origin_x_pt: g.x_pt,
            origin_y_fine: g.y_fine,
            advance_x: g.advance_x,
            advance_x_pt: g.advance_x_pt,
            advance_y: g.advance_y,
            face: g.face.clone(),
            glyph_id: g.glyph_id,
            size_half_points: g.size_half_points,
            size_centipoints: g.size_centipoints,
            source: g.source.map(|(a, b)| SourceRange::new(a, b)),
        }
    }

    /// 与另一条记录的欧氏距离，twips。
    ///
    /// 判定用欧氏距离而非分轴比较，与量具方法一致。
    pub fn distance(&self, other: &GlyphRecord) -> f64 {
        let dx = f64::from(self.origin_x - other.origin_x);
        let dy = f64::from(self.origin_y - other.origin_y);
        (dx * dx + dy * dy).sqrt()
    }
}

/// 比对结果的三态出口。
///
/// **「判不了」这个出口不能取消**，否则这套东西只会输出「成立」。
/// 也不许拿「未发现反例」代替「通过」。
#[derive(Debug, Clone, PartialEq)]
pub enum CompareState {
    /// 三层配对都成功，给出最大偏差。
    Compared { max_abs: f64 },
    /// 某一层数目对不上。**结构失败时不修配对再比**，因为那只会把
    /// 「配对失败」读成「完全一致」：实测有两份不同文档，3 行结构失败而
    /// 剩下 1 行 8 个字形距离恰好 0.0000pt。
    StructuralMismatch {
        level: MismatchLevel,
        expected: usize,
        actual: usize,
    },
    /// 前置条件不满足，无法判定。
    Undecidable { reason: &'static str },
}

/// 结构失败发生在哪一层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchLevel {
    Page,
    Line,
    Glyph,
}

impl CompareState {
    /// 是否算通过。
    ///
    /// 报告必须**先读 state 再读 max_abs**——反过来会把结构失败读成一致。
    pub fn is_pass(&self, tolerance: f64) -> bool {
        matches!(self, CompareState::Compared { max_abs } if *max_abs <= tolerance)
    }
}
