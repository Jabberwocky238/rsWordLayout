//! 真度量：读字体文件 + rustybuzz 整形（feature `shape`）。
//!
//! 与 [`crate::simple_metrics::SimpleMetrics`] 的区别不是「更准一点」，是**换了来源**：
//! 桩按字符类别猜宽度，这里问字体要。实测对照（Liberation Serif 12pt，Word for Mac）：
//!
//! | 字符 | Word 实测 | 桩（0.5 em） | 本模块 |
//! | --- | ---: | ---: | ---: |
//! | 数字 | 6.0000pt | 6.0000pt | 6.0000pt |
//! | `B` | 8.0040pt | 6.0000pt | 8.0039pt |
//! | `[` | 2.6374pt | 6.0000pt | 2.6367pt |
//!
//! 数字那一行是**巧合**——Liberation Serif 的数字恰好是 0.5 em，不代表桩写对了。
//!
//! ## 字体选择走 fontenv
//!
//! 按**码位**选 face（`docx_layout::fontenv::FontEnvironment::select`），因为
//! 「一个 run 一个字体」在全 Unicode 下不成立：一段中英混排里西文与汉字往往落在不同文件。
//! 选择结果带 `FONT_SUBSTITUTED` / `FONT_FALLBACK_GLYPH` / `FONT_MISSING` 诊断，
//! 本模块把它们计数暴露出来——**回退过的度量不该被当成原生度量看**。
//!
//! ## 纵向量化：一条实测的、但证据很薄的规则
//!
//! 见 [`VerticalGrid`]。读那里的限定再决定要不要开。

use std::cell::RefCell;
use std::collections::BTreeMap;

use docx_layout::fontenv::{FaceId, FontEnvironment};
use rustybuzz::{Face, Feature, UnicodeBuffer};
use rustybuzz::ttf_parser::Tag;
use skrifa::{FontRef, MetadataProvider};

use crate::geom::{TWIPS_PER_POINT, Twips, half_points_to_twips};
use crate::measure::{BreakOpportunity, FontMetrics, FontSpec, GlyphPosition, TextMetrics};

/// 纵向量化栅格。
///
/// **实测**：Word for Mac 把字形基线放在 **1/300 英寸（0.24pt）** 的栅格上。
/// 两份夹具、56 个不同的基线 y，**56/56 全部**是 0.24pt 的整数倍；
/// 同批数据里 x 坐标只有 1/85 落在该栅格上——所以这是纵向独有的，不是坐标系的假象。
///
/// 这一条（栅格存在）是**直接观测**，证据强。下面两条不是，**都是拿数据凑出来的**，
/// 按方法 §7.5 标「回测」，**不当独立检验**：
///
/// - **行高 = 四舍五入到栅格**：只对上 2 个点（12pt → 13.68pt，13.92pt → 16.08pt）；
/// - **基线 = 行高 − 量化后的 descent**：只对上 **1** 个点（13.68 − 2.64 = 11.04）。
///
/// 要证实或推翻它们，需要一份**专门变字号与字体**的夹具，本仓库还没有。
///
/// 还有一条范围限定，别丢：这是 **Mac** 侧的观测。方法 §6.6 说 Word 在两个平台上有
/// **两套字体度量**，Windows 侧的栅格**未测**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalGrid {
    /// 不量化：直接用字体声明的值。没有未验证的假设，但对不上 Word 的观测值。
    None,
    /// 按 Mac Word 的观测量化到 1/300 英寸。**含上面两条回测规则。**
    MacWordThreeHundredthsInch,
}

impl VerticalGrid {
    /// 栅格步距，twips。1/300 英寸 = 1440/300 = 4.8 twips。
    fn step(self) -> Option<f64> {
        match self {
            VerticalGrid::None => None,
            VerticalGrid::MacWordThreeHundredthsInch => Some(4.8),
        }
    }

}

/// 字体文件里声明的纵向度量，字体单位。
#[derive(Debug, Clone, Copy)]
struct FaceVertical {
    upem: f64,
    ascent: f64,
    /// 正值。
    descent: f64,
    line_gap: f64,
}

