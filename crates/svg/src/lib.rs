//! SVG 后端：[`VectorCanvas`] 的一个实现。
//!
//! `SvgCanvas` / `render_html` 保留浏览器 `<text>`，文字可选中与搜索，
//! 但浏览器会重新整形，与引擎已经定位的字形可能不同。`fontenv` feature 提供
//! `render_outlined_html`：从实际字体提取矢量轮廓，直接使用每个字形的精确位置，
//! 不再交给浏览器排字。两条路径都不经过栅格化。

use std::fmt::Write as _;

use rsword_layout_core::{
    Color, DrawCmd, FillRule, LineCap, LineJoin, Paint, PaintOp, Path, PathSeg, Stroke, Transform,
    VectorCanvas,
};
use rsword_layout_core::{TWIPS_PER_POINT, Twips};
use rsword_layout_core::PaintList;

#[cfg(feature = "fontenv")]
mod outlined;
#[cfg(feature = "fontenv")]
pub use outlined::{OutlineError, render_outlined_html};

/// twips → SVG 用户单位。1 单位 = 1pt，A4 因此是 595×842。
fn u(v: Twips) -> f64 {
    f64::from(v) / f64::from(TWIPS_PER_POINT)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;").replace('\'', "&#39;")
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn rule_attr(r: FillRule) -> &'static str {
    match r {
        FillRule::NonZero => "nonzero",
        FillRule::EvenOdd => "evenodd",
    }
}

/// `w:w`（横向缩放）与 `w:spacing`（字符间距）落到 `<text>` 上。都是默认值时为空串。
///
/// 布局断行时量的宽度已经含这两项（`FontMetrics::measure`）；`<text>` 若不带，浏览器按
/// 原宽排字，缩放与紧缩的文字画出界、压到下一个片段，加宽的又挤在一起。
///
/// - 缩放：以片段起点为不动点横向压，`matrix(s 0 0 1 x·(1−s) 0)`。字形轮廓跟着压，
///   这也是 `w:w` 本来的样子；
/// - 间距：`letter-spacing`，浏览器加在每个字符之后，与引擎「每个 cluster 一次、
///   末字也加」同口径（组合符号上浏览器怎么数不归这里管）。值写**用户单位**（1 = 1pt）
///   不带 `pt`，避免引入 CSS 绝对单位换算。它处在缩放之后的坐标系里，所以预先除以
///   比例，落到页面上仍是原值：与度量「间距不随缩放」的假定一致。
fn spacing_attrs(font: &rsword_layout_core::FontSpec, origin_x_pt: f64) -> String {
    let mut out = String::new();
    let scale = if font.scale_pct != 100 && font.scale_pct > 0 {
        f64::from(font.scale_pct) / 100.0
    } else {
        1.0
    };
    if scale != 1.0 {
        let _ = write!(
            out,
            " transform=\"matrix({scale:.4} 0 0 1 {:.6} 0)\"",
            origin_x_pt * (1.0 - scale)
        );
    }
    if font.letter_spacing != 0 {
        let spacing = f64::from(font.letter_spacing) / f64::from(TWIPS_PER_POINT) / scale;
        let _ = write!(out, " letter-spacing=\"{spacing:.4}\"");
    }
    out
}

/// `w:caps` / `w:smallCaps` 落到 `<text>` 上。不变换时为空串。
///
/// 与 [`spacing_attrs`] 同理，`<text>` 里保留**原文**（可选中、可搜索），大小写交给渲染器。
/// 这只是近似，字形级几何在绘制指令的字形序列里，这里不用：浏览器的 `text-transform`
/// 按 Unicode 完整映射（`ß` → `SS`），引擎按一对一映射（`ß` 不变）；`font-variant` 的
/// 小型大写用字体的 `smcp` 或浏览器自己的缩小比例（常见约 70%），不是引擎的 80% 取整到半点。
fn caps_attr(font: &rsword_layout_core::FontSpec) -> &'static str {
    match font.caps {
        rsword_layout_core::Caps::None => "",
        rsword_layout_core::Caps::All => " style=\"text-transform:uppercase\"",
        rsword_layout_core::Caps::Small => " font-variant=\"small-caps\"",
    }
}

/// 路径转 SVG 的 `d` 属性。
fn path_d(p: &Path) -> String {
    let mut d = String::new();
    for seg in &p.segs {
        match *seg {
            PathSeg::MoveTo { x, y } => {
                let _ = write!(d, "M{:.2} {:.2} ", u(x), u(y));
            }
            PathSeg::LineTo { x, y } => {
                let _ = write!(d, "L{:.2} {:.2} ", u(x), u(y));
            }
            PathSeg::CurveTo { c1x, c1y, c2x, c2y, x, y } => {
                let _ = write!(
                    d,
                    "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
                    u(c1x),
                    u(c1y),
                    u(c2x),
                    u(c2y),
                    u(x),
                    u(y)
                );
            }
            PathSeg::Close => d.push_str("Z "),
        }
    }
    d.trim_end().to_string()
}

