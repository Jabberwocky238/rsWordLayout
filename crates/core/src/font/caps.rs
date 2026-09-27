//! `w:caps` / `w:smallCaps`：**显示时**的大小写变换。
//!
//! 这是字形层的事，不是文本层的事：Word 的 `Range.Text` 仍是原来的小写，
//! 行的 `cpFirst` / `cpLim` 也按原文数（`caps-on` 窄路径 38 个一行，数的是原文的 `a`）。
//! 所以变换**不在桥接层改 `Run::text`**——那样一来断行会在变换后的文字上找断点，
//! 将来换成一对多的映射（`ß` → `SS`）时源偏移还会整体错位。
//!
//! 变换放在度量与整形共用的这一处：[`display_chars`] 把原文逐字符展开成
//! 「显示字符 + 字号 + 源区间 + 源字符」。真度量的 `measure` 与绘制层的整形都走
//! [`super::FontRegistry::shape_text`]，桩度量也读同一个展开——
//! 于是断行量到的宽度、画出来的字形与字形记录的 `sourceChar` 是同一份结果。
//!
//! 实测（`word_analyse/reports/rsword-diff/caps.md` 与 `reports/diff/caps-on.word.narrow.jsonl`，
//! 手机 Word，Calibri 12pt，120 或 160 个 `a`）。宽度区间按「`n` 个放得下、`n + 1` 个放不下」
//! 反推，**假定 Calibri `A` 的宽度随字号线性缩放、放满按 ≤ 算**：
//!
//! | 夹具 | 版心 | Word 下一行起点 | 推出的每字宽度，twips |
//! | --- | ---: | ---: | --- |
//! | `caps-plain`（无 caps） | 10466 | 91 | `a` 114.96 |
//! | `caps-on` | 10466 | 75 | `A` 138.87 |
//! | `caps-on`（窄路径 jsonl） | 5329 | 38、76、114 | `A` 138.87 |
//! | `smallcaps` | 10466 | 95 | (109.02, 110.17] |
//! | `smallcaps-wide` | 14560 | 132 | (109.47, 110.30] |
//! | `smallcaps-12000` | 10560 | 96 | (108.87, 110.00] |
//!
//! 三刀合起来，小型大写的 `a` 有效宽度在 (109.47, 110.00] twips，即 (9.460, 9.505]pt 的 `A`，
//! 是 12pt 的 0.7883–0.7921。见 [`small_caps_size_centipoints`] 的取值依据。
//! 这些夹具每个都是一个没有断点的长词，Word 的数只有在紧急断行（G0）落地后才对得上。
//!
//! 下面几条**都是假定**，没有 Word 的数：
//!
//! - **大小写映射一对一、只在基本多文种平面内**（[`upper_one_to_one`]）：`ß` 不变，
//!   增补平面的双体字母不变；映射表是编译器带的 Unicode 版本，不是 Word 的；
//! - **大写画不出就画源字符**（[`display_chars_with`]）：已注册的字体都没有那个大写字形、
//!   源字符却有时，照原样画源字符，小型大写下按全字号——不让一个本来画得出的字消失；
//! - **字体槽按源字符选**（`FontRegistry::shape_text`）：`ı` → `I` 仍从 hAnsi 槽的字体画；
//! - **小型大写只缩小一对一映射会变的字符**，组合符号跟着它的基字走；
//! - **行高按 run 的全字号**：一整段小型大写的行高不因缩小而变矮
//!   （`RealMetrics::vertical_raw` 按源字符、全字号取纵向量）。待测：几十段全小写的小型大写
//!   与同样文字的全大写比分页；
//! - `w:mwSmallCaps`（settings.xml 的兼容项，MS-OE376 2.15.3.32 另一套算法）**未实现**：
//!   上面的夹具都没有 settings.xml，走的是默认路径。

use super::FontSpec;

/// 一个 run 的大写显示方式（`w:caps` / `w:smallCaps`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Caps {
    /// 按原字符排。
    #[default]
    None,
    /// `w:caps`：全部换成大写字形，字号不变。实测（`caps-on`）。
    All,
    /// `w:smallCaps`：**有大写形式的字符**换成大写字形并缩小字号；
    /// 原本就是大写的字母、数字、标点、CJK 不变。实测的只有全是小写 `a` 的一段，
    /// 大小写混排、数字与标点保持原字号是**假定**。
    Small,
}

