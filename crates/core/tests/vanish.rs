//! Direct run-level w:vanish keeps source positions but contributes no visible layout.
//!
//! Android Word evidence: word_analyse/reports/rsword-diff/vanish.md records
//! [0, 73, 116] at width 5329 and [0, 116] at width 10466 for 20 visible,
//! 30 hidden, then 80 visible Calibri zeros. webHidden/specVanish retain width.
//! The SimpleMetrics cases test inferred shared semantics, including controls,
//! UTF-16 accounting, tab lookahead and height; those combinations are not Word
//! measurements. The ignored Calibri case checks the observed Android starts.

use rsword_layout_core::{
    DrawCmd, Engine, FontMetrics, Fragment, LayoutRecord, LineTerminator as T, Margins, Page,
    PageSetup, Platform, SimpleMetrics, Size, Twips, View, paint_document, paras_from_document,
};
use serde_json::{Value, json};

const MARGIN: Twips = 720;
const MODES: [(Platform, View); 4] = [
    (Platform::Desktop, View::Print),
    (Platform::Desktop, View::Mobile),
    (Platform::Android, View::Print),
    (Platform::Android, View::Mobile),
];

fn run(text: &str) -> Value {
    json!({"kind": "run", "text": text,
        "props": {"fonts": {"ascii": "Calibri", "hAnsi": "Calibri"}, "size": 24}})
}

fn hidden(text: &str) -> Value {
    let mut r = run(text);
    r["props"]["vanish"] = json!(true);
    r
}

fn para(runs: Vec<Value>) -> Value {
    json!({"kind": "text", "props": {}, "inlines": runs})
}

fn document(paragraphs: Vec<Value>) -> Value {
    json!({"main": paragraphs})
}

fn setup(width: Twips) -> PageSetup {
    PageSetup {
        size: Size::new(width + 2 * MARGIN, 16838),
        margins: Margins::uniform(MARGIN),
    }
}

fn layout(doc: &Value, width: Twips, platform: Platform, view: View) -> Vec<Page> {
    let (paras, _) = paras_from_document(doc);
    Engine::new(&SimpleMetrics, setup(width))
        .with_platform(platform, view)
        .layout(&paras)
}

fn lines(pages: &[Page]) -> Vec<(u32, u32, T)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .map(|line| {
            let source = line.source.expect("line retains its source interval");
            (source.start, source.end, line.terminator)
        })
        .collect()
}

