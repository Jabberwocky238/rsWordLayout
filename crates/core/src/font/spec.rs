//! 文字度量抽象。
//!
//! 布局要算断行就必须先知道文字多宽，而宽度来自字体文件。这一层把「取度量」抽成 trait，
//! 有两个后果：
//!
//! 1. 布局主干不依赖任何具体字体库（HarfBuzz / FreeType / 系统 API 都能接）；
//! 2. **所有 canvas 后端共用同一份度量**——否则同一份文档 PDF 后端分 10 页、
//!    HTML 后端分 11 页，预览就失去意义。所以度量不挂在 `Canvas` 上，是独立输入。
//!
//! shaping（连字、kerning、复杂文种重排）属于实现者的职责：它是由字体 GSUB/GPOS 表
//! 决定的确定性查表，不是布局要解的约束。实现者通常直接转调 HarfBuzz。

use crate::layout::Twips;

/// 一次度量请求的字体条件。
///
/// 只放**影响度量**的字段。颜色、下划线、高亮不影响 advance，不在这里。
#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    /// 字体名。**这是 `ascii` 槽的值**，保留它是为了兼容只认单一字体的调用方；
    /// 真正按字符选字体要走 [`FontSpec::family_for`]。
    pub family: String,
    /// 四个字体槽（`w:rFonts`）。
    ///
    /// Word 不是「一个 run 一个字体」：同一 run 里每个字符按它属于哪个区，
    /// 去查对应的槽。所以「宋体 + Calibri」的中英混排是一个 run 就能表达的。
    /// 见 ECMA-376 §17.3.2.26。
    pub slots: FontSlots,
    /// 字号，半点（`w:sz`）。
    pub size_half_points: u32,
    pub bold: bool,
    pub italic: bool,
    /// `w:spacing`：字符间距调整，twips，可负。
    pub letter_spacing: Twips,
    /// `w:w`：横向缩放百分比，100 为原始。
    pub scale_pct: u32,
    /// 是否启用字距调整（GPOS `kern`）。
    ///
    /// **默认关**，这不是保守取值，是 OOXML 的语义：`w:kern` 给的是
    /// 「字号大到多少才启用字距调整」，不写或写 0 就是**不调整**。
    ///
    /// 实测对得上：Liberation Serif 的 `1`+`1` 有一对 kern，rustybuzz 默认会用上，
    /// 而 Word **没有**——`B11` 一行里两个 `1` 各 6.0000pt，该行从第二个字形起
    /// 整体偏 0.45pt。rustybuzz 不传 feature 时默认**开**，所以必须显式关掉，
    /// 不能靠不传。
    pub kerning: bool,
}

/// `w:rFonts` 的四个字体槽。
///
/// **这是 Word 选字体的真实规则，不是 fallback。** fallback 是「这个字体画不出
/// 这个字，换一个能画的」；Word 是「这个字符属于 CJK 区，所以用 eastAsia 槽指定
/// 的字体」——即使 ascii 槽的字体也能画出那个字。两者选出的字体常常不同。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FontSlots {
    /// U+0000–U+007F。
    pub ascii: Option<String>,
    /// 高位拉丁、西里尔、希腊等「高 ANSI」区。
    pub h_ansi: Option<String>,
    /// CJK、假名、谚文。
    pub east_asia: Option<String>,
    /// 复杂文种（阿拉伯、希伯来、印度系）。
    pub cs: Option<String>,
    /// `w:hint`：歧义字符（全角标点、某些符号）走哪个槽。
    ///
    /// 这就是为什么 Word 文档里的破折号有时呈中文样式——`hint="eastAsia"`
    /// 把它划给了中文字体。
    pub hint: FontHint,
}

/// `w:hint` 的取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontHint {
    #[default]
    Default,
    EastAsia,
    Cs,
}

/// 字符所属的字体槽。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Ascii,
    HAnsi,
    EastAsia,
    Cs,
}

