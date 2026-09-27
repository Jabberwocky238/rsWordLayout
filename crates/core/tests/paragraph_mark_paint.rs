//! Public-pipeline tests for independently resolved paragraph-mark painting.
//! SimpleMetrics checks source/style bookkeeping, not measured Word mark rules.

use std::cell::RefCell;

use rsword_layout_core::{
    Color, DrawCmd, Engine, FontMetrics, FontSpec, Fragment, LayoutRecord, LineRule,
    LineTerminator as T, Margins, Page, PageBreakPosition as B, PageSetup, Para,
    ParagraphMarkProperties, PlaceholderKind as P, Platform, Run, ShapedRun, SimpleMetrics, Size,
    TextFragment, TextShaper, View, document_from_json, load_document, paint_document,
};
use serde_json::{Value, json};

fn style(family: &str, size: u32, position: i32) -> Value {
    json!({"fonts": {"ascii": family, "hAnsi": family, "eastAsia": family, "cs": family},
        "size": size, "position": position})
}

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new("Body Font", 24),
        color: Color::rgb(20, 40, 60),
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn para(text: &str) -> Para {
    Para {
        runs: vec![run(text)],
        line_rule: LineRule::Exact,
        line_value: 480,
        ..Para::default()
    }
}

fn marked(mut para: Para, props: &Value) -> Para {
    para.mark = ParagraphMarkProperties::from_json(None, Some(props));
    para
}

fn setup(width: i32, height: i32) -> PageSetup {
    PageSetup {
        size: Size::new(width, height),
        margins: Margins::uniform(0),
    }
}

fn layout(paras: &[Para]) -> Vec<Page> {
    Engine::new(&SimpleMetrics, setup(6000, 6000)).layout(paras)
}

fn fragments(pages: &[Page]) -> Vec<&TextFragment> {
    pages
        .iter()
        .flat_map(|page| &page.fragments)
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn fragment(pages: &[Page], source: (u32, u32)) -> &TextFragment {
    fragments(pages)
        .into_iter()
        .find(|text| text.source == Some(source))
        .expect("independently styled source suffix")
}

fn containing(pages: &[Page], cp: u32) -> &TextFragment {
    fragments(pages)
        .into_iter()
        .find(|text| {
            !text.text.is_empty()
                && text
                    .source
                    .is_some_and(|(start, end)| start <= cp && cp < end)
        })
        .expect("visible source character")
}

fn lines(pages: &[Page]) -> Vec<(usize, u32, u32, T)> {
    let record = LayoutRecord::from_paint(&paint_document(pages, None, &[]));
    record
        .pages
        .iter()
        .flat_map(|page| {
            page.lines.iter().map(move |line| {
                let source = line.source.unwrap();
                (page.index, source.start, source.end, line.terminator)
            })
        })
        .collect()
}

#[derive(Default)]
struct RecordingShaper(RefCell<Vec<(String, FontSpec)>>);

impl TextShaper for RecordingShaper {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        self.0.borrow_mut().push((text.to_owned(), font.clone()));
        let glyph = |text: &str, glyph_id, source| ShapedRun {
            face_index: 0,
            glyph_id,
            source: Some(source),
            x_advance: SimpleMetrics.measure(text, font).advance,
            x_advance_pt: SimpleMetrics.advance_pt(text, font),
            x_offset: 0,
            y_offset: 0,
            size_centipoints: None,
        };
        // This synthetic ligature makes a change in body shaping input observable.
        if text == "fi" {
            return vec![glyph(text, 777, (0, 2))];
        }
        let mut cp = 0;
        text.chars()
            .map(|ch| {
                let end = cp + ch.len_utf16() as u32;
                let shaped = glyph(&ch.to_string(), ch as u32, (cp, end));
                cp = end;
                shaped
            })
            .collect()
    }
}

