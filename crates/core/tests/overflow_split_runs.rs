//! Fragmentation must not change the observed single-punctuation overflow rule.
//! Synthetic widths isolate adjacency; these are engine regressions, not Word captures.

use rsword_layout_core::{
    Color, Engine, FontSpec, Fragment, Margins, Page, PageSetup, Para, PlaceholderKind, Run,
    SimpleMetrics, Size, TextFragment,
};

fn run(text: &str) -> Run {
    Run {
        text: text.to_owned(),
        font: FontSpec::new("synthetic", 20),
        color: Color::BLACK,
        placeholders: vec![],
        rise: 0,
    }
}

fn layout(runs: Vec<Run>, overflow_punct: bool) -> Vec<Page> {
    let setup = PageSetup {
        size: Size::new(400, 10000),
        margins: Margins::uniform(0),
    };
    Engine::new(&SimpleMetrics, setup).layout(&[Para {
        runs,
        overflow_punct,
        ..Para::default()
    }])
}

fn fragments(pages: &[Page]) -> Vec<&TextFragment> {
    assert_eq!(pages.len(), 1);
    pages[0]
        .fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn lines(pages: &[Page]) -> Vec<String> {
    let mut lines = Vec::new();
    for fragment in fragments(pages) {
        while lines.len() <= fragment.line as usize {
            lines.push(String::new());
        }
        lines[fragment.line as usize].push_str(&fragment.text);
    }
    lines
}

#[test]
fn a_run_initial_punctuation_hangs_on_the_existing_line() {
    let text = "\u{4e2d}\u{6587}\u{3002}\u{6587}";
    let expected = lines(&layout(vec![run(text)], true));
    assert_eq!(expected, ["\u{4e2d}\u{6587}\u{3002}", "\u{6587} "]);
    for runs in [
        vec![run("\u{4e2d}\u{6587}"), run("\u{3002}\u{6587}")],
        vec![
            run("\u{4e2d}\u{6587}"),
            run(""),
            run("\u{3002}"),
            run("\u{6587}"),
        ],
    ] {
        let pages = layout(runs, true);
        assert_eq!(lines(&pages), expected);
        let ranges: Vec<_> = fragments(&pages).iter().filter_map(|f| f.source).collect();
        assert_eq!(&ranges[..2], [(0, 2), (2, 3)]);
        assert_eq!(ranges.last().unwrap().1, 5);
        assert!(ranges.windows(2).all(|pair| pair[0].1 == pair[1].0));
    }
}

#[test]
fn closing_punctuation_in_the_next_run_prevents_single_punctuation_overflow() {
    for runs in [
        vec![run("\u{4e2d}\u{6587}\u{3002}"), run("\u{ff1b}\u{6587}")],
        vec![
            run("\u{4e2d}\u{6587}\u{3002}"),
            run(""),
            run("\u{ff1b}\u{6587}"),
        ],
        vec![
            run("\u{4e2d}\u{6587}"),
            run("\u{3002}"),
            run("\u{ff1b}\u{6587}"),
        ],
    ] {
        assert_eq!(
            lines(&layout(runs.clone(), true)),
            lines(&layout(runs, false)),
        );
    }
}

#[test]
fn punctuation_overflow_preserves_run_styles_and_utf16_spans() {
    let mut punctuation = run("\u{3002}\u{6587}");
    punctuation.font = FontSpec::new("punctuation font", 10);
    punctuation.color = Color {
        r: 10,
        g: 20,
        b: 30,
    };
    punctuation.rise = 40;
    let pages = layout(vec![run("\u{20000}\u{6587}"), punctuation.clone()], true);
    let fragments = fragments(&pages);
    let hung = fragments.iter().find(|f| f.text == "\u{3002}").unwrap();
    assert_eq!(hung.line, 0);
    assert_eq!(hung.source, Some((3, 4)));
    assert_eq!(hung.x, 400);
    assert_eq!(hung.font, punctuation.font);
    assert_eq!(hung.color, punctuation.color);
    assert_eq!(hung.rise, 40);
    assert_eq!(fragments[0].source, Some((0, 3)));
    assert_eq!(fragments[0].font, FontSpec::new("synthetic", 20));
}

#[test]
fn an_object_gap_is_a_barrier_to_previous_character_context() {
    for runs in [
        vec![run("\u{4e2d}\u{6587}\u{fffc}"), run("\u{3002}\u{6587}")],
        vec![run("\u{4e2d}\u{6587}"), run("\u{fffc}\u{3002}\u{6587}")],
    ] {
        assert_eq!(
            lines(&layout(runs.clone(), true)),
            lines(&layout(runs, false)),
        );
    }
}

#[test]
fn lookahead_does_not_cross_an_object_gap() {
    for runs in [
        vec![run("\u{4e2d}\u{6587}\u{3002}\u{fffc}\u{ff1b}\u{6587}")],
        vec![
            run("\u{4e2d}\u{6587}\u{3002}"),
            run("\u{fffc}"),
            run("\u{ff1b}\u{6587}"),
        ],
    ] {
        assert_eq!(lines(&layout(runs, true))[0], "\u{4e2d}\u{6587}\u{3002}");
    }
}

#[test]
fn previous_character_context_does_not_cross_a_line_break() {
    let mut first = run("\u{4e2d}\u{6587}\u{fffc}");
    first.placeholders.push(PlaceholderKind::LineBreak);
    let pages = layout(vec![first, run("\u{3002}\u{6587}")], true);
    assert_eq!(lines(&pages), ["\u{4e2d}\u{6587} ", "\u{3002}\u{6587} "]);
}
