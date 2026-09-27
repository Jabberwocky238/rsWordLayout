//! 紧急断行：一串没有断点的字比行宽还长时，按字符切在最后一个放得下的字符之后。
//!
//! Word 实测（Android Word，Calibri 12pt，`word_analyse/reports/rsword-diff/*.md`）：
//!
//! - `zero-plain` 一串 `0` 在 5329 twips 上每行 43 个；`zero-paper` 在 10466 上每行 86 个；
//!   `caps-plain` 120 个 `a` 在 10466 上 91 个；`m-plain` 的 `M` 在 5329 上每行 25 个
//!   （`tab.md`、`caps.md`、`char-scale.md`）。按理想推进量，第 44 个 `0` 超出 23 twips、
//!   第 26 个 `M` 超出 6 twips，都不收；按「窄路径字宽按像素取整」的假说（它能同时解释
//!   `i-plain` 的 95），余量是 46 与 13 twips。所以量到的是**容差上界**（理想宽度 < 6 twips，
//!   像素取整 < 13 twips），恰好放满（≤ 还是 <）**未测**。
//! - `webhidden` / `specvanish` 是 20 + 30 + 80 个 `0` 分在三个 run 里，纸页仍是 0、86
//!   （`vanish.md`）：**run 边界不是断点**。
//! - 「行首之后有更早的断点就断在那里，长串整个挪到下一行」只在制表符上量过：
//!   `tab-right-1440`（0、1、45）与 `tab-right-fit`（0、4、48）（`tab.md`）。
//!   `tab-after-a`（0、1、43）分不出是不是视觉上的两行，不作依据。空格与 CJK 交界上的
//!   同一条规则是**假设**，对应的测试以 `assumed_` 开头。
//! - `fittext-over`（`fittext.md`，0、10、50）部分支持「空行至少收一个单位」：一个超宽的
//!   fitText run 独占一行。fitText 未实现；实现时整个 fitText run 应当是一个簇。
//! - `italic-a` 不可评：macOS 文件系统不分大小写，`italic-A.docx` 覆盖了它，
//!   盘上那份其实是 100 个斜体 `A`。
//!
//! 原先的引擎在这里只硬塞一个字符就收行，没有断点的长串于是一行一个码元，
//! 直到剩下的尾巴整段放得下为止（`x`×200 在 5329 上是 149 行单字加一行 51 个）。
//!
//! 前面几条用桩度量（拉丁字 100 twips、汉字 200、空格 50，10pt）钉引擎的规则本身，
//! 不依赖字体；`real` 模块走真度量（仓库自带字体），手机 Calibri 那一条要设
//! `RSWORD_TEST_CALIBRI`。

use rsword_layout_core::{
    Align, Color, Engine, FontSpec, Fragment, LayoutRecord, Margins, PageSetup, Para,
    PlaceholderKind, Run, SimpleMetrics, Size, Twips, paint_document,
};

fn run(text: &str) -> Run {
    Run {
        text: text.to_owned(),
        font: FontSpec::new("synthetic", 20),
        color: Color::BLACK,
        placeholders: vec![],
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn setup(width: Twips) -> PageSetup {
    PageSetup {
        size: Size::new(width, 1_000_000),
        margins: Margins::uniform(0),
    }
}

/// 各行的源区间（UTF-16，含段落标记那一格）。
fn lines_with<M: rsword_layout_core::FontMetrics>(
    metrics: &M,
    width: Twips,
    runs: Vec<Run>,
) -> Vec<(u32, u32)> {
    let engine = Engine::new(metrics, setup(width));
    let pages = engine.layout(&[Para {
        runs,
        ..Para::default()
    }]);
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
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
    lines_with(&SimpleMetrics, width, runs)
}

fn starts(lines: &[(u32, u32)]) -> Vec<u32> {
    lines.iter().map(|&(s, _)| s).collect()
}

/// 两端对齐的一段里，第 `line` 行各个有字的片段的起点 x（twips）。
fn justified_piece_xs(width: Twips, runs: Vec<Run>, line: u32) -> Vec<Twips> {
    let engine = Engine::new(&SimpleMetrics, setup(width));
    let pages = engine.layout(&[Para {
        runs,
        align: Align::Justify,
        ..Para::default()
    }]);
    pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) if t.line == line && !t.text.is_empty() => Some(t.x),
            _ => None,
        })
        .collect()
}

