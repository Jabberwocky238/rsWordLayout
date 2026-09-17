//! 比较器契约的性质测试。
//!
//! 重点不是「能跑」，而是把量具方法里那几条**防自欺的纪律**钉成断言：
//! 三态出口不能退化成二态、结构失败不许被读成一致、计数约定与实测数字一致。

use rsword_layout_core::{
    Align, CompareState, Color, Engine, FontSpec, GlyphRecord, LayoutRecord, LineTerminator,
    MismatchLevel, PageSetup, Para, Run, SimpleMetrics, SourceRange, paint_document,
};

fn record() -> LayoutRecord {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let para = Para {
        runs: vec![Run {
            text: "Hello 世界".to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
        rise: 0,
        }],
        align: Align::Left,
        ..Para::default()
    };
    let pages = engine.layout(&[para]);
    LayoutRecord::from_paint(&paint_document(&pages, None, &[]))
}

#[test]
fn record_preserves_page_geometry_in_twips() {
    let r = record();
    assert_eq!(r.page_count(), 1);
    assert_eq!(r.pages[0].width, 11906, "A4 宽应为 11906 twips，不是任何像素数");
    assert_eq!(r.pages[0].height, 16838);
}

#[test]
fn repeatability_is_bit_identical() {
    // 采集链的噪声底是 0.0000pt；引擎这一侧同样不该有抖动。
    // 不相同就先别往下走——那说明产出不确定，后面的比对全无意义。
    assert!(record().is_bit_identical(&record()));
}

#[test]
fn counting_conventions_match_measured_values() {
    // 这些数字来自实测而非规范推导，改动前须回到量具方法 §4 核对范围。
    use rsword_layout_core::PageBreakPosition as P;
    assert_eq!(LineTerminator::ParagraphMark.expected_glyphs(), 1, "段落标记画 1 个空格");
    assert_eq!(LineTerminator::LineBreak.expected_glyphs(), 1, "软回车画 1 个");
    assert_eq!(LineTerminator::SectionBreak.expected_glyphs(), 0, "分节符画 0 个");
    // 手动分页符按位置分三种，这是最容易写错的一条。
    assert_eq!(LineTerminator::PageBreak(P::OwnLine).expected_glyphs(), 0);
    assert_eq!(LineTerminator::PageBreak(P::MidParagraph).expected_glyphs(), 0);
    assert_eq!(
        LineTerminator::PageBreak(P::BeforeMark).expected_glyphs(),
        1,
        "紧跟段落标记时画 1 个空格"
    );
}

#[test]
fn structural_mismatch_is_never_a_pass() {
    // 实测难例：两份不同文档，3 行结构失败，剩下 1 行 8 个字形距离恰好 0.0000pt。
    // 若先读 max_abs 就会把「配对失败」读成「完全一致」。
    let s = CompareState::StructuralMismatch {
        level: MismatchLevel::Line,
        expected: 4,
        actual: 1,
    };
    assert!(!s.is_pass(0.0), "结构失败不论容差多大都不能算通过");
    assert!(!s.is_pass(f64::INFINITY));
}

#[test]
fn undecidable_is_a_distinct_outcome() {
    // 三态出口的第三态不能取消，否则这套东西只会输出「成立」。
    let u = CompareState::Undecidable { reason: "字形数与字符数皆不匹配" };
    assert!(!u.is_pass(0.0));
    assert!(!matches!(u, CompareState::Compared { .. }), "判不了 ≠ 已比对");
}

#[test]
fn zero_tolerance_is_meaningful() {
    // 噪声底为 0 意味着不需要给测量噪声留容差：任何非零差都是信号。
    assert!(CompareState::Compared { max_abs: 0.0 }.is_pass(0.0));
    assert!(!CompareState::Compared { max_abs: 0.0001 }.is_pass(0.0));
}

#[test]
fn source_ranges_cover_the_line_in_reading_order() {
    // 配对靠读序，所以源区间必须按读序单调不减且覆盖全行。
    // 无 shaper 时字形序列为空（SVG 类后端直接排文字），此时改查片段层。
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let para = Para {
        runs: vec![Run {
            text: "abcdef".to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
            placeholders: Vec::new(),
        rise: 0,
        }],
        ..Para::default()
    };
    let pages = engine.layout(&[para]);
    let frags: Vec<_> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            rsword_layout_core::Fragment::Text(t) => t.source,
            _ => None,
        })
        .collect();
    assert!(!frags.is_empty(), "应当记下源区间");
    assert_eq!(frags[0].0, 0, "首片段从第 0 个字符起");
    // 区间首尾相接、单调不减。
    for w in frags.windows(2) {
        assert!(w[1].0 >= w[0].1, "源区间不得回退：{:?} 之后是 {:?}", w[0], w[1]);
    }
    let total: u32 = frags.iter().map(|(a, b)| b - a).sum();
    // 6 个字母 **加段落标记那一个字符**：Word 为段落标记画一个空格（§4），
    // 引擎也画，那个字形的源字符自然要落在区间里。
    assert_eq!(total, 7, "应当覆盖 6 个字母加段落标记那一个字符");
}

#[test]
fn distance_is_euclidean() {
    let a = GlyphRecord {
        origin_x: 0,
        origin_y: 0,
        origin_y_fine: i64::from(0) * rsword_layout_core::font::FINE_PER_TWIP,
        advance_x: 0,
        advance_y: 0,
        face: "f".into(),
        glyph_id: 1,
        size_half_points: 24,
        source: None,
    };
    let b = GlyphRecord { origin_x: 30, origin_y: 40, ..a.clone() };
    assert_eq!(a.distance(&b), 50.0, "3-4-5 直角三角形");
}

#[test]
fn source_range_length() {
    let r = SourceRange::new(5, 9);
    assert_eq!(r.len(), 4);
    assert!(!r.is_empty());
    // 倒置区间不该给出负长度或 panic。
    assert_eq!(SourceRange::new(9, 5).len(), 0);
    assert!(SourceRange::new(9, 5).is_empty());
}