fn painted(pages: &[Page]) -> String {
    paint_document(pages, None, &[])
        .pages
        .iter()
        .flat_map(|p| &p.cmds)
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn text_position(pages: &[Page], prefix: &str) -> (Twips, i64, (u32, u32)) {
    pages
        .iter()
        .flat_map(|p| &p.fragments)
        .find_map(|f| match f {
            Fragment::Text(t) if t.text.starts_with(prefix) => {
                Some((t.x - MARGIN, t.baseline_fine, t.source.unwrap()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing visible fragment {prefix:?}"))
}

fn visible_positions(
    pages: &[Page],
    source_map: impl Fn(u32) -> u32,
) -> Vec<(char, u32, usize, u32, f64, i64)> {
    let mut positions = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        for fragment in &page.fragments {
            let Fragment::Text(text) = fragment else {
                continue;
            };
            let mut source = text.source.unwrap().0;
            for (byte, ch) in text.text.char_indices() {
                let x = text.x_pt + SimpleMetrics.advance_pt(&text.text[..byte], &text.font);
                positions.push((
                    ch,
                    source_map(source),
                    page_index,
                    text.line,
                    x,
                    text.baseline_fine,
                ));
                source += ch.len_utf16() as u32;
            }
        }
    }
    positions
}

fn assert_matches_visible_control(
    actual: &[Page],
    control: &[Page],
    hidden_start: u32,
    hidden_len: u32,
) {
    let visible_source = |source: u32| source - source.saturating_sub(hidden_start).min(hidden_len);
    let actual_lines = lines(actual);
    let control_lines = lines(control);
    assert_eq!(actual_lines.first().unwrap().0, 0);
    assert_eq!(
        actual_lines.last().unwrap().1,
        control_lines.last().unwrap().1 + hidden_len
    );
    assert!(actual_lines.windows(2).all(|pair| pair[0].1 == pair[1].0));
    let compressed: Vec<_> = actual_lines
        .iter()
        .map(|&(start, end, terminator)| (visible_source(start), visible_source(end), terminator))
        .collect();
    assert_eq!(compressed, control_lines);
    assert_eq!(
        visible_positions(actual, visible_source),
        visible_positions(control, |source| source)
    );
}

fn zeros_with_property(property: &str, value: bool) -> Value {
    let mut middle = run(&"0".repeat(30));
    middle["props"][property] = json!(value);
    document(vec![para(vec![
        run(&"0".repeat(20)),
        middle,
        run(&"0".repeat(80)),
    ])])
}

#[test]
fn hidden_text_reclaims_width_without_manufacturing_a_run_boundary_break() {
    let doc = zeros_with_property("vanish", true);
    // SimpleMetrics: each zero is 120 twips; widths fit 43 or 86 visible zeros.
    for (width, expected) in [
        (
            5230,
            vec![
                (0, 73, T::Wrapped),
                (73, 116, T::Wrapped),
                (116, 131, T::ParagraphMark),
            ],
        ),
        (
            10390,
            vec![(0, 116, T::Wrapped), (116, 131, T::ParagraphMark)],
        ),
    ] {
        for (platform, view) in MODES {
            let pages = layout(&doc, width, platform, view);
            assert_eq!(lines(&pages), expected, "{platform:?} {view:?}");
            assert_eq!(painted(&pages), format!("{} ", "0".repeat(100)));
        }
    }
}

#[test]
fn false_vanish_webhidden_and_specvanish_stay_visible() {
    for (property, value) in [("vanish", false), ("webHidden", true), ("specVanish", true)] {
        let doc = zeros_with_property(property, value);
        for (platform, view) in MODES {
            let pages = layout(&doc, 10390, platform, view);
            assert_eq!(
                lines(&pages),
                [(0, 86, T::Wrapped), (86, 131, T::ParagraphMark)]
            );
            assert_eq!(
                painted(&pages),
                format!("{} ", "0".repeat(130)),
                "{property}"
            );
        }
    }
}

#[test]
fn assumed_hidden_prefix_and_suffix_keep_utf16_source_ranges() {
    let doc = document(vec![
        para(vec![hidden("\u{1f600}"), run("AB"), hidden("H")]),
        para(vec![run("Z")]),
    ]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 5000, platform, view);
        assert_eq!(
            lines(&pages),
            [(0, 6, T::ParagraphMark), (6, 8, T::ParagraphMark)]
        );
        assert_eq!(painted(&pages), "AB Z ");
        assert_eq!(text_position(&pages, "AB").2, (2, 4));
        assert_eq!(text_position(&pages, "Z").2, (6, 8));
    }
}

#[test]
fn assumed_all_hidden_paragraph_retains_only_its_visible_paragraph_mark() {
    let doc = document(vec![para(vec![hidden("\u{1f600}Q")]), para(vec![run("Z")])]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 5000, platform, view);
        assert_eq!(
            lines(&pages),
            [(0, 4, T::ParagraphMark), (4, 6, T::ParagraphMark)]
        );
        assert_eq!(painted(&pages), " Z ");
        let paint = paint_document(&pages, None, &[]);
        let mark = paint.pages[0].cmds.iter().find_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { text, source, .. } if text == " " => *source,
            _ => None,
        });
        assert_eq!(
            mark,
            Some((3, 4)),
            "paragraph glyph must not absorb hidden UTF-16 units"
        );
    }
}

#[test]
fn assumed_non_bmp_hidden_text_does_not_shift_later_visible_sources() {
    let doc = document(vec![
        para(vec![run("A"), hidden("\u{1f600}"), run("B")]),
        para(vec![run("C")]),
    ]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 5000, platform, view);
        assert_eq!(
            lines(&pages),
            [(0, 5, T::ParagraphMark), (5, 7, T::ParagraphMark)]
        );
        assert_eq!(painted(&pages), "AB C ");
        assert_eq!(text_position(&pages, "A").2, (0, 1));
        assert_eq!(text_position(&pages, "B").2, (3, 5));
        assert_eq!(text_position(&pages, "B").0, 120);
    }
}

