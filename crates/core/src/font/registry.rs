//! 浏览器侧的字体注册与选择（wasm32）。
//!
//! 把三个东西绑在同一套 face 标识上，缺一不可：
//!
//! - `docx_layout::fontenv` —— 按码位查覆盖、选 face，缺字报 `FONT_MISSING`；
//! - `RustybuzzShaper`      —— 整形，产出 glyph id；
//! - `SkrifaRasterizer`     —— 按 glyph id 栅格化。
//!
//! 标识统一用 `fontenv` 的内容哈希与 TTC 序号，不靠文件名，
//! 同一份字体在三处必然对得上。
//!
//! 字体不编进 wasm，由 JS 运行时 fetch 后交进来（见 `web/public/fonts/README.md`）。

use std::collections::{BTreeMap, BTreeSet};

use super::{FontSpec, RustybuzzShaper, SkrifaRasterizer};
use crate::layout::ShapedRun;
use crate::layout::TextShaper;
use docx_layout::fontenv::{FaceId, FontEnvironment, FontEnvironmentBuilder, normalize_family};
use skrifa::{FontRef, MetadataProvider, string::StringId};

/// 已注册的字体集合。
pub struct FontRegistry {
    builder: FontEnvironmentBuilder,
    env: Option<FontEnvironment>,
    shaper: RustybuzzShaper,
    raster: SkrifaRasterizer,
    /// 字体表中实际存在的额外名称 → face；不从后缀猜测字体族。
    names: BTreeMap<String, BTreeSet<FaceId>>,
    /// face 标识（内容哈希与 TTC 序号）→ shaper 内的下标。
    /// `ShapedRun::face_index` 是下标，paint 层要靠它查回 `FaceId`。
    index_of: std::collections::HashMap<String, usize>,
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FontRegistry {
    pub fn new() -> FontRegistry {
        FontRegistry {
            builder: FontEnvironmentBuilder::new(),
            env: None,
            shaper: RustybuzzShaper::new(),
            raster: SkrifaRasterizer::new(),
            names: BTreeMap::new(),
            index_of: std::collections::HashMap::new(),
        }
    }

    /// 注册一份字体。返回它的 face 标识（内容哈希；非零 TTC 序号加 `:index`）。
    ///
    /// 同一份字节交三处：fontenv 用来查覆盖，shaper 用来整形，rasterizer 用来栅格化。
    pub fn add(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let id = self.builder.add(bytes.clone(), index)?;
        let face = face_key(&id);
        if self.index_of.contains_key(&face) {
            return Ok(face);
        }
        let font = FontRef::from_index(&bytes, index).map_err(|_| "FONT_INVALID")?;
        for name in [
            StringId::FULL_NAME,
            StringId::POSTSCRIPT_NAME,
            StringId::COMPATIBLE_FULL_NAME,
            StringId::WWS_FAMILY_NAME,
        ]
        .into_iter()
        .flat_map(|id| font.localized_strings(id))
        .map(|s| normalize_family(&s.to_string()))
        .filter(|s| !s.is_empty())
        {
            self.names.entry(name).or_default().insert(id.clone());
        }
        let at = self.shaper.add_face(face.clone(), bytes.clone(), index);
        self.index_of.insert(face.clone(), at);
        self.raster.add_face(face.clone(), bytes, index);
        // 每次新增都要重新冻结：env 是不可变快照。
        self.env = Some(self.builder.freeze());
        Ok(face)
    }

    pub fn is_empty(&self) -> bool {
        self.env.is_none()
    }

    /// 字体环境指纹：字体集变了它就变，可用来判断布局是否需要重算。
    pub fn fingerprint(&self) -> Option<&str> {
        self.env.as_ref().map(|e| e.fingerprint())
    }

    /// 按请求的字体族与码位选一个 face。
    ///
    /// 返回 `None` 表示所有已注册字体都覆盖不了这个码位——调用方应当跳过该字形，
    /// 而不是拿一个画不出它的 face 去栅格化（那会得到 .notdef 豆腐块）。
    /// 按 **Word 的槽规则** 为一个字符选 face。
    ///
    /// 两步，顺序不能反：
    ///   1. 按字符所属区查 `w:rFonts` 的对应槽（ascii / hAnsi / eastAsia / cs）——
    ///      这是 Word 的真实规则，不是 fallback；
    ///   2. 槽里的字体装不了或画不出该字符时，才交给 fontenv 做 fallback。
    ///
    /// 只做第 2 步会选错字体：中文该走 eastAsia 槽指定的宋体，
    /// 而 fallback 会挑第一个能覆盖它的字体，两者常常不同。
    pub fn select_face_for(&self, font: &FontSpec, ch: char) -> Option<String> {
        self.select_face(font.family_for(ch), ch, font.bold, font.italic)
    }

    pub fn select_face(&self, family: &str, ch: char, bold: bool, italic: bool) -> Option<String> {
        let env = self.env.as_ref()?;
        let weight = if bold { 700 } else { 400 };
        // 族名优先，避免常规 face 的 full name 恰好等于族名时盖住粗体/斜体。
        if let Some(face) = env
            .candidates(family, weight, italic)
            .into_iter()
            .find(|f| env.covers(f.id(), ch))
        {
            return Some(face_key(face.id()));
        }
        if let Some(ids) = self.names.get(&normalize_family(family)) {
            let mut candidates: Vec<_> = env.faces().filter(|f| ids.contains(f.id())).collect();
            candidates.sort_by_key(|f| (f.italic() != italic, f.weight().abs_diff(weight), f.id()));
            if let Some(face) = candidates.into_iter().find(|f| env.covers(f.id(), ch)) {
                return Some(face_key(face.id()));
            }
        }
        let families = vec![family.to_string()];
        let sel = env.select(&families, weight, italic, ch);
        sel.face.map(|f| face_key(f.id()))
    }

    /// 实际装进来的族名，排序去重。
    ///
    /// 用途只有一个，但很要紧：**核字体有没有被替换**。量具方法 §6.2 实测，
    /// 度量兼容克隆（Liberation Serif ↔ Times New Roman 等）替换后几何一字不差，
    /// 2618 条记录 max |Δ| = 0.000000pt——**任何几何自检都发现不了，只有字体名能**。
    ///
    /// 注意不能拿 [`FontRegistry::select_face`] 代替：它带 fallback，
    /// 族根本没装也会返回一个能盖住该码位的 face，于是核查永远通过。
    pub fn families(&self) -> Vec<String> {
        let Some(env) = self.env.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<String> = env
            .faces()
            .flat_map(|f| f.families().iter().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// 某个族或完整 face 名是否真的装进来了（按 fontenv 的名称归一化比较）。
    /// 只核对字体表中的名称；码位 fallback 不能让缺失字体通过核查。
    pub fn covers_family(&self, family: &str) -> bool {
        let want = normalize_family(family);
        self.names.contains_key(&want)
            || self
                .env
                .as_ref()
                .is_some_and(|env| env.faces().any(|f| f.families().contains(&want)))
    }

    /// 按 face 标识取字体字节与 TTC 序号。
    ///
    /// 纵向量（`hhea` / `OS/2`）要直接解析字体表，而整形器只给推进量，
    /// 所以这里把字节露出来。返回 `None` 表示该 face 没注册。
    pub fn face_data(&self, face: &str) -> Option<(&[u8], u32)> {
        let env = self.env.as_ref()?;
        env.faces()
            .find(|f| face_key(f.id()) == face)
            .and_then(|f| env.data(f.id()).map(|b| (b, f.id().index())))
    }

    /// 全部 face 标识，顺序与 `ShapedRun::face_index` 一致。
    pub fn face_ids(&self) -> Vec<String> {
        self.shaper.face_ids()
    }

    pub fn rasterizer_mut(&mut self) -> &mut SkrifaRasterizer {
        &mut self.raster
    }

    /// 整形一段文字。
    ///
    /// 按码位切段：同一段文字里中英文混排会选到不同 face，必须分段整形，
    /// 否则用一个 face 去 shape 它覆盖不了的字符只会得到 .notdef。
    pub fn shape_text(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        if self.env.is_none() || text.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut run = String::new();
        let mut run_face: Option<String> = None;
        let mut source_offset = 0;
        let mut run_start = 0;
        let append = |out: &mut Vec<ShapedRun>, face: &Option<String>, run: &str, start: u32| {
            if let Some(i) = face.as_ref().and_then(|f| self.index_of.get(f)) {
                for mut shaped in self.shaper.shape_with_face_centipoints(
                    *i,
                    run,
                    font.effective_size_centipoints(),
                    font.kerning,
                ) {
                    if let Some((source_start, source_end)) = shaped.source.as_mut() {
                        *source_start += start;
                        *source_end += start;
                    }
                    out.push(shaped);
                }
            }
        };

        for ch in text.chars() {
            // 按槽选，而不是整段用同一个 family。
            let face = self.select_face_for(font, ch);
            if face != run_face {
                if !run.is_empty() {
                    append(&mut out, &run_face, &run, run_start);
                }
                run.clear();
                run_start = source_offset;
            }
            run_face = face;
            run.push(ch);
            source_offset += ch.len_utf16() as u32;
        }
        if !run.is_empty() {
            append(&mut out, &run_face, &run, run_start);
        }
        out
    }
}

fn face_key(id: &FaceId) -> String {
    if id.index() == 0 {
        id.sha256().to_string()
    } else {
        format!("{}:{}", id.sha256(), id.index())
    }
}

/// `Shaper` 的直通实现，便于单独使用。
impl TextShaper for FontRegistry {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        self.shape_text(text, font)
    }
}
