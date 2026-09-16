//! 用 rustybuzz 实现的文字整形（feature `shape`）。
//!
//! shaping 是**由字体的 GSUB/GPOS 表决定的确定性查表**，不是布局要解的约束：
//! 字体作者已经把答案编码进表里了。所以本模块不做任何决策，只是转调。
//!
//! 它解决的是「一字符一字形」在全 Unicode 下不成立的问题：
//!
//! - `f` + `i` → `ﬁ`，两个字符一个字形；
//! - 阿拉伯字母的词首 / 词中 / 词尾 / 独立四种形态共用一个码位；
//! - 印度系文字的元音符号显示位置与逻辑顺序不同，需要重排；
//! - 组合符号要靠 GPOS 精确附着到基字上。
//!
//! 选 rustybuzz（HarfBuzz 的纯 Rust 移植）而不是链接 `libs/harfbuzz` 的 C 库，
//! 是因为 wasm 目标要能编译。
//!
//! 字体选择与 fallback 不在这里：一次 shaping 只针对一个 face，跨字体的回退
//! 由调用方按 `docx_layout::fontenv` 的覆盖查询先切好段再逐段 shape。

use std::collections::HashMap;

use rustybuzz::{Face, UnicodeBuffer};

use crate::measure::FontSpec;

use super::ShapedGlyph;
use super::atlas::{GlyphKey, Shaper};

/// rustybuzz 整形器。
///
/// 字体按 `face` 标识注册，须与 [`GlyphKey::face`] 以及栅格化器用的是同一套标识——
/// 建议统一用 `docx_layout::fontenv` 的 `FaceId::sha256()`。
#[derive(Default)]
pub struct RustybuzzShaper {
    faces: HashMap<String, (Vec<u8>, u32)>,
    /// 缺字体时用哪个 face 兜底。
    default_face: Option<String>,
}

impl RustybuzzShaper {
    pub fn new() -> RustybuzzShaper {
        RustybuzzShaper::default()
    }

    pub fn add_face(&mut self, face: impl Into<String>, bytes: Vec<u8>, index: u32) {
        let name = face.into();
        if self.default_face.is_none() {
            self.default_face = Some(name.clone());
        }
        self.faces.insert(name, (bytes, index));
    }

    /// 指定按 `FontSpec::family` 找不到时用哪个 face。
    pub fn set_default_face(&mut self, face: impl Into<String>) {
        self.default_face = Some(face.into());
    }

    /// 对指定 face 整形一段文字。
    ///
    /// 返回空表示该 face 未注册或字体无法解析——调用方应据此换字体重试，
    /// 而不是把空结果当作「这段文字不占宽度」。
    pub fn shape_with_face(
        &self,
        face_id: &str,
        text: &str,
        size_half_points: u32,
    ) -> Vec<ShapedGlyph> {
        let Some((bytes, index)) = self.faces.get(face_id) else {
            return Vec::new();
        };
        let Some(face) = Face::from_slice(bytes, *index) else {
            return Vec::new();
        };

        let mut buf = UnicodeBuffer::new();
        buf.push_str(text);
        // 方向与脚本由 rustybuzz 按内容推断：阿拉伯语自动走 RTL，
        // 混排时它会按 Unicode 的脚本属性分段。
        buf.guess_segment_properties();

        let out = rustybuzz::shape(&face, &[], buf);

        // rustybuzz 的位置量以字体设计单位计，需按 upem 缩放到像素。
        // units_per_em 返回 i32，f32::from 对 i32 无实现（可能丢精度），故用 as。
        let upem = face.units_per_em() as f32;
        let px = size_half_points as f32 / 2.0;
        let scale = if upem > 0.0 { px / upem } else { 0.0 };

        let infos = out.glyph_infos();
        let positions = out.glyph_positions();
        infos
            .iter()
            .zip(positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                key: GlyphKey::new(face_id, info.glyph_id, size_half_points),
                x_advance: pos.x_advance as f32 * scale,
                x_offset: pos.x_offset as f32 * scale,
                y_offset: pos.y_offset as f32 * scale,
            })
            .collect()
    }

    /// 已注册的 face 数。
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}

impl Shaper for RustybuzzShaper {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedGlyph> {
        // FontSpec::family 是 OOXML 里的字体名，未必等于注册时用的 face 标识；
        // 先直接试，再退到默认 face。真正的按族选字体应走 fontenv。
        let face = if self.faces.contains_key(&font.family) {
            Some(font.family.clone())
        } else {
            self.default_face.clone()
        };
        match face {
            Some(f) => self.shape_with_face(&f, text, font.size_half_points),
            None => Vec::new(),
        }
    }
}
