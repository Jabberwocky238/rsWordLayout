//! 真度量：问字体要宽度与纵向量（feature `fontenv`）。
//!
//! 与 [`super::SimpleMetrics`] 的区别不是「更准一点」，是**换了来源**：
//! 桩按字符类别猜，这里问字体。实测对照（Liberation Serif 12pt，Word for Mac）：
//!
//! | 字符 | Word 实测 | 桩（0.5 em） | 本模块 |
//! | --- | ---: | ---: | ---: |
//! | 数字 | 6.0000pt | 6.0000pt | 6.0000pt |
//! | `B` | 8.0040pt | 6.0000pt | 8.0039pt |
//! | `[` | 2.6374pt | 6.0000pt | 2.6367pt |
//!
//! 数字那一行是**巧合**——Liberation Serif 的数字恰好 0.5 em，不代表桩写对了。
//!
//! 横向走 [`FontRegistry::shape_text`]（按 Word 的槽规则选字体、rustybuzz 整形），
//! 纵向读字体的 `hhea`/`OS/2`。断点仍与桩共用（`super::linebreak`）——
//! 换度量不该顺带换断行策略。

use std::cell::RefCell;
use std::collections::BTreeMap;

use skrifa::{FontRef, MetadataProvider, raw::TableProvider};

use super::registry::FontRegistry;
use super::spec::{
    BreakOpportunity, FINE_PER_TWIP, FontMetrics, FontSpec, LineFontMetrics, MeasuredFontSpan,
    TextMetrics,
};
use crate::layout::{TWIPS_PER_POINT, Twips, spacing_slots};

/// 纵向量化栅格。
///
/// **实测**：Word for Mac 把字形基线放在 **1/300 英寸（0.24pt）** 的栅格上。
/// 两份夹具、56 个不同的基线 y，**56/56 全部**是 0.24pt 的整数倍；
/// 同批数据里 x 坐标只有 **1/85** 落在该栅格上——所以这是纵向独有的，
/// 不是坐标系的假象。
///
/// 这一条（栅格存在）是**直接观测**，证据强。下面两条不是，**都是拿数据凑出来的**，
/// 按量具方法 §7.5 标「回测」，**不当独立检验**：
///
/// - **行高 = 四舍五入到栅格**：只对上 2 个点（12pt → 13.68pt，13.92pt → 16.08pt）；
/// - **基线 = 行高 − 量化后的 descent**：只对上 **1** 个点（13.68 − 2.64 = 11.04）。
///
/// 要证实或推翻，需要一份**专门变字号与字体**的夹具，本仓库还没有。
///
/// 还有一条范围限定，别丢：这是 **Mac** 侧的观测。量具方法 §6.6 说 Word 在两个平台上
/// 有**两套字体度量**，Windows 侧的栅格**未测**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalGrid {
    /// 不量化：直接用字体声明的值。没有未验证的假设，但对不上 Word 的观测值。
    #[default]
    None,
    /// 按 Mac Word 的观测量化到 1/300 英寸。**含上面两条回测规则。**
    MacWordThreeHundredthsInch,
}

impl VerticalGrid {
    /// 栅格步距，twips。1/300 英寸 = 1440/300 = **4.8 twips**——注意它**不是整数**，
    /// 见 [`RealMetrics`] 关于分辨率的说明。
    fn step(self) -> Option<f64> {
        match self {
            VerticalGrid::None => None,
            VerticalGrid::MacWordThreeHundredthsInch => Some(4.8),
        }
    }
}

