//! 绘制指令：core 的输出契约。
//!
//! **这一层没有像素。** 坐标是 twips，字号是半点，字形是字体内的编号——
//! 全部与分辨率无关。栅格化属于后端，不属于这里。
//!
//! 契约照搬 dvipdfmx 的 `pdfdev.h`：那里的 `pdf_dev_set_string(xpos, ypos, ...)`
//! 用 `spt_t` 定点数，注释写明「They must be position in the **user space**」，
//! 而 device space 的换算系数 `unit_conv` 在 `pdf_init_device` 时设一次，
//! 调用方从不把它烘进坐标。四十年的排版实践给出的分层，照做。
//!
//! 这样分层的直接收益：
//!
//! - SVG / PDF 后端**根本不必栅格化**——`<path>` 与 PDF 文字算子本身就是矢量，
//!   放大无限清晰；
//! - GPU 后端自己决定用图集、距离场还是曲线着色器，那是它的实现细节，
//!   core 不该知道，也不该为此背上一个字形位图缓存；
//! - 同一份 [`PaintList`] 喂给不同后端，几何必然一致。

use crate::canvas::Color;
use crate::geom::{Rect, Twips};
use crate::measure::FontSpec;

/// 字体标识。
///
/// 与 `docx_layout::fontenv` 的 `FaceId::sha256()` 对齐——按内容哈希而非文件名，
/// 因此整形、栅格化、后端三处必然指向同一份字体。
pub type FaceId = String;

/// 一个已定位的字形。
///
/// 位置是**笔位**（基线左端）加 shaping 给出的偏移，单位 twips。
/// 这里存 `glyph_id` 而不是字符：连字与阿拉伯语形态没有对应的单个 `char`。
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedGlyph {
    pub face: FaceId,
    pub glyph_id: u32,
    /// 笔位，twips。
    pub x: Twips,
    pub y: Twips,
    /// 字号，半点。后端按它与自身的 DPI 决定栅格化尺寸。
    pub size_half_points: u32,
}

/// 一条绘制指令。
///
/// 刻意保持小而正交：后端只需要认得这几种，新增效果应当先问能否用已有的表达。
#[derive(Debug, Clone, PartialEq)]
pub enum PaintCmd {
    /// 一串同字体同颜色的字形。
    ///
    /// 保留 `text` 是为了 PDF/SVG 后端能直接输出文字（可选中、可搜索），
    /// 而不是被迫画成路径；GPU 后端用 `glyphs` 即可。
    Glyphs {
        glyphs: Vec<PositionedGlyph>,
        /// 原文，供能直接排文字的后端使用。
        text: String,
        /// 字体请求（族名、粗斜体等），供后端做自己的字体匹配。
        font: FontSpec,
        color: Color,
    },
    /// 实心矩形：底纹、边框、下划线、删除线都归一到它。
    Rect { rect: Rect, color: Color },
    /// 图片。`id` 是媒体句柄，由后端解析取字节。
    Image { id: String, rect: Rect },
}

/// 一页的绘制指令。
#[derive(Debug, Clone, PartialEq)]
pub struct PaintPage {
    /// 页面尺寸，twips。
    pub width: Twips,
    pub height: Twips,
    /// 正文区（已扣页边距），供页眉页脚定位与调试参考。
    pub content_area: Rect,
    pub cmds: Vec<PaintCmd>,
}

impl PaintPage {
    pub fn new(width: Twips, height: Twips, content_area: Rect) -> PaintPage {
        PaintPage { width, height, content_area, cmds: Vec::new() }
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
}

/// 文字整形：把一段文字变成字形序列。
///
/// 留在 core 是因为它的产物**与分辨率无关**——glyph id 与 advance 都是字体单位，
/// 由 GSUB/GPOS 表决定，不是像素。这与栅格化不同，后者必须知道目标分辨率。
pub trait TextShaper {
    /// 整形一段文字。返回的位置相对该段起点，单位 twips。
    ///
    /// 返回空表示无法整形（字体缺失等）；调用方不应把它当作「这段文字不占宽度」。
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun>;
}

/// 整形结果里的一个字形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedRun {
    pub face_index: usize,
    pub glyph_id: u32,
    /// 推进量与偏移，twips。
    pub x_advance: Twips,
    pub x_offset: Twips,
    pub y_offset: Twips,
}

// ---------------------------------------------------------------------------
// 从布局产物生成绘制指令
// ---------------------------------------------------------------------------

use crate::fragment::{Fragment, LaidOutDocument, Page};

/// 把一页布局产物转成绘制指令。
///
/// **注意签名里没有 DPI**：这是与旧 `build_page(page, dpi, ...)` 的关键差别。
/// 分辨率是后端的事，core 只描述文档如何占据空间。
///
/// `shaper` 为 `None` 时 `PaintCmd::Glyphs` 的 `glyphs` 为空但保留 `text`——
/// 能直接排文字的后端（SVG / PDF）照样能画，只有 GPU 后端需要字形序列。
pub fn paint_page(page: &Page, shaper: Option<&dyn TextShaper>, faces: &[FaceId]) -> PaintPage {
    let mut out = PaintPage::new(page.size.width, page.size.height, page.content_area);
    for frag in &page.fragments {
        match frag {
            Fragment::Rect { rect, color } => {
                out.cmds.push(PaintCmd::Rect { rect: *rect, color: *color });
            }
            Fragment::Image { id, rect } => {
                out.cmds.push(PaintCmd::Image { id: id.clone(), rect: *rect });
            }
            Fragment::Text(t) => {
                let glyphs = match shaper {
                    Some(s) => position_glyphs(s, t, faces),
                    None => Vec::new(),
                };
                out.cmds.push(PaintCmd::Glyphs {
                    glyphs,
                    text: t.text.clone(),
                    font: t.font.clone(),
                    color: t.color,
                });
            }
        }
    }
    out
}

/// 把整形结果摊成绝对坐标的字形。
fn position_glyphs(
    shaper: &dyn TextShaper,
    t: &crate::fragment::TextFragment,
    faces: &[FaceId],
) -> Vec<PositionedGlyph> {
    let mut pen = t.x;
    shaper
        .shape(&t.text, &t.font)
        .into_iter()
        .filter_map(|g| {
            let face = faces.get(g.face_index)?.clone();
            let out = PositionedGlyph {
                face,
                glyph_id: g.glyph_id,
                x: pen + g.x_offset,
                y: t.baseline_y - g.y_offset,
                size_half_points: t.font.size_half_points,
            };
            pen += g.x_advance;
            Some(out)
        })
        .collect()
}

/// 整篇文档。
pub fn paint_document(
    doc: &LaidOutDocument,
    shaper: Option<&dyn TextShaper>,
    faces: &[FaceId],
) -> PaintList {
    PaintList {
        pages: doc.pages.iter().map(|p| paint_page(p, shaper, faces)).collect(),
    }
}
