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

    fn advance_units(text: &str, font: &FontSpec) -> (i64, i64) {
        let mut permille = 0;
        let mut count = 0;
        for ch in text.chars() {
            permille += Self::advance_permille(ch);
            count += 1;
        }
        if font.bold {
            permille = permille * 1030 / 1000;
        }
        (permille, count)
    }
}

impl FontMetrics for SimpleMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let em = i128::from(font.effective_size_centipoints());
        let (permille, count) = Self::advance_units(text, font);
        // Five centipoints per twip; retain fractions until each legacy output.
        let mut advance = (em * i128::from(permille) / 5000) as Twips;
        // 横向缩放与字距。
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = ((i64::from(advance) * i64::from(font.scale_pct)) / 100) as Twips;
        }
        advance += (count as Twips) * font.letter_spacing;

        TextMetrics {
            advance,
            ascent: (em * 800 / 5000) as Twips,
            descent: (em * 200 / 5000) as Twips,
            line_gap: (em * 150 / 5000) as Twips,
        }
    }

    fn advance_pt(&self, text: &str, font: &FontSpec) -> f64 {
        let (permille, count) = Self::advance_units(text, font);
        let mut advance = font.size_pt() * permille as f64 / 1000.0;
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance *= f64::from(font.scale_pct) / 100.0;
        }
        advance + count as f64 * f64::from(font.letter_spacing) / 20.0
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
