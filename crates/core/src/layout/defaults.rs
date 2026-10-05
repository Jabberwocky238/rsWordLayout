//! Android Word 给文档没写的属性补的缺省值：字体、字号、段后。只在 [`super::Platform::Android`] 下用。

use std::borrow::Cow;

use super::{FontSpec, Para, Twips};

pub(super) const ANDROID_DEFAULT_FAMILY: &str = "DengXian";

/// 包里没有样式 part 时 Android Word 给没写段后的段落的缺省段后，twips（8pt）。
///
/// 依据（2026-10-04 补测第 10、11 条，Android 打印视图）：60 段精确 480、不写段前段后的 `sp-default`
/// 一页 24 段（段后 0 是 32，160 是 `1 + floor((15398 − 480) / 640)` = 24）；`talltable-080`（格内段落不写段后）
/// 一页 21 行，格内写明段后 0 是 28 行——差的正是每行 160。有样式 part 而文档默认值不写段后的情形没测，不换。
pub(super) const ANDROID_DEFAULT_SPACE_AFTER: Twips = 160;

/// Android Word 的缺省字体：等线（DengXian，云字体）11pt。
///
/// 依据（2026-10-04 补测，Android 打印视图）：word_analyse 的纵向夹具都不写字体，其中显式 `w:sz=24`
/// 的 `longpage-auto12pt` 页 0 是 47 行（「缺省 11pt」的预测；「12pt 起跳」51 行）；打开不写字体
/// 的夹具时进程映射的是 `DengXian-54497409372.ttf`；不写字体的一串数字窄路径每行 45 个、打印视图
/// 90 个，与等线 11pt 的字宽相符。只换桥接层补的缺省值（[`FontSpec::family_is_fallback`]、
/// [`FontSpec::size_is_fallback`]），文档写了的照用。上下标的字号随之按比例缩。
///
/// 量不了等线时（`default_face` 为假：没装这个字体），没写字体的 run 字体、字号都照原来的替身排
/// ——拿别的字体按 11pt 排只会更远；写了字体、没写字号的 run 照样换成 11pt。
fn android_default_font(font: &mut FontSpec, default_face: bool) {
    const FAMILY: &str = ANDROID_DEFAULT_FAMILY;
    const SIZE_HALF_POINTS: u32 = 22;
    if font.family_is_fallback {
        if !default_face {
            return;
        }
        font.family = FAMILY.to_string();
        let hint = font.slots.hint;
        font.slots = crate::font::FontSlots {
            ascii: Some(FAMILY.to_string()),
            h_ansi: Some(FAMILY.to_string()),
            east_asia: Some(FAMILY.to_string()),
            cs: Some(FAMILY.to_string()),
            hint,
        };
        font.family_is_fallback = false;
    }
    if font.size_is_fallback {
        if let Some(centipoints) = font.size_centipoints {
            font.size_centipoints =
                Some(centipoints * u64::from(SIZE_HALF_POINTS) / u64::from(font.size_half_points.max(1)));
        }
        font.size_half_points = SIZE_HALF_POINTS;
        font.size_is_fallback = false;
    }
}

/// 正文段落、表格单元格段落换成 Android 的缺省值；`implied_final_para` 只用来判断要不要换。
pub(super) fn apply_android_defaults(
    paras: &mut Cow<'_, [Para]>,
    tables: &mut Cow<'_, [crate::LayoutTable]>,
    implied_final_para: Option<&Para>,
    default_face: bool,
) {
        let fallback = |p: &Para| p.runs.iter().any(|r| r.font.family_is_fallback || r.font.size_is_fallback);
        if paras.iter().any(fallback) || implied_final_para.is_some_and(fallback) {
            for para in paras.to_mut() {
                para.runs.iter_mut().for_each(|run| android_default_font(&mut run.font, default_face));
            }
        }
        let cells = |t: &crate::LayoutTable| t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.paras).any(fallback);
        if tables.iter().any(cells) {
            for para in tables.to_mut().iter_mut()
                .flat_map(|t| &mut t.rows).flat_map(|r| &mut r.cells).flat_map(|c| &mut c.paras)
            {
                para.runs.iter_mut().for_each(|run| android_default_font(&mut run.font, default_face));
            }
        }
        if paras.iter().any(|p| p.space_after_is_fallback) {
            for para in paras.to_mut().iter_mut().filter(|p| p.space_after_is_fallback) {
                para.space_after = ANDROID_DEFAULT_SPACE_AFTER;
            }
        }
        let cell_paras = |t: &crate::LayoutTable| t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.paras)
            .any(|p| p.space_after_is_fallback);
        if tables.iter().any(cell_paras) {
            for para in tables.to_mut().iter_mut()
                .flat_map(|t| &mut t.rows).flat_map(|r| &mut r.cells).flat_map(|c| &mut c.paras)
                .filter(|p| p.space_after_is_fallback)
            {
                para.space_after = ANDROID_DEFAULT_SPACE_AFTER;
            }
        }
}

/// 表后补的空段（`LayoutDocument::implied_final_para`）单独换。
pub(super) fn apply_android_defaults_to(para: &mut Para, default_face: bool) {
    para.runs.iter_mut().for_each(|run| android_default_font(&mut run.font, default_face));
    if para.space_after_is_fallback {
        para.space_after = ANDROID_DEFAULT_SPACE_AFTER;
    }
}
