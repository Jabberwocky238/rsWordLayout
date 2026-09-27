//! 段末手动分页符之后的段落标记：分页视图收进分页符那行，移动视图另起一行。
//!
//! 两边的依据（见 `View` 的说明）：
//!
//! - **分页视图**（[`View::Print`]，默认）：Mac Word `breaks-sections` / `vmisc2` 逐字符报
//!   分页符与段落标记同页同行（实测）；Android Word 打印视图 `br-page` 第一页 11 条裁剪带、
//!   第二页 5 条，与合并成一行相符（推断）。这一侧由 `tests/control_records.rs` 钉着，
//!   这里只补 `br-page` 形状的页行数。
//! - **移动视图**（[`View::Mobile`]）：Android Word 窄路径 `w3=5329`，
//!   `word_analyse/reports/diff/br-page.word.narrow.jsonl` 在那段排出
//!   `(340,341)`、`(341,342)` 两条行记录（实测，一份夹具、一处）。
//!
//! 实测只有码元区间。`assumed_ooxml_` 开头的测试钉的是**推定**：段落标记那行落到哪一页、
//! 占多高、画几个字形，以及分页符前有文字、连续几个段末分页符时怎么拆——Android 都没量过，
//! 移动视图又没有页（页数组长度 1），这里照 OOXML 兼容项 `w:splitPgBreakAndParaMark`
//! 打开时的行形状推。以后 Android 的读数与它们不符，改的是这些测试，不是实测那一条。
//!
//! 用 `SimpleMetrics`：`br-page` 每段 33 个字符远小于行宽，断点不取决于字宽，
//! 行边界只取决于这里要钉的控制字符归属。

use rsword_layout_core::{
    Align, Color, Engine, FontSpec, Fragment, LayoutRecord, LineRule, LineTerminator as T, Page,
    PageBreakPosition as B, PageSetup, Para, PlaceholderKind as P, Platform, Run, SimpleMetrics,
    View, paint_document, paras_from_document,
};

const TEXT: &str = "P000 line of text for page break.";

fn run(text: &str, placeholders: Vec<P>) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new("Calibri", 24),
        color: Color::BLACK,
        placeholders,
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn para(text: &str, placeholders: Vec<P>) -> Para {
    Para {
        runs: vec![run(text, placeholders)],
        line_rule: LineRule::Exact,
        line_value: 480,
        ..Para::default()
    }
}

/// `fixtures/br-page.docx` 的形状：10 个一行段、一段只有 `w:br w:type="page"`、再 5 个一行段。
fn br_page() -> Vec<Para> {
    let mut paras: Vec<Para> = (0..10).map(|_| para(TEXT, vec![])).collect();
    paras.push(para("\u{fffc}", vec![P::PageBreak]));
    paras.extend((0..5).map(|_| para(TEXT, vec![])));
    paras
}

fn layout_as(paras: &[Para], platform: Platform, view: View) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4())
        .with_platform(platform, view)
        .layout(paras)
}

/// Android Word 的两种视图。
fn layout(paras: &[Para], view: View) -> Vec<Page> {
    layout_as(paras, Platform::Android, view)
}

fn ranges(pages: &[Page]) -> Vec<(usize, u32, u32, T)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|p| {
            p.lines.iter().map(move |l| {
                let s = l.source.expect("every line retains its source range");
                (p.index, s.start, s.end, l.terminator)
            })
        })
        .collect()
}

fn cp_ranges(pages: &[Page]) -> Vec<(u32, u32)> {
    ranges(pages)
        .into_iter()
        .map(|(_, a, b, _)| (a, b))
        .collect()
}

fn lines_per_page(pages: &[Page]) -> Vec<usize> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .map(|p| p.lines.len())
        .collect()
}

/// Android Word 窄路径 `br-page.word.narrow.jsonl` 的全部 17 条 `(cpFirst, cpLim)`。
fn android_narrow_lines() -> Vec<(u32, u32)> {
    let mut lines: Vec<(u32, u32)> = (0..10).map(|i| (34 * i, 34 * i + 34)).collect();
    lines.push((340, 341));
    lines.push((341, 342));
    lines.extend((0..5).map(|i| (342 + 34 * i, 342 + 34 * i + 34)));
    lines
}

#[test]
fn mobile_view_reproduces_android_narrow_br_page_lines() {
    // 实测只有这 17 条区间；页、高度、字形都不在里面，也不在这里断言。
    assert_eq!(
        cp_ranges(&layout(&br_page(), View::Mobile)),
        android_narrow_lines()
    );
}

