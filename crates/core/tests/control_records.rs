//! Control character ownership, backed by the archived breaks-sections sweep.

use rsword_layout_core::{
    Color, Engine, FontSpec, LayoutRecord, LineTerminator as T, PageBreakPosition as B, PageSetup,
    Para, PlaceholderKind as P, Run, SimpleMetrics, paint_document, paras_from_document,
};

fn para(text: &str, placeholders: Vec<P>) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(),
            font: FontSpec::new("DejaVu Sans", 24),
            color: Color::BLACK,
            placeholders,
            rise: 0,
        }],
        ..Para::default()
    }
}

fn record(paras: &[Para]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(
        &Engine::new(&SimpleMetrics, PageSetup::a4()).layout(paras),
        None,
        &[],
    ))
}

fn ranges(record: &LayoutRecord) -> Vec<(usize, u32, u32, T)> {
    record
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

#[test]
fn page_break_belongs_to_the_line_before_the_page_turn() {
    assert_eq!(
        ranges(&record(&[para("ab\u{fffc}cd", vec![P::PageBreak])])),
        vec![
            (0, 0, 3, T::PageBreak(B::MidParagraph)),
            (1, 3, 6, T::ParagraphMark),
        ]
    );
}

#[test]
fn trailing_page_break_keeps_the_paragraph_mark_on_the_breaking_line() {
    assert_eq!(
        ranges(&record(&[para("ab\u{fffc}", vec![P::PageBreak])])),
        vec![(0, 0, 4, T::PageBreak(B::BeforeMark)),]
    );
}

#[test]
fn break_only_paragraph_keeps_both_control_glyphs() {
    let p = para("\u{fffc}", vec![P::PageBreak]);
    let r = record(std::slice::from_ref(&p));
    assert_eq!(ranges(&r), vec![(0, 0, 2, T::PageBreak(B::BeforeMark))]);
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&[p]);
    let text: String = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            rsword_layout_core::Fragment::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "  ");
}

#[test]
fn standalone_and_consecutive_page_breaks_keep_distinct_ranges() {
    assert_eq!(
        ranges(&record(&[para("\u{fffc}\u{fffc}z", vec![P::PageBreak; 2])])),
        vec![
            (0, 0, 1, T::PageBreak(B::OwnLine)),
            (1, 1, 2, T::PageBreak(B::OwnLine)),
            (2, 2, 4, T::ParagraphMark),
        ]
    );
}

#[test]
fn a_trailing_soft_return_leaves_an_empty_paragraph_line() {
    assert_eq!(
        ranges(&record(&[para("ab\u{fffc}", vec![P::LineBreak])])),
        vec![(0, 0, 3, T::LineBreak), (0, 3, 4, T::ParagraphMark),]
    );
}

#[test]
fn a_nonpainting_section_break_still_owns_a_source_unit() {
    let mut p = para("ab", vec![]);
    p.terminator = T::SectionBreak;
    assert_eq!(
        ranges(&record(&[p, para("cd", vec![])])),
        vec![(0, 0, 3, T::SectionBreak), (0, 3, 6, T::ParagraphMark),]
    );
}

#[test]
fn trailing_objects_after_a_page_break_stay_on_the_new_page() {
    assert_eq!(
        ranges(&record(&[para(
            "a\u{fffc}\u{fffc}",
            vec![P::PageBreak, P::Object]
        )])),
        vec![
            (0, 0, 2, T::PageBreak(B::MidParagraph)),
            (1, 2, 4, T::ParagraphMark),
        ]
    );
}

#[test]
fn leading_object_source_is_not_lost() {
    assert_eq!(
        ranges(&record(&[para("\u{fffc}ab", vec![P::Object])])),
        vec![(0, 0, 4, T::ParagraphMark),]
    );
}

