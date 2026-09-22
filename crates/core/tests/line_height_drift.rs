//! 行高的舍入残差不该沿页累加。
//!
//! 做纵向量化的度量里，栅格上的精确行高常常落不到整 twips 上：
//! Mac Word 的 1/300 英寸栅格在 12pt 下是 **273.6 twips**，取整成 274，**每行多 0.4 twip**。
//! 游标按取整值累加，一页 40 行就攒到 0.8pt——而单行看只差 0.02pt，很容易被当成噪声放过。
//!
//! 修法：游标走 1/7200 英寸（twips 与 1/300 英寸的公倍数），
//! 行高走 [`FontMetrics::natural_height_fine`]，只在落位时换回 twips。
//!
//! **这条在 MR1 夹具上看不出来**——它每页最多 3 行，累积还没显出来。
//! 所以这里用一份专门多行的合成输入，而不是拿实测夹具充数。

use rsword_layout_core::{
    BreakOpportunity, Color, Engine, FontMetrics, FontSpec, PageSetup, Para, Run, SimpleMetrics,
    TextMetrics, Twips,
};

/// 行高精确值为 **273.6 twips** 的度量——正是 Mac Word 栅格在 12pt 下的值。
///
/// `measure` 只能给整 twips（`TextMetrics` 是 `#[repr(C)]` 的整数结构），
/// 所以它返回 274；精确值由 `natural_height_fine` 给出。两者的差就是本测试要盯的东西。
struct GridMetrics;

const NATURAL_FINE: i64 = 1368; // 273.6 twips × 5
const NATURAL_TWIPS: Twips = 274; // 273.6 取整

impl FontMetrics for GridMetrics {
    fn measure(&self, text: &str, _font: &FontSpec) -> TextMetrics {
        TextMetrics {
            // 每个字符 100 twips，够宽但不换行。
            advance: text.chars().count() as Twips * 100,
            ascent: NATURAL_TWIPS - 53,
            descent: 53,
            line_gap: 0,
        }
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }

    fn natural_height_fine(&self, _text: &str, _font: &FontSpec) -> i64 {
        NATURAL_FINE
    }
}

fn para(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
            rise_fine: None,
        }],
        ..Para::default()
    }
}

/// 每行第一个片段的基线 y。
fn baselines(pages: &[rsword_layout_core::Page]) -> Vec<Twips> {
    use rsword_layout_core::Fragment;
    pages
        .iter()
        .flat_map(|p| p.fragments.iter())
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t.baseline_y),
            _ => None,
        })
        .collect()
}

#[test]
fn line_tops_track_the_exact_height_not_the_rounded_one() {
    let metrics = GridMetrics;
    let setup = PageSetup::a4();
    let top = setup.content_area().y;
    let paras: Vec<Para> = (0..12).map(|i| para(&format!("L{i}"))).collect();
    let pages = Engine::new(&metrics, setup).layout(&paras);

    let ys = baselines(&pages);
    assert_eq!(ys.len(), 12, "12 段应当排出 12 行：{ys:?}");

    let ascent = NATURAL_TWIPS - 53;
    for (n, &y) in ys.iter().enumerate() {
        // 精确：top + n × 273.6 + ascent，落位时取一次整。
        let exact = f64::from(top) + n as f64 * (NATURAL_FINE as f64 / 5.0) + f64::from(ascent);
        let drift = (f64::from(y) - exact).abs();
        assert!(
            drift <= 0.5,
            "第 {n} 行漂了 {drift} twips——游标按取整后的行高累加了（y={y}，精确={exact}）"
        );
    }
}

#[test]
fn wrapped_lines_keep_the_same_fine_step_as_unwrapped_lines() {
    use rsword_layout_core::Fragment;
    let text = "word ".repeat(100);
    let pages = Engine::new(&GridMetrics, PageSetup::a4())
        .layout(&[para(text.trim_end())]);
    assert_eq!(pages.len(), 1);
    let mut ys = std::collections::BTreeMap::new();
    for fragment in &pages[0].fragments {
        if let Fragment::Text(t) = fragment {
            ys.entry(t.line).or_insert(t.baseline_fine);
        }
    }
    let ys: Vec<_> = ys.into_values().collect();
    assert!(ys.len() > 3);
    for pair in ys.windows(2) {
        assert_eq!(pair[1] - pair[0], NATURAL_FINE);
    }
}

#[test]
fn drift_would_be_visible_at_this_length() {
    // 自证这条测试有意义：按取整值累加时，第 12 行会偏出半个 twip 以上。
    // 若某天行高恰好是整 twips，本测试就不再有鉴别力——这条断言会先失败提醒。
    let n = 11i64;
    let rounded = n * i64::from(NATURAL_TWIPS);
    let exact = n * NATURAL_FINE / 5;
    assert!(
        (rounded - exact).abs() > 0,
        "取整与精确在 {n} 行处没有差别，这份合成输入测不出漂移"
    );
}

#[test]
fn a_metrics_without_fine_override_still_works() {
    // 默认实现由 twips 换算，不提供额外精度——不该因此排错或崩。
    let setup = PageSetup::a4();
    let paras: Vec<Para> = (0..5).map(|i| para(&format!("L{i}"))).collect();
    let pages = Engine::new(&SimpleMetrics, setup).layout(&paras);
    let ys = baselines(&pages);
    assert_eq!(ys.len(), 5);
    // 单调递增：行一行往下走。
    for pair in ys.windows(2) {
        assert!(pair[1] > pair[0], "行没有往下走：{ys:?}");
    }
}