/// 小型大写的缩小字号，0.01pt。
///
/// **规则：80% 向下取整到半点。** 12pt（24 半点）→ ⌊19.2⌋ = 19 半点 = 9.5pt，
/// Calibri `A`（1185/2048 em）在 9.5pt 下 109.94 twips，落在实测的 (109.47, 110.00] 里；
/// 三份夹具的下一行起点 95 / 132 / 96 逐个对上。**实测的只有 12pt 这一个字号**，
/// 而且三份夹具量的是同一个量（12pt 小型大写 `a` 的宽度）。
///
/// 能排除的（假定 `A` 的宽度线性缩放、放满按 ≤）：ECMA-376 §17.3.2.33 的「小两磅」
/// （10pt，纸页会是 90 而不是 95）、不取整的 80%（9.6pt → 94）、取整到整点（9pt → 100）、
/// 固定比例 0.785（`smallcaps` 会是 96）与 0.794（`smallcaps-12000` 会是 95）。
///
/// **不能排除的**：四舍五入到半点（同为 19）、固定比例 0.79 左右（9.48pt）、`size − 2.5pt`。
/// 取向下取整是因为 Word 的字号本来就是整数半点（`hps`），整数除法是最朴素的实现——
/// **这一条是假设**，12pt 以外的字号全是外推（11pt 的默认正文字号上，向下取整 8.5pt、
/// 四舍五入 9pt、0.79 是 8.69pt，宽度差到 6%）。能分开它们的夹具（10466 版心，200 个 `a`）：
///
/// - `smallCaps` + `sz=22`：向下取整 106、四舍五入与「小两磅」100；
/// - `smallCaps` + `sz=36`：向下取整 64、四舍五入 62、0.79 是 63、`size − 2.5pt` 58、「小两磅」56，
///   各自离边界至少 0.35 个字；
/// - 不依赖线性宽度的对照：`w:caps` + `sz=19` 放在同样三个版心上，若也是 95 / 132 / 96，
///   12pt 的小型大写就等于 19 半点的全大写。
///
/// `sz=20` 不行：向下取整与四舍五入同为 8pt，预测的 113 离边界只有 0.05 个字。
///
/// 精确字号（上下标缩出来的 0.01pt）按同一规则直接取整：12pt 上标 792 → 600，
/// 比例 0.758。「先对声明字号取 80% 再乘 0.66」会是约 627——两者都**未测**。
pub fn small_caps_size_centipoints(size_centipoints: u64) -> u64 {
    // ⌊0.8 × 半点⌋ 半点，至少 1 半点：字号为零的字形会在断行里变成零宽。
    let half_points = size_centipoints * 4 / 5 / 50;
    half_points.max(1) * 50
}

