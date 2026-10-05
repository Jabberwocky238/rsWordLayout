//! 段距：行单位段距换算、contextualSpacing、相邻段距的合并。

use std::borrow::Cow;

use super::{Para, Twips, fine, grid};

/// Adjacent nonnegative paragraph spaces overlap. Preserve the existing signed
/// displacement when either value is negative; it is not a collapsible gap.
pub(super) fn paragraph_gap_fine(after: Twips, before: Twips) -> i64 {
    if after >= 0 && before >= 0 {
        fine(after.max(before))
    } else {
        fine(after) + fine(before)
    }
}

/// 行单位段距按所在节换成 twips（见 `Para::space_before_lines`）：
/// 一行是节的行网格 `w:linePitch`，没有行网格时是 240。
pub(super) fn resolve_line_units(paras: &mut Cow<'_, [Para]>, sections: &[crate::LayoutSection]) {
    for section in sections.iter() {
        let unit = grid::line_pitch(section).unwrap_or(240);
        let Some(range) = paras.get(section.para_range.clone()) else { continue };
        if range.iter().all(|p| p.space_before_lines.is_none() && p.space_after_lines.is_none()) {
            continue;
        }
        if let Some(range) = paras.to_mut().get_mut(section.para_range.clone()) {
            for para in range {
                let twips = |lines: i32| (f64::from(lines) * f64::from(unit) / 100.0).round() as Twips;
                if let Some(lines) = para.space_before_lines {
                    para.space_before = twips(lines);
                }
                if let Some(lines) = para.space_after_lines {
                    para.space_after = twips(lines);
                }
            }
        }
    }
}

/// 同样式的相邻段落去掉带 contextualSpacing 那段自己的段距（见 `Para::contextual_spacing`）。
/// 中间隔着表格的两段不算相邻。
pub(super) fn apply_contextual_spacing(paras: &mut Cow<'_, [Para]>, tables: &[crate::LayoutTable]) {
    if paras.iter().any(|p| p.contextual_spacing) {
        let adjacent = |i: usize| {
            paras[i - 1].style_id == paras[i].style_id
                && !tables.iter().any(|t| t.before_para == i)
        };
        let drops: Vec<(bool, bool)> = (0..paras.len())
            .map(|i| {
                let own = paras[i].contextual_spacing;
                (own && i > 0 && adjacent(i), own && i + 1 < paras.len() && adjacent(i + 1))
            })
            .collect();
        // 去掉的段后仍按原值抵下一段的段前：段距是「上一段的段后，加上下一段段前超出它的部分」，
        // 段后不占地方了，超出的部分照算（`both-phase`：段前 300、段后 100，带开关的段之后是 200）。
        let declared_after: Vec<Twips> = paras.iter().map(|p| p.space_after).collect();
        let paras = paras.to_mut();
        for (i, (before, after)) in drops.into_iter().enumerate() {
            if before {
                paras[i].space_before = 0;
            }
            if after {
                paras[i].space_after = 0;
                let next = &mut paras[i + 1];
                if declared_after[i] >= 0 && next.space_before >= 0 {
                    next.space_before = (next.space_before - declared_after[i]).max(0);
                }
            }
        }
    }
}
