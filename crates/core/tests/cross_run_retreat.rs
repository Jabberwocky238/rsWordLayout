//! 跨 run 回退：一截字放不下、又一个断点都塞不下时，退回本行更早的断点——断点在前一个 run 里
//! 也一样（`layout.rs` 里 `Engine::shortfall`）。
//!
//! 依据：**run 边界不是断点**是实测（Android Word，`webhidden` / `specvanish` 20 + 30 + 80 个 `0`
//! 纸页 0、86，`word_analyse/reports/rsword-diff/vanish.md`）；「行首之后有更早的断点就断在那里」
//! 在制表符上实测（`tab-right-1440`、`tab-right-fit`，`tab.md`）。两条合起来预言**拆不拆 run 断法
//! 都一样**——断点落在前一个 run 里、要退回去的情形 Word **未测**（待测：`hello wor` | `ld` + 60 个
//! `0`、`[22汉][）10汉]`、`[22汉][。10汉]`、`[21汉（][10汉]` @5329），所以这里的断言都是**假设**。
//!
//! 性质测试用桩度量（12pt：拉丁字 120 twips、汉字 240、空格 60，全是整数、可加），随机正文在随机
//! 位置拆成几个 run，排出来与一个 run 逐字相同。手机 Calibri 那几条要设 `RSWORD_TEST_CALIBRI`。

use rsword_layout_core::{
    Caps, Color, Engine, FontMetrics, FontSpec, Fragment, LayoutRecord, Margins, Page, PageSetup,
    Para, PlaceholderKind, Platform, Run, SimpleMetrics, Size, TabAlign, TabLeader, TabStop, Twips,
    View, paint_document,
};