fn stroke_attrs(s: &Stroke) -> String {
    let mut out = format!(" stroke-width=\"{:.2}\"", u(s.width));
    match s.cap {
        LineCap::Butt => {}
        LineCap::Round => out.push_str(" stroke-linecap=\"round\""),
        LineCap::Square => out.push_str(" stroke-linecap=\"square\""),
    }
    match s.join {
        LineJoin::Miter => {
            let _ = write!(out, " stroke-miterlimit=\"{:.2}\"", s.miter_limit);
        }
        LineJoin::Round => out.push_str(" stroke-linejoin=\"round\""),
        LineJoin::Bevel => out.push_str(" stroke-linejoin=\"bevel\""),
    }
    if !s.dash.is_empty() {
        let dashes: Vec<String> = s.dash.iter().map(|d| format!("{:.2}", u(*d))).collect();
        let _ = write!(out, " stroke-dasharray=\"{}\"", dashes.join(" "));
        if s.dash_offset != 0 {
            let _ = write!(out, " stroke-dashoffset=\"{:.2}\"", u(s.dash_offset));
        }
    }
    out
}

fn alpha_attr(name: &str, p: &Paint) -> String {
    if p.alpha >= 1.0 {
        String::new()
    } else {
        format!(" {name}=\"{:.3}\"", p.alpha)
    }
}

/// SVG 画布。每页一个 `<svg>`。
#[derive(Default)]
pub struct SvgCanvas {
    pages: Vec<String>,
    cur: String,
    /// Open SVG groups; a saved depth restores every subsequent state group.
    depth: usize,
    saved_depths: Vec<usize>,
    clip_serial: usize,
}

impl SvgCanvas {
    pub fn new() -> SvgCanvas {
        SvgCanvas::default()
    }

    pub fn pages(&self) -> &[String] {
        &self.pages
    }

    /// 包成可直接打开的 HTML。
    pub fn into_html(self, title: &str) -> String {
        self.into_html_with_caption(title, "矢量 SVG · 放大不失真、文字可选中")
    }

    fn into_html_with_caption(self, title: &str, caption: &str) -> String {
        let mut s = String::new();
        s.push_str("<!DOCTYPE html>\n<html lang=\"zh\">\n<head>\n<meta charset=\"utf-8\">\n");
        let _ = writeln!(s, "<title>{}</title>", esc(title));
        s.push_str(
            "<style>\n\
             :root{color-scheme:light dark}\n\
             body{margin:0;padding:24px;background:#8a8a8a;\
             font-family:system-ui,-apple-system,'Segoe UI',sans-serif}\n\
             .bar{max-width:980px;margin:0 auto 16px;color:#fff;font-size:14px;\
             display:flex;gap:16px;align-items:baseline;flex-wrap:wrap}\n\
             .bar b{font-size:16px}\n\
             .pg{display:block;margin:0 auto 24px;background:#fff;\
             box-shadow:0 2px 12px rgba(0,0,0,.4)}\n\
             @media (prefers-color-scheme:dark){body{background:#3a3a3a}}\n\
             </style>\n</head>\n<body>\n",
        );
        let _ = writeln!(
            s,
            "<div class=\"bar\"><b>{}</b><span>{} 页</span>\
             <span>{}</span></div>",
            esc(title),
            self.pages.len(),
            esc(caption)
        );
        for p in &self.pages {
            s.push_str(p);
            s.push('\n');
        }
        s.push_str("</body>\n</html>\n");
        s
    }
}

impl VectorCanvas for SvgCanvas {
    type Error = std::fmt::Error;

    fn begin_page(&mut self, width: Twips, height: Twips) -> Result<(), Self::Error> {
        self.cur.clear();
        self.depth = 0;
        self.saved_depths.clear();
        self.clip_serial = 0;
        write!(
            self.cur,
            "<svg class=\"pg\" xmlns=\"http://www.w3.org/2000/svg\" \
             width=\"{w:.2}\" height=\"{h:.2}\" viewBox=\"0 0 {w:.2} {h:.2}\">",
            w = u(width),
            h = u(height)
        )
    }

    fn end_page(&mut self) -> Result<(), Self::Error> {
        // 未闭合的 Save 在收页时补齐，避免产出非法 XML。
        for _ in 0..self.depth {
            self.cur.push_str("</g>");
        }
        self.depth = 0;
        self.saved_depths.clear();
        self.cur.push_str("</svg>");
        self.pages.push(std::mem::take(&mut self.cur));
        Ok(())
    }