/// 横向推进量的设备栅格。
///
/// Android Word 的移动视图（窄路径）按设备像素量字宽。证据三条，各自独立：
///
/// - 视图宽的换算 `w3 = (1440 × rect + 389) / 778`（word_analyse Q15，动态）：窄路径的
///   5329 twips 就是 2879 个每英寸 778 的像素；
/// - P0-1b 在同一设备上读到的 run 度量（12.5／18／24／30pt 的 ascent、descent）逐个等于
///   `ppem = round(pt × 778 / 72)`、`round(win 度量 × ppem / upem) + 8`，每英寸 777 或 779 都对不全；
/// - 窄路径的单字读数：`i-plain` 每行 95 个 `i`（精确宽度给 96，按像素给 95）、`w:w=80` 的 `0`
///   每行 55 个（精确比例给 54，像素宽 66 截断到 52 给 55），其余单字读数两种量法都对。
///
/// 纸页路径（打印视图）**不**按这个像素量：`zero-paper` 每行 86 个 `0`、无缩进 190 个 `i`，
/// 按 778 dpi 的像素宽都放不下（85、188）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HorizontalGrid {
    /// 不量化：推进量取字体的精确值。
    #[default]
    None,
    /// 每英寸 `per_inch` 个设备像素：字号按像素取整成 ppem，每个字形的推进量取整到像素，
    /// `w:w` 缩放按整数截断，字符间距按像素取整后加在 cluster 末尾。
    DevicePixels { per_inch: u32 },
}

/// 字体声明的纵向量，字体单位。
#[derive(Debug, Clone, Copy)]
struct FaceVertical {
    upem: f64,
    ascent: f64,
    /// 正值。
    descent: f64,
    line_gap: f64,
    /// OS/2 `ulCodePageRange1` 声明了东亚代码页（第 17–21 位：日、简中、韩、繁中、韩 Johab）。
    east_asian: bool,
}

/// Android 上声明了东亚代码页的字体，单倍行高乘的系数（「N7」：hhea 行高 × 1.3）。
///
/// 依据（Android 打印视图）：不写字体的夹具 Word 用等线 11pt 排（2026-10-04 补测：`longpage-auto12pt`
/// 页 0 是 47 行、进程映射里是 `DengXian-54497409372.ttf`）。等线的 hhea（与 win、typo 相同）是
/// 2134 / 2048 = 1.0420 em，乘 1.3 是 1.3546 em：字号阶梯要求的 1.3507–1.3578 em、docGrid 297 加倍
/// 298 不加倍要求的 (297, 298] twips（11pt 下 298.0）、表后空段要求的 (296, 302] 都落在里面；
/// 不乘是 229 twips，哪一条都对不上。Mac 上同一条规则由 `docs/PREREG-2026-09-18-cjk-plain.md` 的 Z2
/// 测过（宋体 4/4 字号），桌面这里尚未启用。1.3 怎么分到升部、降部没测，这里按比例放大三项。
const EAST_ASIAN_LINE_SCALE: f64 = 1.3;

/// 读字体文件的度量实现。
///
/// # 分辨率的天花板
///
/// 布局单位是 `Twips`（i32，1/1440 英寸 = 0.05pt），而 Word 的纵向栅格是
/// 1/300 英寸 = 0.24pt = **4.8 twips**。栅格点根本落不到整 twips 上，
/// 两者只在 **1/7200 英寸**上才通约。所以开了栅格也只能取到最近的 twip，
/// 残差 ≤ 0.5 twip（0.025pt），且会随行数累积。
pub struct RealMetrics<'r> {
    registry: &'r FontRegistry,
    grid: VerticalGrid,
    horizontal: HorizontalGrid,
    kerning_by_default: bool,
    east_asian_line_scale: bool,
    vertical: RefCell<BTreeMap<String, Option<FaceVertical>>>,
}

impl<'r> RealMetrics<'r> {
    pub fn new(registry: &'r FontRegistry) -> RealMetrics<'r> {
        RealMetrics {
            registry,
            grid: VerticalGrid::None,
            horizontal: HorizontalGrid::None,
            kerning_by_default: false,
            east_asian_line_scale: false,
            vertical: RefCell::new(BTreeMap::new()),
        }
    }

    /// 开启横向的设备像素栅格（见 [`HorizontalGrid`]）。
    pub fn with_horizontal_grid(mut self, grid: HorizontalGrid) -> RealMetrics<'r> {
        self.horizontal = grid;
        self
    }

    pub fn horizontal_grid(&self) -> HorizontalGrid {
        self.horizontal
    }

