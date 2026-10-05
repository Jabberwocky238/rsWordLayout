//! 行网格（`w:docGrid`）：节的步距、段落对齐网格、对齐后一行的推进与所需高度。

use std::borrow::Cow;

use super::vertical::VerticalExtent;
use super::{FINE_PER_TWIP, LineRule, Para, Twips};

/// 节有行网格（`lines` / `linesAndChars`，`w:linePitch` 为正）时的步距。
pub(super) fn line_pitch(section: &crate::LayoutSection) -> Option<Twips> {
    use crate::GridKind;
    match (section.grid.kind(), section.grid.line_pitch()) {
        (Ok(Some(GridKind::Lines | GridKind::LinesAndChars)), Ok(Some(pitch))) if pitch > 0 => Some(pitch),
        _ => None,
    }
}

/// Android 的行网格：给对齐网格的段落记上所在节的步距（见 `Para::grid_pitch`）。
/// 连续分节换了网格的两节照装载诊断不用网格（换网格的时机没测）。移动视图没有页，没有读数，不用。
pub(super) fn assign_grid_pitch(paras: &mut Cow<'_, [Para]>, sections: &[crate::LayoutSection]) {
    let switches = |a: &crate::LayoutSection, b: &crate::LayoutSection| {
        matches!(b.kind, crate::SectionStart::Continuous | crate::SectionStart::NextColumn) && a.grid != b.grid
    };
    for (index, section) in sections.iter().enumerate() {
        let Some(pitch) = line_pitch(section) else { continue };
        if index.checked_sub(1).is_some_and(|prev| switches(&sections[prev], section))
            || sections.get(index + 1).is_some_and(|next| switches(section, next))
        {
            continue;
        }
        if let Some(range) = paras.to_mut().get_mut(section.para_range.clone()) {
            for para in range {
                if para.snap_to_grid != Some(false)
                    && para.line_rule == LineRule::Auto
                    && matches!(para.line_value, 240 | 0)
                {
                    para.grid_pitch = Some(pitch);
                }
            }
        }
    }
}

/// 对齐网格的一行：推进量是不小于单倍高的最小整数倍步距，页末只要单倍高加一半余量
/// （规则与读数见 `Engine::line_vertical`）。`height` 是单倍高，1/7200 英寸。
pub(super) fn grid_line_extent(height: i64, pitch: Twips) -> VerticalExtent {
    let pitch_fine = i64::from(pitch) * FINE_PER_TWIP;
    let single_twips = (height + FINE_PER_TWIP / 2) / FINE_PER_TWIP;
    let multiple = ((single_twips * FINE_PER_TWIP + pitch_fine - 1) / pitch_fine).max(1);
    let step = multiple * pitch_fine;
    VerticalExtent { advance_fine: step, required_fine: height + (step - height).max(0) / 2 }
}