#[test]
fn unresolved_incomplete_and_declared_only_mark_properties_preserve_legacy_paint() {
    let mut input = para("ab");
    input.runs[0].rise_fine = Some(413);
    let expected = paint_document(&layout(std::slice::from_ref(&input)), None, &[]);
    let incomplete = [
        None,
        Some(Value::Null),
        Some(json!(false)),
        Some(json!(17)),
        Some(json!([])),
        Some(json!("mark")),
        Some(json!({})),
        Some(json!({"fonts": {}})),
        Some(json!({"fonts": {"ascii": "Mark Font"}})),
        Some(json!({"size": 48})),
        Some(json!({"fonts": {"ascii": ""}, "size": 48})),
        Some(json!({"fonts": {"ascii": "", "hAnsi": "Mark Font"}, "size": 48})),
        Some(json!({"fonts": {"ascii": "Mark Font"}, "size": 0})),
        Some(json!({"fonts": {"ascii": "Mark Font"}, "size": 4294967296_u64})),
        Some(json!({"fonts": {"ascii": "Mark Font"}, "size": 48, "position": "8"})),
        Some(json!({"fonts": {"ascii": "Mark Font"}, "size": 48, "position": i64::MIN})),
        Some(json!({"fonts": {"ascii": "Mark Font"}, "size": 48, "position": 2147483648_i64})),
        Some(json!({"fonts": {"ascii": "Mark Font"}, "size": 48, "vertAlign": "unknown"})),
    ];
    for effective in incomplete {
        let mut changed = input.clone();
        changed.mark = ParagraphMarkProperties::from_json(None, effective.as_ref());
        assert_eq!(
            paint_document(&layout(&[changed]), None, &[]),
            expected,
            "{effective:?}"
        );
    }
    let mut declared_only = input;
    declared_only.mark = ParagraphMarkProperties::from_json(Some(&style("Mark Font", 48, 8)), None);
    assert_eq!(
        paint_document(&layout(&[declared_only]), None, &[]),
        expected
    );
}

#[test]
fn resolved_plain_mark_does_not_inherit_body_position_or_superscript() {
    for body_props in [
        json!({"fonts": {"ascii": "Body Font"}, "size": 24, "position": 8}),
        json!({"fonts": {"ascii": "Body Font"}, "size": 24, "vertAlign": "superscript"}),
    ] {
        let document = document_from_json(&json!({"main": [{"kind":"text",
            "inlines":[{"kind":"run","text":"ab","props":body_props}]}]}));
        let input = marked(
            document.paras[0].clone(),
            &json!({
                "fonts": {"ascii": "Mark Font"}, "size": 24,
            }),
        );
        let original_run = input.runs[0].clone();
        let pages = layout(&[input]);
        let body = fragment(&pages, (0, 2));
        let mark = fragment(&pages, (2, 3));
        assert_eq!(body.font, original_run.font);
        assert!(body.rise_fine > 0);
        assert_eq!(mark.font.family, "Mark Font");
        assert_eq!(mark.font.effective_size_centipoints(), 1200);
        assert_eq!(mark.rise_fine, 0);
        assert_eq!(mark.baseline_fine, body.baseline_fine + body.rise_fine);
    }
}

#[test]
fn mark_size_and_position_change_only_suffix_style_not_color_visibility_or_flow_height() {
    let input = vec![para("ab"), para("N")];
    let before = layout(&input);
    let mut changed = input.clone();
    let mut props = style("Mark Font", 48, -6);
    props["color"] = json!({"val":"FF0000"});
    props["vanish"] = json!(true);
    changed[0] = marked(changed[0].clone(), &props);
    let after = layout(&changed);
    let body = fragment(&after, (0, 2));
    let mark = fragment(&after, (2, 3));
    assert_eq!(mark.text, " ");
    assert_eq!(mark.font.size_half_points, 48);
    assert_eq!(mark.rise_fine, -300);
    assert_eq!(mark.baseline_fine, body.baseline_fine + 300);
    assert_eq!(mark.color, input[0].runs[0].color);
    assert_eq!(mark.x_pt, SimpleMetrics.advance_pt("ab", &body.font));
    assert_eq!(body.font, containing(&before, 0).font);
    assert_eq!(body.baseline_fine, containing(&before, 0).baseline_fine);
    assert_eq!(
        containing(&after, 3).baseline_fine,
        containing(&before, 3).baseline_fine
    );
    assert_eq!(lines(&after), lines(&before));
}

#[test]
fn page_break_before_mark_preserves_break_style_and_advances_to_the_mark() {
    let mut input = para("ab\u{fffc}");
    input.runs[0].placeholders = vec![P::PageBreak];
    let pages = layout(&[marked(input.clone(), &style("Mark Font", 48, 8))]);
    let mark = fragment(&pages, (3, 4));
    assert_eq!(mark.font.family, "Mark Font");
    assert_eq!(mark.rise_fine, 400);
    assert_eq!(
        mark.x_pt,
        SimpleMetrics.advance_pt("ab ", &input.runs[0].font)
    );
    assert_eq!(containing(&pages, 2).font, input.runs[0].font);
    assert_eq!(containing(&pages, 2).rise_fine, 0);
    assert_eq!(lines(&pages), [(0, 0, 4, T::PageBreak(B::BeforeMark))]);
    let shaper = RecordingShaper::default();
    let paint = paint_document(&pages, Some(&shaper), &["synthetic".into()]);
    let sources: Vec<_> = paint
        .pages
        .iter()
        .flat_map(|page| &page.cmds)
        .filter_map(|cmd| {
            if let DrawCmd::DrawGlyphs { glyphs, .. } = cmd {
                Some(glyphs)
            } else {
                None
            }
        })
        .flatten()
        .map(|glyph| glyph.source.unwrap())
        .collect();
    assert_eq!(sources, [(0, 1), (1, 2), (2, 3), (3, 4)]);
}