#[test]
fn a_control_run_keeps_its_own_font_color_and_rise() {
    use rsword_layout_core::{DrawCmd, Fragment};
    let mut p = para("a", vec![]);
    let mut control = para("\u{fffc}", vec![P::LineBreak]).runs.remove(0);
    control.font = FontSpec::new("DejaVu Sans", 60);
    control.color = Color::rgb(40, 80, 120);
    control.rise = 120;
    p.runs.push(control.clone());
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&[p]);
    let tails: Vec<_> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) if t.text == " " => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(tails.len(), 2);
    for tail in tails {
        assert_eq!(tail.font, control.font);
        assert_eq!(tail.color, control.color);
        assert_eq!(tail.rise, control.rise);
    }
    let mut p = para("a\u{fffc}", vec![P::Object]);
    p.runs[0].rise = 120;
    let paint = paint_document(
        &Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&[p]),
        None,
        &[],
    );
    let baselines: Vec<_> = paint.pages[0]
        .cmds
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs {
                text,
                origin_y_fine,
                ..
            } if !text.is_empty() => Some(*origin_y_fine),
            _ => None,
        })
        .collect();
    assert_eq!(baselines.len(), 2);
    assert_eq!(baselines[0], baselines[1]);
}

#[test]
fn all_archived_break_fixture_line_ranges_match_word() {
    use rsword::bind::native::SessionTable;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut sessions = SessionTable::default();
    let id = sessions
        .open(
            &std::fs::read(root.join("fixtures/breaks-sections.docx")).unwrap(),
            None,
        )
        .unwrap();
    let doc = serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
    sessions.close(&id);
    let (paras, _) = paras_from_document(&doc);
    let got: Vec<_> = ranges(&record(&paras))
        .into_iter()
        .map(|(p, a, b, _)| (p, a, b))
        .collect();
    let sweep: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("captures/breaks-sections-2026-09-17/sweep.json")).unwrap(),
    )
    .unwrap();
    let mut expected: Vec<(usize, u32, u32)> = Vec::new();
    let mut previous = None;
    for pos in sweep["positions"].as_array().unwrap() {
        let key = (
            pos["page"].as_u64().unwrap() as usize - 1,
            pos["line"].as_u64().unwrap(),
        );
        let offset = pos["offset"].as_u64().unwrap() as u32;
        if previous == Some(key) {
            expected.last_mut().unwrap().2 = offset + 1;
        } else {
            expected.push((key.0, offset, offset + 1));
            previous = Some(key);
        }
    }
    assert_eq!(expected.len(), 33);
    assert_eq!(got, expected);
}

#[cfg(feature = "fontenv")]
#[test]
fn control_glyphs_keep_their_own_source_positions() {
    use rsword_layout_core::font::FontRegistry;
    let bytes = include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf");
    let mut registry = FontRegistry::new();
    registry.add(bytes.to_vec(), 0).unwrap();
    let mut section = para("ab", vec![]);
    section.terminator = T::SectionBreak;
    let cases = [
        (section, vec![(0, 1), (1, 2)]),
        (
            para("ab\u{fffc}cd", vec![P::PageBreak]),
            vec![(0, 1), (1, 2), (3, 4), (4, 5), (5, 6)],
        ),
        (
            para("ab\u{fffc}", vec![P::PageBreak]),
            vec![(0, 1), (1, 2), (2, 3), (3, 4)],
        ),
        (
            para("ab\u{fffc}", vec![P::LineBreak]),
            vec![(0, 1), (1, 2), (2, 3), (3, 4)],
        ),
        (para("", vec![]), vec![(0, 1)]),
        (para("\u{fffc}", vec![P::PageBreak]), vec![(0, 1), (1, 2)]),
    ];
    for (p, expected) in cases {
        let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&[p]);
        let r = LayoutRecord::from_paint(&paint_document(
            &pages,
            Some(&registry),
            &registry.face_ids(),
        ));
        let got: Vec<_> = r
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .flat_map(|l| &l.glyphs)
            .map(|g| {
                let s = g.source.unwrap();
                (s.start, s.end)
            })
            .collect();
        assert_eq!(got, expected);
    }
}