    fn draw(&mut self, cmd: &DrawCmd) -> Result<(), Self::Error> {
        match cmd {
            DrawCmd::Save => {
                self.saved_depths.push(self.depth);
                self.cur.push_str("<g>");
                self.depth += 1;
                Ok(())
            }
            DrawCmd::Restore => {
                // 容忍多余的 Restore：忽略而非崩溃。
                if let Some(depth) = self.saved_depths.pop() {
                    for _ in depth..self.depth {
                        self.cur.push_str("</g>");
                    }
                    self.depth = depth;
                }
                Ok(())
            }
            DrawCmd::Transform(t) => {
                // SVG 的 matrix 与 PDF 同序（a b c d e f），但平移量以用户单位计。
                let Transform { a, b, c, d, e, f } = *t;
                let [a, b, c, d] = [a, b, c, d].map(f64::from);
                self.depth += 1;
                write!(
                    self.cur,
                    "<g transform=\"matrix({a} {b} {c} {d} {} {})\">",
                    f64::from(e) / f64::from(TWIPS_PER_POINT),
                    f64::from(f) / f64::from(TWIPS_PER_POINT)
                )
            }
            DrawCmd::Clip { path, rule } => {
                // SVG 的裁剪要具名引用，这里用递增 id。
                let id = format!("clip{}-{}", self.pages.len(), self.clip_serial);
                self.clip_serial += 1;
                self.depth += 1;
                write!(
                    self.cur,
                    "<defs><clipPath id=\"{id}\" clip-rule=\"{}\">\
                     <path d=\"{}\"/></clipPath></defs><g clip-path=\"url(#{id})\">",
                    rule_attr(*rule),
                    path_d(path)
                )
            }
            DrawCmd::DrawPath { path, op } => {
                if path.is_empty() {
                    return Ok(());
                }
                let d = path_d(path);
                match op {
                    PaintOp::Fill { paint, rule } => write!(
                        self.cur,
                        "<path d=\"{d}\" fill=\"{}\" fill-rule=\"{}\"{}/>",
                        hex(paint.color),
                        rule_attr(*rule),
                        alpha_attr("fill-opacity", paint)
                    ),
                    PaintOp::Stroke { paint, stroke } => write!(
                        self.cur,
                        "<path d=\"{d}\" fill=\"none\" stroke=\"{}\"{}{}/>",
                        hex(paint.color),
                        stroke_attrs(stroke),
                        alpha_attr("stroke-opacity", paint)
                    ),
                    PaintOp::FillThenStroke { fill, rule, stroke_paint, stroke } => write!(
                        self.cur,
                        "<path d=\"{d}\" fill=\"{}\" fill-rule=\"{}\"{} \
                         stroke=\"{}\"{}{}/>",
                        hex(fill.color),
                        rule_attr(*rule),
                        alpha_attr("fill-opacity", fill),
                        hex(stroke_paint.color),
                        stroke_attrs(stroke),
                        alpha_attr("stroke-opacity", stroke_paint)
                    ),
                }
            }
            DrawCmd::DrawGlyphs { origin_x_pt, origin_y_fine, text, font, paint, .. } => {
                if text.is_empty() {
                    return Ok(());
                }
                // 用 <text> 而非把字形展开成路径：产出可选中、可搜索，
                // 体积也小得多。字形序列在这里用不上。
                write!(
                    self.cur,
                    "<text x=\"{:.6}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"{:.2}\"",
                    origin_x_pt,
                    *origin_y_fine as f64 / 100.0,
                    esc_attr(&font.family),
                    font.size_pt()
                )?;
                self.cur.push_str(&spacing_attrs(font, *origin_x_pt));
                if font.bold {
                    self.cur.push_str(" font-weight=\"bold\"");
                }
                if font.italic {
                    self.cur.push_str(" font-style=\"italic\"");
                }
                self.cur.push_str(caps_attr(font));
                if paint.color != Color::BLACK {
                    write!(self.cur, " fill=\"{}\"", hex(paint.color))?;
                }
                self.cur.push_str(&alpha_attr("fill-opacity", paint));
                write!(self.cur, " xml:space=\"preserve\">{}</text>", esc(text))
            }
            DrawCmd::DrawImage { id, rect } => {
                // 媒体解析未接入：画占位框而不是静默丢内容。
                write!(
                    self.cur,
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
                     fill=\"none\" stroke=\"#c00\" stroke-dasharray=\"4 2\"/>\
                     <text x=\"{:.2}\" y=\"{:.2}\" font-size=\"8\" fill=\"#c00\">image {}</text>",
                    u(rect.x),
                    u(rect.y),
                    u(rect.width),
                    u(rect.height),
                    u(rect.x) + 2.0,
                    u(rect.y) + 10.0,
                    esc(id)
                )
            }
        }
    }
}

/// 便利函数：整篇文档渲染成 HTML。
pub fn render_html(list: &PaintList, title: &str) -> String {
    let mut canvas = SvgCanvas::new();
    // SvgCanvas 的 Error 是 fmt::Error，向 String 写不会失败。
    let _ = list.replay(&mut canvas);
    canvas.into_html(title)
}
