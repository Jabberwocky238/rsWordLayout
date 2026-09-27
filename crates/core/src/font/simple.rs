//! 内置的近似度量实现。
//!
//! **这不是排版级的度量**：它按字符类别给固定的推进宽度，不读字体文件、不做 shaping、
//! 不查 kerning。它存在的理由只有一个——让布局主干能在没有字体后端的环境里跑通并做回归测试。
//!
//! 真实使用应当实现 [`FontMetrics`] 接到 HarfBuzz / FreeType。两者的分工见 `measure` 模块：
//! 那里的 trait 是契约，这里只是一个够用的桩。
//!
//! 近似规则（相对字号的比例，经验值）：
//! - CJK 表意文字与全角标点：1.0 em（方块字）
//! - 西文字母数字：0.5 em
//! - 空格：0.25 em
//! - ascent 0.8 em、descent 0.2 em、line gap 0.15 em

use super::linebreak::is_cjk;
use super::{BreakOpportunity, FontMetrics, FontSpec, TextMetrics};
use crate::layout::Twips;

pub struct SimpleMetrics;

/// 判断是否是「可在其后断行」的 CJK 字符（不含行首禁则处理）。
impl SimpleMetrics {
    /// 单个字符的推进宽度（em 的千分比，避免浮点累积误差）。
    fn advance_permille(c: char) -> i64 {
        if c == ' ' || c == '\t' {
            250
        } else if is_cjk(c) {
            1000
        } else {
            500
        }
    }

    /// （全字号部分的 em 千分比合计，缩小字号部分的 em 千分比合计，字符间距的位置数）。
    ///
    /// 宽度按显示字符数（[`super::caps::display_chars`]），与真度量同一个展开：
    /// `w:caps` 下 `a` 按 `A` 量；`w:smallCaps` 下有大写形式的字符落到缩小的那个字号。
    /// 分两个桶而不是逐字符乘字号，是为了不做大小写变换时与原来的整数运算逐位相同。
    ///
    /// 间距的位置数按**源字符簇**数（`linebreak::cluster_boundaries`），不按 `char`，
    /// 也不按显示字符：`RealMetrics` 按整形 cluster 计，组合符号并进基字，桩也得跳过它们，
    /// 否则 `x` + U+0301 在桩里加两次、真度量里加一次；大小写变换不改源字符，也就不改位置数
    /// （将来换成一对多的映射，`ß` → `SS` 两个字形共用一个源区间，真度量那边同样只数一次）。
    /// 同一张表也是紧急断行的切口，于是切开的两段间距位置数之和等于整段——宽度可加。
    /// 按 cluster 计本身是假定（见 `RealMetrics::apply_spacing`）。
    fn advance_units(text: &str, font: &FontSpec) -> (i64, i64, i64) {
        let full = font.effective_size_centipoints();
        let mut permille = 0;
        let mut reduced = 0;
        for d in super::caps::display_chars(text, font) {
            if d.size_centipoints == full {
                permille += Self::advance_permille(d.ch);
            } else {
                reduced += Self::advance_permille(d.ch);
            }
        }
        if font.bold {
            permille = permille * 1030 / 1000;
            reduced = reduced * 1030 / 1000;
        }
        let slots = if font.letter_spacing == 0 {
            0
        } else {
            super::linebreak::cluster_boundaries(text).count() as i64
        };
        (permille, reduced, slots)
    }
}

impl FontMetrics for SimpleMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let em = i128::from(font.effective_size_centipoints());
        let small = i128::from(super::caps::small_caps_size_centipoints(font.effective_size_centipoints()));
        let (permille, reduced, slots) = Self::advance_units(text, font);
        // Five centipoints per twip; retain fractions until each legacy output.
        let mut advance = ((em * i128::from(permille) + small * i128::from(reduced)) / 5000) as Twips;
        // 横向缩放与字距。字距每个源字符簇一次（见 `advance_units`），不随缩放。
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = ((i64::from(advance) * i64::from(font.scale_pct)) / 100) as Twips;
        }
        advance += (slots as Twips) * font.letter_spacing;
        if font.auto_space_dn {
            advance += super::linebreak::autospace_dn_twips(text, font.effective_size_centipoints());
        }

        TextMetrics {
            advance,
            ascent: (em * 800 / 5000) as Twips,
            descent: (em * 200 / 5000) as Twips,
            line_gap: (em * 150 / 5000) as Twips,
        }
    }

    fn advance_pt(&self, text: &str, font: &FontSpec) -> f64 {
        let (permille, reduced, slots) = Self::advance_units(text, font);
        let mut advance = font.size_pt() * permille as f64 / 1000.0;
        if reduced != 0 {
            let small = super::caps::small_caps_size_centipoints(font.effective_size_centipoints());
            advance += small as f64 / 100.0 * reduced as f64 / 1000.0;
        }
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance *= f64::from(font.scale_pct) / 100.0;
        }
        advance += slots as f64 * f64::from(font.letter_spacing) / 20.0;
        if font.auto_space_dn {
            advance += super::linebreak::autospace_dn_pt(text, font.size_pt());
        }
        advance
    }

    fn natural_height_fine(&self, _text: &str, font: &FontSpec) -> i64 {
        // A fine unit is 1/100 point; round the whole height only once.
        let height = (u128::from(font.effective_size_centipoints()) * 115 + 50) / 100;
        height.min(i64::MAX as u128) as i64
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        // 断点与字体无关，桩度量与真度量共用同一套（见 `super::linebreak`）。
        super::linebreak::break_opportunities(text)
    }
}