#[test]
fn mobile_page_break_and_column_break_keep_the_mark_on_its_own_source_line() {
    for (kind, view, terminator) in [
        (P::PageBreak, View::Mobile, T::PageBreak(B::MidParagraph)),
        (P::ColumnBreak, View::Print, T::ColumnBreak),
    ] {
        let mut input = para("ab\u{fffc}");
        input.runs[0].placeholders = vec![kind];
        let pages = Engine::new(&SimpleMetrics, setup(6000, 6000))
            .with_platform(Platform::Android, view)
            .layout(&[marked(input.clone(), &style("Mark Font", 48, 8))]);
        assert_eq!(
            lines(&pages),
            [(0, 0, 3, terminator), (1, 3, 4, T::ParagraphMark)]
        );
        let control = fragments(&pages)
            .into_iter()
            .find(|text| {
                text.source
                    .is_some_and(|(start, end)| start <= 2 && 2 < end)
            })
            .unwrap();
        assert_eq!(control.font, input.runs[0].font);
        let mark = fragment(&pages, (3, 4));
        assert_eq!(mark.font.family, "Mark Font");
        assert_eq!(mark.rise_fine, 400);
        assert_eq!(mark.x_pt, 0.0);
    }
}

#[test]
fn non_bmp_hidden_text_and_objects_do_not_shift_the_independent_mark_source() {
    let mut first = run("A\u{1f600}\u{fffc}");
    first.placeholders = vec![P::Object];
    let mut hidden = run("ZZ");
    hidden.hidden = true;
    let mut last = run("B\u{fffc}");
    last.placeholders = vec![P::Object];
    let input = Para {
        runs: vec![first, hidden, last],
        ..para("")
    };
    let pages = layout(&[marked(input, &style("Mark Font", 48, 8))]);
    assert_eq!(lines(&pages), [(0, 0, 9, T::ParagraphMark)]);
    assert_eq!(fragment(&pages, (8, 9)).font.family, "Mark Font");
    let shaper = RecordingShaper::default();
    let paint = paint_document(&pages, Some(&shaper), &["synthetic".into()]);
    let sources: Vec<_> = paint
        .pages
        .iter()
        .flat_map(|page| &page.cmds)
        .filter_map(|cmd| {
            if let DrawCmd::DrawGlyphs { glyphs, .. } = cmd {
                Some(glyphs)
            } else {
                None
            }
        })
        .flatten()
        .map(|glyph| glyph.source.unwrap())
        .collect();
    assert_eq!(sources, [(0, 1), (1, 3), (6, 7), (8, 9)]);
}

#[test]
fn soft_empty_tail_and_automatic_wrap_apply_style_only_to_the_final_mark() {
    let mut soft = para("ab\u{fffc}");
    soft.runs[0].placeholders = vec![P::LineBreak];
    let pages = layout(&[marked(soft.clone(), &style("Mark Font", 48, 8))]);
    assert_eq!(
        lines(&pages),
        [(0, 0, 3, T::LineBreak), (0, 3, 4, T::ParagraphMark)]
    );
    assert_eq!(containing(&pages, 2).font, soft.runs[0].font);
    assert_eq!(fragment(&pages, (3, 4)).font.family, "Mark Font");

    let pages = Engine::new(&SimpleMetrics, setup(240, 6000))
        .layout(&[marked(para("abcdef"), &style("Mark Font", 48, 8))]);
    assert_eq!(
        lines(&pages),
        [
            (0, 0, 2, T::Wrapped),
            (0, 2, 4, T::Wrapped),
            (0, 4, 7, T::ParagraphMark)
        ]
    );
    assert_eq!(fragment(&pages, (6, 7)).font.family, "Mark Font");
    for text in fragments(&pages)
        .into_iter()
        .filter(|text| text.source != Some((6, 7)))
    {
        assert_eq!(text.font.family, "Body Font");
        assert_eq!(text.rise_fine, 0);
    }
}

#[test]
fn changing_mark_style_does_not_change_body_shaping_input_or_glyph_clusters() {
    let mut bodies = Vec::new();
    for props in [style("Mark One", 48, 8), style("Mark Two", 72, -4)] {
        let pages = layout(&[marked(para("fi"), &props)]);
        let shaper = RecordingShaper::default();
        let paint = paint_document(&pages, Some(&shaper), &["synthetic".into()]);
        assert_eq!(
            shaper
                .0
                .borrow()
                .iter()
                .filter(|(text, _)| text == "fi")
                .count(),
            1
        );
        let body = paint.pages[0]
            .cmds
            .iter()
            .find(|cmd| matches!(cmd, DrawCmd::DrawGlyphs { text, .. } if text == "fi"))
            .unwrap()
            .clone();
        if let DrawCmd::DrawGlyphs { glyphs, source, .. } = &body {
            assert_eq!(*source, Some((0, 2)));
            assert_eq!(glyphs.len(), 1);
            assert_eq!(glyphs[0].glyph_id, 777);
            assert_eq!(glyphs[0].source, Some((0, 2)));
        }
        bodies.push(body);
    }
    assert_eq!(bodies[0], bodies[1]);
}