#[test]
fn assumed_hidden_controls_neither_break_nor_paint() {
    for (text, kind) in [
        ("\t", json!({"kind": "tab"})),
        ("\n", json!({"kind": "br", "breakKind": "textWrapping"})),
        ("\n", json!({"kind": "cr"})),
        ("\u{fffc}", json!({"kind": "br", "breakKind": "page"})),
        ("\u{fffc}", json!({"kind": "drawing", "anchored": false})),
    ] {
        let mut control = hidden(text);
        control["segments"] = json!([{"kind": kind, "text": [0, text.len()], "utf16Len": 1}]);
        let doc = document(vec![para(vec![run("L"), control, run("R")])]);
        for (platform, view) in MODES {
            let pages = layout(&doc, 5000, platform, view);
            assert_eq!(pages.len(), 1, "{kind:?}");
            assert_eq!(lines(&pages), [(0, 4, T::ParagraphMark)], "{kind:?}");
            assert_eq!(painted(&pages), "LR ", "{kind:?}");
            assert_eq!(text_position(&pages, "R").0, 120);
        }
    }
}

#[test]
fn assumed_right_tab_lookahead_skips_hidden_text_and_hidden_tabs() {
    let mut p = para(vec![run("\t12"), hidden("MMMM\t"), run("34")]);
    p["props"]["tabs"] = json!({"tab": [{"pos": 3000, "val": "right"}]});
    let doc = document(vec![p]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 5000, platform, view);
        assert_eq!(lines(&pages), [(0, 11, T::ParagraphMark)]);
        assert_eq!(text_position(&pages, "12").0, 2520);
        assert_eq!(text_position(&pages, "34").0, 2760);
        assert!(!painted(&pages).contains('M'));
    }
}

#[test]
fn assumed_decimal_tab_lookahead_ignores_hidden_decimal_and_tab() {
    let mut p = para(vec![run("\t1"), hidden(".MMMM\t"), run("2.5")]);
    p["props"]["tabs"] = json!({"tab": [{"pos": 3000, "val": "decimal"}]});
    let doc = document(vec![p]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 5000, platform, view);
        assert_eq!(lines(&pages), [(0, 12, T::ParagraphMark)]);
        assert_eq!(text_position(&pages, "1").0, 2760);
        assert_eq!(text_position(&pages, "2.5").0, 2880);
        assert!(!painted(&pages).contains('M'));
    }
}

#[test]
fn assumed_hidden_space_does_not_create_a_word_break() {
    let doc = document(vec![para(vec![run("ABC"), hidden(" "), run("DEF")])]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 610, platform, view);
        assert_eq!(
            lines(&pages),
            [(0, 6, T::Wrapped), (6, 8, T::ParagraphMark)]
        );
        assert_eq!(painted(&pages), "ABCDEF ");
    }
}

#[test]
fn assumed_hidden_tall_raised_runs_do_not_enlarge_visible_lines() {
    let mut tall = hidden("H");
    tall["props"]["size"] = json!(240);
    tall["props"]["position"] = json!(80);
    let doc = document(vec![
        para(vec![tall.clone(), run("A"), tall]),
        para(vec![run("B")]),
    ]);
    let control = document(vec![para(vec![run("A")]), para(vec![run("B")])]);
    for (platform, view) in MODES {
        let pages = layout(&doc, 5000, platform, view);
        let expected = layout(&control, 5000, platform, view);
        assert_eq!(painted(&pages), "A B ");
        for prefix in ["A", "B"] {
            let actual = text_position(&pages, prefix);
            let expected = text_position(&expected, prefix);
            assert_eq!((actual.0, actual.1), (expected.0, expected.1), "{prefix}");
        }
    }
}

