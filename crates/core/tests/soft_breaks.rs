//! 软回车（`w:br` 缺省或 `textWrapping`、`w:cr`）经解析器 JSON → 桥接 → 排版。
//!
//! 实测：Android Word 窄路径 `br-soft`（`textWrapping`）、`br-bare`（不写 `w:type`）、
//! `br-cr`（`w:cr`）的行起点都是 0、5——`LEFT` 加软回车一行，`RIGHT` 加段落标记一行
//! （word_analyse `reports/rsword-diff/br-soft.md`）。
//!
//! 解析器（rsWordParser 399e36a）把这三种都发成 run 文本里的 `'\n'` 加一个 `br` / `cr` 段，
//! 不是 U+FFFC；桥接层按段的字节区间把那个 `'\n'` 换成 U+FFFC。下面的 JSON 与 `document()`
//! 对这三份夹具的输出同形（段的种类与字节区间照抄，`node`、属性从简）；`br-bare` 那条另把
//! `breakKind` 删掉，钉「缺省即软回车」——解析器自己对不写 `w:type` 的 `w:br` 会补上 textWrapping。
//!
//! 字面的 LF 不是软回车：`xml:space="preserve"` 的 `w:t` 里的 `'\n'` 落在文本段里，
//! Word 不在那里断（word_analyse `breakme.docx` 576 处，窄路径那一行是 561–593）。

use rsword_layout_core::{
    Engine, LayoutRecord, LineTerminator as T, Page, PageSetup, Para, PlaceholderKind as P,
    Platform, SimpleMetrics, View, load_document, paint_document, paras_from_document,
};
use serde_json::{Value, json};

fn doc(runs: Vec<Value>) -> Value {
    json!({"main": [{"kind": "text", "props": {}, "inlines": runs}]})
}

fn run(text: &str, segments: Value) -> Value {
    json!({"kind": "run", "text": text, "props": {"fonts": {"ascii": "Calibri", "hAnsi": "Calibri"}, "size": 24},
           "segments": segments})
}

/// `br-soft` / `br-bare` / `br-cr` 的段：`LEFT`、断行段、`RIGHT`。区间是字节。
fn left_right(break_kind: Value) -> Value {
    run(
        "LEFT\nRIGHT",
        json!([
            {"kind": {"kind": "text"}, "text": [0, 4], "utf16Len": 4},
            {"kind": break_kind, "text": [4, 5], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [5, 10], "utf16Len": 5},
        ]),
    )
}

fn layout(paras: &[Para], platform: Platform) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4())
        .with_platform(platform, View::Print)
        .layout(paras)
}

fn lines(pages: &[Page]) -> Vec<(u32, u32, T)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|p| {
            p.lines.iter().map(|l| {
                let s = l.source.expect("every line retains its source range");
                (s.start, s.end, l.terminator)
            })
        })
        .collect()
}

#[test]
fn soft_break_segments_end_the_line_like_android_word() {
    for (name, kind) in [
        ("br-soft", json!({"kind": "br", "breakKind": "textWrapping"})),
        // 缺 `breakKind` 同样按软回车。
        ("br-bare", json!({"kind": "br"})),
        ("br-cr", json!({"kind": "cr"})),
    ] {
        let (paras, _) = paras_from_document(&doc(vec![left_right(kind)]));
        let r = &paras[0].runs[0];
        assert_eq!(r.text, "LEFT\u{fffc}RIGHT", "{name}");
        assert_eq!(r.placeholders, vec![P::LineBreak], "{name}");
        for platform in [Platform::Desktop, Platform::Android] {
            assert_eq!(
                lines(&layout(&paras, platform)),
                [(0, 5, T::LineBreak), (5, 11, T::ParagraphMark)],
                "{name} {platform:?}"
            );
        }
    }
}

#[test]
fn literal_lf_in_a_text_segment_is_not_a_break() {
    let (paras, _) = paras_from_document(&doc(vec![run(
        "LEFT\nRIGHT",
        json!([{"kind": {"kind": "text"}, "text": [0, 10], "utf16Len": 10}]),
    )]));
    assert_eq!(paras[0].runs[0].text, "LEFT\nRIGHT");
    assert!(paras[0].runs[0].placeholders.is_empty());
    assert_eq!(lines(&layout(&paras, Platform::Android)), [(0, 11, T::ParagraphMark)]);
}