#[test]
fn an_unbreakable_run_is_cut_after_the_last_fitting_character() {
    // 43.5 个字宽：每行 43 个，末行 14 个加段落标记。
    assert_eq!(
        lines(4350, vec![run(&"0".repeat(100))]),
        [(0, 43), (43, 86), (86, 101)]
    );
}

#[test]
fn the_cut_has_no_tolerance() {
    // 少 1 twip：第 43 个超出 1 twip 就不收。Word 那边量到的是上界
    // （`m-plain` 超出 6 twips 不收），零容差是取的最严那一端。
    assert_eq!(
        starts(&lines(4299, vec![run(&"0".repeat(100))])),
        [0, 42, 84]
    );
    // 正好 43 个字宽：放得下。**假设**——恰好放满（≤ 还是 <）Word 未测。
    assert_eq!(
        starts(&lines(4300, vec![run(&"0".repeat(100))])),
        [0, 43, 86]
    );
}

#[test]
fn a_long_word_no_longer_degenerates_to_one_unit_per_line() {
    // `long-word.md` 的 `x`×200：原先 150 行，前 149 行各一个码元。
    let got = lines(5100, vec![run(&"x".repeat(200))]);
    assert_eq!(got, [(0, 51), (51, 102), (102, 153), (153, 201)]);
}

#[test]
fn assumed_an_earlier_space_or_cjk_break_moves_the_long_word_down() {
    // **假设**：更早的断点胜过紧急断行只在制表符上量过（`tab-right-1440`、`tab-right-fit`），
    // 空格与 CJK 交界没有夹具。待测：`hello ` + 100 个 `0`、`汉汉` + 100 个 `0` @5329。
    // 行首之后有空格断点：长串整个挪到下一行，再在那里按字符切。
    let text = format!("hello world {}", "0".repeat(100));
    assert_eq!(
        lines(4350, vec![run(&text)]),
        [(0, 12), (12, 55), (55, 98), (98, 113)]
    );
    // 汉字与数字的交界也是断点。
    let text = format!("\u{6c49}\u{6c49}{}", "0".repeat(100));
    assert_eq!(
        lines(4350, vec![run(&text)]),
        [(0, 2), (2, 45), (45, 88), (88, 103)]
    );
}

#[test]
fn run_boundaries_are_not_break_opportunities() {
    // `webhidden` 的结构：20 + 30 + 80 个 `0`。拆不拆 run 断法都一样（run 里有空格时也一样，
    // 见 `assumed_a_word_split_across_runs_retreats_to_the_earlier_space`）。
    let split = || vec![run(&"0".repeat(20)), run(&"0".repeat(30)), run(&"0".repeat(80))];
    let whole = || vec![run(&"0".repeat(130))];
    assert_eq!(lines(8650, split()), [(0, 86), (86, 131)]);
    assert_eq!(lines(8650, split()), lines(8650, whole()));
    assert_eq!(starts(&lines(4350, split())), [0, 43, 86, 129]);
    assert_eq!(lines(4350, split()), lines(4350, whole()));
}

#[test]
fn assumed_a_run_boundary_after_a_space_is_still_a_break() {
    // **假设**（同上，空格上的「更早断点胜出」没有夹具）。交界两侧按同一套断点查：
    // 前一 run 以空格结尾，交界就是断点，长串挪到下一行。
    assert_eq!(
        starts(&lines(4350, vec![run("hello "), run(&"0".repeat(100))])),
        [0, 6, 49, 92]
    );
}

/// 跨 run 回退（`layout.rs` 里 `Engine::shortfall` 的（甲））：**假设**，照实测的「run 边界不是断点」
/// （`webhidden`）与「行首之后有更早的断点就断在那里」推出来的，Word 未测。
#[test]
fn assumed_a_word_split_across_runs_retreats_to_the_earlier_space() {
    // `hello wor` | `ld` + 60 个 `0` 与排成一个 run 结果相同——行首之后最后一个放得下的断点
    // 在 `hello ` 之后（在前一个 run 里），长词整个挪下去再紧急断行。
    // 跨 run 回退落地之前不回溯已排片段，第一行断在 run 边界（0、9、52）。
    // 待测：`hello wor` | `ld` + 60 个 `0` @5329。
    let split = vec![run("hello wor"), run(&format!("ld{}", "0".repeat(60)))];
    let whole = vec![run(&format!("hello world{}", "0".repeat(60)))];
    assert_eq!(starts(&lines(4350, whole)), [0, 6, 49]);
    assert_eq!(starts(&lines(4350, split)), [0, 6, 49]);
}