/// 一次度量里字体选择的诊断计数。
///
/// 存在的理由：**回退过的度量不该被当成原生度量看**。量具在比较时若发现差值，
/// 先要能分清「引擎算错了」与「根本没找到那个字体」。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectionStats {
    /// 用上了非首选族（`FONT_SUBSTITUTED`）。
    pub substituted: usize,
    /// 首选族没有该码位的字形，回退到别的 face（`FONT_FALLBACK_GLYPH`）。
    pub fallback: usize,
    /// 整个环境都没有覆盖（`FONT_MISSING`）——这些字符**没有宽度可言**。
    pub missing: usize,
}

impl SelectionStats {
    pub fn is_clean(&self) -> bool {
        *self == SelectionStats::default()
    }
}

/// 读字体文件的度量实现。
pub struct FontEnvMetrics<'e> {
    env: &'e FontEnvironment,
    grid: VerticalGrid,
    vertical: RefCell<BTreeMap<FaceId, Option<FaceVertical>>>,
    stats: RefCell<SelectionStats>,
}

impl<'e> FontEnvMetrics<'e> {
    pub fn new(env: &'e FontEnvironment) -> FontEnvMetrics<'e> {
        FontEnvMetrics {
            env,
            grid: VerticalGrid::None,
            vertical: RefCell::new(BTreeMap::new()),
            stats: RefCell::new(SelectionStats::default()),
        }
    }

    /// 开启纵向量化。**先读 [`VerticalGrid`] 的限定**：其中两条是回测，不是已验证的规则。
    pub fn with_vertical_grid(mut self, grid: VerticalGrid) -> FontEnvMetrics<'e> {
        self.grid = grid;
        self
    }

    pub fn vertical_grid(&self) -> VerticalGrid {
        self.grid
    }

    /// 至今累计的字体选择诊断。
    pub fn selection_stats(&self) -> SelectionStats {
        *self.stats.borrow()
    }

    pub fn reset_selection_stats(&self) {
        *self.stats.borrow_mut() = SelectionStats::default();
    }

