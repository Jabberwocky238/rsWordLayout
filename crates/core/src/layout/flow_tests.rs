//! Synthetic pagination geometry, not measured Word line-height behavior.
//!
//! Every extent below uses i64 fine units (1/7200 inch). Paragraph breaking
//! supplies real source/control state; only its vertical extents are replaced.

use super::*;
use crate::SimpleMetrics;
use std::collections::VecDeque;

fn engine() -> Engine<'static, SimpleMetrics> {
    Engine::new(
        &SimpleMetrics,
        PageSetup {
            size: Size::new(10000, 10000),
            margins: Margins::uniform(0),
        },
    )
}

fn para(count: usize) -> Para {
    assert!(count > 0);
    Para {
        runs: vec![Run {
            text: vec!["x"; count].join("\u{fffc}"),
            font: FontSpec::new("synthetic", 24),
            color: Color::BLACK,
            placeholders: vec![PlaceholderKind::LineBreak; count - 1],
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        ..Para::default()
    }
}

fn extent(advance_fine: i64, required_fine: i64) -> VerticalExtent {
    VerticalExtent {
        advance_fine,
        required_fine,
    }
}

fn candidates(
    engine: &Engine<'_, SimpleMetrics>,
    para: &Para,
    vertical: &[(i64, i64)],
) -> VecDeque<PendingLine> {
    let area = engine.setup.content_area();
    let mut lines = engine.break_paragraph(para, area, fine(area.y), 0);
    assert_eq!(lines.len(), vertical.len());
    for (line, &(advance, required)) in lines.iter_mut().zip(vertical) {
        line.vertical = extent(advance, required);
    }
    lines.into()
}

fn quota(
    engine: &Engine<'_, SimpleMetrics>,
    para: &Para,
    lines: &VecDeque<PendingLine>,
    remaining_fine: i64,
    keep_after: Option<VerticalExtent>,
) -> usize {
    let area = engine.setup.content_area();
    engine
        .page_line_quota(
            para,
            lines,
            PageFit {
                area,
                next_region: FlowRegion { area, top_fine: fine(area.y) },
                top_fine: i64::from(area.bottom()) * FINE_PER_TWIP - remaining_fine,
                bottom_fine: i64::from(area.bottom()) * FINE_PER_TWIP,
                source_base: 0,
                keep_after,
            },
        )
        .0
}

#[test]
fn final_cursor_advance_can_extend_past_the_page_bottom() {
    let engine = engine();
    let para = para(2);
    let lines = candidates(&engine, &para, &[(1000, 400), (1000, 400)]);
    // Occupied intervals are [0, 400] and [1000, 1400]. The cursor ends at 2000.
    assert_eq!(quota(&engine, &para, &lines, 1400, None), 2);
    assert_eq!(quota(&engine, &para, &lines, 1399, None), 1);
    assert_eq!(lines_extent(lines.iter()), extent(2000, 1400));
}

#[test]
fn overlapping_line_extents_are_neither_summed_nor_replaced_by_advances() {
    let engine = engine();
    let para = para(2);
    let lines = candidates(&engine, &para, &[(400, 1000), (400, 1000)]);
    // The union ends at 1400, despite total advances of 800 and heights of 2000.
    assert_eq!(quota(&engine, &para, &lines, 1400, None), 2);
    assert_eq!(quota(&engine, &para, &lines, 1399, None), 1);
    assert_eq!(quota(&engine, &para, &lines, 1000, None), 1);
    assert_eq!(quota(&engine, &para, &lines, 800, None), 0);
    assert_eq!(lines_extent(lines.iter()), extent(800, 1400));
}

#[test]
fn keep_successor_starts_at_the_current_cursor_instead_of_occupied_bottom() {
    let engine = engine();
    let para = para(1);
    let lines = candidates(&engine, &para, &[(1000, 400)]);
    let successor = Some(extent(200, 500));
    // The successor occupies [1000, 1500], not [400, 900].
    assert_eq!(quota(&engine, &para, &lines, 1500, successor), 1);
    assert_eq!(quota(&engine, &para, &lines, 1499, successor), 0);
}

#[test]
fn keep_successor_cannot_erase_a_preceding_line_overhang() {
    let engine = engine();
    let para = para(1);
    let lines = candidates(&engine, &para, &[(1000, 2500)]);
    let successor = Some(extent(200, 500));
    // The successor ends at 1500, but the current line still occupies to 2500.
    assert_eq!(quota(&engine, &para, &lines, 2500, successor), 1);
    assert_eq!(quota(&engine, &para, &lines, 2499, successor), 0);
}

#[test]
fn an_early_line_can_extend_past_both_the_last_line_and_kept_successor() {
    let engine = engine();
    let para = para(2);
    let lines = candidates(&engine, &para, &[(1000, 2500), (200, 100)]);
    assert_eq!(lines_extent(lines.iter()), extent(1200, 2500));
    assert_eq!(
        quota(&engine, &para, &lines, 2500, Some(extent(50, 500))),
        2
    );
    assert_eq!(
        quota(&engine, &para, &lines, 2499, Some(extent(50, 500))),
        0
    );
}

#[test]
fn missing_keep_successor_does_not_reserve_an_empty_cursor_position() {
    let engine = engine();
    let para = Para {
        keep_next: true,
        ..para(1)
    };
    let lines = candidates(&engine, &para, &[(1000, 400)]);
    let area = engine.setup.content_area();
    let after = engine.keep_after_extent(
        std::slice::from_ref(&para),
        &[],
        0,
        area,
        (1000, 2, FlowRegion { area, top_fine: fine(area.y) }),
        i64::from(area.height) * FINE_PER_TWIP,
    );
    assert_eq!(after, None);
    assert_eq!(quota(&engine, &para, &lines, 400, after), 1);
    assert_eq!(quota(&engine, &para, &lines, 399, after), 0);
}

#[test]
fn actual_zero_sized_successor_still_requires_its_cursor_position() {
    let engine = engine();
    let para = para(1);
    let lines = candidates(&engine, &para, &[(1000, 400)]);
    let after = Some(VerticalExtent::default());
    assert_eq!(quota(&engine, &para, &lines, 1000, after), 1);
    assert_eq!(quota(&engine, &para, &lines, 999, after), 0);
}

#[test]
fn negative_paragraph_gap_does_not_create_occupied_space_at_the_unshifted_origin() {
    let engine = engine();
    let current = Para {
        keep_next: true,
        space_after: -200,
        ..para(1)
    };
    let next = Para {
        line_rule: LineRule::Exact,
        line_value: 20,
        ..para(1)
    };
    let lines = candidates(&engine, &current, &[(1000, 400)]);
    let area = engine.setup.content_area();
    let after = engine.keep_after_extent(
        &[current.clone(), next],
        &[],
        0,
        area,
        (1000, 2, FlowRegion { area, top_fine: fine(area.y) }),
        i64::from(area.height) * FINE_PER_TWIP,
    );
    // The gap is -1000 fine; the real successor occupies [-1000, -900]
    // relative to the cursor. Together with the current line it ends at 400.
    assert_eq!(after, Some(extent(-900, -900)));
    assert_eq!(quota(&engine, &current, &lines, 400, after), 1);
    assert_eq!(quota(&engine, &current, &lines, 399, after), 0);
}

#[test]
fn explicit_page_break_ends_the_group_even_when_later_lines_fit() {
    let engine = engine();
    let mut para = Para {
        widow_control: true,
        ..para(3)
    };
    para.runs[0].placeholders[0] = PlaceholderKind::PageBreak;
    let lines = candidates(&engine, &para, &[(1000, 400); 3]);
    assert!(lines[0].flow_break.is_hard());
    assert_eq!(quota(&engine, &para, &lines, 5000, None), 1);
    assert_eq!(quota(&engine, &para, &lines, 400, None), 1);
}

#[test]
fn explicit_final_page_break_overrides_keep_reservation() {
    let engine = engine();
    let mut para = para(1);
    para.runs[0].text.push(OBJECT_PLACEHOLDER);
    para.runs[0].placeholders.push(PlaceholderKind::PageBreak);
    let lines = candidates(&engine, &para, &[(1000, 400)]);
    assert!(lines[0].flow_break.is_hard());
    assert_eq!(
        quota(&engine, &para, &lines, 400, Some(extent(5000, 5000))),
        1
    );
}

#[test]
fn changing_required_extent_does_not_move_the_baseline_or_drawn_fragments() {
    let engine = engine();
    let para = para(2);
    let mut lines = candidates(&engine, &para, &[(978, 400); 2]);
    let area = engine.setup.content_area();
    let mut before = Page::new(engine.setup.size, area);
    let mut after = Page::new(engine.setup.size, area);
    let mut top_fine = 17;
    for (index, line) in lines.iter_mut().enumerate() {
        engine.place_line(&mut before, line, &para, top_fine, index as u32);
        line.vertical.required_fine = 2500;
        engine.place_line(&mut after, line, &para, top_fine, index as u32);
        top_fine += line.vertical.advance_fine;
    }
    let drawn = |page: &Page| {
        page.fragments
            .iter()
            .map(|fragment| match fragment {
                Fragment::Text(text) => (
                    text.line,
                    text.text.clone(),
                    text.source,
                    text.x_pt,
                    text.baseline_fine,
                ),
                _ => panic!("synthetic text fixture must only produce text fragments"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(drawn(&before), drawn(&after));
    for (old, new) in before.line_placements.iter().zip(&after.line_placements) {
        let old = old.unwrap();
        let new = new.unwrap();
        assert_eq!(old.required_fine, 400);
        assert_eq!(new.required_fine, 2500);
        assert_eq!(new.advance_fine, 978);
        assert_eq!(new.baseline_fine, old.baseline_fine);
    }
    let baselines = drawn(&after);
    let first = baselines.iter().find(|fragment| fragment.0 == 0).unwrap().4;
    let second = baselines.iter().find(|fragment| fragment.0 == 1).unwrap().4;
    assert_eq!(second - first, 978);
}
