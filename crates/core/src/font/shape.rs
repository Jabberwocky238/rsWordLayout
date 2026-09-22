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

use super::FontSpec;
use crate::layout::{ShapedRun, TextShaper};
use crate::layout::{TWIPS_PER_POINT, Twips};

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
        kerning: bool,
    ) -> Vec<ShapedRun> {
        let Some((_, bytes, index)) = self.faces.get(face_index) else {
            return Vec::new();
        };
        let Some(face) = Face::from_slice(bytes, *index) else {
            return Vec::new();
        };

        let mut buf = UnicodeBuffer::new();
        let mut source_end = 0;
        for ch in text.chars() {
            // 直接用 Word Range 的 UTF-16 单位标 cluster，避免把 UTF-8 字节
            // 或输出字形序号当作字符归属（连字、代理对、RTL 均不一一对应）。
            buf.add(ch, source_end);
            source_end += ch.len_utf16() as u32;
        }
        // 这里只推断当前段的方向与脚本；不同脚本和双向文本须由调用方分段。
        buf.guess_segment_properties();

        // S-1：未启用 Word 连字选项时，关闭 Latin 的可选连字。
        // AAT 字体可在 morx 默认标志中开启 rare/historical 连字：Zapfino
        // 的 di 就走 rare（dlig），只关 liga/clig 仍会合成。rlig 保留。
        // 只覆盖 Latin；其它脚本的 clig/rlig 可能是必需的文字成形步骤。
        let mut features = Vec::new();
        if buf.script() == rustybuzz::script::LATIN {
            for tag in [b"liga", b"clig", b"dlig", b"hlig"] {
                features.push(rustybuzz::Feature::new(
                    rustybuzz::ttf_parser::Tag::from_bytes(tag),
                    0,
                    ..,
                ));
            }
        }
        // 字距调整默认**关**（OOXML `w:kern` 的语义，见 `FontSpec::kerning`）。
        // rustybuzz 不传 feature 时默认开，所以必须显式关掉，不能靠不传。
        if !kerning {
            features.push(rustybuzz::Feature::new(
                rustybuzz::ttf_parser::Tag::from_bytes(b"kern"),
                0,
                ..,
            ));
        }
        let out = rustybuzz::shape(&face, &features, buf);

        // rustybuzz 的位置量以字体设计单位计；先换到点，再换到 twips。
        //
        // **全程用 f64，且只在落位时取一次整。** 逐字形各取各的整会沿行累加：
        // 实测 Word 给 `e` 5.3280pt、引擎给 5.3500pt（差 1 twip），
        // 到第 20 个字形就攒到 3 twips（0.15pt）。
        //
        // 手法是：把未取整的累计推进量留在 f64 里，**推进量取相邻取整位置之差**。
        // 这样任意前缀和都恰好等于「精确累计值取整」，位置不会漂——
        // 而单个推进量仍是整 twips，`ShapedRun` 的契约不变。
        let upem = f64::from(face.units_per_em());
        if upem <= 0.0 {
            return Vec::new();
        }
        let pt_size = f64::from(size_half_points) / 2.0;
        let exact = |v: i32| -> f64 { f64::from(v) * pt_size / upem * f64::from(TWIPS_PER_POINT) };

        let infos = out.glyph_infos();
        let positions = out.glyph_positions();
        // cluster 起点按源顺序求后继，不能按输出顺序：RTL 输出顺序反向，
        // 且一个 cluster 可产出多个字形；它们应共享同一个源区间。
        let mut cluster_starts: Vec<u32> = infos.iter().map(|info| info.cluster).collect();
        cluster_starts.push(source_end);
        cluster_starts.sort_unstable();
        cluster_starts.dedup();
        let cluster_spans: HashMap<u32, u32> = cluster_starts
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect();
        let mut acc = 0.0f64;
        let mut acc_twips: Twips = 0;
        let mut runs = Vec::with_capacity(infos.len());
        for (info, pos) in infos.iter().zip(positions.iter()) {
            let next = acc + exact(pos.x_advance);
            let next_twips = next.round() as Twips;
            runs.push(ShapedRun {
                face_index,
                glyph_id: info.glyph_id,
                x_advance: next_twips - acc_twips,
                // 精确值不参与上面的「取整位置之差」把戏：它本来就不丢精度。
                x_advance_pt: exact(pos.x_advance) / f64::from(TWIPS_PER_POINT),
                // 偏移是相对本字形的，不参与累计，各自取整即可。
                x_offset: exact(pos.x_offset).round() as Twips,
                y_offset: exact(pos.y_offset).round() as Twips,
                source: cluster_spans
                    .get(&info.cluster)
                    .map(|&end| (info.cluster, end)),
            });
            acc = next;
            acc_twips = next_twips;
        }
        runs
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
        let face = self
            .by_name
            .get(&font.family)
            .copied()
            .or(self.default_face);
        match face {
            Some(i) => self.shape_with_face(i, text, font.size_half_points, font.kerning),
            None => Vec::new(),
        }
    }
}
