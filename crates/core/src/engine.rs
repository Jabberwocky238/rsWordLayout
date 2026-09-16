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

/// 排好的一行，尚未定位到页面。
struct PendingLine {
    height: Twips,
    baseline: Twips,
    /// (相对行首的 x, 文字, 字体, 颜色)
    pieces: Vec<(Twips, String, FontSpec, Color)>,
    width: Twips,
    is_last: bool,
    /// 首行要额外吃 `indent_first_line`（可负，即悬挂缩进）。
    is_first: bool,
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
        let area = self.setup.content_area();
        let mut doc = LaidOutDocument::default();
        let mut page = Page::new(self.setup.size, area);
        let mut cursor = area.y;

        for (idx, para) in paras.iter().enumerate() {
            let lines = self.break_paragraph(para, area.width);
            let block_height: Twips = lines.iter().map(|l| l.height).sum();

            if para.page_break_before && !page.fragments.is_empty() {
                doc.pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor = area.y;
            }

            cursor += para.space_before;

            // keepLines：整段放不下就先翻页（除非本页是空的，那样翻了也没用）。
            if para.keep_lines
                && cursor + block_height > area.bottom()
                && !page.fragments.is_empty()
            {
                doc.pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                cursor = area.y;
            }

            // keepNext：本段是最后一段时无意义；否则要保证下一段至少第一行同页。
            let next_first_line = if para.keep_next {
                paras.get(idx + 1).and_then(|n| {
                    self.break_paragraph(n, area.width).first().map(|l| l.height)
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
                    doc.pages.push(std::mem::replace(&mut page, Page::new(self.setup.size, area)));
                    cursor = area.y;
                }
                self.place_line(&mut page, &line, para, area, cursor);
                cursor += line.height;
            }

            cursor += para.space_after;
        }

        doc.pages.push(page);
        doc
    }

    /// 把一行放到页面上，处理水平对齐。
    fn place_line(
        &self,
        page: &mut Page,
        line: &PendingLine,
        para: &Para,
        area: Rect,
        top: Twips,
    ) {
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
        for (i, (dx, text, font, color)) in line.pieces.iter().enumerate() {
            let x = base_x + offset + dx + justify_gap * (i as Twips);
            page.fragments.push(Fragment::Text(TextFragment {
                x,
                baseline_y,
                text: text.clone(),
                font: font.clone(),
                color: *color,
                source_node: para.source_node,
            }));
        }
    }

    /// 段落断行。
    fn break_paragraph(&self, para: &Para, area_width: Twips) -> Vec<PendingLine> {
        let avail = (area_width - para.indent_left - para.indent_right).max(1);
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
            lines.push(PendingLine {
                height: h,
                baseline: m.ascent,
                pieces: Vec::new(),
                width: 0,
                is_last: true,
                is_first: true,
            });
            return lines;
        }

        let mut cur: Vec<(Twips, String, FontSpec, Color)> = Vec::new();
        let mut cur_w: Twips = 0;
        let mut cur_ascent: Twips = 0;
        let mut cur_descent: Twips = 0;
        let mut cur_natural: Twips = 0;
        let mut first_line = true;

        // 首行缩进吃掉的宽度。
        let mut line_avail = (avail - if first_line { para.indent_first_line } else { 0 }).max(1);

        for run in &para.runs {
            let mut rest: &str = &run.text;
            while !rest.is_empty() {
                let remain = (line_avail - cur_w).max(0);
                let m_all = self.metrics.measure(rest, &run.font);

                if m_all.advance <= remain {
                    // 整段剩余放得下。
                    cur.push((cur_w, rest.to_string(), run.font.clone(), run.color));
                    cur_w += m_all.advance;
                    cur_ascent = cur_ascent.max(m_all.ascent);
                    cur_descent = cur_descent.max(m_all.descent);
                    cur_natural = cur_natural.max(m_all.natural_height());
                    break;
                }

                // 放不下：找能塞进去的最长前缀。
                match self.metrics.fit(rest, &run.font, remain) {
                    Some((cut, m)) if cut > 0 => {
                        cur.push((cur_w, rest[..cut].to_string(), run.font.clone(), run.color));
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
                            cur.push((cur_w, rest[..n].to_string(), run.font.clone(), run.color));
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
                });
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
