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

fn is_ideograph(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

/// `autoSpaceDN` boundaries: an ideograph next to an ASCII digit, either order.
///
/// CJK punctuation is not an ideograph. Including it moves the observed
/// `breakme` break one character too early.
pub fn ideograph_numeric_boundaries(text: &str) -> u32 {
    let mut count = 0u32;
    let mut prev = None;
    for ch in text.chars() {
        if let Some(p) = prev {
            let boundary = (is_ideograph(p) && ch.is_ascii_digit())
                || (p.is_ascii_digit() && is_ideograph(ch));
            if boundary {
                count += 1;
            }
        }
        prev = Some(ch);
    }
    count
}

/// Quarter em, in twips. CSS `text-autospace: ideograph-numeric` and Word's
/// default `autoSpaceDN` both use a quarter of the ideographic em.
pub fn autospace_dn_twips(text: &str, size_centipoints: u64) -> i32 {
    let n = ideograph_numeric_boundaries(text);
    if n == 0 {
        return 0;
    }
    // Keep the existing per-boundary truncation, using the authoritative size.
    let quarter = u128::from(size_centipoints) / 20;
    (u128::from(n) * quarter).min(i32::MAX as u128) as i32
}

pub fn autospace_dn_pt(text: &str, size_pt: f64) -> f64 {
    ideograph_numeric_boundaries(text) as f64 * size_pt / 4.0
}

#[cfg(test)]
mod autospace_dn_tests {
    use super::{autospace_dn_twips, ideograph_numeric_boundaries};

    #[test]
    fn counts_ideograph_digit_pairs_only() {
        // 汉0汉 has two boundaries; 、 and （ are punctuation, not ideographs.
        assert_eq!(ideograph_numeric_boundaries("汉0汉"), 2);
        assert_eq!(ideograph_numeric_boundaries("2、汉3。"), 1);
        assert_eq!(ideograph_numeric_boundaries("5（汉"), 0);
        assert_eq!(ideograph_numeric_boundaries("A0 B0"), 0);
    }

    #[test]
    fn autospace_retains_each_boundary_truncation_at_exact_and_legacy_sizes() {
        assert_eq!(autospace_dn_twips("\u{4e2d}0\u{4e2d}", 792), 2 * 39);
        assert_eq!(autospace_dn_twips("\u{4e2d}0\u{4e2d}", 850), 2 * 42);
        assert_eq!(autospace_dn_twips("\u{4e2d}0\u{4e2d}", 0), 0);
    }

    #[test]
    fn autospace_large_sizes_do_not_wrap_or_overflow_before_the_twip_boundary() {
        assert_eq!(autospace_dn_twips("\u{4e2d}0\u{4e2d}", u64::MAX), i32::MAX);
        assert_eq!(autospace_dn_twips("A0", u64::MAX), 0);
    }
}

/// 行首禁则：这些字符不能出现在行首，断点要往前挪。
///
/// 依据分三档，每个字符归哪一档写在下面：
///
/// 1. **输出层实测**：
///    - Android Word（word_analyse `reports/rsword-diff/kinsoku.md`，窄路径 5329 twips）：
///      越界的 `）`、`)`、`。`、`））` 都不留在行首，断点退回前一个合法断点；
///    - Mac Word（`docs/PREREG-2026-09-18-kinsoku2.md`，显式关掉 `w:overflowPunct`）：
///      `。，）、` 四个都不留在行首。
/// 2. **类读数 + 推断的规则**：word_analyse `reports/brkcls-map.csv`（`LserrGetBreakingClasses`
///    = `0xb4d4c0` 的运行时读数，350 个码位）里 `brkclsFollowing = 1` 的字符。读数本身是实测，
///    「`foll = 1` 就不在其前断」这条规则**是推断**：`findings/brkcls.md` 明说那张配对表暂缓、
///    不作为断行规则的定义，运行时也从没见到受限的配对真正触发过。原有集合里除上一档外的
///    字符也都在这一列里（ASCII `?` 除外，见下）。
/// 3. **外推**：同一族括号的另一半，读数里没有采到，照括号成对补上。不是 Word 的哪一张表——
///    两个仓库里都没有 Word 默认禁则表可对。
///
/// 读数有**简体中文偏向**：夹具都没写 `w:lang`，量到的是那台手机的默认设置；`·` 前后都禁断
/// 看上去就是简体中文的规矩。用到日文、韩文（小假名、`ー`、`々`）上是否成立未测。
///
/// **ASCII `?` 与 `%` 不对称**：读数里两者同是 `leading = 2`、`following = 3`（与 `-`、
/// U+2010–2014 同属一组硬编码的类），照读数汉字后可在它们前面断。这里 `%` 照读数（不在此列），
/// `?` 却**仍按行首禁则**——那是本引擎原有的行为，没有 Android 或 Mac 的输出层读数说明该改
/// （Mac 的 `cjk-plain` 采集用的是全角 `？`）。整组 2/3 大概是别处按上下文另行处理的，未查。
pub fn is_no_line_start(c: char) -> bool {
    matches!(c,
        // 原有集合。其中 `）`、`)`、`。`（Android）与 `。，）、`（Mac）是第 1 档，
        // 其余是第 2 档；`?` 是上面说的例外。
        '，' | '。' | '、' | '；' | '：' | '？' | '！' | '）' | '】' | '》' | '」' | '』'
        | ',' | '.' | ';' | ':' | '?' | '!' | ')' | ']' | '}' | '”' | '’'
        // 第 2 档：brkcls-map.csv 里 `following = 1`、原先漏掉的。
        | '〉' | '＂' | '％' | '＇' | '］' | '｀' | '｜' | '｝' | '～' | '．'
        | '>' | '¢' | '¨' | '°' | '·' | '―' | '…' | '′' | '″'
        // 第 3 档（外推）：〔〕、〖〗、〝〞 与小型括号、全角 ￠。
        // 〘〙〚〛（U+3018–301B）没有来源，不补。
        | '〕' | '〗' | '〞' | '﹚' | '﹜' | '﹞' | '￠')
}

/// 行尾禁则：这些字符不能收在行尾，断点不能落在它们后面。
///
/// 依据同 [`is_no_line_start`] 的三档：
///
/// 1. **输出层实测**（Android Word，word_analyse `reports/rsword-diff/kinsoku.md`，窄路径
///    5329 twips）：21 个「汉」后面放得下的 `（` 或 ASCII `(` 不留在行尾，断在 21，
///    下一行以它开头。Mac 上没有一份采集让开括号落到行尾，未测。
/// 2. **类读数 + 推断的规则**：`reports/brkcls-map.csv` 里 `brkclsLeading = 0` 的其余字符。
///    「`lead = 0` 就不在其后断」同样是推断，见 [`is_no_line_start`]。
/// 3. **外推**：同一族括号的另一半，读数里没有。
///
/// 注意 `｟`（U+FF5F）在读数里是 2/2，**不在**此列：不是所有开括号都行尾禁则。
/// `·`、`．` 两个列表里都有（读数 0/1），它们前后都不断。
pub fn is_no_line_end(c: char) -> bool {
    matches!(c,
        // 第 1 档：`（`、`(`。其余是第 2 档：brkcls-map.csv 里 `leading = 0`。
        '(' | '[' | '{' | '$' | '£' | '¥' | '·' | '‘' | '“'
        | '〈' | '《' | '「' | '『' | '【'
        | '（' | '［' | '｛' | '＄' | '．'
        // 第 3 档（外推）：〔〕、〖〗、〝〞 与小型括号、全角 ￡￥。
        // 〘〙〚〛（U+3018–301B）没有来源，不补。
        | '〔' | '〖' | '〝' | '﹙' | '﹛' | '﹝' | '￡' | '￥')
}

/// Candidates for the observed single-punctuation overflow, as byte ranges.
/// Adjacent closing punctuation stays on the ordinary kinsoku path.
///
/// 前后两个字的判断读的是 [`is_no_line_start`]，所以那张表扩充之后 Mac（桌面，库的默认）的
/// 挂出也跟着变，前后两侧都变，**Mac 上都未测**——Mac 的挂出只量过「汉 + 标点 + 汉」
/// （`docs/PREREG-2026-09-18-kinsoku.md`），前后都是汉字：
///
/// - 后一个字：`。〉`、`，…` 这样后面紧跟新增的行首禁则字符时不再挂出，走普通断行（22 汉
///   后接它们原先挂出到 23，现在退回 21）。原先 `〉`、`…` 会落到下一行行首，现在的结果
///   看上去更像 Word。
/// - 前一个字：新增的行首禁则字符里属于 [`is_cjk`] 的 13 个（`〉〕〗〞＂％＇．］｀｜｝～`）
///   后面越界的 `。`、`，` 等不再挂出。「前一个字是收尾标点就不挂」是原有的假设（连着两个
///   收尾标点不挂出，`tests/overflow_punctuation.rs` 的
///   `consecutive_closing_punctuation_does_not_gain_overflow`），表扩充后这 13 个也算收尾标点。
///   走普通断行时 `汉|％`、`％|。` 都不断，于是退**三**个字：`21汉％。10汉` 原先挂出到 23，
///   现在 20。`％。`、`～。` 在中文里不算少见，这一侧的影响比后一侧大。
///   只拿原先那张表查前一个字、让 `％。` 照旧挂出，同样说得通，没有采集分得出哪个对；这里照
///   一张表走，钉在 `tests/kinsoku_android.rs` 的
///   `desktop_no_longer_hangs_after_a_newly_restricted_character`。
///   待测（Mac）：`21汉％。10汉`、`22汉。〉10汉`。
///
/// 前后两个字可以来自相邻的 run（`OverflowPunctuationContext`）。所以上面两侧的序列拆在两个
/// run 里时挂出同样被挡住；排版层的跨 run 回退（`layout.rs` 里 `Engine::shortfall`）退回行里
/// 更早的断点，`[21汉％][。10汉]`、`[22汉][。〉10汉]` 与一个 run 里一样是 20、21，`。` 不开下一行。
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

/// 紧急断行时不能从前一个字符拆开的字符：组合附加符号、变体选择符、肤色修饰符等。
///
/// **这一张表是假设，不是实测**：Word 的紧急断行只量过单一 BMP 字母（`0`、`a`、`i`、
/// `M`……），从没切到过组合序列。按 UAX #29 的 Extend 取常见的几段，
/// 宁可少切也不把附加符号甩到下一行行首。
///
/// 表是**薄**的：泰文 SARA AM（U+0E33）、老挝文 AM（U+0EB3）、高棉文与孟加拉、泰米尔等
/// 印度系的元音符号（GB9a 的 SpacingMark）都不在里面，切在它们前面会把符号甩到下一行行首。
/// 所以它只给没有整形器的 [`crate::SimpleMetrics`] 兜底；真度量按整形器的 cluster 切
/// （`RealMetrics::fit_clusters`）。另有一处已知不一致：谚文字母 0x1100–0x11FF 在
/// [`is_cjk`] 里逐字可断，这里却算一簇——普通断行与紧急断行对它们的切法不同。
fn is_cluster_extend(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F   // 组合附加符号
        | 0x0483..=0x0489 // 西里尔组合符号
        | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7 // 希伯来点
        | 0x064B..=0x065F | 0x0670 // 阿拉伯元音符号
        | 0x0900..=0x0903 | 0x093A..=0x093C | 0x093E..=0x094F | 0x0951..=0x0957 | 0x0962..=0x0963 // 天城文
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E // 泰文上下元音与声调
        | 0x1160..=0x11FF // 谚文中声、终声字母（接在初声后成一个音节）
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
        | 0x3099..=0x309A // 假名浊音、半浊音组合符
        | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF // 变体选择符
        | 0x1F3FB..=0x1F3FF // 肤色修饰符
        | 0xE0020..=0xE007F // 标签字符（旗帜序列）
        | 0x200D          // 零宽连接符本身挂在前一个字符上
    )
}

