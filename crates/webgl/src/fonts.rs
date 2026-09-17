//! 浏览器侧的字体注册与选择（wasm32）。
//!
//! 把三个东西绑在同一套 face 标识上，缺一不可：
//!
//! - `docx_layout::fontenv` —— 按码位查覆盖、选 face，缺字报 `FONT_MISSING`；
//! - `RustybuzzShaper`      —— 整形，产出 glyph id；
//! - `SkrifaRasterizer`     —— 按 glyph id 栅格化。
//!
//! 标识统一用 `fontenv` 的 `FaceId::sha256()`：按内容哈希，不靠文件名，
//! 同一份字体在三处必然对得上。
//!
//! 字体不编进 wasm，由 JS 运行时 fetch 后交进来（见 `web/public/fonts/README.md`）。

use docx_layout::fontenv::{FontEnvironment, FontEnvironmentBuilder};
use rsword_layout_core::measure::FontSpec;
use rsword_layout_core::paint::{ShapedRun, TextShaper};
use rsword_layout_core::shape::RustybuzzShaper;
use rsword_layout_gpu::atlas::GlyphKey;
use rsword_layout_gpu::raster::SkrifaRasterizer;
use rsword_layout_gpu::{GlyphQuad, GlyphSource};

/// 已注册的字体集合。
pub struct FontRegistry {
    builder: FontEnvironmentBuilder,
    env: Option<FontEnvironment>,
    shaper: RustybuzzShaper,
    raster: SkrifaRasterizer,
    /// face 标识（内容哈希）→ shaper 内的下标。
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
            index_of: std::collections::HashMap::new(),
        }
    }

    /// 注册一份字体。返回它的 face 标识（内容哈希）。
    ///
    /// 同一份字节交三处：fontenv 用来查覆盖，shaper 用来整形，rasterizer 用来栅格化。
    pub fn add(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let id = self.builder.add(bytes.clone(), index)?;
        let face = id.sha256().to_string();
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
    pub fn select_face(&self, family: &str, ch: char, bold: bool, italic: bool) -> Option<String> {
        let env = self.env.as_ref()?;
        let weight = if bold { 700 } else { 400 };
        let families = vec![family.to_string()];
        let sel = env.select(&families, weight, italic, ch);
        sel.face.map(|f| f.id().sha256().to_string())
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

        for ch in text.chars() {
            let face = self.select_face(&font.family, ch, font.bold, font.italic);
            if face != run_face && !run.is_empty() {
                if let Some(i) = run_face.as_ref().and_then(|f| self.index_of.get(f)) {
                    out.extend(self.shaper.shape_with_face(*i, &run, font.size_half_points));
                }
                run.clear();
            }
            run_face = face;
            run.push(ch);
        }
        if let Some(i) = run_face.as_ref().and_then(|f| self.index_of.get(f))
            && !run.is_empty()
        {
            out.extend(self.shaper.shape_with_face(*i, &run, font.size_half_points));
        }
        out
    }
}

/// 把注册表接到 `GlyphSource`：整形走 fontenv 分段，取图集走按需填充。
pub struct RegistrySource<'a> {
    pub registry: std::cell::RefCell<&'a mut FontRegistry>,
    pub atlas: std::cell::RefCell<&'a mut rsword_layout_gpu::GlyphAtlas>,
}

impl GlyphSource for RegistrySource<'_> {
    fn glyph(&self, key: &GlyphKey) -> Option<GlyphQuad> {
        let mut reg = self.registry.borrow_mut();
        let mut atlas = self.atlas.borrow_mut();
        atlas.get(key, reg.rasterizer_mut())
    }
}

/// `Shaper` 的直通实现，便于单独使用。
impl TextShaper for FontRegistry {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        self.shape_text(text, font)
    }
}
