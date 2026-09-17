//! 布局引擎：y 游标模型。
//!
//! 主循环就是「量高度 → 放不下就翻页 → 放置 → 推进游标」。选它而不是全局最优（TeX §38 那种
//! 动态规划）是因为 Word 本身就是逐行贪心，追求与 Word 接近的结果时贪心才是对的模型。
//!
//! 本版实现段落布局与分页，不含表格、浮动环绕、分栏。缺口见 `docs` 与 README。

use crate::canvas::Color;
use crate::fragment::{Fragment, LaidOutDocument, Page, TextFragment};
use crate::geom::{Margins, Rect, Size, Twips};
use crate::measure::{FontMetrics, FontSpec};
use crate::trace::{DocumentTrace, LineTrace, PageTrace, Terminator, expand_glyphs, pt};

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
        }
    }
}

/// 段内断开（`w:br`）。
///
/// 计数约定（量具方法 §4）在这里有直接后果，别只当成「换个行」：
///
/// - 软回车（`textWrapping`）在 `Range.Text` 里占 1 个字符，Word **画 1 个字形**；
/// - 手动分页符在 `Range.Text` 里也占 1 个字符，但画几个**取决于位置**
///   （行首独占 0 / 紧跟段落标记 1 / 段中 0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakKind {
    /// `w:br`、`w:br w:type="textWrapping"`：断行不翻页。
    Line,
    /// `w:br w:type="page"`：翻页。
    Page,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    /// 本 run 之后的段内断开。
    ///
    /// 只带断开不带文字的 run（`text` 为空）是合法的，用来表达段首就有的分页符——
    /// 实测夹具里确实有这种（`\x0c` 自成一条行记录，Word 为它画 0 个字形）。
    pub break_after: Option<BreakKind>,
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

/// 行内一段同字体的文字，已知它相对行首的位置。
struct Piece {
    /// 相对行首的 x。
    dx: Twips,
    text: String,
    font: FontSpec,
    color: Color,
    /// 首字符在 Word 偏移空间里的下标，供 `trace` 出具「所属源字符区间」（量具方法 §9.7）。
    source_char: usize,
}

/// 一行定位之后的结果：每个片段的最终 x 与共用的基线 y。
struct PlacedLine {
    baseline_y: Twips,
    xs: Vec<Twips>,
}

/// 排好的一行，尚未定位到页面。
struct PendingLine {
    height: Twips,
    baseline: Twips,
    pieces: Vec<Piece>,
    width: Twips,
    is_last: bool,
    /// 首行要额外吃 `indent_first_line`（可负，即悬挂缩进）。
    is_first: bool,
    /// 本行覆盖的源字符区间（不含段落标记；段落标记由末行的终止符表达）。
    source_start: usize,
    source_end: usize,
    /// 本行之后要翻页（`w:br w:type="page"`）。
    page_break_after: bool,
}

pub struct Engine<'m, M: FontMetrics> {
    metrics: &'m M,
    setup: PageSetup,
}

