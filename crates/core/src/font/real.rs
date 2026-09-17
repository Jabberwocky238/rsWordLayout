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

use skrifa::{FontRef, MetadataProvider};

use super::registry::FontRegistry;
use super::spec::{BreakOpportunity, FontMetrics, FontSpec, TextMetrics};
use crate::layout::{Twips, half_points_to_twips};

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

/// 字体声明的纵向量，字体单位。
#[derive(Debug, Clone, Copy)]
struct FaceVertical {
    upem: f64,
    ascent: f64,
    /// 正值。
    descent: f64,
    line_gap: f64,
}

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
    vertical: RefCell<BTreeMap<String, Option<FaceVertical>>>,
}

impl<'r> RealMetrics<'r> {
    pub fn new(registry: &'r FontRegistry) -> RealMetrics<'r> {
        RealMetrics { registry, grid: VerticalGrid::None, vertical: RefCell::new(BTreeMap::new()) }
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
            (upem > 0.0).then_some(FaceVertical {
                upem,
                ascent: f64::from(m.ascent),
                descent: f64::from(m.descent).abs(),
                line_gap: f64::from(m.leading),
            })
        })();
        self.vertical.borrow_mut().insert(face.to_string(), computed);
        computed
    }

    /// 本次请求用到的各 face 取最大，再按栅格量化。
    fn vertical_for(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let em = f64::from(half_points_to_twips(font.size_half_points));
        let mut ascent = 0.0f64;
        let mut descent = 0.0f64;
        let mut line_gap = 0.0f64;
        let mut found = false;

        // 空串也要给纵向量：空段落的行高由段落标记的字体决定。
        let probe = if text.is_empty() { "x" } else { text };
        let mut seen: Vec<String> = Vec::new();
        for ch in probe.chars() {
            let Some(face) = self.registry.select_face_for(font, ch) else { continue };
            if seen.contains(&face) {
                continue;
            }
            seen.push(face.clone());
            let Some(v) = self.face_vertical(&face) else { continue };
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
                let quantize = |v: f64| (v / step).round() * step;
                // 行高只取整**一次**，descent 单独取整，差额全部归 ascent。
                // 分别取整再相加会让和偏出栅格一整个 twip：
                // round(278.4 − 52.8) + round(52.8) = 279，而 round(278.4) = 278。
                // 行高要逐行累加下去，偏一个 twip 就会随页面往下漂。
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
    fn apply_spacing(&self, advance: i64, glyphs: usize, font: &FontSpec) -> Twips {
        let mut advance = advance;
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = advance * i64::from(font.scale_pct) / 100;
        }
        (advance + glyphs as i64 * i64::from(font.letter_spacing)) as Twips
    }
}

impl FontMetrics for RealMetrics<'_> {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let shaped = self.registry.shape_text(text, font);
        let advance: i64 = shaped.iter().map(|g| i64::from(g.x_advance)).sum();
        TextMetrics {
            advance: self.apply_spacing(advance, shaped.len(), font),
            ..self.vertical_for(text, font)
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        // 断点与字体无关，与桩共用同一套——否则换度量之后的差值里会混进
        // 断行策略的变化，分不出是哪一边错。
        super::linebreak::break_opportunities(text)
    }
}
