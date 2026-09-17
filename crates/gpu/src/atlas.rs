//! 字形图集（feature `gpu`）。
//!
//! 几千甚至几万个 CJK 字形不可能全部常驻：按 32px 栅格化，两万字约 24MB 纹素，
//! 超出很多设备 4096×4096 的纹理上限，再乘上字号与粗斜体的组合更不可行。
//!
//! 所以图集是**固定大小、按需填充、满了淘汰**的：
//!
//! ```text
//! 要画 ch ──► 查缓存 ──有──► 返回 UV
//!               │无
//!               ▼
//!          栅格化一个字形 ──► 找空位 ──放得下──► 写入、返回 UV
//!                              │放不下
//!                              ▼
//!                        淘汰最久未用的整行 ──► 重试
//! ```
//!
//! 任一时刻驻留的只是最近用到的那些字形。一页文档通常几百个不同字符，
//! 2048² 的图集能放约四千个 32px 字形，足够。
//!
//! # 分层
//!
//! 本模块**只管缓存与摆放**，不做栅格化——那需要字体后端（FreeType / fontdue /
//! 浏览器 Canvas2D），与 [`crate::measure::FontMetrics`] 同理由，由调用方以
//! [`Rasterizer`] 注入。三个 GPU 后端共用本模块，各自只负责把脏区域传成纹理。

use std::collections::HashMap;

use rsword_layout_core::font::{GlyphKey, RasterGlyph, Rasterizer};

use crate::GlyphQuad;

/// 图集里一个已放置的字形。
#[derive(Debug, Clone, Copy)]
struct Slot {
    /// 所属货架的行顶 y：淘汰整行时据此判断本字形是否失效。
    y: u32,
    /// 字形高度：整行上移后重算 v 需要它。
    /// x 与 width 不必存——淘汰只在纵向重排，横向位置不变，UV 里已有。
    h: u32,
    quad: GlyphQuad,
    /// 最近一次使用的逻辑时钟，用于淘汰。
    used: u64,
}

/// 货架式摆放：图集按行（shelf）切分，每行高度由该行第一个字形决定。
///
/// 选它而不是更紧凑的装箱算法，是因为字形高度本就相近（同字号的 CJK 几乎等高），
/// 货架的浪费很小，而实现简单、淘汰时能整行回收。
#[derive(Debug, Clone, Copy)]
struct Shelf {
    /// 行顶 y。
    y: u32,
    /// 行高。
    height: u32,
    /// 已用到的 x。
    cursor: u32,
    /// 本行最近一次被使用的时钟。
    used: u64,
}

/// 纹理中被修改的矩形区域，后端据此做增量上传。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// 字形图集。
///
/// 像素格式是**单通道覆盖率**（R8）。着色器把它当 alpha 用，颜色来自顶点。
pub struct GlyphAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    slots: HashMap<GlyphKey, Slot>,
    shelves: Vec<Shelf>,
    clock: u64,
    dirty: Option<DirtyRect>,
    /// 淘汰发生过：后端需要整幅重传，因为旧 UV 已失效。
    reset: bool,
}

/// 左上角第一个纹素保留为不透明白，供纯色批次复用同一套着色器采样。
const SOLID_TEXEL: u32 = 1;
/// 字形之间留一像素，避免线性采样时相邻字形渗色。
const PADDING: u32 = 1;