#[test]
fn cjk_and_latin_word_breaks_are_unchanged() {
    // 有断点的文字不走紧急断行。
    assert_eq!(
        lines(4400, vec![run(&"\u{6c49}".repeat(40))]),
        [(0, 22), (22, 41)]
    );
    let words = "aaaaaaaaaa ".repeat(12);
    let got = lines(5329, vec![run(&words)]);
    // 每行都从词首开始（前一个字符是空格）。
    let text: Vec<u16> = words.encode_utf16().collect();
    for &(start, _) in &got[1..] {
        assert_eq!(text[start as usize - 1], u16::from(b' '), "{got:?}");
    }
    // 一个词加空格 1050 twips：每行 5 个词 5250，第 6 个放不下。
    assert_eq!(starts(&got), [0, 55, 110]);
}

#[test]
fn a_character_cluster_is_never_split() {
    // e + U+0301 是一个字符，桩度量给它 200 twips：每行 21 个簇，42 个码元。
    let got = lines(4350, vec![run(&"e\u{301}".repeat(60))]);
    assert_eq!(got, [(0, 42), (42, 84), (84, 121)]);
    // U+1D400 在 UTF-16 里是代理对：每行 43 个字，86 个码元，不会切在两半中间。
    let got = lines(4350, vec![run(&"\u{1D400}".repeat(100))]);
    assert_eq!(got, [(0, 86), (86, 172), (172, 201)]);
}

#[test]
fn a_line_narrower_than_one_character_still_advances() {
    // 一个字都放不下：每行仍收一个字，不出空行，也不死循环（**假设**：Word 未测）。
    assert_eq!(
        lines(50, vec![run("0000")]),
        [(0, 1), (1, 2), (2, 3), (3, 5)]
    );
}

#[test]
fn an_object_placeholder_remains_a_break_opportunity() {
    // 行内对象两侧可断（UAX #14 的 CB，**假设**），与此前一致：长串挪到对象之后的下一行。
    let mut r = run(&format!("{}\u{fffc}{}", "0".repeat(30), "0".repeat(100)));
    r.placeholders = vec![PlaceholderKind::Object];
    assert_eq!(
        lines(4350, vec![r]),
        [(0, 31), (31, 74), (74, 117), (117, 132)]
    );
}

#[test]
fn an_object_at_the_line_start_is_followed_by_a_break_too() {
    // 同一条规则放在行首：对象占着行首那个源位置，它后面的交界在行首之后，所以是断点，
    // 这一行只收下对象（**假设**，与上一条同源）。若把这一行当空行紧急填满，对象就会与
    // 后面 43 个 `0` 排进同一行，与对象在行中时的断法不一致。
    let mut r = run(&format!("\u{fffc}{}", "0".repeat(100)));
    r.placeholders = vec![PlaceholderKind::Object];
    assert_eq!(
        lines(4350, vec![r.clone()]),
        [(0, 1), (1, 44), (44, 87), (87, 102)]
    );
    // 只有对象的那一行也有行高：各行基线等距，后面的行不会叠上来。
    let engine = Engine::new(&SimpleMetrics, setup(4350));
    let pages = engine.layout(&[Para {
        runs: vec![r],
        ..Para::default()
    }]);
    let mut baselines: Vec<Twips> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t.baseline_y),
            _ => None,
        })
        .collect();
    baselines.dedup();
    assert_eq!(baselines.len(), 4, "{baselines:?}");
    let pitch = baselines[1] - baselines[0];
    assert!(pitch > 0, "{baselines:?}");
    assert!(baselines.windows(2).all(|w| w[1] - w[0] == pitch), "{baselines:?}");
}

#[test]
fn a_justified_emergency_line_has_no_gap_inside_the_word() {
    // 20 + 30 + 80 个 `0` 分三个 run、两端对齐：第一行是 20 个加 23 个，交界在词中间，
    // 不摊空隙——与排在一个 run 里一样，第二个片段紧接着第一个（x = 2000）。
    let split = vec![run(&"0".repeat(20)), run(&"0".repeat(30)), run(&"0".repeat(80))];
    assert_eq!(justified_piece_xs(4350, split, 0), [0, 2000]);
    assert_eq!(justified_piece_xs(4350, vec![run(&"0".repeat(130))], 0), [0]);
}

