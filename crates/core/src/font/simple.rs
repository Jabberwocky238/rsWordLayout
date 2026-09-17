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

use crate::layout::{Twips, half_points_to_twips};
use super::{BreakOpportunity, FontMetrics, FontSpec, TextMetrics};

pub struct SimpleMetrics;

/// 判断是否是「可在其后断行」的 CJK 字符（不含行首禁则处理）。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF   | // 谚文字母
        0x2E80..=0x2EFF   | // 部首补充
        0x3000..=0x303F   | // CJK 符号与标点
        0x3040..=0x30FF   | // 假名
        0x3400..=0x4DBF   | // 扩展 A
        0x4E00..=0x9FFF   | // 基本区
        0xAC00..=0xD7AF   | // 谚文音节
        0xF900..=0xFAFF   | // 兼容表意
        0xFF00..=0xFF60   | // 全角形式
        0x20000..=0x2FA1F   // 扩展 B 及以后
    )
}

/// 行首禁则：这些字符不能出现在行首，断点要往前挪。
fn is_no_line_start(c: char) -> bool {
    matches!(c, '，' | '。' | '、' | '；' | '：' | '？' | '！' | '）' | '】' | '》' | '」' | '』'
                | ',' | '.' | ';' | ':' | '?' | '!' | ')' | ']' | '}' | '”' | '’')
}

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
        let mut out = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let next_start = i + c.len_utf8();
            let next_char = chars.peek().map(|&(_, n)| n);
            // 西文：空格之后可断。
            let after_space = c == ' ' || c == '\t';
            // CJK：字与字之间可断，但下一个字是行首禁则字符时不断。
            let cjk_boundary = is_cjk(c)
                && next_char.is_some_and(|n| !is_no_line_start(n));
            // CJK 之前是西文、之后是 CJK 的边界也可断。
            let enter_cjk = next_char.is_some_and(is_cjk) && !is_cjk(c) && !after_space;

            if after_space || cjk_boundary || enter_cjk {
                out.push(BreakOpportunity { offset: next_start, hyphen: false });
            }
        }
        // 串尾总是一个合法断点。
        if !text.is_empty() {
            let end = text.len();
            if out.last().map(|b| b.offset) != Some(end) {
                out.push(BreakOpportunity { offset: end, hyphen: false });
            }
        }
        out
    }
}