    /// 没写 `w:kern` 时也按字体做字距调整（GPOS `kern`），并且跨 run 调。见 [`kerning_font`]
    /// 与 [`FontMetrics::piece_boundary_kern`] 的实现。
    pub fn with_kerning_by_default(mut self, always: bool) -> RealMetrics<'r> {
        self.kerning_by_default = always;
        self
    }

    /// 声明了东亚代码页的字体按 [`EAST_ASIAN_LINE_SCALE`] 放大单倍行高（Android）。
    pub fn with_east_asian_line_scale(mut self, on: bool) -> RealMetrics<'r> {
        self.east_asian_line_scale = on;
        self
    }

    /// 整形，开了像素栅格时把推进量落到像素上（缩放与字符间距随之在像素上做完）。
    fn shaped(&self, text: &str, font: &FontSpec) -> Vec<crate::layout::ShapedRun> {
        let font = kerning_font(font, self.kerning_by_default);
        let mut runs = self.registry.shape_text(text, &font);
        if let HorizontalGrid::DevicePixels { per_inch } = self.horizontal {
            quantize_to_pixels(&mut runs, &font, per_inch);
        }
        runs
    }

    /// 开启纵向量化。**先读 [`VerticalGrid`] 的限定**：其中两条是回测，不是已验证的规则。
    pub fn with_vertical_grid(mut self, grid: VerticalGrid) -> RealMetrics<'r> {
        self.grid = grid;
        self
    }

    pub fn vertical_grid(&self) -> VerticalGrid {
        self.grid
    }

    fn face_vertical(&self, face: &str) -> Option<FaceVertical> {
        if let Some(cached) = self.vertical.borrow().get(face) {
            return *cached;
        }
        let computed = (|| {
            let (bytes, index) = self.registry.face_data(face)?;
            let font = FontRef::from_index(bytes, index).ok()?;
            // 不给 Size，拿字体单位下的原始值；缩放留到用的时候做一次，
            // 先缩放再相加会在每一项上各丢一点。
            let m = font.metrics(
                skrifa::instance::Size::unscaled(),
                skrifa::instance::LocationRef::default(),
            );
            let upem = f64::from(m.units_per_em);
            let east_asian = font.os2().ok()
                .and_then(|os2| os2.ul_code_page_range_1())
                .is_some_and(|pages| pages & (0b1_1111 << 17) != 0);
            (upem > 0.0).then_some(FaceVertical {
                upem,
                ascent: f64::from(m.ascent),
                descent: f64::from(m.descent).abs(),
                line_gap: f64::from(m.leading),
                east_asian,
            })
        })();
        self.vertical
            .borrow_mut()
            .insert(face.to_string(), computed);
        computed
    }

    /// 本次请求用到的各 face 的纵向量取最大，单位 twips，**未取整**。
    fn vertical_raw(&self, text: &str, font: &FontSpec) -> Option<(f64, f64, f64)> {
        let em = font.size_pt() * 20.0;
        let (mut ascent, mut descent, mut line_gap) = (0.0f64, 0.0f64, 0.0f64);
        let mut found = false;

        // 空串也要给纵向量：空段落的行高由段落标记的字体决定。
        let probe = if text.is_empty() { "x" } else { text };
        let mut seen: Vec<String> = Vec::new();
        for ch in probe.chars() {
            // 与整形同一口径：名义 `.notdef` 借哪个 face，纵向量就取哪个 face 的。
            // 否则一段全是缺字的汉字行高为零。
            let Some((face, _)) = self.registry.face_for_char(font, ch) else {
                continue;
            };
            if seen.contains(&face) {
                continue;
            }
            seen.push(face.clone());
            let Some(v) = self.face_vertical(&face) else {
                continue;
            };
            found = true;
            let scale = if self.east_asian_line_scale && v.east_asian { EAST_ASIAN_LINE_SCALE } else { 1.0 };
            ascent = ascent.max(v.ascent / v.upem * em * scale);
            descent = descent.max(v.descent / v.upem * em * scale);
            line_gap = line_gap.max(v.line_gap / v.upem * em * scale);
        }
        found.then_some((ascent, descent, line_gap))
    }

    /// 自然行高，单位 twips，**既不取整也不量化**。
    ///
    /// 它是游标每行推进的步长。**步长不量化，只有落位量化**——
    /// 这条是量出来的，不是想出来的。cursor-unit 那一批（`docs/PREREG-2026-09-17-
    /// cursor-unit.md` 的 R2）把步长分别取整到 1/600、1/1200、twip、半 twip、
    /// 1/7200 英寸去比，得分随单位变粗**单调下降**：
    ///
    /// | 步长取整到 | 逐位相等 |
    /// | --- | ---: |
    /// | 不取整 | **654 / 752** |
    /// | 1/7200 英寸 | 450 / 752 |
    /// | 半 twip | 264 / 752 |
    /// | twip | 192 / 752 |
    /// | 1/1200 英寸 | 99 / 752 |
    /// | 1/600 英寸 | 26 / 752 |
    ///
    /// 这里原来把步长量化到 **0.24pt**，比上表最粗的那一档还粗。
    /// 量化该做的地方是 [`FontMetrics::quantize_baseline_fine`]——落位时做一次。
    fn natural_raw(&self, text: &str, font: &FontSpec) -> f64 {
        let Some((a, d, g)) = self.vertical_raw(text, font) else {
            return 0.0;
        };
        a + d + g
    }

    /// 本次请求用到的各 face 取最大，再按栅格量化。
    fn vertical_for(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let Some((ascent, descent, line_gap)) = self.vertical_raw(text, font) else {
            // 一个 face 都没有：给零，让上层看到「没度量」而不是一个编出来的高度。
            return TextMetrics::default();
        };
        self.project_vertical(ascent, descent, line_gap)
    }

    fn project_vertical(&self, ascent: f64, descent: f64, line_gap: f64) -> TextMetrics {
        let round = |v: f64| v.round() as Twips;
        match self.grid.step() {
            None => TextMetrics {
                advance: 0,
                ascent: round(ascent),
                descent: round(descent),
                line_gap: round(line_gap),
            },
            Some(step) => {
                let quantize = |v: f64| (v / step).round() * step;
                // 行高只取整**一次**，descent 单独取整，差额全部归 ascent。
                // 分别取整再相加会让和偏出栅格一整个 twip：
                // round(278.4 − 52.8) + round(52.8) = 279，而 round(278.4) = 278。
                let natural = round(quantize(ascent + descent + line_gap));
                let descent_q = round(quantize(descent));
                // line_gap 折进 ascent，好让布局主干现有的
                // `natural = ascent + descent + line_gap`、`baseline = ascent` 直接成立。
                TextMetrics {
                    advance: 0,
                    ascent: natural - descent_q,
                    descent: descent_q,
                    line_gap: 0,
                }
            }
        }
    }

    /// 缩放与字距。与 `SimpleMetrics` 同一口径——换度量不该顺带换这部分语义。
    ///
    /// `slots` 是加间距的位置数，取 [`crate::layout::spacing_slots`]（每个整形 cluster 一次），
    /// 与画字的 `apply_char_spacing` 数同一个东西。`SimpleMetrics` 不整形，按
    /// `linebreak::cluster_boundaries` 数源字符簇；组合序列（`x` + U+0301）两边都是一次。
    ///
    /// 依据（见 `bridge::run_letter_spacing`）：空格也加是实测（`latinspace`）；
    /// 行尾最后一个可见字符也加是**假定**——只在「行尾空格不计宽」时才被 `latinspace` 证实，
    /// 而 rsword 计行尾空格；按 cluster 计、间距不随 `w:w` 缩放也都是假定（未测）。
    fn apply_spacing(&self, advance: i64, slots: usize, font: &FontSpec) -> Twips {
        let mut advance = advance;
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = advance * i64::from(font.scale_pct) / 100;
        }
        (advance + slots as i64 * i64::from(font.letter_spacing)) as Twips
    }

    /// `apply_spacing` 的精确版，单位点。
    ///
    /// 与整数版差在缩放那一步：整数版的 `advance * pct / 100` 是截断除法，
    /// 这里是实数除法。要的就是这个差——横向的取整全部推迟到出数时做一次。
    fn apply_spacing_pt(&self, advance: f64, slots: usize, font: &FontSpec) -> f64 {
        let mut advance = advance;
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = advance * f64::from(font.scale_pct) / 100.0;
        }
        advance + slots as f64 * f64::from(font.letter_spacing) / 20.0
    }
}

