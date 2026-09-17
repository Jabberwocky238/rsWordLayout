//! 用 skrifa + zeno 实现的字形栅格化（feature `raster`）。
//!
//! 分工是明确的：**skrifa 只给轮廓，不做光栅化**（它自己的依赖只有 `bytemuck`
//! 与 `core_maths`）。它的 `OutlineGlyph::draw` 往一个 `OutlinePen` 里吐
//! `move_to` / `line_to` / `quad_to` / `curve_to`；zeno 负责把这些路径填成
//! 覆盖率位图。两边的命令一一对应，中间的 [`PenBridge`] 只是转接。
//!
//! 字体数据由调用方提供。全 Unicode 的字体选择与 fallback 交给
//! `docx_layout::fontenv`——那里有按码位的覆盖查询与环境指纹，
//! 本模块只负责「给定 face 与 glyph id，画出位图」。

use std::collections::HashMap;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};
use zeno::{Format, Mask, PathBuilder};

use crate::atlas::{GlyphKey, GlyphMetrics, RasterGlyph, Rasterizer};

/// 把 skrifa 的画笔命令转成 zeno 的路径。
///
/// 两边的 y 轴方向相反：字体坐标 y 向上，位图 y 向下，所以这里统一取反。
struct PenBridge {
    path: Vec<zeno::Command>,
}

impl OutlinePen for PenBridge {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to([x, -y]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to([x, -y]);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path.quad_to([cx0, -cy0], [x, -y]);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to([cx0, -cy0], [cx1, -cy1], [x, -y]);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

/// 一份字体数据。
pub struct FaceData {
    pub bytes: Vec<u8>,
    /// TTC 里的子字体序号，普通 TTF/OTF 为 0。
    pub index: u32,
}

/// skrifa + zeno 栅格化器。
///
/// 按 `GlyphKey::face` 查字体数据，所以调用方要先用同一套 face 标识注册字体——
/// 建议直接用 `docx_layout::fontenv` 的 `FaceId::sha256()`，两边就对得上。
#[derive(Default)]
pub struct SkrifaRasterizer {
    faces: HashMap<String, FaceData>,
}

impl SkrifaRasterizer {
    pub fn new() -> SkrifaRasterizer {
        SkrifaRasterizer::default()
    }

    /// 注册一份字体。`face` 是本栅格化器内的标识，须与 [`GlyphKey::face`] 一致。
    pub fn add_face(&mut self, face: impl Into<String>, bytes: Vec<u8>, index: u32) {
        self.faces.insert(face.into(), FaceData { bytes, index });
    }

    pub fn has_face(&self, face: &str) -> bool {
        self.faces.contains_key(face)
    }

    /// 查一个字符在某 face 里的 glyph id。
    ///
    /// 这是**仅供简单场景的便利方法**：真正的文字要走 shaping（连字、阿拉伯语形态、
    /// 印度语重排都不是一字符一字形），此处只做 cmap 查表。
    pub fn glyph_id(&self, face: &str, ch: char) -> Option<u32> {
        let data = self.faces.get(face)?;
        let font = FontRef::from_index(&data.bytes, data.index).ok()?;
        let gid = font.charmap().map(ch)?;
        (gid.to_u32() != 0).then(|| gid.to_u32())
    }
}

impl Rasterizer for SkrifaRasterizer {
    fn rasterize(&mut self, key: &GlyphKey) -> Option<RasterGlyph> {
        let data = self.faces.get(&key.face)?;
        let font = FontRef::from_index(&data.bytes, data.index).ok()?;

        // 半点 → 像素。这里按 1pt = 1px 处理；DPI 缩放由调用方在 key 的字号上体现，
        // 这样同一字号不同 DPI 会各自缓存，不会互相污染。
        let px = key.size_half_points as f32 / 2.0;
        let size = Size::new(px);

        let outlines = font.outline_glyphs();
        let glyph = outlines.get(skrifa::GlyphId::new(key.glyph_id))?;

        let mut pen = PenBridge { path: Vec::new() };
        glyph
            .draw(DrawSettings::unhinted(size, LocationRef::default()), &mut pen)
            .ok()?;

        // 空白字形（空格等）：没有轮廓，但仍要返回以便缓存，避免反复栅格化。
        if pen.path.is_empty() {
            let advance = font
                .glyph_metrics(size, LocationRef::default())
                .advance_width(skrifa::GlyphId::new(key.glyph_id))
                .unwrap_or(0.0);
            return Some(RasterGlyph {
                metrics: GlyphMetrics { width: 0, height: 0, left: 0.0, top: 0.0, advance },
                coverage: Vec::new(),
            });
        }

        // zeno 填充：Alpha 格式即单通道覆盖率，正是图集要的。
        let (coverage, placement) = Mask::new(&pen.path[..]).format(Format::Alpha).render();

        let advance = font
            .glyph_metrics(size, LocationRef::default())
            .advance_width(skrifa::GlyphId::new(key.glyph_id))
            .unwrap_or(px);

        Some(RasterGlyph {
            metrics: GlyphMetrics {
                width: placement.width,
                height: placement.height,
                left: placement.left as f32,
                // placement.top 是位图顶边相对基线的位置（y 向下为正），
                // 而 GlyphMetrics::top 约定为基线到顶边的距离、向上为正，故取反。
                top: -placement.top as f32,
                advance,
            },
            coverage,
        })
    }
}
