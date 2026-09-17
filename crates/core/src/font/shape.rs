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

use crate::layout::{Twips, points_to_twips};
use super::FontSpec;
use crate::layout::{ShapedRun, TextShaper};

/// rustybuzz 整形器。
///
/// 字体按 `face` 标识注册，须与 [`GlyphKey::face`] 以及栅格化器用的是同一套标识——
/// 建议统一用 `docx_layout::fontenv` 的 `FaceId::sha256()`。
#[derive(Default)]
pub struct RustybuzzShaper {
    /// (face 标识, 字节, TTC 序号)。用有序表而非哈希表：
    /// `ShapedRun::face_index` 是下标，paint 层据此查 `FaceId`。
    faces: Vec<(String, Vec<u8>, u32)>,
    by_name: HashMap<String, usize>,
    /// 缺字体时用哪个 face 兜底。
    default_face: Option<usize>,
}

impl RustybuzzShaper {
    pub fn new() -> RustybuzzShaper {
        RustybuzzShaper::default()
    }

    /// 注册一份字体，返回它的下标（即 [`ShapedRun::face_index`]）。
    pub fn add_face(&mut self, face: impl Into<String>, bytes: Vec<u8>, index: u32) -> usize {
        let name = face.into();
        let at = self.faces.len();
        self.by_name.insert(name.clone(), at);
        self.faces.push((name, bytes, index));
        if self.default_face.is_none() {
            self.default_face = Some(at);
        }
        at
    }

    /// 指定按 `FontSpec::family` 找不到时用哪个 face。
    pub fn set_default_face(&mut self, index: usize) {
        self.default_face = Some(index);
    }

    /// 按下标取 face 标识，供 paint 层构造 `FaceId` 列表。
    pub fn face_id(&self, index: usize) -> Option<&str> {
        self.faces.get(index).map(|(n, _, _)| n.as_str())
    }

    /// 全部 face 标识，顺序与下标一致。
    pub fn face_ids(&self) -> Vec<String> {
        self.faces.iter().map(|(n, _, _)| n.clone()).collect()
    }

    /// 对指定 face 整形一段文字。
    ///
    /// 返回空表示该 face 未注册或字体无法解析——调用方应据此换字体重试，
    /// 而不是把空结果当作「这段文字不占宽度」。
    /// 对指定 face 整形一段文字。
    ///
    /// **产出单位是 twips，不是像素**——整形结果由字体的 GSUB/GPOS 表决定，
    /// 与分辨率无关，所以它属于 core。栅格化才需要知道目标 DPI。
    ///
    /// 返回空表示该 face 未注册或字体无法解析；调用方应据此换字体重试，
    /// 而不是把空结果当作「这段文字不占宽度」。
    pub fn shape_with_face(
        &self,
        face_index: usize,
        text: &str,
        size_half_points: u32,
    ) -> Vec<ShapedRun> {
        let Some((_, bytes, index)) = self.faces.get(face_index) else {
            return Vec::new();
        };
        let Some(face) = Face::from_slice(bytes, *index) else {
            return Vec::new();
        };

        let mut buf = UnicodeBuffer::new();
        buf.push_str(text);
        // 方向与脚本由 rustybuzz 按内容推断：阿拉伯语自动走 RTL，
        // 混排时按 Unicode 的脚本属性分段。
        buf.guess_segment_properties();

        let out = rustybuzz::shape(&face, &[], buf);

        // rustybuzz 的位置量以字体设计单位计；先换到点，再换到 twips。
        let upem = face.units_per_em() as f32;
        let pt_size = size_half_points as f32 / 2.0;
        let to_twips = |v: i32| -> Twips {
            if upem <= 0.0 {
                return 0;
            }
            points_to_twips(f64::from(v as f32 * pt_size / upem))
        };

        let infos = out.glyph_infos();
        let positions = out.glyph_positions();
        infos
            .iter()
            .zip(positions.iter())
            .map(|(info, pos)| ShapedRun {
                face_index,
                glyph_id: info.glyph_id,
                x_advance: to_twips(pos.x_advance),
                x_offset: to_twips(pos.x_offset),
                y_offset: to_twips(pos.y_offset),
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

impl TextShaper for RustybuzzShaper {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        // FontSpec::family 是 OOXML 里的字体名，未必等于注册时用的 face 标识；
        // 先直接试，再退到默认 face。真正的按族选字体应走 docx-layout 的 fontenv。
        let face = self.by_name.get(&font.family).copied().or(self.default_face);
        match face {
            Some(i) => self.shape_with_face(i, text, font.size_half_points),
            None => Vec::new(),
        }
    }
}
