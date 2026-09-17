//! U+FFFC 是控制字符，不是文字。
//!
//! rsword 用对象替换符在 run 文本里为 `w:br` 与行内对象占位。
//! 它**占 1 个源字符位**（Word 的 `Range` 数它），但**不成字形、不占宽度**——
//! 量具方法 §4 对行内对象的实测就是「`Range.Text` 里 1 个占位字符，PDF 里 0 个字形」。
//!
//! 不切掉的后果是实测过的：整形器把它当普通字符画出来，12pt 下 advance 12.0pt，
//! 其后整行字形集体右移；一份 11 页夹具上 8 次共凭空占掉 96pt。
//!
//! **必须在断行处切，不能只在绘制层滤**：只滤绘制，行宽照样把它算进去，
//! 而那种错在轨迹里看不出来——字形没了，但断行位置已经被它带偏。

use rsword_layout_core::{
    Color, DrawCmd, Engine, FontSpec, LayoutRecord, OBJECT_PLACEHOLDER, PageSetup, Para, Run,
    SimpleMetrics, paint_document,
};

fn para(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
        }],
        ..Para::default()
    }
}

fn paint(paras: &[Para]) -> rsword_layout_core::PaintList {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    paint_document(&engine.layout(paras), None, &[])
}

/// 所有交给后端的文字。
fn drawn_text(list: &rsword_layout_core::PaintList) -> String {
    let mut out = String::new();
    for page in &list.pages {
        for cmd in &page.cmds {
            if let DrawCmd::DrawGlyphs { text, .. } = cmd {
                out.push_str(text);
            }
        }
    }
    out
}

fn ranges(list: &rsword_layout_core::PaintList) -> Vec<(u32, u32)> {
    LayoutRecord::from_paint(list)
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .filter_map(|l| l.source.map(|s| (s.start, s.end)))
        .collect()
}

#[test]
fn placeholder_never_reaches_the_backend() {
    // 能直接排文字的后端（SVG / PDF）会把 `text` 原样输出，
    // 所以占位符也不能出现在那里，不只是不能变成字形。
    let list = paint(&[para("前\u{FFFC}后")]);
    let drawn = drawn_text(&list);
    assert!(
        !drawn.contains(OBJECT_PLACEHOLDER),
        "占位符被交给了后端：{drawn:?}"
    );
    assert_eq!(drawn, "前后");
}

#[test]
fn placeholder_takes_no_width() {
    // 这条是关键：只在绘制层滤掉它，本条就会红——行宽仍然把它算进去。
    let with = paint(&[para("abc\u{FFFC}def")]);
    let without = paint(&[para("abcdef")]);

    let width = |list: &rsword_layout_core::PaintList| -> Vec<(i32, i32)> {
        LayoutRecord::from_paint(list)
            .pages
            .iter()
            .flat_map(|p| p.lines.iter())
            .flat_map(|l| l.glyphs.iter())
            .map(|g| (g.origin_x, g.origin_y))
            .collect()
    };
    // 没接整形器时没有字形可比，退而比片段的绘制原点。
    let origins = |list: &rsword_layout_core::PaintList| -> Vec<i32> {
        let mut out = Vec::new();
        for page in &list.pages {
            for cmd in &page.cmds {
                if let DrawCmd::DrawGlyphs { origin_x, .. } = cmd {
                    out.push(*origin_x);
                }
            }
        }
        out
    };
    let _ = width(&with);
    assert_eq!(
        origins(&with).first(),
        origins(&without).first(),
        "占位符影响了起始位置"
    );
    // 关键断言：含占位符与不含占位符，画出来的文字完全一样宽。
    assert_eq!(drawn_text(&with), drawn_text(&without));
}

#[test]
fn placeholder_still_occupies_one_source_unit() {
    // 它不画不占宽，但**占一个源字符位**——Word 的 `Range` 数它。
    // 不占位的话，其后每个片段的源区间都会前移一格，
    // 而那种错在几何上看不出来，只会让配对悄悄错位。
    let got = ranges(&paint(&[para("ab\u{FFFC}cd")]));
    // "ab" 是 [0,2)，占位符吃掉 2，"cd" 应当从 3 起。
    assert_eq!(got.first().copied(), Some((0, 2)), "{got:?}");
    assert_eq!(got.last().copied(), Some((3, 5)), "占位符没有占掉一个源位：{got:?}");
}

#[test]
fn placeholder_at_run_start_and_end_is_handled() {
    // 实测里两种都出现过：`w:br` 独立成 run（文本就是一个占位符），
    // 以及夹在文字中间（`'分页符之前￼分页符之后'`）。
    let lone = ranges(&paint(&[para("\u{FFFC}"), para("xy")]));
    // 独占一段：不产生任何片段，但仍占 1 个源位，下一段从 2 起。
    assert_eq!(lone.last().copied(), Some((2, 4)), "{lone:?}");

    let trailing = ranges(&paint(&[para("ab\u{FFFC}"), para("xy")]));
    assert_eq!(trailing.first().copied(), Some((0, 2)), "{trailing:?}");
    // "ab"(2) + 占位符(1) + 终止符(1) = 4
    assert_eq!(trailing.last().copied(), Some((4, 6)), "{trailing:?}");
}

#[test]
fn consecutive_placeholders_each_take_one_unit() {
    let got = ranges(&paint(&[para("a\u{FFFC}\u{FFFC}b")]));
    assert_eq!(got.first().copied(), Some((0, 1)), "{got:?}");
    assert_eq!(got.last().copied(), Some((3, 4)), "两个占位符应当各占 1 位：{got:?}");
}