#[test]
fn kept_paragraphs_and_continuous_column_replay_retain_sources_and_body_positions() {
    let blocks: Vec<_> = (0..7)
        .map(|index| {
            json!({"kind":"text",
        "props":{"keepNext":index == 0,"widowControl":false,
            "spacing":{"lineRule":"exact","line":480,"before":0,"after":0}},
        "facts":{"hasSectPr":index == 5},
        "inlines":[{"kind":"run","text":"A",
            "props":{"fonts":{"ascii":"Body Font"},"size":24}}]})
        })
        .collect();
    let section = |start, end, columns, kind| {
        json!({"blockRange":[start,end],"props":{
        "kind":kind,"pageSize":{"w":2400,"h":6000},
        "pageMargins":{"top":0,"right":0,"bottom":0,"left":0},
        "columns":{"equalWidth":true,"num":columns,"space":0}}})
    };
    let mut document = document_from_json(&json!({"main":blocks,"sections":[
        section(0,6,2,"nextPage"),section(6,7,1,"continuous")]}));
    let engine = Engine::new(&SimpleMetrics, PageSetup::a4());
    let before = engine.layout_document(&document);
    for para in &mut document.paras {
        para.mark = ParagraphMarkProperties::from_json(None, Some(&style("Mark Font", 48, 8)));
    }
    let after = engine.layout_document(&document);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].line_columns, [0, 0, 0, 1, 1, 1, 2]);
    assert_eq!(lines(&after), lines(&before));
    assert_eq!(
        lines(&after)
            .iter()
            .map(|(_, a, b, _)| (*a, *b))
            .collect::<Vec<_>>(),
        (0..7)
            .map(|index| (index * 2, index * 2 + 2))
            .collect::<Vec<_>>()
    );
    for index in 0..7 {
        let old = containing(&before, index * 2);
        let new = containing(&after, index * 2);
        assert_eq!(
            (new.x_pt, new.baseline_fine, &new.font),
            (old.x_pt, old.baseline_fine, &old.font)
        );
    }
    assert_eq!(fragment(&after, (13, 14)).font.family, "Mark Font");
}

#[test]
fn canonical_docx_mark24_resolves_and_paints_without_changing_body12_flow() {
    let control = load_document(include_bytes!(
        "../../../fixtures/paragraph-mark-canonical-2026-09-27/body12-mark12.docx"
    ))
    .unwrap()
    .layout_document();
    let probe = load_document(include_bytes!(
        "../../../fixtures/paragraph-mark-canonical-2026-09-27/body12-mark24.docx"
    ))
    .unwrap()
    .layout_document();
    assert_eq!(probe.paras[0].mark.effective().unwrap()["size"], 48);
    assert_eq!(probe.paras[0].runs[0].font.size_half_points, 24);
    let engine = Engine::new(&SimpleMetrics, PageSetup::a4());
    let before = engine.layout_document(&control);
    let after = engine.layout_document(&probe);
    assert_eq!(fragment(&after, (4, 5)).font.size_half_points, 48);
    assert_eq!(lines(&after), lines(&before));
    for cp in [0, 5, 10] {
        assert_eq!(
            containing(&after, cp).baseline_fine,
            containing(&before, cp).baseline_fine
        );
        assert_eq!(containing(&after, cp).font, containing(&before, cp).font);
    }
}

#[test]
fn empty_and_fully_hidden_paragraphs_keep_legacy_height_but_paint_independent_marks() {
    let mut hidden = para("\u{1f600}H");
    hidden.runs[0].hidden = true;
    for first in [Para::default(), hidden] {
        let before = layout(&[first.clone(), para("N")]);
        let mark_cp = first
            .runs
            .iter()
            .map(|run| run.text.encode_utf16().count() as u32)
            .sum::<u32>();
        let after = layout(&[marked(first, &style("Mark Font", 48, -2)), para("N")]);
        assert_eq!(lines(&after), lines(&before));
        let mark = fragment(&after, (mark_cp, mark_cp + 1));
        assert_eq!(mark.text, " ");
        assert_eq!(mark.font.size_half_points, 48);
        assert_eq!(mark.rise_fine, -100);
        assert_eq!(
            containing(&after, mark_cp + 1).baseline_fine,
            containing(&before, mark_cp + 1).baseline_fine
        );
    }
}