fn is_regional_indicator(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// 紧急断行可切的位置：每个字符簇的终点，字节偏移，升序，含串尾。
///
/// 切的单位是「字符」而不是 UTF-16 码元：代理对由 `char` 天然保住，
/// 组合序列、零宽连接符连起的序列、成对的区域指示符各算一簇。
/// 这是 UAX #29 的近似，不是完整实现（见 [`is_cluster_extend`] 的限定）。
///
/// `pub(crate)`：[`super::FontMetrics::fit_clusters`] 的默认实现用它，
/// 需要「源字符归哪个基字」的其它地方（小型大写的字号、字距的计数）也该用同一张表。
pub(crate) fn cluster_boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    let mut chars = text.char_indices().peekable();
    std::iter::from_fn(move || {
        let (_, first) = chars.next()?;
        let mut prev = first;
        let mut regional = usize::from(is_regional_indicator(first));
        while let Some(&(offset, c)) = chars.peek() {
            let joins = is_cluster_extend(c)
                || prev == '\u{200D}'
                || (regional % 2 == 1 && is_regional_indicator(c));
            if !joins {
                return Some(offset);
            }
            if is_regional_indicator(c) {
                regional += 1;
            }
            prev = c;
            chars.next();
        }
        Some(text.len())
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
            let after_space = c == ' ';
            // 制表符：断点在它**之前**，不在之后——制表符与后面的字粘在一起。
            // 实测（Android，`tab.md`）：`tab-right-1440` 的 `A<TAB>000…` 排成 `A` / `<TAB>000…`
            // （0、1、45，第二行的 `0` 得从比 `A` 的宽还小的 x 起，所以制表符之前确有换行）；
            // `tab-after-a`、`tab-stop-720`、`tab-stop-1440` 的起点与之一致。排版层把制表符
            // 切成独立片段、自己处理这条（见 `layout.rs`）。
            // 连续两个制表符之间可断是**假设**。
            // 制表符之前也守行尾禁则（`（<TAB>` 之间不断）是**假设**：没有夹具把开括号放在
            // 制表符前面，只是让同一条禁则对每种边界都成立。排版层挪粘着的制表符时照这一条查
            // 制表符之前那处交界（`layout.rs` 里 `Engine::shortfall` 的（丙）），`（` 跟着制表符
            // 下去：`20汉（<TAB>Sincerely` @5329 退到 `汉|（`，20，默认档 221 与 720 都是
            // （`tab_stops.rs` 的 `assumed_an_opening_bracket_goes_down_with_its_glued_tab…`）。
            // 例外：制表符到了下一行仍然够不着（左对齐停靠点在行外），`（` 留在行尾，是**已知偏差**
            // （`known_deviation_an_opening_bracket_stays_before_a_tab_that_cannot_land_on_the_next_line`）。
            let before_tab = next_char == Some('\t') && !is_no_line_end(c);
            // 禁则：前一个字行尾禁则、后一个字行首禁则，都不在此断。下面两种边界都查它。
            // 实测（Android，`kinsoku.md`）：`汉|）`、`汉|)`、`汉|。` 不断（越界时退回 21），
            // `（|汉` 不断（21 个汉字后放得下的 `（` 也挪到下一行）。
            let kinsoku_ok =
                !is_no_line_end(c) && next_char.is_some_and(|n| !is_no_line_start(n));
            // CJK：字与字之间可断。
            let cjk_boundary = is_cjk(c) && kinsoku_ok;
            // CJK 之前是西文、之后是 CJK 的边界也可断（制表符之后除外：同上一条，
            // 制表符之后不断；后面是 CJK 时未测）。
            // 这里原先不查禁则，`(|汉`、`6|）` 都记断点。`(|汉` 不断是实测
            // （`kinsoku-open-ascii`：21 个汉字后的 `(` 挪到下一行）；`6|）` 不断只是
            // 同一条禁则推过来的——`breakme` 的尾巴 `（汉6）汉7` 与它一致，但那里选中的是
            // `汉|6`，这一处分不出来。
            //
            // 空格之后照样可断，不查禁则（`汉 ）` 之间可断）。**未测**。
            let enter_cjk = next_char.is_some_and(is_cjk)
                && !is_cjk(c)
                && !after_space
                && c != '\t'
                && kinsoku_ok;

            if after_space || before_tab || cjk_boundary || enter_cjk {
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

#[cfg(test)]
mod cluster_tests {
    use super::cluster_boundaries;

    #[test]
    fn clusters_keep_surrogates_marks_and_joiners_together() {
        let ends = |s: &str| cluster_boundaries(s).collect::<Vec<_>>();
        assert_eq!(ends("ab"), [1, 2]);
        // U+1D400 在 UTF-16 里是代理对，不能切成两半。
        assert_eq!(ends("\u{1D400}\u{1D400}"), [4, 8]);
        // e + U+0301 是一个字符。
        assert_eq!(ends("e\u{301}x"), [3, 4]);
        // 👩‍💻：ZWJ 两侧连成一簇。
        assert_eq!(ends("\u{1F469}\u{200D}\u{1F4BB}a"), [11, 12]);
        // 两个区域指示符一面旗，第三个另起。
        assert_eq!(ends("\u{1F1E8}\u{1F1F3}\u{1F1E8}"), [8, 12]);
        assert_eq!(ends(""), Vec::<usize>::new());
    }
}