impl FontSlots {
    /// 一个字符该走哪个槽。
    ///
    /// 区间划分依 ECMA-376 §17.3.2.26。歧义区（如全角标点）由 `hint` 决定，
    /// 这也是 Word 里同一个符号在中英文段落中样式不同的原因。
    pub fn slot_for(&self, ch: char) -> SlotKind {
        let c = ch as u32;
        // 复杂文种：阿拉伯、希伯来、印度系、泰文等。
        let complex = matches!(c,
            0x0590..=0x074F   // 希伯来、阿拉伯、叙利亚
            | 0x0780..=0x07BF // 他拿
            | 0x0900..=0x0DFF // 印度系
            | 0x0E00..=0x0E7F // 泰文
            | 0x1780..=0x17FF // 高棉
            | 0xFB1D..=0xFDFF // 阿拉伯呈现形式
            | 0xFE70..=0xFEFF
        );
        if complex {
            return SlotKind::Cs;
        }
        let east_asian = matches!(c,
            0x1100..=0x11FF   // 谚文字母
            | 0x2E80..=0x2EFF // 部首补充
            | 0x3000..=0x303F // CJK 符号与标点
            | 0x3040..=0x30FF // 假名
            | 0x3100..=0x312F // 注音
            | 0x3130..=0x318F // 谚文兼容
            | 0x3400..=0x4DBF // 扩展 A
            | 0x4E00..=0x9FFF // 基本区
            | 0xA000..=0xA4CF // 彝文
            | 0xAC00..=0xD7AF // 谚文音节
            | 0xF900..=0xFAFF // 兼容表意
            | 0xFF00..=0xFFEF // 全角与半角形式
            | 0x20000..=0x2FA1F
        );
        if east_asian {
            return SlotKind::EastAsia;
        }
        // 歧义区：ASCII 以外的拉丁与通用标点，由 hint 决定。
        if c > 0x007F {
            return match self.hint {
                FontHint::EastAsia => SlotKind::EastAsia,
                FontHint::Cs => SlotKind::Cs,
                FontHint::Default => SlotKind::HAnsi,
            };
        }
        // ASCII 区同样受 hint 影响：hint="eastAsia" 时 Word 用中文字体排西文。
        match self.hint {
            FontHint::EastAsia => SlotKind::EastAsia,
            _ => SlotKind::Ascii,
        }
    }

    /// 取某个槽的字体名。
    pub fn get(&self, slot: SlotKind) -> Option<&str> {
        match slot {
            SlotKind::Ascii => self.ascii.as_deref(),
            SlotKind::HAnsi => self.h_ansi.as_deref(),
            SlotKind::EastAsia => self.east_asia.as_deref(),
            SlotKind::Cs => self.cs.as_deref(),
        }
    }

    /// 未指定的槽从 `other` 继承（样式链与 docDefaults 都靠它）。
    pub fn inherit(&mut self, other: &FontSlots) {
        if self.ascii.is_none() {
            self.ascii = other.ascii.clone();
        }
        if self.h_ansi.is_none() {
            self.h_ansi = other.h_ansi.clone();
        }
        if self.east_asia.is_none() {
            self.east_asia = other.east_asia.clone();
        }
        if self.cs.is_none() {
            self.cs = other.cs.clone();
        }
        if self.hint == FontHint::Default {
            self.hint = other.hint;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ascii.is_none()
            && self.h_ansi.is_none()
            && self.east_asia.is_none()
            && self.cs.is_none()
    }
}

impl FontSpec {
    /// 按 Word 的规则为一个字符选字体名。
    ///
    /// 槽里没写就退到 `family`——那是 `ascii` 槽的值，也是最接近「文档默认」的东西。
    /// 注意这**不是 fallback**：fallback 发生在更后面，即选定的字体也画不出该字符时。
    pub fn family_for(&self, ch: char) -> &str {
        self.slots
            .get(self.slots.slot_for(ch))
            .unwrap_or(&self.family)
    }

    /// 默认西文正文：五号宋体的常见等价物由调用方给，这里只兜字号。
    pub fn new(family: impl Into<String>, size_half_points: u32) -> FontSpec {
        let family = family.into();
        FontSpec {
            slots: FontSlots { ascii: Some(family.clone()), ..FontSlots::default() },
            family,
            size_half_points,
            bold: false,
            italic: false,
            letter_spacing: 0,
            scale_pct: 100,
            kerning: false,
        }
    }
}

/// 1 twip 等于多少个「精细单位」。
///
/// 精细单位是 **1/7200 英寸**：它是 twips（1/1440 英寸）与 Word 纵向栅格
/// （1/300 英寸）的公倍数——两者只在这里通约。取它做行高累加的内部单位，
/// 就不会因为栅格点落不到整 twips 上而逐行漂移。
pub const FINE_PER_TWIP: i64 = 5;

/// 一段同字体文字的度量结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct TextMetrics {
    /// 排版推进量（含字距与缩放后的值）。
    pub advance: Twips,
    /// 基线以上高度。
    pub ascent: Twips,
    /// 基线以下高度（正值）。
    pub descent: Twips,
    /// 建议行距（字体推荐的 line gap），不含 `w:spacing` 的段落行距设置。
    pub line_gap: Twips,
}

impl TextMetrics {
    /// 字体自身的自然行高。
    pub fn natural_height(&self) -> Twips {
        self.ascent + self.descent + self.line_gap
    }
}

/// 断行候选位置：可以断开的字符边界。
///
/// 断行规则（UAX #14）与字体无关，但和语言有关（中日文可在字间断，西文按词），
/// 所以和度量放在同一个 trait 里由实现者一并提供。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct BreakOpportunity {
    /// 断点在原串中的字节偏移。
    pub offset: usize,
    /// 断开后前半段是否要保留一个连字符（西文 hyphenation）。
    pub hyphen: bool,
}

