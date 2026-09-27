//! Run segmentation must not change the existing real-font natural-height policy.
#![cfg(feature = "fontenv")]

use rsword_layout_core::font::{FontRegistry, RealMetrics, VerticalGrid};
use rsword_layout_core::{
    Color, Engine, FontMetrics, FontSlots, FontSpec, LayoutRecord, LinePlacement, Margins, Page,
    PageSetup, Para, Run, Size, paint_document,
};

fn registry() -> FontRegistry {
    let mut registry = FontRegistry::new();
    for bytes in [
        include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf").as_slice(),
        include_bytes!("../../../fixtures/fonts/LiberationMono-Regular.ttf").as_slice(),
        include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf").as_slice(),
    ] {
        registry.add(bytes.to_vec(), 0).unwrap();
    }
    registry
}

fn font(ascii: &str) -> FontSpec {
    let mut font = FontSpec::new(ascii, 24);
    font.slots = FontSlots {
        ascii: Some(ascii.into()),
        h_ansi: Some("Liberation Serif".into()),
        east_asia: Some("Droid Sans Fallback".into()),
        ..FontSlots::default()
    };
    font
}

fn paragraph(ascii: &str, text: &str, split: bool) -> Para {
    let texts = if split {
        text.chars().map(|ch| ch.to_string()).collect()
    } else {
        vec![text.to_string()]
    };
    Para {
        runs: texts
            .into_iter()
            .map(|text| run(&text, font(ascii)))
            .collect(),
        ..Para::default()
    }
}

fn run(text: &str, font: FontSpec) -> Run {
    Run {
        text: text.into(),
        font,
        color: Color::BLACK,
        hidden: false,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
    }
}

fn layout(metrics: &RealMetrics<'_>, para: Para, height: i32) -> Vec<Page> {
    let setup = PageSetup {
        size: Size::new(3000, height),
        margins: Margins::uniform(0),
    };
    Engine::new(metrics, setup).layout(&[para.clone(), para])
}

fn placements(pages: &[Page]) -> Vec<LinePlacement> {
    pages
        .iter()
        .flat_map(|page| &page.line_placements)
        .map(|placement| placement.unwrap())
        .collect()
}

fn sources(pages: &[Page]) -> Vec<(usize, u32, u32)> {
    let record = LayoutRecord::from_paint(&paint_document(pages, None, &[]));
    record
        .pages
        .iter()
        .flat_map(|page| {
            page.lines.iter().map(move |line| {
                let source = line.source.unwrap();
                (page.index, source.start, source.end)
            })
        })
        .collect()
}

fn assert_segmentation_invariant(ascii: &str, text: &str, grid: VerticalGrid, advance: i64) {
    let registry = registry();
    let metrics = RealMetrics::new(&registry).with_vertical_grid(grid);
    let merged = layout(&metrics, paragraph(ascii, text, false), 10000);
    let split = layout(&metrics, paragraph(ascii, text, true), 10000);
    assert_eq!(sources(&merged), [(0, 0, 3), (0, 3, 6)]);
    assert_eq!(sources(&split), sources(&merged));
    let vertical = |pages: &[Page]| {
        placements(pages)
            .into_iter()
            .map(|p| (p.top_fine, p.advance_fine, p.required_fine))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        vertical(&merged),
        [(0, advance, advance), (advance, advance, advance)]
    );
    assert_eq!(
        vertical(&split),
        vertical(&merged),
        "{ascii}, {text:?}, {grid:?}"
    );
    let baselines = |pages: &[Page]| {
        placements(pages)
            .into_iter()
            .map(|p| (p.baseline_offset_fine, p.baseline_fine))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        baselines(&split),
        baselines(&merged),
        "{ascii}, {text:?}, {grid:?}"
    );

    // These are current engine-policy thresholds, not measured Word line boxes.
    let exact_fit_twips = ((advance * 2 + 4) / 5) as i32;
    for (height, expected_sources) in [
        (exact_fit_twips - 1, vec![(0, 0, 3), (1, 3, 6)]),
        (exact_fit_twips, vec![(0, 0, 3), (0, 3, 6)]),
    ] {
        for split in [false, true] {
            let pages = layout(&metrics, paragraph(ascii, text, split), height);
            assert_eq!(
                sources(&pages),
                expected_sources,
                "{ascii}, {text:?}, {grid:?}, split={split}, height={height}"
            );
            assert_eq!(pages.len(), expected_sources.last().unwrap().0 + 1);
        }
    }
}

#[test]
fn sans_serif_no_grid_merged_and_split_have_same_height_and_page_boundary() {
    assert_segmentation_invariant("Liberation Sans", "A\u{03b1}", VerticalGrid::None, 1397);
}

#[test]
fn mono_serif_no_grid_merged_and_split_have_same_height_and_page_boundary() {
    assert_segmentation_invariant("Liberation Mono", "A\u{03b1}", VerticalGrid::None, 1481);
}

#[test]
fn sans_droid_no_grid_merged_and_split_have_same_height_and_page_boundary() {
    assert_segmentation_invariant("Liberation Sans", "A\u{4e2d}", VerticalGrid::None, 1610);
}

#[test]
fn sans_serif_mac_grid_merged_and_split_have_same_height_and_page_boundary() {
    assert_segmentation_invariant(
        "Liberation Sans",
        "A\u{03b1}",
        VerticalGrid::MacWordThreeHundredthsInch,
        1397,
    );
}

#[test]
fn mono_serif_mac_grid_merged_and_split_have_same_height_and_page_boundary() {
    assert_segmentation_invariant(
        "Liberation Mono",
        "A\u{03b1}",
        VerticalGrid::MacWordThreeHundredthsInch,
        1490,
    );
}