impl GlyphAtlas {
    /// 建一个 `width × height` 的图集。建议 1024 或 2048。
    pub fn new(width: u32, height: u32) -> GlyphAtlas {
        let w = width.max(16);
        let h = height.max(16);
        let mut a = GlyphAtlas {
            width: w,
            height: h,
            pixels: vec![0; (w as usize) * (h as usize)],
            slots: HashMap::new(),
            shelves: Vec::new(),
            clock: 0,
            dirty: None,
            reset: true,
        };
        // 纯色纹素：`vertex::SOLID_UV` 约定取 (0,0)。
        a.pixels[0] = 255;
        a
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// 图集像素（单通道），供后端上传纹理。
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// 自上次 [`Self::clear_dirty`] 以来被修改的区域。
    ///
    /// `reset` 为真时表示发生过淘汰、旧 UV 已失效，后端必须整幅重传。
    pub fn dirty(&self) -> Option<DirtyRect> {
        if self.reset {
            Some(DirtyRect { x: 0, y: 0, width: self.width, height: self.height })
        } else {
            self.dirty
        }
    }

    /// 后端上传完纹理后调用。
    pub fn clear_dirty(&mut self) {
        self.dirty = None;
        self.reset = false;
    }

    /// 取一个字形，必要时栅格化并放入图集。
    ///
    /// 返回 `None` 表示栅格化失败或字形大到放不进图集。
    pub fn get(&mut self, key: &GlyphKey, r: &mut dyn Rasterizer) -> Option<GlyphQuad> {
        self.clock += 1;

        if let Some(slot) = self.slots.get_mut(key) {
            slot.used = self.clock;
            let (y, h) = (slot.y, slot.h);
            let quad = slot.quad;
            self.touch_shelf(y, h);
            return Some(quad);
        }

        let g = r.rasterize(key)?;
        self.insert(key.clone(), &g)
    }

    /// 标记某行最近被用过，避免它被优先淘汰。
    fn touch_shelf(&mut self, y: u32, _h: u32) {
        let clock = self.clock;
        if let Some(s) = self.shelves.iter_mut().find(|s| s.y == y) {
            s.used = clock;
        }
    }

    fn insert(&mut self, key: GlyphKey, g: &RasterGlyph) -> Option<GlyphQuad> {
        // 空白字形（空格等）不占图集，但要缓存以免反复栅格化。
        if g.metrics.width == 0 || g.metrics.height == 0 {
            let quad = GlyphQuad {
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                left: g.metrics.left,
                top: g.metrics.top,
                width: 0.0,
                height: 0.0,
            };
            self.slots.insert(
                key,
                Slot { y: 0, h: 0, quad, used: self.clock },
            );
            return Some(quad);
        }

        let need_w = g.metrics.width + PADDING;
        let need_h = g.metrics.height + PADDING;
        if need_w > self.width || need_h > self.height {
            // 字形比整个图集还大：放弃，调用方会跳过它而不是画错。
            return None;
        }

        let (x, y) = match self.find_space(need_w, need_h) {
            Some(p) => p,
            None => {
                self.evict(need_h);
                self.find_space(need_w, need_h)?
            }
        };

        self.blit(x, y, g);

        let quad = GlyphQuad {
            u0: x as f32 / self.width as f32,
            v0: y as f32 / self.height as f32,
            u1: (x + g.metrics.width) as f32 / self.width as f32,
            v1: (y + g.metrics.height) as f32 / self.height as f32,
            left: g.metrics.left,
            top: g.metrics.top,
            width: g.metrics.width as f32,
            height: g.metrics.height as f32,
        };
        self.slots.insert(
            key,
            Slot { y, h: g.metrics.height, quad, used: self.clock },
        );
        Some(quad)
    }

    /// 找一块空位：先试已有货架，再开新货架。
    fn find_space(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let clock = self.clock;
        // 已有货架：高度够且剩余宽度够。挑高度最接近的，减少浪费。
        let mut best: Option<usize> = None;
        for (i, s) in self.shelves.iter().enumerate() {
            if s.height >= h && s.cursor + w <= self.width {
                let better = match best {
                    Some(b) => s.height < self.shelves[b].height,
                    None => true,
                };
                if better {
                    best = Some(i);
                }
            }
        }
        if let Some(i) = best {
            let s = &mut self.shelves[i];
            let x = s.cursor;
            s.cursor += w;
            s.used = clock;
            return Some((x, s.y));
        }

        // 开新货架。第一行要避开左上角的纯色纹素。
        let top = self.shelves.iter().map(|s| s.y + s.height).max().unwrap_or(0);
        if top + h <= self.height {
            let x = if top == 0 { SOLID_TEXEL + PADDING } else { 0 };
            if x + w <= self.width {
                self.shelves.push(Shelf { y: top, height: h, cursor: x + w, used: clock });
                return Some((x, top));
            }
        }
        None
    }

    /// 淘汰：丢掉最久未用的货架，直到腾出至少 `need_h` 的高度。
    ///
    /// 整行回收而不是逐个字形回收——货架内部无法产生可复用的碎片，
    /// 逐个回收只会留下用不上的洞。
    fn evict(&mut self, need_h: u32) {
        if self.shelves.is_empty() {
            return;
        }
        self.shelves.sort_by_key(|s| s.used);

        let mut freed = 0u32;
        let mut drop_ys: Vec<u32> = Vec::new();
        for s in &self.shelves {
            drop_ys.push(s.y);
            freed += s.height;
            if freed >= need_h {
                break;
            }
        }

        // 被丢掉的货架上的字形全部失效。
        self.slots.retain(|_, slot| !drop_ys.contains(&slot.y));
        self.shelves.retain(|s| !drop_ys.contains(&s.y));

        // 剩下的货架重新自上而下紧凑排布：UV 变了，所以整幅重传。
        let mut y = 0u32;
        let old = std::mem::take(&mut self.shelves);
        let mut pixels = vec![0u8; self.pixels.len()];
        pixels[0] = 255;

        let mut moved: Vec<(u32, u32)> = Vec::new(); // (旧 y, 新 y)
        for s in old {
            moved.push((s.y, y));
            for row in 0..s.height {
                let src = ((s.y + row) as usize) * (self.width as usize);
                let dst = ((y + row) as usize) * (self.width as usize);
                let n = self.width as usize;
                if src + n <= self.pixels.len() && dst + n <= pixels.len() {
                    pixels[dst..dst + n].copy_from_slice(&self.pixels[src..src + n]);
                }
            }
            self.shelves.push(Shelf { y, ..s });
            y += s.height;
        }
        self.pixels = pixels;

        // 修正存活字形的 y 与 UV。
        for slot in self.slots.values_mut() {
            if let Some(&(_, ny)) = moved.iter().find(|&&(oy, _)| oy == slot.y) {
                slot.y = ny;
                slot.quad.v0 = ny as f32 / self.height as f32;
                slot.quad.v1 = (ny + slot.h) as f32 / self.height as f32;
            }
        }
        self.reset = true;
    }

    /// 把覆盖率位图写进图集，并累计脏区域。
    fn blit(&mut self, x: u32, y: u32, g: &RasterGlyph) {
        for row in 0..g.metrics.height {
            let src = (row as usize) * (g.metrics.width as usize);
            let dst = ((y + row) as usize) * (self.width as usize) + (x as usize);
            let n = g.metrics.width as usize;
            if src + n <= g.coverage.len() && dst + n <= self.pixels.len() {
                self.pixels[dst..dst + n].copy_from_slice(&g.coverage[src..src + n]);
            }
        }
        self.mark_dirty(x, y, g.metrics.width, g.metrics.height);
    }

    fn mark_dirty(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.dirty = Some(match self.dirty {
            None => DirtyRect { x, y, width: w, height: h },
            Some(d) => {
                let x0 = d.x.min(x);
                let y0 = d.y.min(y);
                let x1 = (d.x + d.width).max(x + w);
                let y1 = (d.y + d.height).max(y + h);
                DirtyRect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
            }
        });
    }

    /// 当前缓存的字形数，供调试与测试。
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// 把图集接到 [`crate::GlyphSource`]：查询时按需填充。
///
/// 只负责「查字形位置」。整形不在这里——它的产物与分辨率无关，归 core
/// 的 `paint::TextShaper`；本 crate 只做与像素有关的事。
///
/// `GlyphSource::glyph` 是 `&self` 而按需填充要改图集，故用内部可变性。
/// 单线程使用（wasm 与大多数渲染循环都是），用 `RefCell` 而非锁。
pub struct AtlasSource<'a, R: Rasterizer> {
    atlas: std::cell::RefCell<&'a mut GlyphAtlas>,
    raster: std::cell::RefCell<&'a mut R>,
}

impl<'a, R: Rasterizer> AtlasSource<'a, R> {
    pub fn new(atlas: &'a mut GlyphAtlas, raster: &'a mut R) -> AtlasSource<'a, R> {
        AtlasSource {
            atlas: std::cell::RefCell::new(atlas),
            raster: std::cell::RefCell::new(raster),
        }
    }
}

impl<R: Rasterizer> crate::GlyphSource for AtlasSource<'_, R> {
    fn glyph(&self, key: &GlyphKey) -> Option<crate::GlyphQuad> {
        let mut atlas = self.atlas.borrow_mut();
        let mut raster = self.raster.borrow_mut();
        atlas.get(key, &mut **raster)
    }
}
