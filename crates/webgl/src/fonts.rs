//! 把 core 的字体集接到图集（wasm32）。
//!
//! 字体的加载、选择、整形、栅格化全在 `rsword_layout_core::font`；本模块只补
//! 设备空间那一段：**持有图集、按需填充、实现 `GlyphSource`**。
//!
//! 这条线划在「无状态 vs 有状态」上：栅格化是纯函数（同一 key 同一结果），
//! 故在 core；图集是缓存，故在这里。栅格化器以 `&mut dyn Rasterizer` 交进来：
//! 只能画字形，不能增删字体。

use rsword_layout_core::font::{GlyphKey, Rasterizer};
use rsword_layout_gpu::{GlyphAtlas, GlyphQuad, GlyphSource};

/// 栅格化器 + 图集：查字形时按需栅格化并填入图集。
pub struct RegistrySource<'a> {
    pub rasterizer: std::cell::RefCell<&'a mut dyn Rasterizer>,
    pub atlas: std::cell::RefCell<&'a mut GlyphAtlas>,
}

impl GlyphSource for RegistrySource<'_> {
    fn glyph(&self, key: &GlyphKey) -> Option<GlyphQuad> {
        let mut rasterizer = self.rasterizer.borrow_mut();
        let mut atlas = self.atlas.borrow_mut();
        atlas.get(key, *rasterizer)
    }
}
