//! 行首 / 行尾禁则与「不挂出」：Android Word 的实测表。
//!
//! 真值是 word_analyse `reports/rsword-diff/kinsoku.md`（Android Word 16.0.20513，移动视图
//! 窄路径 `w3=5329` twips，12pt）。那一组夹具都是「若干个汉 + 一个标点 + 若干个汉」，
//! 第一行在哪里断：
//!
//! | 夹具 | 正文 | Word 第一行结束 |
//! |---|---|---|
//! | `han22` | 30 汉 | 22 |
//! | `kinsoku` | 22 汉 `）` 10 汉 | 21 |
//! | `kinsoku-ascii` | 22 汉 `)` 10 汉 | 21 |
//! | `kinsoku-fit` | 21 汉 `）` 10 汉 | 22 |
//! | `kinsoku-pair` | 22 汉 `））` 8 汉 | 21 |
//! | `kinsoku-open` | 21 汉 `（` 10 汉 | 21 |
//! | `kinsoku-open-ascii` | 21 汉 `(` 10 汉 | 21 |
//! | `kinsoku-period` | 22 汉 `。` 10 汉 | 21 |
//!
//! 这里用桩度量（汉字 1 em = 240 twips，ASCII 0.5 em），只取两件事：22 个汉字放得下
//! （5280 ≤ 5329），第 23 个放不下。断在哪里只由断点规则决定，与字体无关——
//! 所以这是规则的回归，不是 Word 几何的复现。真字体那一条用仓库自带的 Droid。
//!
//! Mac Word 在同样没写 `w:overflowPunct` 的文档上会把 `。，）、` 挂出去
//! （`docs/PREREG-2026-09-18-kinsoku.md`，`tests/overflow_*.rs`）。两个平台的差别由
//! [`Platform`] 表达，库的默认仍是桌面（Mac）。
//!
//! 禁则字符表的依据分三档（输出层实测 / 断行类读数 + 推断的规则 / 同族外推），见
//! `font::linebreak::is_no_line_start` 的说明；测试名里 `class_read_`、`extrapolated_`
//! 标的是后两档，`assumed_` 是没有夹具的选择。
//!
//! 禁则跨 run 的几条（`assumed_…_backs_off…`、`assumed_…_moves_down`）靠跨 run 回退：
//! 拆不拆 run 断法都一样是**假设**，Word 未测。

use rsword_layout_core::{
    BreakOpportunity, Color, Engine, FontMetrics, FontSpec, LayoutRecord, Margins, PageSetup,
    Para, Platform, Run, SimpleMetrics, Size, View, paint_document, paras_from_document,
};

/// Android 窄路径的版心宽。
const NARROW: i32 = 5329;

/// 窄路径是移动视图；纸页路径是分页视图。
const ANDROID_NARROW: (Platform, View) = (Platform::Android, View::Mobile);
const DESKTOP: (Platform, View) = (Platform::Desktop, View::Print);

fn narrow_setup() -> PageSetup {
    let slack = 11906 - NARROW;
    PageSetup {
        size: Size::new(11906, 16838),
        margins: Margins::new(720, slack - slack / 2, 720, slack / 2),
    }
}

fn han(n: usize) -> String {
    "汉".repeat(n)
}