/// 基线必须落在度量声明的栅格上，**一条不差**。
///
/// 这是三份互相独立的夹具量出来的：Word for Mac 把每条基线放在 1/300 英寸
/// （0.24pt）的栅格上，**1080 条基线零例外**（见 `docs/PREREG-2026-09-17-*.md`
/// 的 Q0 / R0）。它是本项目目前证据最硬的一条。
///
/// 引擎原来做不到，而且是**结构上**做不到：落位走 `Twips`（1/1440 英寸），
/// 而 0.24pt = **4.8 twips**，栅格点根本落不到整 twips 上。实测那会儿
/// 240 条基线里落在栅格上的是 **0** 条。改成在 1/7200 英寸上落位并交给度量
/// 量化之后是 240 条。
struct QuantisingMetrics;

/// 0.24pt = 4.8 twips = **24 个 1/7200 英寸**，整数——所以量化是精确的。
const GRID_FINE: i64 = 24;

impl FontMetrics for QuantisingMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        GridMetrics.measure(text, font)
    }
    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        SimpleMetrics.break_opportunities(text)
    }
    fn natural_height_fine(&self, text: &str, font: &FontSpec) -> i64 {
        GridMetrics.natural_height_fine(text, font)
    }
    fn quantize_baseline_fine(&self, y_fine: i64) -> i64 {
        (y_fine + GRID_FINE / 2) / GRID_FINE * GRID_FINE
    }
}

#[test]
fn every_baseline_lands_on_the_metrics_grid() {
    use rsword_layout_core::Fragment;

    let paras: Vec<Para> = (0..20).map(|i| para(&format!("L{i}"))).collect();
    let pages = Engine::new(&QuantisingMetrics, PageSetup::a4()).layout(&paras);

    let mut n = 0;
    for page in &pages {
        for f in &page.fragments {
            if let Fragment::Text(t) = f {
                n += 1;
                assert_eq!(
                    t.baseline_fine % GRID_FINE,
                    0,
                    "第 {n} 条基线落在栅格外：{} (1/7200 英寸)",
                    t.baseline_fine
                );
            }
        }
    }
    assert_eq!(n, 20, "应当排出 20 行");
}

/// 自证上面那条有鉴别力：不量化的度量给出的基线**不**全在栅格上。
///
/// 少了这条，`quantize_baseline_fine` 哪天变成恒等，上面那条也可能碰巧全绿。
#[test]
fn without_quantising_the_baselines_miss_the_grid() {
    use rsword_layout_core::Fragment;

    let paras: Vec<Para> = (0..20).map(|i| para(&format!("L{i}"))).collect();
    let pages = Engine::new(&GridMetrics, PageSetup::a4()).layout(&paras);
    let off = pages
        .iter()
        .flat_map(|p| p.fragments.iter())
        .filter(|f| matches!(f, Fragment::Text(t) if t.baseline_fine % GRID_FINE != 0))
        .count();
    assert!(off > 0, "这组输入区分不出量化与不量化，本组测试没有鉴别力");
}

/// `exact` 行距**不看字体**：行高就是 `w:line`，字比行高也不撑开。
///
/// 这是量出来的，不是照规范推的。font-free 那一批
/// （`docs/PREREG-2026-09-17-font-free.md`）在 `exact` 下换六个 `ascent/upem`
/// 从 0.795 到 1.875 的族，40 个位置基线**逐格相同**——其中 Zapfino 在 13pt 下
/// ascent 有 24.4pt，比那批最大的行距 14.6pt 还高，Word 照样放在同一格。
///
/// 引擎原来对每种行距规则都套一个「内容下限」，于是 `exact` 下行高会被
/// ascent + descent 撑开，字大一点行距就不是 `w:line` 了。
#[test]
fn exact_spacing_ignores_a_font_taller_than_the_line() {
    use rsword_layout_core::{Fragment, LineRule};

    // 内容高 274 twips（GridMetrics），行距只给 100 twips——字比行高得多。
    let paras: Vec<Para> = (0..6)
        .map(|i| Para {
            line_rule: LineRule::Exact,
            line_value: 100,
            ..para(&format!("L{i}"))
        })
        .collect();
    let pages = Engine::new(&GridMetrics, PageSetup::a4()).layout(&paras);

    let ys: Vec<Twips> = pages
        .iter()
        .flat_map(|p| p.fragments.iter())
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t.baseline_y),
            _ => None,
        })
        .collect();
    assert_eq!(ys.len(), 6, "6 段应当排出 6 行：{ys:?}");
    for pair in ys.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            100,
            "exact 行距被内容撑开了：行距应当恒为 w:line = 100 twips，实得 {:?}",
            ys
        );
    }
}

/// 自证上面那条有鉴别力：同样的输入换成 `atLeast`，行距**应当**被内容撑开。
///
/// 少了这条，把 `exact` 的分支写成「永远返回 w:line」也能全绿，
/// 而那会把 `atLeast` 一起弄坏。
#[test]
fn at_least_spacing_still_grows_to_fit() {
    use rsword_layout_core::{Fragment, LineRule};

    let paras: Vec<Para> = (0..6)
        .map(|i| Para {
            line_rule: LineRule::AtLeast,
            line_value: 100,
            ..para(&format!("L{i}"))
        })
        .collect();
    let pages = Engine::new(&GridMetrics, PageSetup::a4()).layout(&paras);
    let ys: Vec<Twips> = pages
        .iter()
        .flat_map(|p| p.fragments.iter())
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t.baseline_y),
            _ => None,
        })
        .collect();
    assert!(
        ys.windows(2).all(|p| p[1] - p[0] > 100),
        "atLeast 没有被内容撑开，这组输入区分不出两种规则：{ys:?}"
    );
}