#[test]
fn assumed_hidden_gaps_preserve_desktop_hanging_punctuation_context() {
    let head = "\u{6c49}".repeat(37);
    let tail = "\u{6c49}".repeat(10);
    let control_doc = document(vec![para(vec![run(&head), run("\u{3002}"), run(&tail)])]);
    let control = layout(&control_doc, 8880, Platform::Desktop, View::Print);
    assert_eq!(lines(&control)[0].1, 38, "control hangs the punctuation");
    // Hidden Latin must not replace the preceding CJK context; hidden closing
    // punctuation must not become the next visible character either.
    for (runs, hidden_start) in [
        (
            vec![run(&head), hidden("X"), run("\u{3002}"), run(&tail)],
            37,
        ),
        (
            vec![run(&head), run("\u{3002}"), hidden("\u{ff1b}"), run(&tail)],
            38,
        ),
    ] {
        let actual = layout(
            &document(vec![para(runs)]),
            8880,
            Platform::Desktop,
            View::Print,
        );
        assert_matches_visible_control(&actual, &control, hidden_start, 1);
    }
}

#[test]
fn assumed_retreat_across_hidden_text_matches_visible_split_runs() {
    let tail = format!("ld{}", "0".repeat(60));
    let doc = document(vec![para(vec![
        run("hello wor"),
        hidden(" \t\u{1f600}"),
        run(&tail),
    ])]);
    let control_doc = document(vec![para(vec![run("hello wor"), run(&tail)])]);
    for (platform, view) in MODES {
        let actual = layout(&doc, 5230, platform, view);
        let control = layout(&control_doc, 5230, platform, view);
        assert_eq!(
            lines(&control)[0].1,
            6,
            "control retreats to the earlier space"
        );
        assert_matches_visible_control(&actual, &control, 9, 4);
    }
}

#[test]
fn assumed_hidden_suffix_after_exact_fill_preserves_sources_without_an_extra_line() {
    // The second case also exercises a preceding automatic wrap. The hidden
    // suffix belongs to the source interval of the last visible line.
    for count in [43, 86] {
        let first = "0".repeat(20);
        let rest = "0".repeat(count - 20);
        let doc = document(vec![
            para(vec![run(&first), run(&rest), hidden("\u{1f600}X")]),
            para(vec![run("Z")]),
        ]);
        let control_doc = document(vec![
            para(vec![run(&first), run(&rest)]),
            para(vec![run("Z")]),
        ]);
        for (platform, view) in MODES {
            let actual = layout(&doc, 5160, platform, view);
            let control = layout(&control_doc, 5160, platform, view);
            assert_eq!(lines(&control).len(), count / 43 + 1);
            assert_matches_visible_control(&actual, &control, count as u32, 3);
        }
    }
}

#[cfg(feature = "fontenv")]
#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI"]
fn phone_calibri_reproduces_word_vanish_and_control_starts() {
    use rsword_layout_core::{RealMetrics, font::FontRegistry};

    let path = std::env::var_os("RSWORD_TEST_CALIBRI")
        .expect("set RSWORD_TEST_CALIBRI to the Android Word calibri.ttf path");
    let mut registry = FontRegistry::new();
    registry.add(std::fs::read(path).unwrap(), 0).unwrap();
    let metrics = RealMetrics::new(&registry);
    for (property, width, view, expected) in [
        ("vanish", 5329, View::Mobile, vec![0, 73, 116]),
        ("vanish", 10466, View::Print, vec![0, 116]),
        ("webHidden", 10466, View::Print, vec![0, 86]),
        ("specVanish", 10466, View::Print, vec![0, 86]),
    ] {
        let (paras, _) = paras_from_document(&zeros_with_property(property, true));
        let pages = Engine::new(&metrics, setup(width))
            .with_platform(Platform::Android, view)
            .layout(&paras);
        let paint = paint_document(&pages, Some(&registry), &registry.face_ids());
        let record = LayoutRecord::from_paint(&paint);
        let starts: Vec<_> = record
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .map(|line| line.source.unwrap().start)
            .collect();
        assert_eq!(starts, expected, "{property} {width}");
        let glyphs: Vec<_> = record
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .flat_map(|line| &line.glyphs)
            .collect();
        assert_eq!(glyphs.len(), if property == "vanish" { 101 } else { 131 });
        if property == "vanish" {
            for glyph in glyphs {
                let source = glyph
                    .source
                    .expect("painted glyph retains a source interval");
                assert!(
                    source.end <= 20 || source.start >= 50,
                    "hidden glyph: {source:?}"
                );
            }
        }
    }
}
