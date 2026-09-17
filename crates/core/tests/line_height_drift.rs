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
