//! 内置的近似度量实现。
//!
//! **这不是排版级的度量**：它按字符类别给固定的推进宽度，不读字体文件、不做 shaping、
//! 不查 kerning。它存在的理由只有一个——让布局主干能在没有字体后端的环境里跑通并做回归测试。
//!
//! 真实使用应当实现 [`FontMetrics`] 接到 HarfBuzz / FreeType。两者的分工见 `measure` 模块：
//! 那里的 trait 是契约，这里只是一个够用的桩。
//!
//! 断行位置不在这里：它与字体无关，共用 [`crate::linebreak`]。
//!
//! 近似规则（相对字号的比例，经验值）：
//! - CJK 表意文字与全角标点：1.0 em（方块字）
//! - 西文字母数字：0.5 em
//! - 空格：0.25 em
//! - ascent 0.8 em、descent 0.2 em、line gap 0.15 em

use crate::geom::{Twips, half_points_to_twips};
use crate::linebreak::is_cjk;
use crate::measure::{BreakOpportunity, FontMetrics, FontSpec, TextMetrics};

pub struct SimpleMetrics;

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
}

impl FontMetrics for SimpleMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        let em = i64::from(half_points_to_twips(font.size_half_points));
        let mut permille: i64 = 0;
        let mut count: i64 = 0;
        for c in text.chars() {
            permille += Self::advance_permille(c);
            count += 1;
        }
        // 粗体略宽；这是桩实现的近似，真实度量由字体给。
        if font.bold {
            permille = permille * 1030 / 1000;
        }
        let mut advance = (em * permille / 1000) as Twips;
        // 横向缩放与字距。
        if font.scale_pct != 100 && font.scale_pct > 0 {
            advance = ((i64::from(advance) * i64::from(font.scale_pct)) / 100) as Twips;
        }
        advance += (count as Twips) * font.letter_spacing;

        TextMetrics {
            advance,
            ascent: (em * 800 / 1000) as Twips,
            descent: (em * 200 / 1000) as Twips,
            line_gap: (em * 150 / 1000) as Twips,
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        // 断点与字体无关，桩度量与真度量共用同一套（见 `crate::linebreak`）。
        crate::linebreak::break_opportunities(text)
    }
}