#[test]
fn justify_gaps_stay_at_word_joins() {
    // 对照：交界在两词之间时照旧摊空隙。`aaaa ` | `bbbb ` 之后的 `cccc dddd` 放不下，
    // 行宽 900，余 400 全摊在唯一一条词间交界上。
    let runs = vec![run("aaaa "), run("bbbb "), run("cccc dddd")];
    assert_eq!(justified_piece_xs(1300, runs, 0), [0, 850]);
    // 空格在右侧片段开头也算词间（断点在空格之后，但交界仍在两词之间）。
    // 行宽 940：`bbbb ` 之后可断，第三个 run 的 `cccc ` 放不下，行宽 900，余 40。
    let runs = vec![run("aaaa"), run(" bbbb "), run("cccc dddd")];
    assert_eq!(justified_piece_xs(940, runs, 0), [0, 440]);
    // 第三个 run 以空格开头、连这个空格都放不下时（行宽 890），断点在更早的 ` bbbb` 的空格之后：
    // 跨 run 回退到那里，与排成一个 run 相同（`aaaa ` | `bbbb cccc dddd`，`fit` 的前缀含词后的
    // 空格）。原先 `aaaa` + ` bbbb` 留在本行、下一行以空格开头。
    let runs = vec![run("aaaa"), run(" bbbb"), run(" cccc dddd")];
    assert_eq!(lines(890, runs), lines(890, vec![run("aaaa bbbb cccc dddd")]));
}

#[cfg(feature = "fontenv")]
mod real {
    use super::*;
    use rsword_layout_core::{FontMetrics, RealMetrics, TextMetrics, font::FontRegistry};
    use std::path::PathBuf;

    fn repo_font(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/fonts")
            .join(name);
        std::fs::read(&path).expect("字体读得到")
    }

    /// 仓库自带的 Liberation Sans：期望值从字体表独立算出，不抄字面量。
    #[test]
    fn real_metrics_cut_after_the_last_fitting_glyph() {
        use skrifa::{FontRef, MetadataProvider};
        let bytes = repo_font("LiberationSans-Regular.ttf");
        let advance = {
            let font = FontRef::new(&bytes).unwrap();
            let size = skrifa::instance::Size::unscaled();
            let loc = skrifa::instance::LocationRef::default();
            let upem = f64::from(font.metrics(size, loc).units_per_em);
            let gid = font.charmap().map('0').unwrap();
            f64::from(font.glyph_metrics(size, loc).advance_width(gid).unwrap()) / upem * 240.0
        };
        let mut registry = FontRegistry::new();
        registry.add(bytes, 0).expect("字体装得进");
        let metrics = RealMetrics::new(&registry);
        let width = 5329;
        let per_line = (f64::from(width) / advance).floor();
        // 离边界够远，累加取整不会翻转结果。
        assert!(f64::from(width) - per_line * advance > 1.0);
        assert!((per_line + 1.0) * advance - f64::from(width) > 1.0);
        let per_line = per_line as u32;
        let mut r = run(&"0".repeat(100));
        r.font = FontSpec::new("Liberation Sans", 24);
        assert_eq!(metrics.measure("0", &r.font).advance, advance.round() as Twips);
        let got = starts(&lines_with(&metrics, width, vec![r]));
        assert_eq!(got, [0, per_line, 2 * per_line]);
    }