#[test]
fn print_view_keeps_the_mark_on_the_break_line() {
    let pages = layout(&br_page(), View::Print);
    let r = ranges(&pages);
    assert_eq!(r.len(), 16);
    assert_eq!(r[10], (0, 340, 342, T::PageBreak(B::BeforeMark)));
    assert_eq!(r[11].1, 342);
    // Android 打印视图第一页 11 条裁剪带、第二页 5 条（推断合并成一行的依据）。
    assert_eq!(lines_per_page(&pages), [11, 5]);
}

#[test]
fn the_library_default_is_the_print_view() {
    let paras = br_page();
    let default = Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&paras);
    assert_eq!(
        ranges(&default),
        ranges(&layout_as(&paras, Platform::Desktop, View::Print))
    );
    assert_eq!(ranges(&default), ranges(&layout(&paras, View::Print)));
}

#[test]
fn the_split_follows_the_view_not_the_platform() {
    // 拆不拆只看视图。桌面 + 移动视图没有任何实测依据（见 `View::Mobile`），
    // 但不拒绝，排法与 Android 移动视图相同。
    let paras = br_page();
    for view in [View::Print, View::Mobile] {
        assert_eq!(
            format!("{:?}", layout_as(&paras, Platform::Desktop, view)),
            format!("{:?}", layout_as(&paras, Platform::Android, view)),
            "{view:?}"
        );
    }
}

#[test]
fn breaks_followed_by_content_are_the_same_in_both_views() {
    // 段中与段首分页符、分页符后跟对象、段末软回车：两种视图都在分页符处收行，
    // 其余内容从下一页起。
    for (text, kinds) in [
        ("ab\u{fffc}cd", vec![P::PageBreak]),
        ("\u{fffc}ab", vec![P::PageBreak]),
        ("a\u{fffc}\u{fffc}", vec![P::PageBreak, P::Object]),
        ("ab\u{fffc}", vec![P::LineBreak]),
        ("ab\u{fffc}", vec![P::ColumnBreak]),
    ] {
        let p = [para(text, kinds)];
        assert_eq!(
            format!("{:?}", layout(&p, View::Mobile)),
            format!("{:?}", layout(&p, View::Print)),
            "{text:?}"
        );
    }
}

#[test]
fn a_section_mark_after_a_trailing_break_stays_on_the_break_line() {
    // **假设**：段内带 `w:sectPr` 的段不拆，分节符照分页视图收进分页符那行。
    // Android 只量到以段落标记结束的 `br-page`；拆开的话分节符那行落到下一页、
    // 分节（下一节 `nextPage`，即下一段 `page_break_before`）再翻一页，多出一页。
    let paras = [
        Para {
            terminator: T::SectionBreak,
            ..para("ab\u{fffc}", vec![P::PageBreak])
        },
        Para {
            page_break_before: true,
            ..para("cd", vec![])
        },
    ];
    let print = layout(&paras, View::Print);
    let mobile = layout(&paras, View::Mobile);
    assert_eq!(format!("{mobile:?}"), format!("{print:?}"));
    assert_eq!(
        ranges(&mobile),
        vec![
            (0, 0, 4, T::PageBreak(B::MidParagraph)),
            (1, 4, 7, T::ParagraphMark)
        ]
    );
}

#[test]
fn a_justified_line_before_a_trailing_break_is_not_stretched_in_mobile_view() {
    // **假设**：拆开后分页符那行仍是本段最后一行内容，照分页视图不拉伸，
    // 两种视图的横向落位逐点相同。Android 窄路径只有码元区间，没有横向位置。
    let p = [Para {
        runs: vec![run("ab ", vec![]), run("cd\u{fffc}", vec![P::PageBreak])],
        align: Align::Justify,
        ..Para::default()
    }];
    let x_of_cd = |view: View| -> f64 {
        layout(&p, view)[0]
            .fragments
            .iter()
            .find_map(|f| match f {
                Fragment::Text(t) if t.text.starts_with("cd") => Some(t.x_pt),
                _ => None,
            })
            .expect("page one has 'cd'")
    };
    let print = x_of_cd(View::Print);
    assert_eq!(x_of_cd(View::Mobile), print);
    // 对照：'cd' 紧跟 'ab '，没被推到右边距（没拉伸时离左边距不到一个词宽）。
    let left = f64::from(PageSetup::a4().content_area().x) / 20.0;
    assert!(
        print - left < 72.0,
        "'cd' at {print}pt, left margin {left}pt"
    );
}