/// 大写映射：**一对一，一个 UTF-16 单位换一个**。源字符与它的大写都在基本多文种平面、
/// Unicode 的大写映射恰好是一个字符时取它，否则原样保留。
///
/// 所以 `ß` 不变（完整映射是 `SS`）、`ﬁ` 不变（`FI`）、`ŉ` 不变（`ʼN`），
/// `ı` → `I`、`ſ` → `S`、`µ` → `Μ` 照变；增补平面的双体字母（德瑟雷特 `𐐨`、阿德拉姆 `𞤢`）不变。
/// 不做语言相关的特例（土耳其语的 `i` → `İ`）。
///
/// **这一条是假定**，依据只是静态的：手机 Word 的 `libwlibandroid.so` 导入的大写函数
/// （`MsoWchToUpper`、`MsoPwchUpper` 等）都是一个 UTF-16 单位换一个，没有导入会改长度的映射。
/// 一个单位进、一个单位出，代理对就换不了——所以只在基本多文种平面内映射。
/// Word 在 `w:caps` / `w:smallCaps` 下画不画 `SS` 或 `ẞ` **未测**，待测的是 `ß` 夹具。
/// 与 Unicode 的「简单映射」（UnicodeData 第 12 栏）也不完全相同：`ᾀ` 这类带下标 iota 的
/// 希腊字母简单映射是 `ᾈ`，这里因为完整映射是两个字符而原样保留。
///
/// **映射表跟着编译器走**：`char::to_uppercase` 用的是标准库自带的 Unicode 数据
/// （`char::UNICODE_VERSION`，rustc 1.98 是 17.0），升级工具链就可能多出新的大小写对。
/// Unicode 只增不改大小写对，所以变化只会是「原来不变的小写字母有了大写」，而且多半是
/// 很晚才编进来的大写：格鲁吉亚文 Mkhedruli → Mtavruli（`ა` → U+1C90，Unicode 11）、
/// 国际音标 `ɪ` → U+A7AE（Unicode 9）、`ʂ` → U+A7C5（Unicode 12）、`ƛ` → U+A7DC（Unicode 16）。
/// Word 的大写表是哪个版本**未测**，这些字母在 caps 下 Word 画大写还是原样都不知道；
/// 待测：`w:caps` 下一段格鲁吉亚文与一段 `ɪʂƛ`，比字形。常见字体多半没有这些大写字形，
/// 这时 [`display_chars_with`] 退回源字符，结果与不映射相同。
///
/// 换成一对多的映射只改这里与 [`display_chars_with`]：显示字符带着源区间，
/// 整形按调用方给的 cluster 值走（`RustybuzzShaper::shape_clusters_with_face_centipoints`），
/// 一个源字符展开出的几个字形会共用它的源区间。
pub fn upper_one_to_one(ch: char) -> char {
    if ch.len_utf16() != 1 {
        return ch;
    }
    let mut upper = ch.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) if u.len_utf16() == 1 => u,
        _ => ch,
    }
}

/// 一个显示字符：变换之后真正拿去整形的字符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayChar {
    pub ch: char,
    /// 变换前的源字符。选字体槽按它（`FontSpec::slot_for` 按码位分区，
    /// `ı` 在 hAnsi、`I` 在 ascii），纵向量也按它选 face，两边才一致。
    pub source_ch: char,
    /// 整形用的字号，0.01pt。小型大写缩小的那部分与原字号不同。
    pub size_centipoints: u64,
    /// 来源字符在原文里的 UTF-16 区间（相对传入的文字）。
    ///
    /// 一对一映射下每个显示字符正好对着一个源字符；字形记录的 `sourceChar` 因此落在原文上。
    pub source: (u32, u32),
}

/// 按 `font.caps` 把原文展开成显示字符，**不查字体覆盖**：全当画得出来。
///
/// 给没有字体的桩度量（`SimpleMetrics`）用。有字体的整形走 [`display_chars_with`]，
/// 大写画不出时退回源字符；两边只在这种字符上不同。
pub fn display_chars<'a>(text: &'a str, font: &FontSpec) -> impl Iterator<Item = DisplayChar> + 'a {
    display_chars_with(text, font, |_, _| Some(())).map(|(d, _)| d)
}