    /// `FontSpec::family` 可能是逗号分隔的候选串（桥接层就这么给的），拆成有序候选。
    fn families(font: &FontSpec) -> Vec<String> {
        font.family
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn weight(font: &FontSpec) -> u16 {
        if font.bold { 700 } else { 400 }
    }

    /// 按码位挑 face，并记录诊断。
    fn face_for(&self, c: char, families: &[String], font: &FontSpec) -> Option<FaceId> {
        let selection = self.env.select(families, Self::weight(font), font.italic, c);
        {
            let mut stats = self.stats.borrow_mut();
            for diagnostic in &selection.diagnostics {
                match *diagnostic {
                    "FONT_SUBSTITUTED" => stats.substituted += 1,
                    "FONT_FALLBACK_GLYPH" => stats.fallback += 1,
                    "FONT_MISSING" => stats.missing += 1,
                    _ => {}
                }
            }
        }
        selection.face.map(|f| f.id().clone())
    }

    /// 把一段文字按选中的 face 切成若干段。相邻同 face 的合并。
    fn segments(&self, text: &str, font: &FontSpec) -> Vec<(Option<FaceId>, usize, usize)> {
        let families = Self::families(font);
        let mut out: Vec<(Option<FaceId>, usize, usize)> = Vec::new();
        for (at, c) in text.char_indices() {
            let face = self.face_for(c, &families, font);
            let end = at + c.len_utf8();
            match out.last_mut() {
                Some(last) if last.0 == face => last.2 = end,
                _ => out.push((face, at, end)),
            }
        }
        out
    }

    fn face_vertical(&self, id: &FaceId) -> Option<FaceVertical> {
        if let Some(cached) = self.vertical.borrow().get(id) {
            return *cached;
        }
        let computed = (|| {
            let bytes = self.env.data(id)?;
            let font = FontRef::from_index(bytes, id.index()).ok()?;
            // 不给 Size，拿字体单位下的原始值，缩放留到用的时候做——
            // 先缩放再相加会在每一步引入舍入。
            let metrics = font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
            let upem = f64::from(metrics.units_per_em);
            if upem <= 0.0 {
                return None;
            }
            Some(FaceVertical {
                upem,
                ascent: f64::from(metrics.ascent),
                descent: f64::from(metrics.descent).abs(),
                line_gap: f64::from(metrics.leading),
            })
        })();
        self.vertical.borrow_mut().insert(id.clone(), computed);
        computed
    }

    /// 对一个 face 整形一段文字，返回逐字形的 (簇起点字节偏移, 推进量 twips 的**未取整值**)。
    ///
    /// 刻意不在这里取整：每个字形各取一次整，误差会沿行累加。实测桩改真度量后
    /// 行尾仍差 0.47pt，就是这么攒出来的——Word 那边是按字体单位精确累加的。
    /// 取整留到落位的最后一刻，见 [`FontEnvMetrics::glyph_positions`]。
    fn shape_segment(&self, id: &FaceId, text: &str, font: &FontSpec) -> Vec<(usize, f64)> {
        let Some(bytes) = self.env.data(id) else { return Vec::new() };
        let Some(face) = Face::from_slice(bytes, id.index()) else { return Vec::new() };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        // 方向与脚本由 rustybuzz 按内容推断：阿拉伯语自动走 RTL。
        buffer.guess_segment_properties();

        // 字距调整默认**关**（OOXML `w:kern` 的语义，见 `FontSpec::kerning`）。
        // rustybuzz 不给就默认开，所以这里必须显式关掉，不能靠不传。
        let features: &[Feature] = if font.kerning {
            &[]
        } else {
            &[Feature::new(Tag::from_bytes(b"kern"), 0, ..)]
        };
        let shaped = rustybuzz::shape(&face, features, buffer);

        let upem = f64::from(face.units_per_em());
        if upem <= 0.0 {
            return Vec::new();
        }
        let pt = f64::from(font.size_half_points) / 2.0;
        let to_twips = |v: i32| -> f64 { f64::from(v) * pt / upem * f64::from(TWIPS_PER_POINT) };

        shaped
            .glyph_infos()
            .iter()
            .zip(shaped.glyph_positions().iter())
            .map(|(info, pos)| (info.cluster as usize, to_twips(pos.x_advance)))
            .collect()
    }

    /// 缩放与字距。与 `SimpleMetrics` 用同一口径，换度量不该顺带换这部分语义。
    /// 同样保持未取整：取整只在最后落位时做一次。
    fn apply_spacing(&self, advance: f64, glyphs: usize, font: &FontSpec) -> f64 {
        let mut advance = advance;
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = advance * f64::from(font.scale_pct) / 100.0;
        }
        advance + glyphs as f64 * f64::from(font.letter_spacing)
    }