#[test]
fn a_tab_or_zero_width_segment_in_the_same_run_does_not_cost_the_break() {
    // `w:tab` 推 '\t'、`w:lastRenderedPageBreak` 是零长的段：都不落成 U+FFFC。按「非文本段
    // 个数」对位的旧法会把它们算进去、个数对不上、整体退回 Object，软回车就丢了。
    let (paras, _) = paras_from_document(&doc(vec![run(
        "A\tB\nC",
        json!([
            {"kind": {"kind": "text"}, "text": [0, 1], "utf16Len": 1},
            {"kind": {"kind": "tab"}, "text": [1, 2], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [2, 3], "utf16Len": 1},
            {"kind": {"kind": "lastRenderedPageBreak"}, "text": [3, 3], "utf16Len": 0},
            {"kind": {"kind": "br", "breakKind": "textWrapping"}, "text": [3, 4], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [4, 5], "utf16Len": 1},
        ]),
    )]));
    let r = &paras[0].runs[0];
    assert_eq!(r.text, "A\tB\u{fffc}C");
    assert_eq!(r.placeholders, vec![P::LineBreak]);
    assert_eq!(
        lines(&layout(&paras, Platform::Android)),
        [(0, 4, T::LineBreak), (4, 6, T::ParagraphMark)]
    );
}

#[test]
fn placeholders_take_the_kind_of_the_segment_that_holds_them() {
    // 图（U+FFFC，3 字节）、软回车（'\n'）、分页符（U+FFFC）同在一个 run，按文档顺序各归各类。
    let (paras, _) = paras_from_document(&doc(vec![run(
        "a\u{fffc}b\nc\u{fffc}d",
        json!([
            {"kind": {"kind": "text"}, "text": [0, 1], "utf16Len": 1},
            {"kind": {"kind": "drawing", "anchored": false}, "text": [1, 4], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [4, 5], "utf16Len": 1},
            {"kind": {"kind": "br", "breakKind": "textWrapping"}, "text": [5, 6], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [6, 7], "utf16Len": 1},
            {"kind": {"kind": "br", "breakKind": "page"}, "text": [7, 10], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [10, 11], "utf16Len": 1},
        ]),
    )]));
    let r = &paras[0].runs[0];
    assert_eq!(r.text, "a\u{fffc}b\u{fffc}c\u{fffc}d");
    assert_eq!(r.placeholders, vec![P::Object, P::LineBreak, P::PageBreak]);
}

#[test]
fn assumed_a_trailing_soft_break_leaves_the_mark_on_its_own_line() {
    // 段末软回车之后，段落标记另起一条空行。桌面 Word 的行为，引擎原有的规则；Android 未测。
    let (paras, _) = paras_from_document(&doc(vec![run(
        "LEFT\n",
        json!([
            {"kind": {"kind": "text"}, "text": [0, 4], "utf16Len": 4},
            {"kind": {"kind": "br", "breakKind": "textWrapping"}, "text": [4, 5], "utf16Len": 1},
        ]),
    )]));
    assert_eq!(
        lines(&layout(&paras, Platform::Android)),
        [(0, 5, T::LineBreak), (5, 6, T::ParagraphMark)]
    );
}

#[test]
fn segments_without_byte_ranges_keep_the_order_pairing() {
    // 手写的 JSON 可能不带区间：退回按种类次序对位，软回车无从定位，不改文本。
    let (paras, _) = paras_from_document(&doc(vec![json!({
        "kind": "run", "text": "a\u{fffc}b\nc", "props": {},
        "segments": [{"kind": {"kind": "br", "breakKind": "page"}}, {"kind": {"kind": "cr"}}],
    })]));
    let r = &paras[0].runs[0];
    assert_eq!(r.text, "a\u{fffc}b\nc");
    // 两个非文本段对一个 U+FFFC：个数对不上，整体按 Object。
    assert_eq!(r.placeholders, vec![P::Object]);
}