    /// 只借真度量的宽度，紧急断行用 trait 的默认实现（`cluster_boundaries` 那张近似表）。
    /// 用来证明下面的断法确实来自整形器的 cluster，而不是那张表。
    struct TableCut<'a>(RealMetrics<'a>);

    impl FontMetrics for TableCut<'_> {
        fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
            self.0.measure(text, font)
        }
        fn break_opportunities(&self, text: &str) -> Vec<rsword_layout_core::BreakOpportunity> {
            self.0.break_opportunities(text)
        }
    }

    /// 真度量的紧急断行只切在整形器的 cluster 起点上：组合序列与 SARA AM 类元音不被切开，
    /// 第二行起没有一行以附加符号开头。**切在 cluster 上是假设**，Word 只量过单一 BMP 字母。
    ///
    /// 仓库里没有泰文字体。老挝文 AM（U+0EB3）与泰文 SARA AM（U+0E33）在 HarfBuzz 的
    /// Thai 整形器里走同一条路径（分解成 NIKHAHIT + AA，并进前一个辅音的 cluster），
    /// DejaVu Sans 有老挝文，就拿它代替；它也不在近似表里，正好是那张表切错的一类。
    #[test]
    fn real_metrics_cut_only_at_shaper_cluster_starts() {
        let mut registry = FontRegistry::new();
        registry.add(repo_font("DejaVuSans.ttf"), 0).expect("字体装得进");
        let metrics = RealMetrics::new(&registry);
        let font = FontSpec::new("DejaVu Sans", 24);
        let styled = |text: &str| {
            let mut r = run(text);
            r.font = font.clone();
            r
        };
        let marks = ['\u{301}', '\u{eb3}'];
        let no_line_starts_with_a_mark = |text: &str, got: &[(u32, u32)]| {
            let units: Vec<u16> = text.encode_utf16().collect();
            for &(start, _) in &got[1..] {
                let c = char::from_u32(u32::from(units[start as usize])).unwrap();
                assert!(!marks.contains(&c), "第 {start} 个码元是附加符号：{got:?}");
            }
        };

        // 老挝文 ກຳ（KO + AM）。行宽取「10 个音节再加一个 KO」：近似表会切在 KO 之后，
        // 把 AM 甩到下一行行首；整形器的 cluster 只允许切在音节之间。
        let syllable = "\u{e81}\u{eb3}";
        let ten = metrics.measure(&syllable.repeat(10), &font).advance;
        let ten_and_ko = metrics.measure(&format!("{}\u{e81}", syllable.repeat(10)), &font).advance;
        let eleven = metrics.measure(&syllable.repeat(11), &font).advance;
        assert!(ten < ten_and_ko && ten_and_ko < eleven, "{ten} {ten_and_ko} {eleven}");
        let text = syllable.repeat(40);
        let got = lines_with(&metrics, ten_and_ko, vec![styled(&text)]);
        assert_eq!(got[0], (0, 20), "{got:?}");
        no_line_starts_with_a_mark(&text, &got);
        let table = lines_with(&TableCut(RealMetrics::new(&registry)), ten_and_ko, vec![styled(&text)]);
        assert_eq!(table[0], (0, 21), "近似表应当切在 KO 与 AM 之间：{table:?}");

        // e + U+0301：整形器可能合成 é，也可能挂成附加符号；两种情形都是一个 cluster。
        let text = "e\u{301}".repeat(60);
        let got = lines_with(&metrics, 2000, vec![styled(&text)]);
        assert!(got.len() > 2, "{got:?}");
        assert!(got.iter().all(|&(s, _)| s % 2 == 0), "{got:?}");
        no_line_starts_with_a_mark(&text, &got);
    }

    /// 手机 Word 自带的 Calibri 不进仓库，设 `RSWORD_TEST_CALIBRI=<calibri.ttf>` 才跑。
    /// 这一条对的是 Word 的实测数，不是引擎自己算的数。
    #[test]
    #[ignore = "needs RSWORD_TEST_CALIBRI"]
    fn phone_calibri_reproduces_word_line_starts() {
        let path = std::env::var_os("RSWORD_TEST_CALIBRI")
            .expect("设 RSWORD_TEST_CALIBRI=<手机 Word 的 calibri.ttf>");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
        let mut registry = FontRegistry::new();
        registry.add(bytes, 0).expect("字体装得进");
        let metrics = RealMetrics::new(&registry);
        let calibri = |text: &str| {
            let mut r = run(text);
            r.font = FontSpec::new("Calibri", 24);
            r
        };
        let cases: [(&str, Twips, Vec<Run>, u32); 5] = [
            ("zero-plain", 5329, vec![calibri(&"0".repeat(180))], 43),
            ("zero-paper", 10466, vec![calibri(&"0".repeat(100))], 86),
            ("caps-plain", 10466, vec![calibri(&"a".repeat(120))], 91),
            ("m-plain", 5329, vec![calibri(&"M".repeat(120))], 25),
            (
                "webhidden",
                10466,
                vec![
                    calibri(&"0".repeat(20)),
                    calibri(&"0".repeat(30)),
                    calibri(&"0".repeat(80)),
                ],
                86,
            ),
        ];
        for (name, width, runs, word) in cases {
            let got = starts(&lines_with(&metrics, width, runs));
            assert_eq!(got[..2], [0, word], "{name}: {got:?}");
        }
    }
}