    /// 本次请求用到的各 face 的纵向度量取最大，再按栅格量化。
    ///
    /// 全程用 f64 算到最后一步才落回 twips：先把 ascent / descent / line_gap 各自
    /// 取整再相加，会在每一项上各丢半个 twip，三项加起来足以把行高推偏 0.1pt。
    fn vertical_for(&self, faces: &[FaceId], font: &FontSpec) -> TextMetrics {
        let em = f64::from(half_points_to_twips(font.size_half_points));
        let mut ascent = 0.0f64;
        let mut descent = 0.0f64;
        let mut line_gap = 0.0f64;
        let mut found = false;

        for id in faces {
            let Some(v) = self.face_vertical(id) else { continue };
            found = true;
            ascent = ascent.max(v.ascent / v.upem * em);
            descent = descent.max(v.descent / v.upem * em);
            line_gap = line_gap.max(v.line_gap / v.upem * em);
        }

        if !found {
            // 一个 face 都没有：给零，让上层看到「没度量」而不是一个编出来的高度。
            return TextMetrics::default();
        }

        let round = |v: f64| v.round() as Twips;

        match self.grid.step() {
            None => TextMetrics {
                advance: 0,
                ascent: round(ascent),
                descent: round(descent),
                line_gap: round(line_gap),
            },
            Some(step) => {
                // 观测到的两件事：行高与 descent 各自落在栅格上，基线 = 行高 − descent。
                // 把 line_gap 折进 ascent 返回，好让引擎现有的
                // `natural = ascent + descent + line_gap`、`baseline = ascent`
                // 直接得出量化后的结果，不必为这条规则改布局主干。
                //
                // **注意单位精度**：栅格是 1/300 英寸 = **4.8 twips**，而 `Twips` 是 i32、
                // 步长 1/1440 英寸。栅格点根本落不到整 twips 上，所以这里最多只能取到
                // 最近的 twip，残差 ≤ 0.5 twip（0.025pt），且会随行数累积。
                // 要真正对上 Word 的纵向栅格，布局单位得细到 1/7200 英寸。
                let quantize = |v: f64| (v / step).round() * step;
                // 行高只取整一次，descent 单独取整，**差额全部归 ascent**。
                // 分别取整再相加会让和偏出栅格一整个 twip：
                // round(278.4 − 52.8) + round(52.8) = 226 + 53 = 279，而 round(278.4) = 278。
                // 行高是要逐行累加下去的，偏一个 twip 就会随页面往下漂。
                let natural_t = round(quantize(ascent + descent + line_gap));
                let descent_t = round(quantize(descent));
                TextMetrics {
                    advance: 0,
                    ascent: natural_t - descent_t,
                    descent: descent_t,
                    line_gap: 0,
                }
            }
        }
    }
}

impl FontMetrics for FontEnvMetrics<'_> {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let segments = self.segments(text, font);
        let faces: Vec<FaceId> = segments.iter().filter_map(|(f, _, _)| f.clone()).collect();

        // 全程用 f64 累加，最后取一次整：逐字形取整会沿行攒出可观的偏差。
        let mut advance = 0.0f64;
        let mut glyphs = 0usize;
        for (face, start, end) in &segments {
            let Some(id) = face else { continue };
            for (_, a) in self.shape_segment(id, &text[*start..*end], font) {
                advance += a;
                glyphs += 1;
            }
        }

        // 空串也要给纵向度量：空段落的行高由段落标记的字体决定。
        let face_set = if faces.is_empty() {
            let families = Self::families(font);
            self.face_for('x', &families, font).into_iter().collect()
        } else {
            faces
        };

        TextMetrics {
            advance: self.apply_spacing(advance, glyphs, font).round() as Twips,
            ..self.vertical_for(&face_set, font)
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        crate::linebreak::break_opportunities(text)
    }

    fn glyph_positions(&self, text: &str, font: &FontSpec) -> Vec<GlyphPosition> {
        // **必须覆盖默认实现**：默认按前缀宽度算，而这里有 kerning 与连字，
        // 前缀和不等于逐字形推进（理由见 `FontMetrics::glyph_positions` 的文档）。
        let mut out = Vec::new();
        // 位置在 f64 上精确累加，只在落位那一刻取整——**推进量由相邻位置相减得到**，
        // 这样逐字形推进量之和恒等于整串宽度，不会沿行攒出偏差。
        let mut x = 0.0f64;
        for (face, seg_start, seg_end) in self.segments(text, font) {
            let Some(id) = face else { continue };
            let segment = &text[seg_start..seg_end];
            let shaped = self.shape_segment(&id, segment, font);
            for (i, (cluster, advance)) in shaped.iter().enumerate() {
                // 簇边界即该字形覆盖的源字符区间；下一个簇的起点是本簇的终点。
                let start = seg_start + cluster;
                let end = shaped
                    .get(i + 1)
                    .map(|(next, _)| seg_start + *next)
                    .unwrap_or(seg_end);
                let next_x = x + self.apply_spacing(*advance, 1, font);
                let (xi, next_i) = (x.round() as Twips, next_x.round() as Twips);
                out.push(GlyphPosition { start, end: end.max(start), x: xi, advance: next_i - xi });
                x = next_x;
            }
        }
        out
    }
}