/// 把一段整形结果的推进量落到每英寸 `per_inch` 的设备像素上（见 [`HorizontalGrid`]）。
///
/// 每个字形：`ppem = round(字号 × per_inch / 72)`，`px = round(推进量 / em × ppem)`；
/// 有 `w:w` 时 `px = px × pct / 100`（整数截断）；cluster 末尾再加 `round(间距 × per_inch / 1440)`。
/// 像素宽换回点之后，整 twips 的推进量按累计值取整之差重算，与整形器同一手法。
/// 字距、缩放都在这里做完，所以调用方不再 `apply_spacing`。
fn quantize_to_pixels(runs: &mut [crate::layout::ShapedRun], font: &FontSpec, per_inch: u32) {
    let per_inch = f64::from(per_inch);
    let ends: Vec<bool> = (0..runs.len()).map(|i| crate::layout::is_cluster_end(runs, i)).collect();
    let scale = (font.scale_pct != 100 && font.scale_pct > 0).then_some(i64::from(font.scale_pct));
    let spacing_px = (f64::from(font.letter_spacing) * per_inch / 1440.0).round() as i64;
    let (mut acc_pt, mut acc_twips) = (0.0f64, 0);
    for (g, end) in runs.iter_mut().zip(ends) {
        let size_pt = g.size_centipoints.unwrap_or_else(|| font.effective_size_centipoints()) as f64 / 100.0;
        let mut px = if size_pt > 0.0 {
            let ppem = (size_pt * per_inch / 72.0).round();
            (g.x_advance_pt / size_pt * ppem).round() as i64
        } else {
            0
        };
        if let Some(pct) = scale {
            px = px * pct / 100;
        }
        if end {
            px += spacing_px;
        }
        g.x_advance_pt = px as f64 * 72.0 / per_inch;
        acc_pt += g.x_advance_pt;
        let next = (acc_pt * f64::from(TWIPS_PER_POINT)).round() as Twips;
        g.x_advance = next - acc_twips;
        acc_twips = next;
    }
}