fn run_in(text: &str, font: &FontSpec) -> Run {
    Run {
        text: text.to_owned(),
        font: font.clone(),
        color: Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

/// 10pt 桩字体：拉丁字 100 twips、空格 50，与 `emergency_break.rs` 同一套。
fn synthetic10() -> FontSpec {
    FontSpec::new("synthetic", 20)
}

fn setup(width: Twips) -> PageSetup {
    PageSetup {
        size: Size::new(width, 1_000_000),
        margins: Margins::uniform(0),
    }
}

/// 各行的源区间（UTF-16，含段落标记那一格）。
fn lines_with<M: FontMetrics>(
    metrics: &M,
    width: Twips,
    runs: Vec<Run>,
    profile: (Platform, View),
) -> Vec<(u32, u32)> {
    let pages = Engine::new(metrics, setup(width))
        .with_platform(profile.0, profile.1)
        .layout(&[Para { runs, ..Para::default() }]);
    record_lines(&pages)
}

fn record_lines(pages: &[Page]) -> Vec<(u32, u32)> {
    let record = LayoutRecord::from_paint(&paint_document(pages, None, &[]));
    record
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| {
            let s = l.source.expect("每行都有源区间");
            (s.start, s.end)
        })
        .collect()
}

fn lines(width: Twips, runs: Vec<Run>) -> Vec<(u32, u32)> {
    lines_with(&SimpleMetrics, width, runs, (Platform::Desktop, View::Print))
}

fn starts(lines: &[(u32, u32)]) -> Vec<u32> {
    lines.iter().map(|&(s, _)| s).collect()
}

fn zeros(n: usize) -> String {
    "0".repeat(n)
}

#[test]
fn assumed_a_long_word_after_a_space_moves_down_across_three_runs() {
    // `webhidden` 的三个 run 前面加一个 `0 `：`0 ` + 18 个 `0` | 30 个 `0` | 80 个 `0`。
    // 长词（128 个 `0`）从第一个 run 的空格之后起、跨三个 run，一行放不下：退回空格之后，
    // 第一行只有 `0 `，长词整个挪下去再紧急断行——与排在一个 run 里相同。
    // 桩里每行 86 个（8650）/ 43 个（4350），对应 Calibri 在 10466 / 5329 上的每行字数。
    let split = || {
        let f = synthetic10();
        vec![run_in(&format!("0 {}", zeros(18)), &f), run_in(&zeros(30), &f), run_in(&zeros(80), &f)]
    };
    let whole = || vec![run_in(&format!("0 {}", zeros(128)), &synthetic10())];
    assert_eq!(starts(&lines(8650, split())), [0, 2, 88]);
    assert_eq!(lines(8650, split()), lines(8650, whole()));
    assert_eq!(starts(&lines(4350, split())), [0, 2, 45, 88]);
    assert_eq!(lines(4350, split()), lines(4350, whole()));
}

#[test]
fn assumed_spaces_at_a_run_boundary_go_to_the_ending_line() {
    // 43 个 `0` | ` hello world`：行尾余量（20）连那个空格（50）都放不下，本行一个断点也没有，
    // 按紧急断行在 43 个 `0` 之后收行；换行处的空格记进本行（`(0, 44)`），下一行从 `hello` 起——
    // 与排在一个 run 里相同。原先下一行以那个空格开头（`(43, 56)`）。
    let f = synthetic10();
    let split = vec![run_in(&zeros(43), &f), run_in(" hello world", &f)];
    assert_eq!(lines(4320, split.clone()), [(0, 44), (44, 56)]);
    assert_eq!(lines(4320, split), lines(4320, vec![run_in(&format!("{} hello world", zeros(43)), &f)]));
    // 空格本身跨 run：`aaaa ` | `  bbbb`，`aaaa ` 之后的三个空格都吃进本行。
    let split = vec![run_in("aaaa ", &f), run_in("  bbbb", &f)];
    assert_eq!(lines(450, split.clone()), [(0, 7), (7, 12)]);
    assert_eq!(lines(450, split), lines(450, vec![run_in("aaaa   bbbb", &f)]));
}

#[test]
fn a_retreat_never_crosses_a_hard_line_break() {
    // 软回车之后的一行从软回车起算：`aaaa bbbb` 与软回车在前一行，退回只在本行之内找断点。
    // `cc` | 60 个 `0`：本行行首之后没有断点，紧急断行，不退回软回车之前的空格。
    let f = synthetic10();
    let mut first = run_in("aaaa bbbb\u{fffc}cc", &f);
    first.placeholders = vec![PlaceholderKind::LineBreak];
    let got = lines(4350, vec![first, run_in(&zeros(60), &f)]);
    assert_eq!(starts(&got), [0, 10, 53]);
}

/// 性质测试里用的 12pt 桩字体。
fn synthetic12() -> FontSpec {
    FontSpec::new("synthetic", 24)
}

/// 线性同余，够用就行：测试不引外部 crate。
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// 排出来的每个字：(源位置 UTF-16, 页, 行, x 精确值 pt, 基线 1/7200 英寸, 制表符推进量 pt)。
/// 空文字片段（行尾的空片段）不产生字，不比。
fn glyph_positions<M: FontMetrics>(metrics: &M, pages: &[Page]) -> Vec<(u32, usize, u32, f64, i64, Option<f64>)> {
    let mut out = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        for f in &page.fragments {
            let Fragment::Text(t) = f else { continue };
            let Some((start, _)) = t.source else { continue };
            let mut at = start;
            for (byte, c) in t.text.char_indices() {
                let x = t.x_pt + metrics.advance_pt(&t.text[..byte], &t.font);
                out.push((at, page_index, t.line, x, t.baseline_fine, t.tab_advance_pt));
                at += c.len_utf16() as u32;
            }
        }
    }
    out
}

/// 同一段正文在 `cuts`（字节偏移，升序，可重复——重复即空 run）处拆成几个 run。
fn split_runs(text: &str, kinds: &[PlaceholderKind], cuts: &[usize]) -> Vec<Run> {
    split_runs_in(text, kinds, cuts, &synthetic12())
}

/// 同 [`split_runs`]，每个 run 都用 `font`。
fn split_runs_in(text: &str, kinds: &[PlaceholderKind], cuts: &[usize], font: &FontSpec) -> Vec<Run> {
    let mut kinds = kinds.iter().copied();
    let mut bounds = vec![0];
    bounds.extend_from_slice(cuts);
    bounds.push(text.len());
    bounds
        .windows(2)
        .map(|w| {
            let piece = &text[w[0]..w[1]];
            let mut r = run_in(piece, font);
            r.placeholders = piece.matches('\u{fffc}').map(|_| kinds.next().unwrap()).collect();
            r
        })
        .collect()
}

