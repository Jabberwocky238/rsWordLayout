//! 绘制批次（feature `gpu`）。
//!
//! 一帧 = 一个顶点缓冲 + 一个索引缓冲 + 若干批次。批次只在**需要换纹理或换管线**时才切分，
//! 所以纯文字页通常只有一个批次。

use rsword_layout_core::{Color, PositionedGlyph};
use rsword_layout_core::Rect;
use rsword_layout_core::FontSpec;

use crate::atlas::GlyphKey;
use crate::vertex::{SOLID_UV, Vertex};
use crate::{GlyphSource, rect_px};

/// 批次类型决定用哪个纹理与混合模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchKind {
    /// 纯色（矩形、下划线、底纹），采样图集的纯色纹素。
    Solid,
    /// 文字，采样字形图集（单通道 alpha）。
    Glyph,
    /// 图片，采样各自的纹理，`texture` 是调用方分配的句柄。
    Image,
}

/// 一次 draw call。
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub kind: BatchKind,
    /// 索引缓冲中的起始位置与数量。
    pub index_offset: u32,
    pub index_count: u32,
    /// `Image` 批次用：媒体句柄，纹理由调用方按它查找。
    pub texture: Option<String>,
}

/// 一页的 GPU 数据。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<Batch>,
}

impl Frame {
    /// 顶点缓冲字节数，用于分配 GPU 内存。
    pub fn vertex_bytes(&self) -> usize {
        std::mem::size_of_val(self.vertices.as_slice())
    }

    pub fn index_bytes(&self) -> usize {
        std::mem::size_of_val(self.indices.as_slice())
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// 逐片段累积顶点，合并同类批次。
pub struct FrameBuilder {
    dpi: f32,
    frame: Frame,
    /// 当前未提交的批次。
    cur: Option<(BatchKind, Option<String>, u32)>,
}

impl FrameBuilder {
    pub fn new(dpi: f32) -> FrameBuilder {
        FrameBuilder { dpi, frame: Frame::default(), cur: None }
    }

    /// 切换批次：类型或纹理变了才真正切，否则继续累积。
    fn ensure(&mut self, kind: BatchKind, texture: Option<&str>) {
        let same = match &self.cur {
            Some((k, t, _)) => *k == kind && t.as_deref() == texture,
            None => false,
        };
        if !same {
            self.flush();
            self.cur = Some((kind, texture.map(str::to_owned), self.frame.indices.len() as u32));
        }
    }

    fn flush(&mut self) {
        if let Some((kind, texture, start)) = self.cur.take() {
            let count = self.frame.indices.len() as u32 - start;
            if count > 0 {
                self.frame.batches.push(Batch {
                    kind,
                    index_offset: start,
                    index_count: count,
                    texture,
                });
            }
        }
    }

    /// 压入一个四边形（两个三角形）。
    ///
    /// `rect` 是像素矩形 `(x, y, w, h)`，`uv` 是 `(u0, v0, u1, v1)`。
    fn quad(&mut self, rect: (f32, f32, f32, f32), uv: (f32, f32, f32, f32), c: Color, a: f32) {
        let base = self.frame.vertices.len() as u32;
        let (x, y, w, h) = rect;
        let (u0, v0, u1, v1) = uv;
        self.frame.vertices.extend_from_slice(&[
            Vertex::new(x, y, u0, v0, c, a),
            Vertex::new(x + w, y, u1, v0, c, a),
            Vertex::new(x + w, y + h, u1, v1, c, a),
            Vertex::new(x, y + h, u0, v1, c, a),
        ]);
        self.frame.indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    pub fn push_rect(&mut self, rect: Rect, color: Color) {
        self.ensure(BatchKind::Solid, None);
        let [x, y, w, h] = rect_px(rect, self.dpi);
        let (u, v) = SOLID_UV;
        self.quad((x, y, w, h), (u, v, u, v), color, 1.0);
    }

    /// 文字：逐字形取图集位置。
    ///
    /// `glyphs` 为 `None` 时**只推进笔位、不产生顶点**——没有图集就画不了字，
    /// 与其画错不如不画，调用方能从批次为空看出缺了字形源。
    /// 画一串已定位的字形。
    ///
    /// 与旧版的关键差别：**整形已经在 core 做完了**。这里拿到的是绝对坐标的字形，
    /// 只需把 twips 换成像素、查图集取 UV。整形属于 core（产物与分辨率无关），
    /// 栅格化属于本 crate（必须知道目标分辨率）——这条线就划在这里。
    ///
    /// `glyphs` 为 `None` 时不产生顶点：没有图集就画不了字，与其画错不如不画。
    pub fn push_glyphs(
        &mut self,
        positioned: &[PositionedGlyph],
        color: Color,
        _font: &FontSpec,
        glyphs: Option<&dyn GlyphSource>,
    ) {
        let Some(src) = glyphs else { return };
        if positioned.is_empty() {
            return;
        }
        self.ensure(BatchKind::Glyph, None);
        for pg in positioned {
            let key = GlyphKey::new(pg.face.clone(), pg.glyph_id, pg.size_half_points);
            let Some(g) = src.glyph(&key) else { continue };
            if g.width <= 0.0 || g.height <= 0.0 {
                continue;
            }
            let x = crate::to_px(pg.x, self.dpi);
            let y = crate::to_px(pg.y, self.dpi);
            self.quad(
                (x + g.left, y - g.top, g.width, g.height),
                (g.u0, g.v0, g.u1, g.v1),
                color,
                1.0,
            );
        }
    }

    pub fn push_image(&mut self, id: &str, rect: Rect) {
        self.ensure(BatchKind::Image, Some(id));
        let [x, y, w, h] = rect_px(rect, self.dpi);
        self.quad((x, y, w, h), (0.0, 0.0, 1.0, 1.0), Color::rgb(255, 255, 255), 1.0);
    }

    pub fn finish(mut self) -> Frame {
        self.flush();
        self.frame
    }
}
