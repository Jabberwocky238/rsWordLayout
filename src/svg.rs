//! SVG 后端。
//!
//! 选 SVG 做第一个后端是因为它**保持矢量**：布局产物的 twips 坐标只做一次线性换算就落到
//! SVG 用户单位，不经过栅格化，所以浏览器里看到的偏移就是布局算出来的偏移，便于核对几何。
//! 接 Vulkan / OpenGL 时换一个 `Canvas` 实现即可，布局层不动。
//!
//! 坐标：布局用 y 向下，SVG 也是 y 向下，不需要翻转（PDF 后端才需要）。

use std::fmt::Write as _;

use crate::canvas::{Canvas, Color, TextRun};
use crate::geom::{Rect, Size, TWIPS_PER_POINT, Twips};

/// twips → SVG 用户单位。1 单位 = 1 pt，页面尺寸因此是点数（A4 ≈ 595×842）。
fn u(v: Twips) -> f64 {
    f64::from(v) / f64::from(TWIPS_PER_POINT)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// 每页一个 `<svg>`，拼成一个 HTML 页面便于浏览。
#[derive(Default)]
pub struct SvgCanvas {
    pages: Vec<String>,
    cur: String,
    size: Size,
}

impl SvgCanvas {
    pub fn new() -> SvgCanvas {
        SvgCanvas::default()
    }

    /// 全部页面的 SVG 片段。
    pub fn pages(&self) -> &[String] {
        &self.pages
    }

    /// 包成可直接打开的 HTML。
    pub fn into_html(self, title: &str) -> String {
        let mut s = String::new();
        s.push_str("<!DOCTYPE html>\n<html lang=\"zh\">\n<head>\n<meta charset=\"utf-8\">\n");
        let _ = write!(s, "<title>{}</title>\n", esc(title));
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
        let _ = write!(
            s,
            "<div class=\"bar\"><b>{}</b><span>{} 页</span>\
             <span>rsword-layout · SVG 后端</span></div>\n",
            esc(title),
            self.pages.len()
        );
        for p in &self.pages {
            s.push_str(p);
            s.push('\n');
        }
        s.push_str("</body>\n</html>\n");
        s
    }
}

impl Canvas for SvgCanvas {
    type Error = std::fmt::Error;

    fn begin_page(&mut self, size: Size) -> Result<(), Self::Error> {
        self.size = size;
        self.cur.clear();
        write!(
            self.cur,
            "<svg class=\"pg\" xmlns=\"http://www.w3.org/2000/svg\" \
             width=\"{w:.2}\" height=\"{h:.2}\" viewBox=\"0 0 {w:.2} {h:.2}\">",
            w = u(size.width),
            h = u(size.height)
        )
    }

    fn end_page(&mut self) -> Result<(), Self::Error> {
        self.cur.push_str("</svg>");
        self.pages.push(std::mem::take(&mut self.cur));
        Ok(())
    }

    fn draw_text(&mut self, run: &TextRun<'_>) -> Result<(), Self::Error> {
        let f = run.font;
        // 字号：半点 → 点。
        let size_pt = f64::from(f.size_half_points) / 2.0;
        write!(
            self.cur,
            "<text x=\"{x:.2}\" y=\"{y:.2}\" font-family=\"{fam}\" font-size=\"{sz:.2}\"",
            x = u(run.x),
            y = u(run.baseline_y),
            fam = esc(&f.family),
            sz = size_pt
        )?;
        if f.bold {
            self.cur.push_str(" font-weight=\"bold\"");
        }
        if f.italic {
            self.cur.push_str(" font-style=\"italic\"");
        }
        if run.color != Color::BLACK {
            write!(
                self.cur,
                " fill=\"#{:02x}{:02x}{:02x}\"",
                run.color.r, run.color.g, run.color.b
            )?;
        }
        // 关键：布局已算好每个片段的 x，交给浏览器自己排版会偏移。
        // `xml:space` 保留前后空格，`textLength` 不设——我们要看真实字体下的自然宽度，
        // 与布局用的近似度量的差异正是要观察的东西。
        write!(self.cur, " xml:space=\"preserve\">{}</text>", esc(run.text))
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) -> Result<(), Self::Error> {
        write!(
            self.cur,
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
             fill=\"#{:02x}{:02x}{:02x}\"/>",
            u(rect.x),
            u(rect.y),
            u(rect.width),
            u(rect.height),
            color.r,
            color.g,
            color.b
        )
    }

    fn draw_image(&mut self, id: &str, rect: Rect) -> Result<(), Self::Error> {
        // 媒体解析尚未接入：先画占位框，避免静默丢内容。
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