/// 按 `font.caps` 把原文展开成显示字符，每个显示字符同时问一次 `draw` 用什么画。
///
/// `draw(源字符, 显示字符)` 返回画它用的东西（注册表里是 face 标识），`None` 表示画不出。
/// 不变换的字符只问一次 `draw(c, c)`，与不做 caps 时逐位相同。
///
/// **大写画不出、源字符画得出时，画源字符**，小型大写下按全字号——当它没有大写形式。
/// 这一条是**假定**，没有 Word 的数：映射表比字体新（见 [`upper_one_to_one`] 的版本说明），
/// 手机上的 Calibri 就缺 `Ɑ`（U+2C6D）、`Ɪ`（U+A7AE）这类大写。不退回的话这个字符
/// 没有 face，整形时整段被丢掉：宽度为零、什么都不画，比不做 caps 还糟。
/// 源字符也画不出时保留映射后的字符，缺字交调用方处理（与不做 caps 时一样画不出）。
///
/// 小型大写的字号按**字符簇**定（`linebreak::cluster_boundaries`，与紧急断行同一张表）：
/// 簇的基字（退回之后）是大写形式就整簇缩小，组合符号（Mn/Me）、零宽连接符、变体选择符
/// 跟着基字走。不然 `e` + U+0301 的基字缩小、附加符号全尺寸，两个字号切成两段整形，
/// 附加符号就挂不到基字上。宽度不受影响（附加符号推进量为零），受影响的是绘制与字形记录。
pub fn display_chars_with<'a, R: 'a>(
    text: &'a str,
    font: &FontSpec,
    mut draw: impl FnMut(char, char) -> Option<R> + 'a,
) -> impl Iterator<Item = (DisplayChar, Option<R>)> + 'a {
    let caps = font.caps;
    let full = font.effective_size_centipoints();
    let small = small_caps_size_centipoints(full);
    // 簇的终点（字节偏移，升序）；一个字符落在某个终点上就是新簇的基字。只在小型大写时推进。
    let mut cluster_ends = super::linebreak::cluster_boundaries(text).peekable();
    let mut cluster_size = full;
    let mut offset = 0u32;
    text.char_indices().map(move |(byte, source_ch)| {
        let start = offset;
        offset += source_ch.len_utf16() as u32;
        let source = (start, offset);
        // 热路径：断行对同一段文字反复试宽，不做变换时不该多付一次映射。
        let upper = match caps {
            Caps::None => source_ch,
            Caps::All | Caps::Small => upper_one_to_one(source_ch),
        };
        let (ch, drawn) = if upper == source_ch {
            (source_ch, draw(source_ch, source_ch))
        } else {
            match draw(source_ch, upper) {
                Some(r) => (upper, Some(r)),
                None => match draw(source_ch, source_ch) {
                    Some(r) => (source_ch, Some(r)),
                    None => (upper, None),
                },
            }
        };
        let size_centipoints = match caps {
            Caps::None | Caps::All => full,
            Caps::Small => {
                if byte == 0 || cluster_ends.next_if_eq(&byte).is_some() {
                    // 映射会变才算「有大写形式」。只看 `is_lowercase` 会漏掉标题大小写字母，
                    // 也会把没有一对一大写的小写字母（`ĸ`、`ß`）错缩小。
                    cluster_size = if ch != source_ch { small } else { full };
                }
                cluster_size
            }
        };
        (DisplayChar { ch, source_ch, size_centipoints, source }, drawn)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(caps: Caps) -> FontSpec {
        let mut f = FontSpec::new("Calibri", 24);
        f.caps = caps;
        f
    }

    fn sizes(text: &str, caps: Caps) -> Vec<u64> {
        display_chars(text, &spec(caps)).map(|d| d.size_centipoints).collect()
    }

    fn shown(text: &str, caps: Caps) -> String {
        display_chars(text, &spec(caps)).map(|d| d.ch).collect()
    }

    /// 实测：12pt 的小型大写落在 (9.460, 9.505]pt（`caps.md` 三份 `smallcaps` 夹具）。
    #[test]
    fn small_caps_size_at_twelve_points_is_in_the_measured_band() {
        let cp = small_caps_size_centipoints(1200);
        assert_eq!(cp, 950);
        assert!(946.0 < cp as f64 && cp as f64 <= 950.5);
    }

    /// 假定：12pt 以外的字号是同一规则的外推，没有 Word 的数。
    #[test]
    fn assumed_small_caps_size_floors_eighty_percent_at_other_sizes() {
        assert_eq!(small_caps_size_centipoints(2000), 1600);
        assert_eq!(small_caps_size_centipoints(1100), 850);
        assert_eq!(small_caps_size_centipoints(1800), 1400);
        // 至少 1 半点。
        assert_eq!(small_caps_size_centipoints(50), 50);
    }

    /// 假定：上标（12pt × 0.66 = 7.92pt）直接对精确字号取整，得 6pt。
    #[test]
    fn assumed_small_caps_on_superscript_floors_the_exact_size() {
        assert_eq!(small_caps_size_centipoints(792), 600);
    }

    /// 假定（D8）：一对一映射，`ß` 在全大写与小型大写下都原样、全字号。
    #[test]
    fn assumed_sharp_s_stays_sharp_s_under_one_to_one_mapping() {
        let got: Vec<_> = display_chars("aß", &spec(Caps::All)).collect();
        assert_eq!(got.iter().map(|d| d.ch).collect::<String>(), "Aß");
        assert_eq!(got[0].source, (0, 1));
        assert_eq!(got[1].source, (1, 2));
        assert_eq!(shown("ßﬁŉ", Caps::All), "ßﬁŉ");
        assert_eq!(shown("aß", Caps::Small), "Aß");
        assert_eq!(sizes("aß", Caps::Small), [950, 1200]);
    }

    #[test]
    fn one_to_one_mapping_keeps_the_source_char_for_slot_selection() {
        let got: Vec<_> = display_chars("ıſµ", &spec(Caps::All)).collect();
        assert_eq!(got.iter().map(|d| d.ch).collect::<String>(), "ISΜ");
        assert_eq!(got.iter().map(|d| d.source_ch).collect::<String>(), "ıſµ");
        assert_eq!(shown("ıſµ", Caps::None), "ıſµ");
    }

    #[test]
    fn small_caps_only_shrink_characters_that_have_an_uppercase_form() {
        assert_eq!(sizes("aA1汉", Caps::Small), [950, 1200, 1200, 1200]);
        assert_eq!(shown("aA1汉", Caps::Small), "AA1汉");
        // `ĸ` 没有大写形式，不缩小。
        assert_eq!(sizes("ĸ", Caps::Small), [1200]);
    }

    #[test]
    fn combining_marks_take_the_size_of_their_base() {
        // e + U+0301、A + U+0301：附加符号与基字同一个字号，整形才不会被切成两段。
        assert_eq!(sizes("e\u{301}A\u{301}", Caps::Small), [950, 950, 1200, 1200]);
        // 零宽连接符连起的序列、变体选择符同理。
        assert_eq!(sizes("a\u{200D}b", Caps::Small), [950, 950, 950]);
        assert_eq!(sizes("1\u{FE0F}a", Caps::Small), [1200, 1200, 950]);
    }

    #[test]
    fn surrogate_pairs_keep_two_utf16_units() {
        let got: Vec<_> = display_chars("𝒶b", &spec(Caps::All)).collect();
        assert_eq!(got[0].source, (0, 2));
        assert_eq!(got[1].source, (2, 3));
    }

    /// 假定：一个 UTF-16 单位换一个，增补平面的双体字母不映射，小型大写下也不缩小。
    #[test]
    fn assumed_supplementary_plane_letters_are_not_mapped() {
        assert_eq!(upper_one_to_one('\u{10428}'), '\u{10428}');
        assert_eq!(upper_one_to_one('\u{1E922}'), '\u{1E922}');
        let got: Vec<_> = display_chars("\u{10428}a", &spec(Caps::Small)).collect();
        assert_eq!(
            got.iter().map(|d| (d.ch, d.size_centipoints, d.source)).collect::<Vec<_>>(),
            [('\u{10428}', 1200, (0, 2)), ('A', 950, (2, 3))]
        );
    }

    /// 假定：大写画不出而源字符画得出时画源字符，按没有大写形式算（全字号），
    /// 组合符号跟着退回后的基字走。
    #[test]
    fn assumed_uppercase_that_cannot_be_drawn_falls_back_to_the_source_char() {
        // 只有 `B` 画不出。
        let draw = |_: char, c: char| (c != 'B').then_some(c);
        let got: Vec<_> = display_chars_with("ab\u{301}", &spec(Caps::Small), draw).collect();
        assert_eq!(
            got.iter().map(|(d, r)| (d.ch, d.source_ch, d.size_centipoints, *r)).collect::<Vec<_>>(),
            [('A', 'a', 950, Some('A')), ('b', 'b', 1200, Some('b')), ('\u{301}', '\u{301}', 1200, Some('\u{301}'))]
        );
        let drawn: String = display_chars_with("ab", &spec(Caps::All), draw).map(|(d, _)| d.ch).collect();
        assert_eq!(drawn, "Ab");
        // 桩度量不查覆盖，照映射。
        assert_eq!(shown("ab", Caps::All), "AB");
        // 源字符也画不出：保留映射，缺字交给调用方。
        let got: Vec<_> = display_chars_with("b", &spec(Caps::Small), |_, _| None::<()>).collect();
        assert_eq!((got[0].0.ch, got[0].0.size_centipoints, got[0].1), ('B', 950, None));
        // 不变换的字符只问一次，问的是它自己。
        let mut asked = Vec::new();
        let _: Vec<_> = display_chars_with("1", &spec(Caps::All), |s, c| {
            asked.push((s, c));
            Some(())
        })
        .collect();
        assert_eq!(asked, [('1', '1')]);
    }
}