/// 度量提供者。
///
/// 实现者需保证**同一输入给出同一输出**：布局会对同一段文字反复试宽（断行二分），
/// 结果不稳定会导致分页抖动。
pub trait FontMetrics {
    /// 整段文字的推进宽度与纵向度量。
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics;

    /// 在给定宽度内最多能放下多少字节，返回该前缀的字节长度与其度量。
    ///
    /// 返回 `None` 表示一个字符都放不下——调用方据此决定是硬塞还是换行。
    /// 默认实现按断行点线性试探；实现者若能直接问 shaper 要 cluster 边界，应覆盖它。
    fn fit(&self, text: &str, font: &FontSpec, max_width: Twips) -> Option<(usize, TextMetrics)> {
        let mut best: Option<(usize, TextMetrics)> = None;
        for op in self.break_opportunities(text) {
            if op.offset == 0 {
                continue;
            }
            let m = self.measure(&text[..op.offset], font);
            if m.advance > max_width {
                break;
            }
            best = Some((op.offset, m));
        }
        best
    }

    /// 文字的可断行位置，按偏移升序。
    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity>;

    /// 整段文字的推进宽度的**精确值**，单位**点**。
    ///
    /// 横向为什么不学纵向走 1/7200 英寸的整数：**横向没有栅格**。
    /// 纵向能用定点，是因为 Word 的基线落在 1/300 英寸上，而 1/7200 英寸是它与
    /// twips 的公倍数——那是个能整除的目标。横向不是：实测 Word 导出的 PDF 里，
    /// 字形推进量落在 **1/10000 em** 上（`accumulation` 那份采集里 761/1100 对
    /// 相邻字形的间距是 1/10000 em 的整数倍），而 em 随字号变，没有哪个固定的
    /// 绝对单位能整除它。既然无论取多细的定点都要留残差，就不取——
    /// 把精确值一路留在 f64 里，只在最后出数时落一次。
    ///
    /// 存在的理由与 [`Self::natural_height_fine`] 同源：**行宽会沿行累加**。
    /// `measure` 给的 `advance` 是整 twips（0.05pt），逐片段相加的残差会一路右移。
    /// 实测引擎 1340/1340 个推进量落在整 twips 上、Word **0/1340**，
    /// 行内累积到 0.04pt（`docs/MEASUREMENT-BACKLOG.md` 的 H2）。
    ///
    /// 默认实现由 `measure` 的 twips 值换算，**不提供额外精度**——
    /// 对不做取整的实现（如 [`crate::SimpleMetrics`]）这就是精确值。
    fn advance_pt(&self, text: &str, font: &FontSpec) -> f64 {
        f64::from(self.measure(text, font).advance) / 20.0
    }

    /// 自然行高的**精确值**，单位 1/7200 英寸（= twips × [`FINE_PER_TWIP`]）。
    ///
    /// 存在的理由是**行高会沿页累加**：做纵向量化的实现里，栅格上的精确行高
    /// 常常落不到整 twips 上（Mac Word 的 1/300 英寸栅格在 12pt 下是 273.6 twips，
    /// 取整成 274，每行多 0.4 twip）。游标按取整值累加，一页 40 行就攒到 0.8pt。
    ///
    /// 默认实现由 `measure` 的 twips 值换算，**不提供额外精度**——
    /// 对不做量化的实现（如 [`crate::SimpleMetrics`]）这就是精确值。
    /// **做量化的实现应当覆盖它**，并且不要在里面做整形：行高只取决于字体的纵向量，
    /// 覆盖版应当比 `measure` 便宜。
    fn natural_height_fine(&self, text: &str, font: &FontSpec) -> i64 {
        i64::from(self.measure(text, font).natural_height()) * FINE_PER_TWIP
    }

    /// 把一条基线的纵向位置量化到本度量的栅格，单位 1/7200 英寸。
    ///
    /// **这是已确立的观测**，不是猜测：Word for Mac 把每条基线放在
    /// 1/300 英寸（0.24pt）的栅格上，三份互相独立的夹具、1080 条基线、
    /// **零例外**（见 `docs/PREREG-2026-09-17-*.md` 的 Q0 / R0）。
    ///
    /// 为什么非得在 1/7200 英寸上做：布局的 `Twips` 是 1/1440 英寸，
    /// 而 0.24pt = **4.8 twips**——栅格点根本落不到整 twips 上，
    /// 两者只在 1/7200 英寸上通约（0.24pt = 24 个单位，整数）。
    /// 所以落位若走 twips，引擎的基线**在结构上就不可能**落到 Word 的栅格上。
    ///
    /// 默认恒等：不做量化的实现（如 [`crate::SimpleMetrics`]）没有栅格可言。
    /// **做量化的实现应当覆盖它。**
    fn quantize_baseline_fine(&self, y_fine: i64) -> i64 {
        y_fine
    }

    /// 空行高度：没有任何文字时，行高取决于段落标记的字体。
    fn empty_line_metrics(&self, font: &FontSpec) -> TextMetrics {
        self.measure("", font)
    }
}
