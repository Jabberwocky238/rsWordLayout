//! Preserve existing control painting while separating explicit source tails.
//! These are engine contracts; they add no Word glyph-count or geometry evidence.

use rsword_layout_core::{
    Color, Engine, FontSpec, Fragment, LayoutRecord, LineTerminator as T, Page,
    PageBreakPosition as B, PageSetup, Para, PlaceholderKind as P, Run, SimpleMetrics,
    TextFragment, paint_document,
};

fn run(text: &str, size: u32, placeholders: Vec<P>) -> Run {
    Run {
        text: text.into(),
        font: FontSpec::new("DejaVu Sans", size),
        color: Color::BLACK,
        placeholders,
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn layout(runs: Vec<Run>, terminator: T) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&[Para {
        runs,
        terminator,
        ..Para::default()
    }])
}

fn texts(pages: &[Page]) -> Vec<&TextFragment> {
    pages
        .iter()
        .flat_map(|page| &page.fragments)
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn same_style_body_page_break_and_mark_keep_one_shaping_fragment() {
    let pages = layout(
        vec![run("A\u{fffc}", 24, vec![P::PageBreak])],
        T::ParagraphMark,
    );
    let fragments = texts(&pages);
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].text, "A  ");
    assert_eq!(fragments[0].source, Some((0, 3)));
    assert_eq!(fragments[0].terminator, T::PageBreak(B::BeforeMark));
}

#[test]
fn a_different_control_style_keeps_both_existing_tail_spaces_together() {
    let mut control = run("\u{fffc}", 16, vec![P::PageBreak]);
    control.rise_fine = Some(137);
    let pages = layout(vec![run("A", 24, vec![]), control], T::ParagraphMark);
    let fragments = texts(&pages);
    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0].text, "A");
    assert_eq!(fragments[1].text, "  ");
    assert_eq!(fragments[1].source, Some((1, 3)));
    assert_eq!(fragments[1].font.size_half_points, 16);
    assert_eq!(fragments[1].rise_fine, 137);
    assert_eq!(fragments[0].terminator, T::Wrapped);
    assert_eq!(fragments[1].terminator, T::PageBreak(B::BeforeMark));
}

#[test]
fn hidden_and_object_source_holes_do_not_belong_to_the_mark_space() {
    let mut hidden = run("\u{1f600}", 12, vec![]);
    hidden.hidden = true;
    let pages = layout(
        vec![
            run("A", 24, vec![]),
            run("\u{fffc}", 16, vec![P::Object]),
            hidden,
        ],
        T::ParagraphMark,
    );
    let fragments = texts(&pages);
    assert_eq!(fragments.len(), 3);
    assert_eq!(
        (fragments[0].text.as_str(), fragments[0].source),
        ("A", Some((0, 1)))
    );
    assert_eq!(
        (fragments[1].text.as_str(), fragments[1].source),
        ("", Some((1, 4)))
    );
    assert_eq!(
        (fragments[2].text.as_str(), fragments[2].source),
        (" ", Some((4, 5)))
    );
    assert_eq!(fragments[1].x_pt, fragments[2].x_pt);
}

#[test]
fn two_nonpainting_controls_keep_both_source_units_without_visible_spaces() {
    let pages = layout(
        vec![run("A\u{fffc}", 24, vec![P::PageBreak])],
        T::SectionBreak,
    );
    let fragments = texts(&pages);
    assert_eq!(fragments.len(), 2);
    assert_eq!(
        (fragments[0].text.as_str(), fragments[0].source),
        ("A", Some((0, 1)))
    );
    assert_eq!(
        (fragments[1].text.as_str(), fragments[1].source),
        ("", Some((1, 3)))
    );
    assert_eq!(fragments[1].terminator, T::PageBreak(B::MidParagraph));
    let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    let source = record.pages[0].lines[0].source.unwrap();
    assert_eq!((source.start, source.end), (0, 3));
}

#[test]
fn fully_hidden_non_bmp_text_leaves_the_mark_at_its_real_source_position() {
    let mut hidden = run("\u{1f600}a\u{301}", 18, vec![]);
    hidden.hidden = true;
    let pages = layout(vec![hidden], T::ParagraphMark);
    let fragments = texts(&pages);
    assert_eq!(fragments.len(), 2);
    assert_eq!(
        (fragments[0].text.as_str(), fragments[0].source),
        ("", Some((0, 4)))
    );
    assert_eq!(
        (fragments[1].text.as_str(), fragments[1].source),
        (" ", Some((4, 5)))
    );
}

#[cfg(feature = "shape")]
#[test]
fn combining_and_non_bmp_clusters_keep_page_control_and_mark_source_units() {
    use rsword_layout_core::{DrawCmd, RustybuzzShaper};

    let mut shaper = RustybuzzShaper::new();
    shaper.add_face(
        "DejaVu Sans",
        include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf").to_vec(),
        0,
    );
    let pages = layout(
        vec![run("\u{1f600}a\u{301}\u{fffc}", 24, vec![P::PageBreak])],
        T::ParagraphMark,
    );
    let sources: Vec<_> = paint_document(&pages, Some(&shaper), &shaper.face_ids())
        .pages
        .into_iter()
        .flat_map(|page| page.cmds)
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .map(|glyph| glyph.source.unwrap())
        .collect();
    assert_eq!(sources, [(0, 2), (2, 4), (4, 5), (5, 6)]);
}