#[test]
fn assumed_random_text_split_into_runs_lays_out_like_one_run() {
    // This legacy comparator reconstructs glyph origins from measured prefixes;
    // numeric/CJK spacing is checked against actual painting in autospace_consistency.
    // Combining clusters are excluded because the approximation does not cross runs.
    let tokens = [
        "a", "b", "ab", "abc ", " ", "  ", ".", "\u{6c49}", "\u{6c49}\u{6c49}", "\u{ff08}",
        "\u{ff09}", "\u{3002}", "\u{ff0c}", "\u{ff05}", "\u{3009}", "\u{2026}", "(", ")", "$",
        "\t", "\t\t", "\u{fffc}",
    ];
    let kinds = [PlaceholderKind::Object, PlaceholderKind::LineBreak, PlaceholderKind::PageBreak];
    let positions = [300, 720, 1440, 2600, 5000, 5329, 9000];
    let aligns = [TabAlign::Left, TabAlign::Right, TabAlign::Center, TabAlign::Decimal, TabAlign::Bar];
    let widths = [600, 1000, 1500, 2400, 5329];
    let profiles = [(Platform::Desktop, View::Print), (Platform::Android, View::Mobile)];
    let mut rng = Lcg(0x0c0ffee);
    let mut checked = 0;
    for _ in 0..3000 {
        let mut text = String::new();
        for _ in 0..1 + rng.below(12) {
            match rng.below(4) {
                0 => text.push_str(&"x".repeat(1 + rng.below(60))),
                _ => text.push_str(tokens[rng.below(tokens.len())]),
            }
        }
        let placeholder_kinds: Vec<PlaceholderKind> =
            text.matches('\u{fffc}').map(|_| kinds[rng.below(kinds.len())]).collect();
        let mut tabs: Vec<TabStop> = (0..rng.below(3))
            .map(|_| TabStop {
                pos: positions[rng.below(positions.len())],
                align: aligns[rng.below(aligns.len())],
                leader: TabLeader::None,
            })
            .collect();
        tabs.sort_by_key(|t| t.pos);
        tabs.dedup_by_key(|t| t.pos);
        let default_tab_stop = [Some(221), Some(720), None][rng.below(3)];
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let mut cuts: Vec<usize> = (0..1 + rng.below(4)).map(|_| bounds[rng.below(bounds.len())]).collect();
        cuts.sort();
        let width = widths[rng.below(widths.len())];
        let para = |runs| Para { runs, tabs: tabs.clone(), default_tab_stop, ..Para::default() };
        let whole = para(split_runs(&text, &placeholder_kinds, &[]));
        let split = para(split_runs(&text, &placeholder_kinds, &cuts));
        for (platform, view) in profiles {
            let engine = Engine::new(&SimpleMetrics, setup(width)).with_platform(platform, view);
            let a = engine.layout(std::slice::from_ref(&whole));
            let b = engine.layout(std::slice::from_ref(&split));
            let texts: Vec<&str> = split.runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(
                record_lines(&a),
                record_lines(&b),
                "{texts:?} tabs {tabs:?} default {default_tab_stop:?} @{width} {platform:?}"
            );
            let (ga, gb) = (glyph_positions(&SimpleMetrics, &a), glyph_positions(&SimpleMetrics, &b));
            assert_eq!(ga.len(), gb.len(), "{texts:?} @{width} {platform:?}");
            for (x, y) in ga.iter().zip(&gb) {
                let same = x.0 == y.0
                    && x.1 == y.1
                    && x.2 == y.2
                    && (x.3 - y.3).abs() < 1e-9
                    && x.4 == y.4
                    && match (x.5, y.5) {
                        (Some(p), Some(q)) => (p - q).abs() < 1e-9,
                        (p, q) => p == q,
                    };
                assert!(same, "{texts:?} tabs {tabs:?} @{width} {platform:?}: {x:?} vs {y:?}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 6000);
}

#[test]
fn assumed_split_runs_with_caps_spacing_and_scale_lay_out_like_one_run() {
    // 跨 run 回退量宽、切段都按源文字，大小写变换、字符间距与缩放在度量里面：它们不该让拆开的
    // run 断得与一个 run 不同。桩度量下这几样都可加——间距按源字符簇数（`linebreak::cluster_boundaries`），
    // 小型大写 12pt 缩到 9.5pt，拉丁字 95 twips，缩放取 200% 不截断——所以要求逐字相同。
    // 正文有大小写字母（小型大写只缩有大写形式的）、空格、汉字与禁则标点、制表符、对象占位符。
    // Numeric/CJK painting uses the source-aware comparator in autospace_consistency;
    // this legacy measured-prefix comparator also excludes combining characters.
    let tokens = [
        "a", "B", "ab", "Abc ", " ", "  ", ".", "\u{6c49}", "\u{6c49}\u{6c49}", "\u{ff08}",
        "\u{ff09}", "\u{3002}", "\u{ff0c}", "\u{2026}", "(", ")", "\t", "\u{fffc}",
    ];
    let kinds = [PlaceholderKind::Object, PlaceholderKind::LineBreak];
    let fonts: Vec<FontSpec> = [
        (Caps::All, 0, 100),
        (Caps::Small, 0, 100),
        (Caps::None, 20, 100),
        (Caps::None, -10, 100),
        (Caps::Small, 40, 100),
        (Caps::All, 20, 200),
        (Caps::None, 0, 200),
    ]
    .into_iter()
    .map(|(caps, letter_spacing, scale_pct)| {
        let mut font = synthetic12();
        font.caps = caps;
        font.letter_spacing = letter_spacing;
        font.scale_pct = scale_pct;
        font
    })
    .collect();
    let widths = [600, 1000, 2400, 5329];
    let profiles = [(Platform::Desktop, View::Print), (Platform::Android, View::Mobile)];
    let mut rng = Lcg(0xca95);
    let mut checked = 0;
    for _ in 0..1500 {
        let mut text = String::new();
        for _ in 0..1 + rng.below(12) {
            match rng.below(4) {
                0 => text.push_str(&["x", "X"][rng.below(2)].repeat(1 + rng.below(60))),
                _ => text.push_str(tokens[rng.below(tokens.len())]),
            }
        }
        let placeholder_kinds: Vec<PlaceholderKind> =
            text.matches('\u{fffc}').map(|_| kinds[rng.below(kinds.len())]).collect();
        let tabs = match rng.below(3) {
            0 => Vec::new(),
            1 => vec![TabStop { pos: 1440, align: TabAlign::Left, leader: TabLeader::None }],
            _ => vec![TabStop { pos: 2600, align: TabAlign::Right, leader: TabLeader::None }],
        };
        let font = &fonts[rng.below(fonts.len())];
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let mut cuts: Vec<usize> = (0..1 + rng.below(4)).map(|_| bounds[rng.below(bounds.len())]).collect();
        cuts.sort();
        let width = widths[rng.below(widths.len())];
        let para = |runs| Para { runs, tabs: tabs.clone(), default_tab_stop: Some(720), ..Para::default() };
        let whole = para(split_runs_in(&text, &placeholder_kinds, &[], font));
        let split = para(split_runs_in(&text, &placeholder_kinds, &cuts, font));
        for (platform, view) in profiles {
            let engine = Engine::new(&SimpleMetrics, setup(width)).with_platform(platform, view);
            let a = engine.layout(std::slice::from_ref(&whole));
            let b = engine.layout(std::slice::from_ref(&split));
            let texts: Vec<&str> = split.runs.iter().map(|r| r.text.as_str()).collect();
            let what = format!(
                "{texts:?} caps {:?} spacing {} scale {} tabs {tabs:?} @{width} {platform:?}",
                font.caps, font.letter_spacing, font.scale_pct
            );
            assert_eq!(record_lines(&a), record_lines(&b), "{what}");
            let (ga, gb) = (glyph_positions(&SimpleMetrics, &a), glyph_positions(&SimpleMetrics, &b));
            assert_eq!(ga.len(), gb.len(), "{what}");
            for (x, y) in ga.iter().zip(&gb) {
                let same = x.0 == y.0
                    && x.1 == y.1
                    && x.2 == y.2
                    && (x.3 - y.3).abs() < 1e-9
                    && x.4 == y.4
                    && match (x.5, y.5) {
                        (Some(p), Some(q)) => (p - q).abs() < 1e-9,
                        (p, q) => p == q,
                    };
                assert!(same, "{what}: {x:?} vs {y:?}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 3000);
}

#[cfg(feature = "fontenv")]
mod calibri {
    use super::*;
    use rsword_layout_core::{RealMetrics, font::FontRegistry};

    fn registry() -> FontRegistry {
        let path = std::env::var_os("RSWORD_TEST_CALIBRI")
            .expect("设 RSWORD_TEST_CALIBRI=<手机 Word 的 calibri.ttf>");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
        let mut registry = FontRegistry::new();
        registry.add(bytes, 0).expect("字体装得进");
        registry
    }

    /// 手机 Calibri 12pt 上的同几条，宽度是 word_analyse 的两条路径（5329 / 10466）。
    /// 拆开的结果与排在一个 run 里相同；一个 run 的那一半对的是 `zero-plain`（43）/
    /// `zero-paper`（86）的实测每行字数。拆开的一半 Word **未测**。
    #[test]
    #[ignore = "needs RSWORD_TEST_CALIBRI"]
    fn phone_calibri_split_runs_break_like_one_run() {
        let registry = registry();
        let metrics = RealMetrics::new(&registry);
        let font = FontSpec::new("Calibri", 24);
        let android = (Platform::Android, View::Mobile);
        let lay = |width, runs: &[&str]| {
            lines_with(&metrics, width, runs.iter().map(|t| run_in(t, &font)).collect(), android)
        };
        let head = format!("0 {}", zeros(18));
        let (thirty, eighty) = (zeros(30), zeros(80));
        let whole = format!("0 {}", zeros(128));
        assert_eq!(starts(&lay(10466, &[&head, &thirty, &eighty])), [0, 2, 88]);
        assert_eq!(lay(10466, &[&head, &thirty, &eighty]), lay(10466, &[&whole]));
        assert_eq!(starts(&lay(5329, &[&head, &thirty, &eighty]))[..3], [0, 2, 45]);
        assert_eq!(lay(5329, &[&head, &thirty, &eighty]), lay(5329, &[&whole]));

        let tail = format!("ld{}", zeros(60));
        let whole = format!("hello world{}", zeros(60));
        assert_eq!(lay(5329, &["hello wor", &tail]), lay(5329, &[&whole]));
        assert_eq!(starts(&lay(5329, &["hello wor", &tail]))[..2], [0, 6]);

        // 43 个 `0` 是 5230.5 twips，5260 上余 29.5，放不下空格（54.3）。
        let zeros43 = zeros(43);
        assert_eq!(lay(5260, &[&zeros43, " hello world"]), [(0, 44), (44, 56)]);
    }
}

/// 回退链上的字体与跨 run 回退一起：只用仓库里的字体（Liberation Sans 做正文、Droid Sans Fallback
/// 做回退），不要手机字体。
#[cfg(feature = "fontenv")]
mod fallback {
    use super::*;
    use rsword_layout_core::{FontHint, FontSlots, RealMetrics, font::FontRegistry};

    const SANS: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf");
    const CJK: &[u8] = include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf");

    /// CJK 夹具的写法：eastAsia 槽是装不上的 SimSun，汉字与全角标点全靠回退链。
    fn simsun12() -> FontSpec {
        let mut font = FontSpec::new("Liberation Sans", 24);
        font.slots = FontSlots {
            ascii: Some("Liberation Sans".into()),
            h_ansi: Some("Liberation Sans".into()),
            east_asia: Some("SimSun".into()),
            cs: None,
            hint: FontHint::Default,
        };
        font
    }

    #[test]
    fn assumed_split_run_kinsoku_backs_off_the_same_through_the_fallback_face() {
        // 与 `kinsoku_android.rs` 的三种拆 run 情形同一组正文，那边用桩度量；这里汉字的宽度来自
        // 回退链上的 Droid（1 em，与 `nofb-kinsoku` 同一条路），紧急断行的切口来自它的整形 cluster。
        // 一个 run 里 21 对的是 Android 实测（`kinsoku.md`）；拆开也是 21 是**假设**（Word 未测）。
        let mut registry = FontRegistry::new();
        registry.add(SANS.to_vec(), 0).unwrap();
        let droid = registry.add_fallback(CJK.to_vec(), 0).unwrap();
        let metrics = RealMetrics::new(&registry);
        let font = simsun12();
        assert_eq!(registry.select_face_for(&font, '\u{6c49}').as_ref(), Some(&droid), "前提：汉字走回退链");
        let android = (Platform::Android, View::Mobile);
        let han = |n: usize| "\u{6c49}".repeat(n);
        let lay = |texts: &[String]| {
            lines_with(&metrics, 5329, texts.iter().map(|t| run_in(t, &font)).collect(), android)
        };
        for (head, tail) in [
            (han(22), format!("\u{ff09}{}", han(10))),
            (han(22), format!("\u{3002}{}", han(10))),
            (format!("{}\u{ff08}", han(21)), han(10)),
        ] {
            let whole = lay(&[format!("{head}{tail}")]);
            assert_eq!(starts(&whole), [0, 21], "{head}{tail}");
            assert_eq!(lay(&[head.clone(), tail.clone()]), whole, "[{head}][{tail}]");
        }
    }
}
