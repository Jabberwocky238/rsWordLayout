//! 断行位置。
//!
//! 断行规则（UAX #14）**与字体无关**，只和语言有关：西文按词断，中日文可在字间断，
//! 另有行首禁则。所以它不该跟着度量实现走——桩度量与真度量必须给出**同一套断点**。
//!
//! 这不是洁癖：换度量之后如果差值里混进了断行策略的变化，就分不出是哪一边错了。
//! 量具方法 §9.6 的三层配对里，行数一旦不同就是结构失败，连几何都量不到。

use super::spec::{BreakOpportunity, OverflowPunctuationContext};

/// 判断是否是「可在其后断行」的 CJK 字符（不含行首禁则处理）。
pub fn is_cjk(c: char) -> bool {
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
pub fn is_no_line_start(c: char) -> bool {
    matches!(c, '，' | '。' | '、' | '；' | '：' | '？' | '！' | '）' | '】' | '》' | '」' | '』'
                | ',' | '.' | ';' | ':' | '?' | '!' | ')' | ']' | '}' | '”' | '’')
}

/// Candidates for the observed single-punctuation overflow, as byte ranges.
/// Adjacent closing punctuation stays on the ordinary kinsoku path.
pub(super) fn overflow_punctuation_candidates(
    text: &str,
    context: OverflowPunctuationContext,
) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut previous = context.previous;
    let mut chars = text.char_indices().peekable();
    std::iter::from_fn(move || {
        while let Some((start, ch)) = chars.next() {
            let next = chars.peek().map(|&(_, c)| c).or(context.next);
            let candidate = matches!(ch, '\u{3002}' | '\u{ff0c}' | '\u{ff09}' | '\u{3001}')
                && previous.is_some_and(|c| is_cjk(c) && !is_no_line_start(c))
                && !next.is_some_and(is_no_line_start);
            previous = Some(ch);
            if candidate {
                return Some((start, start + ch.len_utf8()));
            }
        }
        None
    })
}

/// 文字的可断行位置，按偏移升序。
pub fn break_opportunities(text: &str) -> Vec<BreakOpportunity> {
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
