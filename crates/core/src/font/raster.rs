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
//!
//! # 三项影响清晰度的设置
//!
//! DirectWrite / FreeType 比朴素栅格化清晰，靠的是这三样。默认全开：
//!
//! | 设置 | 作用 | 代价 |
//! | --- | --- | --- |
//! | **字形提示**（[`HintingMode`]） | 把轮廓对齐到像素网格，小字号不发虚 | 需按字号建 `HintingInstance` |
//! | **亚像素渲染**（[`RasterFormat::Subpixel`]） | 用 LCD 的 RGB 子像素，水平分辨率变三倍 | 位图三通道；**只在 LCD 有效**，OLED/旋转屏会出彩边 |
//! | **伽马校正** | 在感知空间混合，笔画不显细发灰 | 一次查表 |

/// 一个字形的度量与位图尺寸，不含像素数据。
///
/// 纯 POD，可跨 C ABI 传递——覆盖率数据另行以指针+长度给出，
/// 因为 `Vec` 的布局没有保证，带上它就无法 `repr(C)`。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct GlyphMetrics {
    /// 位图宽高（像素）。可以是 0×0，表示空白字形（如空格），仍应缓存以免反复栅格化。
    pub width: u32,
    pub height: u32,
    /// 相对笔位的偏移：`left` 向右为正，`top` 是基线到位图顶边的距离（向上为正）。
    pub left: f32,
    pub top: f32,
    /// 排版推进量，像素。
    pub advance: f32,
}

/// 一个字形的栅格化结果：度量 + 覆盖率位图。
pub struct RasterGlyph {
    pub metrics: GlyphMetrics,
    /// 覆盖率字节，行优先。每像素字节数由 [`RasterGlyph::format`] 决定：
    /// `Alpha` 为 1，`Subpixel` 为 3。空白字形为空。
    pub coverage: Vec<u8>,
    /// 本位图的像素格式。图集据此决定纹理通道数。
    pub format: RasterFormat,
}

impl RasterGlyph {
    pub fn width(&self) -> u32 {
        self.metrics.width
    }

    pub fn height(&self) -> u32 {
        self.metrics.height
    }
}

/// 字形标识：**shaping 的产物，不是字符**。
///
/// 全 Unicode 下「一字符一字形」不成立：`fi` 连字是两字符一字形，阿拉伯字母的
/// 词首/词中/词尾/独立四种形态共用一个码位，印度系文字还会重排。所以图集必须按
/// 字体内的字形编号索引，由 shaper（HarfBuzz / skrifa）给出。
///
/// `face` 区分不同字体文件——同一 glyph id 在不同字体里是完全不同的形状。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// 字体标识，通常是 face 的内容哈希。
    pub face: String,
    /// 字体内的字形编号。
    pub glyph_id: u32,
    /// 字号，半点。同一字形不同字号要分别栅格化。
    pub size_half_points: u32,
}

impl GlyphKey {
    pub fn new(face: impl Into<String>, glyph_id: u32, size_half_points: u32) -> GlyphKey {
        GlyphKey { face: face.into(), glyph_id, size_half_points }
    }
}

/// 覆盖率位图的像素格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RasterFormat {
    /// 单通道 alpha，每像素 1 字节。
    Alpha,
    /// RGB 亚像素，每像素 3 字节。水平分辨率变三倍（ClearType 那种做法），
    /// **只在 LCD 面板有效**；OLED 与旋转屏上会出彩边。
    #[default]
    Subpixel,
}

impl RasterFormat {
    /// 每像素字节数。
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            RasterFormat::Alpha => 1,
            RasterFormat::Subpixel => 3,
        }
    }
}

/// 字形提示方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HintingMode {
    /// 不提示。轮廓不对齐像素网格，小字号会发虚。
    None,
    /// 适用于抗锯齿渲染。对应 FreeType 的非单色加载目标。
    #[default]
    Smooth,
    /// 强提示，只适合单色（无抗锯齿）渲染。对应 `FT_LOAD_TARGET_MONO`。
    Mono,
}

/// 字形栅格化器，由调用方实现。
///
/// 同一 key 必须给出同一结果——图集会缓存，结果不稳定会导致画面抖动。
pub trait Rasterizer {
    /// 栅格化一个字形。返回 `None` 表示该字体画不出这个字形，
    /// 调用方应当先做 fallback（见 docx-layout 的 `fontenv::select`）再交给图集。
    fn rasterize(&mut self, key: &GlyphKey) -> Option<RasterGlyph>;
}

use std::collections::HashMap;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target};
use skrifa::{FontRef, MetadataProvider};
use zeno::{Format, Mask, PathBuilder};


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
    /// 提示实例按 (face, 字号, 模式) 缓存。
    ///
    /// `HintingInstance::new` 要解释字体的提示字节码，每个字形重建一次会让
    /// 栅格化慢一个量级，所以必须缓存。
    hinters: HashMap<(String, u32, HintingMode), HintingInstance>,
    hinting: HintingMode,
    format: RasterFormat,
    /// 伽马查表：下标是线性覆盖率，值是感知空间的结果。
    gamma: Option<[u8; 256]>,
}