#[test]
fn sans_droid_mac_grid_merged_and_split_have_same_height_and_page_boundary() {
    assert_segmentation_invariant(
        "Liberation Sans",
        "A\u{4e2d}",
        VerticalGrid::MacWordThreeHundredthsInch,
        1610,
    );
}

#[test]
fn merged_run_preserves_existing_one_sum_rounding_and_baseline_policy() {
    let registry = registry();
    for (ascii, text, grid, advance, baseline_offset) in [
        (
            "Liberation Sans",
            "A\u{03b1}",
            VerticalGrid::None,
            1397,
            1085,
        ),
        (
            "Liberation Mono",
            "A\u{03b1}",
            VerticalGrid::None,
            1481,
            1070,
        ),
        (
            "Liberation Sans",
            "A\u{4e2d}",
            VerticalGrid::None,
            1610,
            1250,
        ),
        (
            "Liberation Sans",
            "A\u{03b1}",
            VerticalGrid::MacWordThreeHundredthsInch,
            1397,
            1125,
        ),
        (
            "Liberation Mono",
            "A\u{03b1}",
            VerticalGrid::MacWordThreeHundredthsInch,
            1490,
            1130,
        ),
        (
            "Liberation Sans",
            "A\u{4e2d}",
            VerticalGrid::MacWordThreeHundredthsInch,
            1610,
            1300,
        ),
    ] {
        let metrics = RealMetrics::new(&registry).with_vertical_grid(grid);
        let pages = layout(&metrics, paragraph(ascii, text, false), 10000);
        let line = placements(&pages)[0];
        assert_eq!(
            (
                line.advance_fine,
                line.required_fine,
                line.baseline_offset_fine
            ),
            (advance, advance, baseline_offset),
            "{ascii}, {text:?}, {grid:?}"
        );
    }
}

#[test]
fn fitted_prefix_excludes_the_rejected_faces_vertical_metrics() {
    let registry = registry();
    for grid in [VerticalGrid::None, VerticalGrid::MacWordThreeHundredthsInch] {
        let metrics = RealMetrics::new(&registry).with_vertical_grid(grid);
        let text_font = font("Liberation Sans");
        let width = metrics.measure("A", &text_font).advance;
        let setup = PageSetup {
            size: Size::new(width, 10000),
            margins: Margins::uniform(0),
        };
        let pages = Engine::new(&metrics, setup).layout(&[paragraph(
            "Liberation Sans",
            "A\u{4e2d}",
            false,
        )]);
        assert_eq!(sources(&pages), [(0, 0, 1), (0, 1, 3)], "{grid:?}");
        let actual = placements(&pages);
        for (index, text) in ["A", "\u{4e2d}"].into_iter().enumerate() {
            let isolated = layout(&metrics, paragraph("Liberation Sans", text, false), 10000);
            let expected = placements(&isolated)[0];
            assert_eq!(
                (
                    actual[index].advance_fine,
                    actual[index].required_fine,
                    actual[index].baseline_offset_fine
                ),
                (
                    expected.advance_fine,
                    expected.required_fine,
                    expected.baseline_offset_fine
                ),
                "{grid:?}, accepted text={text:?}"
            );
        }
        assert!(actual[0].advance_fine < actual[1].advance_fine);
    }
}

#[test]
fn rollback_removes_a_maximum_contributor_from_a_whole_or_partial_piece() {
    let registry = registry();
    for grid in [VerticalGrid::None, VerticalGrid::MacWordThreeHundredthsInch] {
        let metrics = RealMetrics::new(&registry).with_vertical_grid(grid);
        let base = font("Liberation Mono");
        for partial_piece in [false, true] {
            let mut tall = base.clone();
            if !partial_piece {
                tall.size_half_points = 48;
            }
            let tail = vec![run("\u{03b1}b", tall.clone()), run("cdef", base.clone())];
            let width = tail
                .iter()
                .map(|r| metrics.measure(&r.text, &r.font).advance)
                .sum();
            let runs = if partial_piece {
                vec![run("a \u{03b1}b", base.clone()), run("cdef", base.clone())]
            } else {
                vec![
                    run("a ", base.clone()),
                    run("\u{03b1}b", tall),
                    run("cdef", base.clone()),
                ]
            };
            assert!(metrics.measure(&runs[0].text, &runs[0].font).advance <= width);
            let setup = PageSetup {
                size: Size::new(width, 10000),
                margins: Margins::uniform(0),
            };
            let pages = Engine::new(&metrics, setup).layout(&[Para {
                runs,
                ..Para::default()
            }]);
            assert_eq!(
                sources(&pages),
                [(0, 0, 2), (0, 2, 9)],
                "{grid:?}, partial_piece={partial_piece}"
            );
            let actual = placements(&pages);
            let isolated_prefix = layout(
                &metrics,
                Para {
                    runs: vec![run("a ", base.clone())],
                    ..Para::default()
                },
                10000,
            );
            let isolated_tail = layout(
                &metrics,
                Para {
                    runs: tail,
                    ..Para::default()
                },
                10000,
            );
            for (index, reference) in [isolated_prefix, isolated_tail].iter().enumerate() {
                let expected = placements(reference)[0];
                assert_eq!(
                    (
                        actual[index].advance_fine,
                        actual[index].required_fine,
                        actual[index].baseline_offset_fine
                    ),
                    (
                        expected.advance_fine,
                        expected.required_fine,
                        expected.baseline_offset_fine
                    ),
                    "{grid:?}, partial_piece={partial_piece}, line={index}"
                );
            }
            assert!(actual[0].advance_fine < actual[1].advance_fine);
        }
    }
}
