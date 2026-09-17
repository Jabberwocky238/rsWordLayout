//! 文字度量抽象。
//!
//! 布局要算断行就必须先知道文字多宽，而宽度来自字体文件。这一层把「取度量」抽成 trait，
//! 有两个后果：
//!
//! 1. 布局主干不依赖任何具体字体库（HarfBuzz / FreeType / 系统 API 都能接）；
//! 2. **所有 canvas 后端共用同一份度量**——否则同一份文档 PDF 后端分 10 页、
//!    HTML 后端分 11 页，预览就失去意义。所以度量不挂在 `Canvas` 上，是独立输入。
//!
//! shaping（连字、kerning、复杂文种重排）属于实现者的职责：它是由字体 GSUB/GPOS 表
//! 决定的确定性查表，不是布局要解的约束。实现者通常直接转调 HarfBuzz。

use crate::geom::Twips;

/// 一次度量请求的字体条件。
///
/// 只放**影响度量**的字段。颜色、下划线、高亮不影响 advance，不在这里。
#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    /// 字体名（已由 `resolve` 的主题字体与 EA/CS 槽选择定下来）。
    pub family: String,
    /// 字号，半点（`w:sz`）。
    pub size_half_points: u32,
    pub bold: bool,
    pub italic: bool,
    /// `w:spacing`：字符间距调整，twips，可负。
    pub letter_spacing: Twips,
    /// `w:w`：横向缩放百分比，100 为原始。
    pub scale_pct: u32,
    /// 是否启用字距调整（GPOS `kern`）。
    ///
    /// **默认关**，这不是保守取值，是 OOXML 的语义：`w:kern` 给的是「字号大到多少才启用
    /// 字距调整」，不写或写 0 就是**不调整**。实测对得上——一份 Liberation Serif 的夹具里
    /// `B11` 的两个 `1` 之间有一对 kern，rustybuzz 默认会用上，而 Word **没有用**，
    /// 于是该行从第二个字形起整体偏了 0.45pt。
    pub kerning: bool,
}

impl FontSpec {
    /// 默认西文正文：五号宋体的常见等价物由调用方给，这里只兜字号。
    pub fn new(family: impl Into<String>, size_half_points: u32) -> FontSpec {
        FontSpec {
            family: family.into(),
            size_half_points,
            bold: false,
            italic: false,
            letter_spacing: 0,
            scale_pct: 100,
            kerning: false,
        }
    }
}

/// 一段同字体文字的度量结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct TextMetrics {
    /// 排版推进量（含字距与缩放后的值）。
    pub advance: Twips,
    /// 基线以上高度。
    pub ascent: Twips,
    /// 基线以下高度（正值）。
    pub descent: Twips,
    /// 建议行距（字体推荐的 line gap），不含 `w:spacing` 的段落行距设置。
    pub line_gap: Twips,
}

impl TextMetrics {
    /// 字体自身的自然行高。
    pub fn natural_height(&self) -> Twips {
        self.ascent + self.descent + self.line_gap
    }
}

/// 断行候选位置：可以断开的字符边界。
///
/// 断行规则（UAX #14）与字体无关，但和语言有关（中日文可在字间断，西文按词），
/// 所以和度量放在同一个 trait 里由实现者一并提供。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct BreakOpportunity {
    /// 断点在原串中的字节偏移。
    pub offset: usize,
    /// 断开后前半段是否要保留一个连字符（西文 hyphenation）。
    pub hyphen: bool,
}

/// 一个已定位的字形：它覆盖哪些源字符、落在串内什么位置。
///
/// 存在的理由是**一字符一字形不成立**：连字把多个字符并成一个字形，
/// kerning 又让相邻字形的间距取决于两边是谁。所以位置不能由调用方拿前缀宽度自己拼，
/// 必须由度量实现给——它才知道 shaper 干了什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct GlyphPosition {
    /// 覆盖的源字符区间，**字节偏移**，相对本次请求的 `text`。
    pub start: usize,
    pub end: usize,
    /// 相对串首的 x 偏移。
    pub x: Twips,
    /// 本字形的推进量。
    pub advance: Twips,
}

/// 度量提供者。
///
/// 实现者需保证**同一输入给出同一输出**：布局会对同一段文字反复试宽（断行二分），
/// 结果不稳定会导致分页抖动。
pub trait FontMetrics {
    /// 整段文字的推进宽度与纵向度量。
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics;

    /// 在给定宽度内最多能放下多少字节，返回该前缀的字节长度与其度量。
    ///
    /// 返回 `None` 表示一个字符都放不下——调用方据此决定是硬塞还是换行。
    /// 默认实现按断行点线性试探；实现者若能直接问 shaper 要 cluster 边界，应覆盖它。
    fn fit(&self, text: &str, font: &FontSpec, max_width: Twips) -> Option<(usize, TextMetrics)> {
        let mut best: Option<(usize, TextMetrics)> = None;
        for op in self.break_opportunities(text) {
            if op.offset == 0 {
                continue;
            }
            let m = self.measure(&text[..op.offset], font);
            if m.advance > max_width {
                break;
            }
            best = Some((op.offset, m));
        }
        best
    }

    /// 文字的可断行位置，按偏移升序。
    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity>;

    /// 空行高度：没有任何文字时，行高取决于段落标记的字体。
    fn empty_line_metrics(&self, font: &FontSpec) -> TextMetrics {
        self.measure("", font)
    }

    /// 逐字形的位置与推进量。
    ///
    /// 默认实现按**前缀推进量**算，一字符一字形：第 i 个字符的 x = `measure(text[..i]).advance`。
    /// 这对「前缀可加」的度量是准确的（[`crate::simple_metrics::SimpleMetrics`] 属于此类）。
    ///
    /// **做 shaping 的实现必须覆盖它。** kerning 跨字符边界——单独量 `"A"` 没有 kern，
    /// 量 `"AB"` 时 A 的推进量会被 GPOS 调整，于是前缀和不再等于逐字形推进；
    /// 连字更直接：`f` + `i` 并成一个字形，字符数与字形数就对不上了。
    fn glyph_positions(&self, text: &str, font: &FontSpec) -> Vec<GlyphPosition> {
        let mut out = Vec::new();
        let mut prev_x = 0;
        let mut prev_start: Option<usize> = None;
        for (i, _) in text.char_indices() {
            let x = self.measure(&text[..i], font).advance;
            if let Some(start) = prev_start {
                out.push(GlyphPosition { start, end: i, x: prev_x, advance: x - prev_x });
            }
            prev_start = Some(i);
            prev_x = x;
        }
        if let Some(start) = prev_start {
            let total = self.measure(text, font).advance;
            out.push(GlyphPosition { start, end: text.len(), x: prev_x, advance: total - prev_x });
        }
        out
    }
}