#[test]
fn assumed_ooxml_the_mark_line_starts_the_next_page() {
    // 移动视图没有页；段落标记那行按兼容项的定义排在翻页之后，落到下一页顶。
    let pages = layout(&br_page(), View::Mobile);
    assert_eq!(lines_per_page(&pages), [11, 6]);
    let r = ranges(&pages);
    assert_eq!(r[10], (0, 340, 341, T::PageBreak(B::OwnLine)));
    assert_eq!(r[11], (1, 341, 342, T::ParagraphMark));
}

#[test]
fn assumed_ooxml_text_before_a_trailing_break_splits_too() {
    // Android 只量到「只有分页符的段」。按兼容项的定义，分页符前面有没有文字
    // 不影响段落标记另起一行。
    let got = ranges(&layout(
        &[para("ab\u{fffc}", vec![P::PageBreak])],
        View::Mobile,
    ));
    assert_eq!(
        got,
        vec![
            (0, 0, 3, T::PageBreak(B::MidParagraph)),
            (1, 3, 4, T::ParagraphMark)
        ]
    );
}

#[test]
fn assumed_ooxml_consecutive_trailing_breaks_each_get_a_line() {
    let p = [para("\u{fffc}\u{fffc}", vec![P::PageBreak; 2])];
    assert_eq!(
        ranges(&layout(&p, View::Mobile)),
        vec![
            (0, 0, 1, T::PageBreak(B::OwnLine)),
            (1, 1, 2, T::PageBreak(B::OwnLine)),
            (2, 2, 3, T::ParagraphMark),
        ]
    );
    // 分页视图：第二个分页符收下段落标记，只翻一次多余的页。
    assert_eq!(
        ranges(&layout(&p, View::Print)),
        vec![
            (0, 0, 1, T::PageBreak(B::OwnLine)),
            (1, 1, 3, T::PageBreak(B::BeforeMark))
        ]
    );
}

#[test]
fn assumed_ooxml_the_mark_line_draws_the_mark_space_on_the_new_page() {
    // Android 的轨迹没有字形。照计数约定：独占一行的分页符画 0 个，段落标记画 1 个空格。
    let pages = layout(&[para("\u{fffc}", vec![P::PageBreak])], View::Mobile);
    let text = |page: &Page| -> String {
        page.fragments
            .iter()
            .filter_map(|f| match f {
                Fragment::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(pages.len(), 2);
    assert_eq!(text(&pages[0]), "");
    assert_eq!(text(&pages[1]), " ");
}

#[test]
fn assumed_ooxml_the_mark_line_takes_one_line_of_height_on_the_new_page() {
    let first_baseline_on_page_two = |view: View| -> i64 {
        let pages = layout(&br_page(), view);
        pages[1]
            .fragments
            .iter()
            .find_map(|f| match f {
                Fragment::Text(t) if t.text.starts_with("P000") => Some(t.baseline_fine),
                _ => None,
            })
            .expect("page two has text")
    };
    // 段落标记那行照段落的精确行距占 480 twips = 480 × 5 个 1/7200 英寸。
    assert_eq!(
        first_baseline_on_page_two(View::Mobile) - first_baseline_on_page_two(View::Print),
        480 * 5
    );
}

#[test]
fn parser_json_break_only_paragraph_splits_through_the_bridge() {
    // 与 `rsword` 对 `br-page.docx` 的输出同形：分页符是一个 U+FFFC，
    // 种类在 `segments[].kind.breakKind`。
    let text_block = serde_json::json!({
        "kind": "text",
        "inlines": [{"kind": "run", "text": TEXT, "props": {}}],
        "props": {"spacing": {"after": 0, "before": 0, "line": 480, "lineRule": "exact"}},
    });
    let break_block = serde_json::json!({
        "kind": "text",
        "inlines": [{
            "kind": "run",
            "text": "\u{fffc}",
            "props": {},
            "segments": [{"kind": {"breakKind": "page", "kind": "br"}, "utf16Len": 1}],
        }],
        "props": {"spacing": {"after": 0, "before": 0, "line": 480, "lineRule": "exact"}},
    });
    let doc = serde_json::json!({"main": [text_block.clone(), break_block, text_block]});
    let (paras, skipped) = paras_from_document(&doc);
    assert_eq!(skipped, 0);
    assert_eq!(
        cp_ranges(&layout(&paras, View::Mobile)),
        [(0, 34), (34, 35), (35, 36), (36, 70)]
    );
    assert_eq!(
        cp_ranges(&layout(&paras, View::Print)),
        [(0, 34), (34, 36), (36, 70)]
    );
}