fn run(text: &str, font: &FontSpec) -> Run {
    Run {
        text: text.to_string(),
        font: font.clone(),
        color: Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn synthetic() -> FontSpec {
    FontSpec::new("synthetic", 24)
}

fn para(text: &str, font: FontSpec) -> Para {
    Para { runs: vec![run(text, &font)], ..Para::default() }
}

/// 一段拆成几个 run，字体相同。
fn split_para(texts: &[&str]) -> Para {
    let font = synthetic();
    Para { runs: texts.iter().map(|t| run(t, &font)).collect(), ..Para::default() }
}

/// 每行的源起点（UTF-16），与 `layout-trace` 的 `sourceStart` 同一口径。
fn line_starts<M: FontMetrics>(
    metrics: &M,
    (platform, view): (Platform, View),
    paras: &[Para],
) -> Vec<u32> {
    let pages = Engine::new(metrics, narrow_setup())
        .with_platform(platform, view)
        .layout(paras);
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    record
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| l.source.expect("每行都有源区间").start)
        .collect()
}

/// `kinsoku.md` 的整张表：（名字，正文，Android 第一行结束）。
fn android_table() -> Vec<(&'static str, String, u32)> {
    vec![
        ("han22", han(30), 22),
        ("kinsoku", format!("{}）{}", han(22), han(10)), 21),
        ("kinsoku-ascii", format!("{}){}", han(22), han(10)), 21),
        ("kinsoku-fit", format!("{}）{}", han(21), han(10)), 22),
        ("kinsoku-pair", format!("{}））{}", han(22), han(8)), 21),
        ("kinsoku-open", format!("{}（{}", han(21), han(10)), 21),
        ("kinsoku-open-ascii", format!("{}({}", han(21), han(10)), 21),
        ("kinsoku-period", format!("{}。{}", han(22), han(10)), 21),
    ]
}

#[test]
fn android_rule_reproduces_the_kinsoku_table() {
    for (name, text, first_end) in android_table() {
        let starts = line_starts(&SimpleMetrics, ANDROID_NARROW, &[para(&text, synthetic())]);
        assert_eq!(starts, vec![0, first_end], "{name}");
    }
}

#[test]
fn assumed_android_print_view_does_not_hang_either() {
    // **假设**：纸页路径（分页视图）与移动视图一样不挂出。只量过窄路径；
    // 挂不挂由平台定、不看视图，这里只钉住这个选择。
    for (name, text, first_end) in android_table() {
        let starts = line_starts(
            &SimpleMetrics,
            (Platform::Android, View::Print),
            &[para(&text, synthetic())],
        );
        assert_eq!(starts, vec![0, first_end], "{name}");
    }
}

#[test]
fn desktop_rule_still_hangs_the_single_closing_punctuation() {
    // 同一张表在桌面规则下：只有越界的单个 `）`、`。` 挂出（23），其余与 Android 相同。
    // 这两格就是两个平台实测相反的地方；Mac 那一半由 `tests/overflow_*.rs` 钉住。
    for (name, text, android_end) in android_table() {
        let expected = match name {
            "kinsoku" | "kinsoku-period" => 23,
            _ => android_end,
        };
        let starts = line_starts(&SimpleMetrics, DESKTOP, &[para(&text, synthetic())]);
        assert_eq!(starts, vec![0, expected], "{name}");
    }
}

#[test]
fn the_library_default_keeps_the_desktop_hang() {
    // svg / wasm / cffi 走 `Engine::new`，不说明平台：照旧挂出。
    assert_eq!(Platform::default(), Platform::Desktop);
    let text = format!("{}）{}", han(22), han(10));
    let p = [para(&text, synthetic())];
    let pages = Engine::new(&SimpleMetrics, narrow_setup()).layout(&p);
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    let first = record.pages[0].lines[0].source.unwrap();
    assert_eq!(first.end, 23);
}

#[test]
fn assumed_android_ignores_an_explicit_overflow_punct_on() {
    // 显式开在 Android 上怎样**未测**；按「从不挂出」处理，这里只钉住这个选择。
    let text = format!("{}。{}", han(22), han(10));
    let mut p = para(&text, synthetic());
    p.overflow_punct = true;
    assert_eq!(line_starts(&SimpleMetrics, ANDROID_NARROW, &[p]), vec![0, 21]);
}

#[test]
fn desktop_no_longer_hangs_before_a_newly_restricted_character() {
    // 行首禁则表扩充的桌面侧后果（挂出的候选要求后一个字不是行首禁则）：`。〉`、`，…`
    // 原先挂出到 23、`〉` / `…` 落到下一行行首，现在不挂出、退回 21。**Mac 上未测**——
    // 没有一份 Mac 采集在溢出标点后面放这些字符；这里钉住这个选择。
    for text in [format!("{}。〉{}", han(22), han(10)), format!("{}，…{}", han(22), han(10))] {
        assert_eq!(
            line_starts(&SimpleMetrics, DESKTOP, &[para(&text, synthetic())]),
            vec![0, 21],
            "{text}"
        );
    }
}

#[test]
fn desktop_no_longer_hangs_after_a_newly_restricted_character() {
    // 同一张表扩充的另一侧（挂出的候选还要求前一个字是汉字、且不是行首禁则）：新增的行首
    // 禁则字符里属于 `is_cjk` 的 13 个，后面越界的 `。`、`，` 原先挂出到 23，现在不挂出；
    // 普通断行 `汉|％`、`％|。` 都不断，于是退三个字，到 20。**Mac 上未测**——Mac 只量过
    // 汉字后面的挂出（待测：Mac 上 `21汉％。10汉`）。只拿原先的表查前一个字、照旧挂出也说得通，
    // 这里钉住照一张表走的选择。
    for prev in ['〉', '〕', '〗', '〞', '＂', '％', '＇', '．', '］', '｀', '｜', '｝', '～'] {
        for punct in ['。', '，'] {
            let text = format!("{}{prev}{punct}{}", han(21), han(10));
            assert_eq!(
                line_starts(&SimpleMetrics, DESKTOP, &[para(&text, synthetic())]),
                vec![0, 20],
                "{text}"
            );
        }
    }
    // 不属于 CJK 的新增字符本来就当不了挂出的前一个字；它们的变化来自西文进 CJK 的边界
    // 也查禁则：`…|。` 原先可断，`。` 落到下一行行首（22），现在退到 20。同样未测。
    for prev in ['…', '°'] {
        let text = format!("{}{prev}。{}", han(21), han(10));
        assert_eq!(
            line_starts(&SimpleMetrics, DESKTOP, &[para(&text, synthetic())]),
            vec![0, 20],
            "{text}"
        );
    }
}

#[test]
fn a_run_of_opening_brackets_does_not_degenerate() {
    // 行尾禁则让一串开括号之间一个断点都没有。紧急断行照剩余宽度切（22 个），不会
    // 退成一行一个码元（禁则落地而紧急断行还没落地时，这里排成 11 行）。
    // 紧急断行不守禁则（**假设**，Word 未测）。
    let text = format!("{}{}", "（".repeat(30), han(5));
    for profile in [ANDROID_NARROW, DESKTOP] {
        assert_eq!(
            line_starts(&SimpleMetrics, profile, &[para(&text, synthetic())]),
            vec![0, 22],
            "{profile:?}"
        );
    }
}

fn offsets(text: &str) -> Vec<usize> {
    SimpleMetrics
        .break_opportunities(text)
        .iter()
        .map(|b: &BreakOpportunity| b.offset)
        .collect()
}

#[test]
fn opening_punctuation_is_not_left_at_line_end() {
    // `（`、`(` 是输出层实测（`kinsoku-open`、`kinsoku-open-ascii`），其余是断行类读数
    // `brkclsLeading = 0` 推出来的。开括号后面不记断点，前面照样可断。
    for open in ['（', '(', '「', '『', '《', '〈', '【', '［', '｛', '“', '‘', '[', '{', '$'] {
        let text = format!("汉{open}汉汉");
        let after_open = "汉".len() + open.len_utf8();
        let got = offsets(&text);
        assert!(!got.contains(&after_open), "{open}: {got:?}");
        assert!(got.contains(&"汉".len()), "{open} 前面应能断: {got:?}");
    }
}

#[test]
fn closing_punctuation_after_latin_is_not_a_break_point() {
    // `breakme` 的尾巴 `（汉6）汉7`：以前西文进 CJK 的边界不查禁则，`6|）` 也算断点。
    // `6|）` 不断是同一条禁则推过来的（`breakme` 选中的是 `汉|6`，分不出这一处）。
    let text = "（汉6）汉7";
    let got = offsets(text);
    let before_close = "（汉6".len();
    assert!(!got.contains(&before_close), "{got:?}");
    // `（|汉` 也不再是断点；`汉|6` 与 `）|汉` 仍是。
    assert!(!got.contains(&"（".len()), "{got:?}");
    assert!(got.contains(&"（汉".len()), "{got:?}");
    assert!(got.contains(&"（汉6）".len()), "{got:?}");
}

#[test]
fn class_read_closing_classes_are_no_line_start() {
    // `brkclsFollowing = 1` 里原先漏掉的几个：汉字后面不能在它们前面断。
    // 读数是实测，「`foll = 1` 就不断」是推断（`findings/brkcls.md` 说那张表不作为规则定义）。
    for close in ['〉', '］', '｝', '～', '…', '°', '＇', '＂', '·', '．'] {
        let text = format!("汉汉{close}汉");
        let before = "汉汉".len();
        assert!(!offsets(&text).contains(&before), "{close}");
    }
    // `·`、`．` 读数是 0/1：前后都不断。
    for both in ['·', '．'] {
        let text = format!("汉{both}汉");
        assert_eq!(offsets(&text), vec![text.len()], "{both}");
    }
    // `｟` 在读数里是 2/2，不是开括号那一类：两边都可断。
    let got = offsets("汉｟汉");
    assert!(got.contains(&"汉".len()) && got.contains(&"汉｟".len()), "{got:?}");
}

#[test]
fn extrapolated_bracket_counterparts_follow_their_family() {
    // 读数里没采到的同族括号，照括号成对补上（外推，不是 Word 的哪一张表）。
    for (open, close) in [('〔', '〕'), ('〖', '〗'), ('〝', '〞'), ('﹙', '﹚'), ('﹛', '﹜'), ('﹝', '﹞')] {
        let text = format!("汉{open}汉{close}汉");
        let got = offsets(&text);
        let after_open = "汉".len() + open.len_utf8();
        let before_close = after_open + "汉".len();
        assert!(!got.contains(&after_open), "{open}: {got:?}");
        assert!(!got.contains(&before_close), "{close}: {got:?}");
    }
    // 〘〙〚〛（U+3018–301B）没有来源，不补：两边照 CJK 字间可断。
    for c in ['〘', '〙', '〚', '〛'] {
        let text = format!("汉{c}汉");
        assert_eq!(offsets(&text), vec!["汉".len(), text.len() - "汉".len(), text.len()], "{c}");
    }
}

#[test]
fn ascii_question_mark_stays_no_line_start_unlike_percent() {
    // 读数里 `?` 与 `%` 同是 2/3（汉字后可在其前断）。`%` 照读数；`?` 保留引擎原有的
    // 行首禁则——没有输出层读数说明该改。这条测试钉住这个不对称，改哪一边都要先有夹具。
    let got = offsets("汉汉?汉");
    assert!(!got.contains(&"汉汉".len()), "`?` 前面不该可断：{got:?}");
    assert!(got.contains(&"汉汉?".len()), "{got:?}");
    let got = offsets("汉汉%汉");
    assert!(got.contains(&"汉汉".len()), "`%` 前面照读数可断：{got:?}");
}

#[test]
fn android_table_through_the_parser_json_bridge() {
    // 与 word_analyse 夹具同形的 rsword JSON：一段一个 run，Calibri + SimSun、sz 24、没有
    // `overflowPunct`。桥接层给出默认开，Android 规则仍然不挂出。
    for (name, text, first_end) in android_table() {
        let doc = serde_json::json!({"main": [{"kind": "text", "props": {},
            "inlines": [{"kind": "run", "text": text,
                "props": {"fonts": {"ascii": "Calibri", "hAnsi": "Calibri", "eastAsia": "SimSun"},
                          "size": 24}}]}]});
        let (paras, _) = paras_from_document(&doc);
        assert!(paras[0].overflow_punct, "{name}");
        assert_eq!(line_starts(&SimpleMetrics, ANDROID_NARROW, &paras), vec![0, first_end], "{name}");
    }
}

/// 禁则跨 run 的三种形状，第一行结束都是 21（与排在一个 run 里相同）。
///
/// Word **未测**：Line Services 按字符断行，推断是 21（待测：`[22汉][）10汉]`、
/// `[22汉][。10汉]`、`[21汉（][10汉]` @5329）。交界本身按禁则不断（`汉|）`、`（|汉`），
/// 跨 run 回退退到行里更早的 `汉|汉`（`layout.rs` 里 `Engine::shortfall` 的（甲））。
/// 跨 run 回退落地之前三种都是 22：run 边界成了断点，`）`、`。` 开下一行，`（` 留在行尾。
/// 前两种在禁则落地之前是 23——桌面的挂出拿得到跨 run 的前后字，把 `）`、`。` 挂在本行；
/// 桌面现在仍是 23（`overflow_split_runs.rs`）。桌面仍是 23 只因越界标点前后都是汉字；
/// 挨着新增的行首禁则字符时挂出被挡住，见下面 `…_on_desktop` 的几条。
fn split_cases() -> [(&'static str, Vec<&'static str>); 3] {
    [
        ("close", vec!["汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉", "）汉汉汉汉汉汉汉汉汉汉"]),
        ("period", vec!["汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉", "。汉汉汉汉汉汉汉汉汉汉"]),
        ("open", vec!["汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉汉（", "汉汉汉汉汉汉汉汉汉汉"]),
    ]
}

fn check_split_case(name: &str) {
    let (_, texts) = split_cases().into_iter().find(|(n, _)| *n == name).unwrap();
    assert_eq!(texts[0].chars().count(), 22, "{name}");
    // 对照：排在一个 run 里是 21。
    let whole = split_para(&[texts.concat().as_str()]);
    assert_eq!(line_starts(&SimpleMetrics, ANDROID_NARROW, &[whole]), vec![0, 21], "{name}");
    let got = line_starts(&SimpleMetrics, ANDROID_NARROW, &[split_para(&texts)]);
    assert_eq!(got, vec![0, 21], "split runs give {got:?} ({name})");
}

#[test]
fn assumed_closing_punctuation_starting_the_next_run_backs_off() {
    check_split_case("close");
}

#[test]
fn assumed_a_period_starting_the_next_run_backs_off() {
    check_split_case("period");
}

#[test]
fn assumed_an_opening_bracket_ending_a_run_moves_down() {
    check_split_case("open");
}

#[test]
fn assumed_a_restricted_character_starting_the_next_run_backs_off_on_desktop() {
    // 桌面上同一种形状，禁则表扩充后才出现：`[21汉][％。10汉]` 原先挂出到 23（挂出拿得到
    // 跨 run 的前后字，`％` 在旧表里不是行首禁则）；现在 `％` 挡住挂出，交界 `汉|％` 按禁则不断，
    // 跨 run 回退退到行里更早的 `汉|汉`，与排在一个 run 里相同，20。跨 run 回退落地之前断在
    // run 边界上，`％` 开下一行（21）。Word 未测。
    let tail = format!("％。{}", han(10));
    let whole = split_para(&[format!("{}{tail}", han(21)).as_str()]);
    assert_eq!(line_starts(&SimpleMetrics, DESKTOP, &[whole]), vec![0, 20]);
    let got = line_starts(&SimpleMetrics, DESKTOP, &[split_para(&[han(21).as_str(), tail.as_str()])]);
    assert_eq!(got, vec![0, 20], "split runs give {got:?}");
}

/// 桌面上的另一族，也是禁则表扩充后才出现：run 边界正挨着一处被新增的行首禁则字符挡住的挂出。
/// 越界的 `。`、`，` 开一个新 run，挂不出去，本片段又一个断点都塞不下；交界本身不断，跨 run 回退
/// 退到行里更早的 `汉|汉`，`。`、`，` 不在行首——与 Mac 实测的「`。，）、` 不在行首」一致
/// （`docs/PREREG-2026-09-18-kinsoku2.md` J-a 4/4）。跨 run 回退落地之前收在 run 边界上（22），
/// `。`、`，` 开下一行；禁则表扩充之前它们挂出到 23。
///
/// 每一格是（各 run 的正文，排在一个 run 里的第一行结束）。先核对照，再把拆开的结果一起比，
/// 失败信息里就是全部现值。与排在一个 run 里相同是**假设**；Word 未测。
fn check_desktop_split_hang_cases(cases: &[(Vec<String>, u32)]) {
    let mut got = Vec::new();
    let mut expected = Vec::new();
    for (texts, whole_end) in cases {
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        let whole = split_para(&[texts.concat().as_str()]);
        assert_eq!(line_starts(&SimpleMetrics, DESKTOP, &[whole]), vec![0, *whole_end], "{texts:?}");
        got.push(line_starts(&SimpleMetrics, DESKTOP, &[split_para(&texts)]));
        expected.push(vec![0, *whole_end]);
    }
    assert_eq!(got, expected, "split runs give {got:?} ({cases:?})");
}

#[test]
fn assumed_a_hang_blocked_on_the_previous_side_backs_off_across_runs_on_desktop() {
    // 前一侧：`％`、`～` 收在前一个 run 的末尾。挂出的候选拿得到跨 run 的前一个字，
    // 它们如今是行首禁则，挡住挂出；排在一个 run 里 `汉|％`、`％|。` 都不断，退三个字到 20。
    // 跨 run 回退落地之前是 22。
    check_desktop_split_hang_cases(&[
        (vec![format!("{}％", han(21)), format!("。{}", han(10))], 20),
        (vec![format!("{}～", han(21)), format!("，{}", han(10))], 20),
    ]);
}

#[test]
fn assumed_a_hang_blocked_on_the_next_side_backs_off_across_runs_on_desktop() {
    // 后一侧：`〉`、`…` 跟在越界的标点后面，同一个 run 里（`desktop_no_longer_hangs_before_…`
    // 的两格拆在标点前面）。排在一个 run 里退回 21；跨 run 回退落地之前是 22。
    check_desktop_split_hang_cases(&[
        (vec![han(22), format!("。〉{}", han(10))], 21),
        (vec![han(22), format!("，…{}", han(10))], 21),
    ]);
}

#[cfg(feature = "fontenv")]
#[test]
fn android_table_with_the_android_system_cjk_font() {
    use rsword_layout_core::{FontSlots, RealMetrics, font::FontRegistry};

    // Droid Sans Fallback 是 Android 的系统 CJK 回退字体，随仓库带着（`fixtures/fonts`）。
    // 夹具的 ascii 槽是 Calibri；这里用同样随仓库带着的 Liberation Sans 占西文槽——
    // Droid 自己的 ASCII `)` 窄到 22 个汉字后面还放得下，那就量不到越界了。
    let fonts = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts");
    let mut registry = FontRegistry::new();
    for name in ["DroidSansFallbackFull.ttf", "LiberationSans-Regular.ttf"] {
        registry.add(std::fs::read(fonts.join(name)).expect("字体读得到"), 0).unwrap();
    }
    let metrics = RealMetrics::new(&registry);
    let mut font = FontSpec::new("Liberation Sans", 24);
    font.slots = FontSlots {
        ascii: Some("Liberation Sans".into()),
        h_ansi: Some("Liberation Sans".into()),
        east_asia: Some("Droid Sans Fallback".into()),
        ..FontSlots::default()
    };
    // 前提：22 个汉字放得下、23 个放不下；22 个汉字后的 `)` 越界，21 个汉字后的 `(` 放得下
    // 而再加一个汉字放不下。不成立就不是在量禁则。
    let w = |s: &str| metrics.measure(s, &font).advance;
    assert!(w(&han(22)) <= NARROW && w(&han(23)) > NARROW);
    assert!(w(&format!("{})", han(22))) > NARROW);
    assert!(w(&format!("{}(", han(21))) <= NARROW);
    assert!(w(&format!("{}(汉", han(21))) > NARROW);
    for (name, text, first_end) in android_table() {
        assert_eq!(
            line_starts(&metrics, ANDROID_NARROW, &[para(&text, font.clone())]),
            vec![0, first_end],
            "{name}"
        );
    }
}