impl SkrifaRasterizer {
    /// 默认三项全开：平滑提示、亚像素、伽马 1.8。
    pub fn new() -> SkrifaRasterizer {
        let mut r = SkrifaRasterizer::default();
        r.set_gamma(1.8);
        r
    }

    /// 关掉亚像素与提示，产出单通道未提示位图。
    ///
    /// 用于需要与其他渲染器逐位对比、或目标不是 LCD 的场合。
    pub fn plain() -> SkrifaRasterizer {
        SkrifaRasterizer {
            hinting: HintingMode::None,
            format: RasterFormat::Alpha,
            gamma: None,
            ..SkrifaRasterizer::default()
        }
    }

    pub fn set_hinting(&mut self, mode: HintingMode) -> &mut Self {
        if mode != self.hinting {
            // 模式变了，旧的提示实例作废。
            self.hinters.clear();
        }
        self.hinting = mode;
        self
    }

    pub fn set_format(&mut self, format: RasterFormat) -> &mut Self {
        self.format = format;
        self
    }

    pub fn format(&self) -> RasterFormat {
        self.format
    }

    /// 设伽马值。1.0 等于不校正；Windows 的文字渲染惯用 1.8~2.2。
    pub fn set_gamma(&mut self, gamma: f32) -> &mut Self {
        if (gamma - 1.0).abs() < f32::EPSILON {
            self.gamma = None;
            return self;
        }
        let inv = 1.0 / gamma;
        let mut table = [0u8; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let v = (i as f32 / 255.0).powf(inv);
            *slot = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        self.gamma = Some(table);
        self
    }

    /// 对覆盖率逐字节做伽马映射。亚像素时三个通道各自映射，与单通道同理。
    fn apply_gamma(&self, mut raw: Vec<u8>) -> Vec<u8> {
        if let Some(t) = &self.gamma {
            for b in raw.iter_mut() {
                *b = t[*b as usize];
            }
        }
        raw
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

        // 半点 → 像素。DPI 缩放由调用方折进 key 的字号，这样同一字号不同 DPI
        // 各自缓存，不会互相污染。
        let px = key.size_half_points as f32 / 2.0;
        let size = Size::new(px);
        let outlines = font.outline_glyphs();
        let glyph = outlines.get(skrifa::GlyphId::new(key.glyph_id))?;

        let mut pen = PenBridge { path: Vec::new() };

        // 字形提示：把轮廓对齐到像素网格。这是小字号清晰与发虚的分界线，
        // 也是 unhinted 输出「放大了也只是更大的模糊」的原因。
        //
        // `HintingInstance` 按 (字体, 字号, 目标) 建，故按 key 缓存——
        // 每个字形重建一次会让栅格化慢一个量级。
        let hinted = match self.hinting {
            HintingMode::None => false,
            mode => {
                let cache_key = (key.face.clone(), key.size_half_points, mode);
                if !self.hinters.contains_key(&cache_key) {
                    let target = match mode {
                        HintingMode::Mono => Target::Mono,
                        HintingMode::Smooth => Target::Smooth {
                            mode: SmoothMode::Normal,
                            symmetric_rendering: false,
                            preserve_linear_metrics: false,
                        },
                        // 上面已排除。
                        HintingMode::None => Target::Mono,
                    };
                    let opts = HintingOptions { engine: Default::default(), target };
                    if let Ok(h) = HintingInstance::new(&outlines, size, LocationRef::default(), opts)
                    {
                        self.hinters.insert(cache_key.clone(), h);
                    }
                }
                match self.hinters.get(&cache_key) {
                    // is_pedantic = false：提示字节码出错时退回未提示，
                    // 而不是整个字形画不出来。
                    Some(h) => glyph.draw(DrawSettings::hinted(h, false), &mut pen).is_ok(),
                    None => false,
                }
            }
        };
        if !hinted {
            glyph
                .draw(DrawSettings::unhinted(size, LocationRef::default()), &mut pen)
                .ok()?;
        }

        let advance = font
            .glyph_metrics(size, LocationRef::default())
            .advance_width(skrifa::GlyphId::new(key.glyph_id))
            .unwrap_or(px);

        // 空白字形（空格等）：没有轮廓，仍要返回以便缓存，避免反复栅格化。
        if pen.path.is_empty() {
            return Some(RasterGlyph {
                metrics: GlyphMetrics { width: 0, height: 0, left: 0.0, top: 0.0, advance },
                coverage: Vec::new(),
                format: self.format,
            });
        }

        let (raw, placement) = Mask::new(&pen.path[..])
            .format(match self.format {
                RasterFormat::Alpha => Format::Alpha,
                RasterFormat::Subpixel => Format::Subpixel,
            })
            .render();

        // 伽马校正：覆盖率是线性的，直接当 alpha 混合会让笔画显细发灰。
        // 在感知空间做一次映射再交给图集。
        let coverage = self.apply_gamma(raw);

        Some(RasterGlyph {
            metrics: GlyphMetrics {
                width: placement.width,
                height: placement.height,
                left: placement.left as f32,
                // placement.top 是位图顶边相对基线的位置（y 向下为正），
                // 而 GlyphMetrics::top 约定为基线到顶边、向上为正，故取反。
                top: -placement.top as f32,
                advance,
            },
            coverage,
            format: self.format,
        })
    }
}