/// Android 上度量与绘制整形用的字体：没写 `w:kern` 也调字距，写了就照阈值。
///
/// Android Word 不写 `w:kern` 也做字距调整：`kern-off`（Calibri 12pt 的 `AVAV…`，纸页 10466）与写了
/// `w:kern` 的 `kern-on` 一样每行 82 个字母。按 Calibri 的 GPOS（A→V −89、V→A −96 字体单位）两个方向
/// 都调，82 个字母宽 10399 twips、84 个宽 10652，正好是 82；不调是 76。Mac Word 不写 `w:kern` 就不调
/// （`B11` 的两个 `1`，见 [`FontSpec::kerning`]），所以这是平台差异。
///
/// 写了的阈值照样算数（2026-10-04 打印视图补测）：41 个 `AV` 一段，不调 11277 twips、调了 10399，
/// 纸页 10466。`w:kern w:val="48"`（24pt，大于 12pt）两行、末行 6 个字母；不写一行；`w:kern="2"` 一行。
fn kerning_font(font: &FontSpec, always: bool) -> std::borrow::Cow<'_, FontSpec> {
    if always && !font.kerning && !font.kern_declared {
        std::borrow::Cow::Owned(FontSpec { kerning: true, ..font.clone() })
    } else {
        std::borrow::Cow::Borrowed(font)
    }
}

/// 绘制用的整形器：与断行同一口径地整形——字距调整总是开时照开，像素栅格时把推进量落到像素上。
///
/// 像素栅格的缩放与字符间距在整形里一并做完（[`crate::layout::TextShaper::applies_char_spacing`]），
/// 绘制不再加一次；否则行宽按像素、行内字形按精确宽，两边对不上。
pub(crate) struct PaintShaper<'s> {
    pub inner: &'s dyn crate::layout::TextShaper,
    pub pixels: Option<u32>,
    pub kerning_by_default: bool,
}