impl<'m, M: FontMetrics> Engine<'m, M> {
    pub fn new(metrics: &'m M, setup: PageSetup) -> Engine<'m, M> {
        Engine { metrics, setup }
    }

    /// 把段落序列排成页面。
    pub fn layout(&self, paras: &[Para]) -> LaidOutDocument {
        self.layout_inner(paras, None).0
    }

    /// 排版并同时出具验收量具要的「页 → 行 → 字形」轨迹（量具方法 §9.7）。
    ///
    /// 与 [`Engine::layout`] 走的是**同一条主循环**，不是另算一遍——
    /// 否则轨迹与真实产物可能悄悄分叉，那样量出来的就不是这个引擎。
    pub fn layout_traced(&self, paras: &[Para]) -> (LaidOutDocument, DocumentTrace) {
        let mut trace = DocumentTrace::default();
        let doc = self.layout_inner(paras, Some(&mut trace)).0;
        (doc, trace)
    }

    fn layout_inner(
        &self,
        paras: &[Para],
        mut trace: Option<&mut DocumentTrace>,
    ) -> (LaidOutDocument, ()) {
        let area = self.setup.content_area();
        let mut doc = LaidOutDocument::default();
        let mut page = Page::new(self.setup.size, area);
        let mut cursor = area.y;
        // 当前页已排好的行轨迹；翻页时连同页面一起收走。
        let mut page_lines: Vec<LineTrace> = Vec::new();
        // Word 偏移空间里的游标：每段末尾算一个段落标记。
        let mut source_cursor = 0usize;

        // 翻页：把页面与对应的行轨迹一起收走，保证两者页号始终一致。
        macro_rules! flush_page {
            () => {{
                doc.pages
                    .push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                if let Some(t) = trace.as_mut() {
                    let index = t.pages.len();
                    t.pages.push(PageTrace {
                        index,
                        width: pt(self.setup.size.width),
                        height: pt(self.setup.size.height),
                        lines: std::mem::take(&mut page_lines),
                    });
                }
                cursor = area.y;
            }};
        }

        for (idx, para) in paras.iter().enumerate() {
            let para_base = source_cursor;
            let para_chars: usize = para.runs.iter().map(|r| r.text.chars().count()).sum();
            // 段落标记本身占一个偏移位，与 Word 的 `Range` 数法一致。
            source_cursor = para_base + para_chars + 1;

            let lines = self.break_paragraph(para, area.width, para_base);
            let block_height: Twips = lines.iter().map(|l| l.height).sum();

            if para.page_break_before && !page.fragments.is_empty() {
                flush_page!();
            }

            cursor += para.space_before;

            // keepLines：整段放不下就先翻页（除非本页是空的，那样翻了也没用）。
            if para.keep_lines
                && cursor + block_height > area.bottom()
                && !page.fragments.is_empty()
            {
                flush_page!();
            }

            // keepNext：本段是最后一段时无意义；否则要保证下一段至少第一行同页。
            let next_first_line = if para.keep_next {
                paras.get(idx + 1).and_then(|n| {
                    self.break_paragraph(n, area.width, 0).first().map(|l| l.height)
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
                    flush_page!();
                }
                let placed = self.place_line(&mut page, &line, para, area, cursor);
                if trace.is_some() {
                    page_lines.push(self.trace_line(&line, &placed, page_lines.len(), cursor));
                }
                cursor += line.height;

                // 手动分页符：本行之后翻页。
                //
                // 与「放不下就翻页」不同，这一条**不看还剩多少空间**，也不管本页是否为空——
                // 源里写了分页就是分页。实测夹具里有连续两个分页符的情形，
                // 那确实产生了一张只有一条行记录的页。
                if line.page_break_after {
                    flush_page!();
                }
            }

            cursor += para.space_after;
        }

        doc.pages.push(page);
        if let Some(t) = trace.as_mut() {
            let index = t.pages.len();
            t.pages.push(PageTrace {
                index,
                width: pt(self.setup.size.width),
                height: pt(self.setup.size.height),
                lines: page_lines,
            });
        }
        (doc, ())
    }

    /// 把一行的轨迹折出来：逐片段按前缀推进量展开成字形。
    fn trace_line(
        &self,
        line: &PendingLine,
        placed: &PlacedLine,
        index: usize,
        top: Twips,
    ) -> LineTrace {
        let mut glyphs = Vec::new();
        for (piece, &x) in line.pieces.iter().zip(placed.xs.iter()) {
            glyphs.extend(expand_glyphs(
                self.metrics,
                &piece.text,
                &piece.font,
                pt(x),
                pt(placed.baseline_y),
                piece.source_char,
            ));
        }
        LineTrace {
            index,
            top: pt(top),
            height: pt(line.height),
            baseline: pt(line.baseline),
            source_start: line.source_start,
            source_end: line.source_end,
            // 段落末行以段落标记收尾；其余是自动换行，源侧没有对应字符。
            terminator: if line.is_last { Terminator::ParagraphMark } else { Terminator::Wrap },
            glyphs,
        }
    }

    /// 把一行放到页面上，处理水平对齐。返回每个片段的最终 x 与基线 y，供轨迹复用。
    fn place_line(
        &self,
        page: &mut Page,
        line: &PendingLine,
        para: &Para,
        area: Rect,
        top: Twips,
    ) -> PlacedLine {
        let first = if line.is_first { para.indent_first_line } else { 0 };
        let avail = (area.width - para.indent_left - para.indent_right - first).max(0);
        let base_x = area.x + para.indent_left + first;
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
        let mut xs = Vec::with_capacity(line.pieces.len());
        for (i, piece) in line.pieces.iter().enumerate() {
            let x = base_x + offset + piece.dx + justify_gap * (i as Twips);
            xs.push(x);
            page.fragments.push(Fragment::Text(TextFragment {
                x,
                baseline_y,
                text: piece.text.clone(),
                font: piece.font.clone(),
                color: piece.color,
                source_node: para.source_node,
            }));
        }
        PlacedLine { baseline_y, xs }
    }

    /// 段落断行。
    ///
    /// `para_base` 是本段首字符在 Word 偏移空间里的下标，用来给每个片段标源字符位置
    /// （量具方法 §9.7 要求「所属源字符区间」）。只做记账，不影响断行决策。
    fn break_paragraph(&self, para: &Para, area_width: Twips, para_base: usize) -> Vec<PendingLine> {
        let avail = (area_width - para.indent_left - para.indent_right).max(1);
        let mut lines: Vec<PendingLine> = Vec::new();

        // 段内已消耗的字符数（含被 trim 掉的换行处空格：它们在源侧占位，
        // 引擎不画，但偏移必须继续走，否则后面所有片段的源下标都会错位）。
        let mut consumed = 0usize;

        // 空段落：高度由段落标记字体决定。
        if para.runs.iter().all(|r| r.text.is_empty()) {
            let font = para
                .runs
                .first()
                .map(|r| r.font.clone())
                .unwrap_or_else(|| FontSpec::new("Times New Roman", 24));
            let m = self.metrics.empty_line_metrics(&font);
            let h = self.line_height(para, m.ascent + m.descent, m.natural_height());
            lines.push(PendingLine {
                height: h,
                baseline: m.ascent,
                pieces: Vec::new(),
                width: 0,
                is_last: true,
                is_first: true,
                source_start: para_base,
                source_end: para_base,
                page_break_after: false,
            });
            return lines;
        }

        let mut cur: Vec<Piece> = Vec::new();
        let mut cur_w: Twips = 0;
        let mut cur_ascent: Twips = 0;
        let mut cur_descent: Twips = 0;
        let mut cur_natural: Twips = 0;
        let mut first_line = true;
        let mut line_start = para_base;

        // 首行缩进吃掉的宽度。
        let mut line_avail = (avail - if first_line { para.indent_first_line } else { 0 }).max(1);

        for run in &para.runs {
            let mut rest: &str = &run.text;
            while !rest.is_empty() {
                let remain = (line_avail - cur_w).max(0);
                let m_all = self.metrics.measure(rest, &run.font);

                if m_all.advance <= remain {
                    // 整段剩余放得下。
                    cur.push(Piece {
                        dx: cur_w,
                        text: rest.to_string(),
                        font: run.font.clone(),
                        color: run.color,
                        source_char: para_base + consumed,
                    });
                    consumed += rest.chars().count();
                    cur_w += m_all.advance;
                    cur_ascent = cur_ascent.max(m_all.ascent);
                    cur_descent = cur_descent.max(m_all.descent);
                    cur_natural = cur_natural.max(m_all.natural_height());
                    break;
                }

                // 放不下：找能塞进去的最长前缀。
                match self.metrics.fit(rest, &run.font, remain) {
                    Some((cut, m)) if cut > 0 => {
                        cur.push(Piece {
                            dx: cur_w,
                            text: rest[..cut].to_string(),
                            font: run.font.clone(),
                            color: run.color,
                            source_char: para_base + consumed,
                        });
                        consumed += rest[..cut].chars().count();
                        cur_w += m.advance;
                        cur_ascent = cur_ascent.max(m.ascent);
                        cur_descent = cur_descent.max(m.descent);
                        cur_natural = cur_natural.max(m.natural_height());
                        let after = rest[cut..].trim_start_matches(' ');
                        // 换行处被吃掉的空格：源侧仍占位。Word 对这些空格是**照画**的
                        // （量具方法 §4「行尾／终止符前的尾随空格」），引擎当前不画——
                        // 这是一处已知差异，靠偏移记账让它在比较时暴露成计数不符，而不是静默串行。
                        consumed += rest[cut..].chars().count() - after.chars().count();
                        rest = after;
                    }
                    _ => {
                        // 一个断点都塞不下：若本行已有内容就换行重试，否则硬塞一个字符避免死循环。
                        if cur.is_empty() && cur_w == 0 {
                            let c = rest.chars().next().expect("rest 非空");
                            let n = c.len_utf8();
                            let m = self.metrics.measure(&rest[..n], &run.font);
                            cur.push(Piece {
                                dx: cur_w,
                                text: rest[..n].to_string(),
                                font: run.font.clone(),
                                color: run.color,
                                source_char: para_base + consumed,
                            });
                            consumed += 1;
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
                    source_start: line_start,
                    source_end: para_base + consumed,
                    page_break_after: false,
                });
                line_start = para_base + consumed;
                cur_w = 0;
                cur_ascent = 0;
                cur_descent = 0;
                cur_natural = 0;
                first_line = false;
                line_avail = avail;
            }

            // 段内断开（`w:br`）：强制收行。
            //
            // 断开本身在源侧占 1 个字符位（`Range.Text` 里软回车是 \x0b、手动分页符是 \x0c），
            // 所以 `consumed` 要加 1——否则后面所有片段的源字符下标都会整体偏移，
            // 而那种错位在几何上看不出来，只会让比较器报计数不符。
            if let Some(kind) = run.break_after {
                consumed += 1;
                let h = if cur.is_empty() && cur_w == 0 {
                    // 断开独占一行：行高由断开所在 run 的字体定。
                    let m = self.metrics.empty_line_metrics(&run.font);
                    // 只有 ascent 会被下面的 PendingLine 用作基线；descent 随即重置，不赋。
                    cur_ascent = m.ascent;
                    self.line_height(para, m.ascent + m.descent, m.natural_height())
                } else {
                    self.line_height(para, cur_ascent + cur_descent, cur_natural)
                };
                lines.push(PendingLine {
                    height: h,
                    baseline: cur_ascent,
                    pieces: std::mem::take(&mut cur),
                    width: cur_w,
                    is_last: false,
                    is_first: first_line,
                    source_start: line_start,
                    source_end: para_base + consumed,
                    page_break_after: kind == BreakKind::Page,
                });
                line_start = para_base + consumed;
                cur_w = 0;
                cur_ascent = 0;
                cur_descent = 0;
                cur_natural = 0;
                first_line = false;
                line_avail = avail;
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
                source_start: line_start,
                source_end: para_base + consumed,
                page_break_after: false,
            });
        } else if let Some(last) = lines.last_mut() {
            last.is_last = true;
        }

        lines
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
