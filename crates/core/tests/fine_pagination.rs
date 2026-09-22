//! Pagination reservations must use the same fine height as the page cursor.
//! SimpleMetrics at 8.5pt advances 195.6 twips while its coarse metrics say 195.
//! These synthetic boundaries verify engine consistency, not Word's size model.

use std::collections::BTreeSet;

use rsword_layout_core::{
    Color, Engine, FontSpec, Fragment, Margins, Page, PageSetup, Para, PlaceholderKind, Run,
    SimpleMetrics, Size,
};

fn para(text: &str) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(),
            font: FontSpec::new("synthetic", 17),
            color: Color::BLACK,
            placeholders: text
                .matches('\u{fffc}')
                .map(|_| PlaceholderKind::LineBreak)
                .collect(),
            rise: 0,
            rise_fine: None,
        }],
        ..Para::default()
    }
}

fn layout(paras: &[Para], height: i32) -> Vec<Page> {
    Engine::new(
        &SimpleMetrics,
        PageSetup {
            size: Size::new(10000, height),
            margins: Margins::uniform(0),
        },
    )
    .layout(paras)
}

fn lines_per_page(pages: &[Page]) -> Vec<usize> {
    pages
        .iter()
        .map(|p| {
            p.fragments
                .iter()
                .filter_map(|f| match f {
                    Fragment::Text(t) => Some(t.line),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .len()
        })
        .collect()
}

#[test]
fn keep_lines_moves_a_fine_height_block_before_placing_any_of_it() {
    let block = Para {
        keep_lines: true,
        ..para("x\u{fffc}x\u{fffc}x")
    };
    // 195.6 + 3 * 195.6 = 782.4, exceeding the page. The block alone fits.
    let pages = layout(&[para("intro"), block], 781);
    assert_eq!(lines_per_page(&pages), [1, 3]);
    let spans: Vec<_> = pages
        .iter()
        .flat_map(|p| &p.fragments)
        .filter_map(|f| match f {
            Fragment::Text(t) => t.source,
            _ => None,
        })
        .collect();
    assert_eq!(spans.first().unwrap().0, 0);
    assert_eq!(spans.last().unwrap().1, 12);
    assert!(spans.windows(2).all(|pair| pair[0].1 == pair[1].0));
}

#[test]
fn keep_lines_accepts_an_exact_fine_fit_and_rejects_one_twip_less() {
    let block = Para {
        keep_lines: true,
        ..para("x\u{fffc}x\u{fffc}x\u{fffc}x")
    };
    let input = [para("intro"), block];
    // Five lines total exactly 978 twips. Coarse summation loses three twips.
    assert_eq!(lines_per_page(&layout(&input, 978)), [5]);
    assert_eq!(lines_per_page(&layout(&input, 977)), [1, 4]);
}

#[test]
fn keep_next_reserves_the_following_lines_fine_height() {
    let current = Para {
        keep_next: true,
        ..para("keep")
    };
    let input = [para("intro"), current, para("next")];
    assert_eq!(lines_per_page(&layout(&input, 586)), [1, 2]);
    assert_eq!(lines_per_page(&layout(&input, 587)), [3]);
}

#[test]
fn keep_next_accepts_an_exact_fine_fit_and_rejects_one_twip_less() {
    let current = Para {
        keep_next: true,
        ..para("keep")
    };
    let input = [
        para("one"),
        para("two"),
        para("three"),
        current,
        para("next"),
    ];
    assert_eq!(lines_per_page(&layout(&input, 978)), [5]);
    assert_eq!(lines_per_page(&layout(&input, 977)), [3, 2]);
}

#[test]
fn ordinary_pagination_reserves_the_same_height_it_advances() {
    let input = [para("one"), para("two")];
    // Two lines total 391.2 twips, so 391 does not fit.
    assert_eq!(lines_per_page(&layout(&input, 391)), [1, 1]);
    assert_eq!(lines_per_page(&layout(&input, 392)), [2]);
    let input = [
        para("one"),
        para("two"),
        para("three"),
        para("four"),
        para("five"),
    ];
    assert_eq!(lines_per_page(&layout(&input, 978)), [5]);
    assert_eq!(lines_per_page(&layout(&input, 977)), [4, 1]);
}