impl crate::layout::TextShaper for PaintShaper<'_> {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<crate::layout::ShapedRun> {
        let font = kerning_font(font, self.kerning_by_default);
        let mut runs = self.inner.shape(text, &font);
        if let Some(per_inch) = self.pixels {
            quantize_to_pixels(&mut runs, &font, per_inch);
        }
        runs
    }

    fn applies_char_spacing(&self) -> bool {
        self.pixels.is_some()
    }
}

impl FontMetrics for RealMetrics<'_> {
    fn boundary_spacing(
        &self, left: char, left_font: &FontSpec, right: char, right_font: &FontSpec,
    ) -> super::SpacingAdvance {
        super::linebreak::autospace_dn_boundary(left, left_font, right, right_font)
    }

    fn has_family(&self, family: &str) -> bool {
        self.registry.has_family(family)
    }

    /// Android 上跨片段（跨 run）照样调字距：两侧度量属性相同、开了字距调整时，补上分开整形
    /// 丢掉的那一对的调整量——两字合在一起量、减去各自量、再减去已另算的边界间距。
    ///
    /// 依据（2026-10-04 打印视图补测，纸页 10466）：41 个 `AV` 每 10 个字母换一种颜色（8 处 run
    /// 边界都是 V→A，每处 −96 字体单位即 −11.25 twips），Word 排成一行；分开整形是 10489 twips，
    /// 放不下。只量过只差颜色的 run；字号、字体不同的两侧不调（未测）。桌面没有读数，不调。
    fn piece_boundary_kern(
        &self, left: char, left_font: &FontSpec, right: char, right_font: &FontSpec,
    ) -> super::SpacingAdvance {
        if !self.kerning_by_default
            || left_font != right_font
            || left_font.fit_text.is_some()
            || !kerning_font(left_font, true).kerning
        {
            return super::SpacingAdvance::default();
        }
        let (mut l, mut r) = ([0u8; 4], [0u8; 4]);
        let (l, r) = (&*left.encode_utf8(&mut l), &*right.encode_utf8(&mut r));
        let pair = format!("{l}{r}");
        let between = self.boundary_spacing(left, left_font, right, right_font);
        let font = left_font;
        super::SpacingAdvance {
            fit_twips: self.measure(&pair, font).advance
                - self.measure(l, font).advance
                - self.measure(r, font).advance
                - between.fit_twips,
            paint_pt: self.advance_pt(&pair, font)
                - self.advance_pt(l, font)
                - self.advance_pt(r, font)
                - between.paint_pt,
        }
    }
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let shaped = self.shaped(text, font);
        let advance: i64 = shaped.iter().map(|g| i64::from(g.x_advance)).sum();
        let autospace = super::linebreak::autospace_dn(text, font).fit_twips;
        let advance = match self.horizontal {
            HorizontalGrid::None => self.apply_spacing(advance, spacing_slots(&shaped), font),
            HorizontalGrid::DevicePixels { .. } => advance as Twips,
        };
        TextMetrics {
            advance: advance + autospace,
            ..self.vertical_for(text, font)
        }
    }

    fn line_metrics(&self, spans: &[MeasuredFontSpan<'_>]) -> LineFontMetrics {
        let (mut ascent, mut descent, mut gap) = (0.0f64, 0.0f64, 0.0f64);
        for span in spans {
            if let Some((a, d, g)) = self.vertical_raw(span.text, span.font) {
                ascent = ascent.max(a);
                descent = descent.max(d);
                gap = gap.max(g);
            }
        }
        // Apply the same projection as a single mixed-face request, after
        // collecting every accepted fragment. No shaping is needed here.
        let m = self.project_vertical(ascent, descent, gap);
        LineFontMetrics {
            ascent: m.ascent,
            descent: m.descent,
            natural_height: m.natural_height(),
            natural_height_fine: ((ascent + descent + gap) * FINE_PER_TWIP as f64).round() as i64,
            ascent_fine: i64::from(m.ascent) * FINE_PER_TWIP,
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        // 断点与字体无关，与桩共用同一套——否则换度量之后的差值里会混进
        // 断行策略的变化，分不出是哪一边错。
        super::linebreak::break_opportunities(text)
    }

    /// 紧急断行只切在整形器的 cluster 起点上。
    ///
    /// 默认实现用 `linebreak::cluster_boundaries` 那张近似表，而它是薄的：泰文 SARA AM、
    /// 老挝文 AM、高棉文与印度系的元音符号都不在里面，切在它们前面会把符号甩到下一行行首
    /// （辅音 + SARA AM 的长串，第二行以 SARA AM 开头）。整形器本来就知道哪些字符归一簇——rustybuzz
    /// 默认的 cluster 级别按字素合并：组合符号并进基字，SARA AM 分解重排后并进前一个辅音，
    /// 连字与音节各成一簇——所以直接用 `ShapedRun::source` 的起点，UTF-16 换回字节偏移。
    /// 「按 cluster 切」本身是**假设**，Word 只量过单一 BMP 字母（见 [`FontMetrics::fit_clusters`]）。
    ///
    /// 每个候选前缀仍重新 `measure`，与 [`FontMetrics::fit`] 同一口径：切口处的整形
    /// 可能与整段不同，宽度以切下来的那一段为准。整形给不出字形或给不出源区间时
    /// （该段一个 face 都没有），退回默认那张表。
    ///
    /// 代价：每条紧急断行多整形一次剩余全文。调用方每行本来就对剩余全文 `measure`、`fit`
    /// 各一次，量级不变；2 万字不带断点的一段在 5329 上约 5.3 s（用近似表约 4.1 s，
    /// 改造前约 285 s）。只整形一个窗口能省掉这一次，但窗口尾部的连字与音节会让切口
    /// 落进 cluster 中间，这里不冒这个险。
    fn fit_clusters(&self, text: &str, font: &FontSpec, max_width: Twips) -> (usize, TextMetrics) {
        let starts: std::collections::BTreeSet<u32> = self
            .registry
            .shape_text(text, font)
            .iter()
            .filter_map(|g| g.source.map(|(start, _)| start))
            .collect();
        if starts.is_empty() {
            return super::spec::fit_cluster_ends(
                self,
                text,
                font,
                max_width,
                super::linebreak::cluster_boundaries(text),
            );
        }
        let mut unit = 0u32;
        let mut ends = Vec::new();
        for (offset, ch) in text.char_indices() {
            if offset > 0 && starts.contains(&unit) {
                ends.push(offset);
            }
            unit += ch.len_utf16() as u32;
        }
        ends.push(text.len());
        super::spec::fit_cluster_ends(self, text, font, max_width, ends)
    }

    fn advance_pt(&self, text: &str, font: &FontSpec) -> f64 {
        let shaped = self.shaped(text, font);
        let advance: f64 = shaped.iter().map(|g| g.x_advance_pt).sum();
        let autospace = super::linebreak::autospace_dn(text, font).paint_pt;
        match self.horizontal {
            HorizontalGrid::None => self.apply_spacing_pt(advance, spacing_slots(&shaped), font) + autospace,
            HorizontalGrid::DevicePixels { .. } => advance + autospace,
        }
    }

    fn quantize_baseline_fine(&self, y_fine: i64) -> i64 {
        // 栅格步距换算到 1/7200 英寸：0.24pt = 4.8 twips = **24 个单位**，是整数，
        // 所以这里的量化是精确的整数运算，不像落到 twips 那样必然有残差。
        let Some(step_twips) = self.grid.step() else {
            return y_fine;
        };
        let step = (step_twips * FINE_PER_TWIP as f64).round() as i64;
        if step <= 0 {
            return y_fine;
        }
        // 四舍五入到最近的栅格点（远离零，与 `quantize` 的其余处一致）。
        let half = step / 2;
        if y_fine >= 0 {
            (y_fine + half) / step * step
        } else {
            -((-y_fine + half) / step * step)
        }
    }

    fn natural_height_fine(&self, text: &str, font: &FontSpec) -> i64 {
        // **不整形**：行高只取决于字体的纵向量，所以这条比 `measure` 便宜得多——
        // 布局主干会为每个片段调用它。
        (self.natural_raw(text, font) * FINE_PER_TWIP as f64).round() as i64
    }
}