#[test]
fn a_range_off_a_char_boundary_falls_back_to_objects() {
    // '汉' 占 3 字节，区间 [1,2] 切在字符中间：不信任这组区间，文本不改，U+FFFC 全按 Object。
    let (paras, _) = paras_from_document(&doc(vec![run(
        "汉\u{fffc}\n",
        json!([
            {"kind": {"kind": "text"}, "text": [1, 2], "utf16Len": 1},
            {"kind": {"kind": "br", "breakKind": "page"}, "text": [3, 6], "utf16Len": 1},
            {"kind": {"kind": "cr"}, "text": [6, 7], "utf16Len": 1},
        ]),
    )]));
    let r = &paras[0].runs[0];
    assert_eq!(r.text, "汉\u{fffc}\n");
    assert_eq!(r.placeholders, vec![P::Object]);
}

#[test]
fn repo_breaks_docx_soft_break_reaches_layout_through_the_parser() {
    // 端到端：真的解析器。`fixtures/breaks.docx` 第一段是「软回车之前」+ textWrapping + 「软回车之后」。
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join("fixtures/breaks.docx")).unwrap();
    let loaded = load_document(&bytes).unwrap();
    let (paras, _) = paras_from_document(&loaded.json);
    let first = &paras[0];
    assert_eq!(first.runs[0].text, "软回车之前\u{fffc}软回车之后");
    assert_eq!(first.runs[0].placeholders, vec![P::LineBreak]);
    assert_eq!(
        lines(&layout(std::slice::from_ref(first), Platform::Desktop)),
        [(0, 6, T::LineBreak), (6, 12, T::ParagraphMark)]
    );
}

#[test]
fn a_soft_break_in_a_hidden_run_does_not_break_the_line() {
    // Hidden text retains its original source units, but none of its controls execute.
    let plain = |t: &str| {
        let n = t.len() as u64;
        run(t, json!([{"kind": {"kind": "text"}, "text": [0, n], "utf16Len": n}]))
    };
    let hidden = json!({"kind": "run", "text": "HID\n", "props": {"vanish": true},
        "segments": [
            {"kind": {"kind": "text"}, "text": [0, 3], "utf16Len": 3},
            {"kind": {"kind": "br", "breakKind": "textWrapping"}, "text": [3, 4], "utf16Len": 1},
        ]});
    let (paras, _) = paras_from_document(&doc(vec![plain("LEFT"), hidden, plain("RIGHT")]));
    assert_eq!(paras[0].runs[1].text, "HID\n");
    assert!(paras[0].runs[1].placeholders.is_empty());
    assert_eq!(lines(&layout(&paras, Platform::Android)), [(0, 14, T::ParagraphMark)]);
}

#[test]
fn assumed_a_soft_break_right_after_hung_punctuation_joins_the_hung_line() {
    // Desktop 把越界的 `。` 挂在行末、当场收行；紧跟的软回车收进那一行，不自成一条空行——
    // 同一位置的段落标记也收进那一行。SimpleMetrics 每个汉字 1 em，A4 版心 9026 twips 放 37 个。
    let text = format!("{}。\n{}", "汉".repeat(37), "汉".repeat(10));
    let head = 37 * 3 + 3;
    let (paras, _) = paras_from_document(&doc(vec![run(
        &text,
        json!([
            {"kind": {"kind": "text"}, "text": [0, head], "utf16Len": 38},
            {"kind": {"kind": "br", "breakKind": "textWrapping"}, "text": [head, head + 1], "utf16Len": 1},
            {"kind": {"kind": "text"}, "text": [head + 1, text.len()], "utf16Len": 10},
        ]),
    )]));
    assert_eq!(
        lines(&layout(&paras, Platform::Desktop)),
        [(0, 39, T::LineBreak), (39, 50, T::ParagraphMark)]
    );
    // 对照：同样位置换成段落标记，也是收进挂出的那一行。
    let (mark, _) = paras_from_document(&doc(vec![run(
        &format!("{}。", "汉".repeat(37)),
        json!([{"kind": {"kind": "text"}, "text": [0, head], "utf16Len": 38}]),
    )]));
    assert_eq!(lines(&layout(&mark, Platform::Desktop)), [(0, 39, T::ParagraphMark)]);
    // Android 不挂出：`。` 连同前一个汉字退到下一行，软回车跟着它，也没有空行。
    let android = lines(&layout(&paras, Platform::Android));
    assert_eq!(android.last(), Some(&(39, 50, T::ParagraphMark)));
    assert!(android.iter().all(|&(a, b, _)| b > a));
    assert_eq!(android.iter().filter(|l| l.2 == T::LineBreak).count(), 1);
}
