//! `w:rFonts` 四槽选字体的性质测试。
//!
//! 核心断言：**槽规则与 fallback 不是一回事**。fallback 是「这个字体画不出这个字，
//! 换一个能画的」；Word 是「这个字符属于 CJK 区，所以用 eastAsia 槽指定的字体」，
//! 即使 ascii 槽的字体也能画出它。两者选出的字体常常不同。

use rsword_layout_core::{FontHint, FontSlots, FontSpec, SlotKind};

fn slots() -> FontSlots {
    FontSlots {
        ascii: Some("Times New Roman".into()),
        h_ansi: Some("Times New Roman".into()),
        east_asia: Some("SimSun".into()),
        cs: Some("Arial".into()),
        hint: FontHint::Default,
    }
}

#[test]
fn ascii_goes_to_ascii_slot() {
    assert_eq!(slots().slot_for('A'), SlotKind::Ascii);
    assert_eq!(slots().slot_for('0'), SlotKind::Ascii);
}

#[test]
fn cjk_goes_to_east_asia_slot() {
    for ch in ['中', '日', '本', 'あ', 'ア', '가'] {
        assert_eq!(slots().slot_for(ch), SlotKind::EastAsia, "{ch:?} 应走 eastAsia");
    }
}

#[test]
fn complex_scripts_go_to_cs_slot() {
    // 阿拉伯、希伯来、天城文、泰文。
    for ch in ['م', 'א', 'अ', 'ก'] {
        assert_eq!(slots().slot_for(ch), SlotKind::Cs, "{ch:?} 应走 cs");
    }
}

#[test]
fn high_latin_goes_to_hansi_slot() {
    // 带变音符的拉丁字母不属于 ASCII，走高 ANSI 槽。
    for ch in ['é', 'ü', 'ñ', 'Ω'] {
        assert_eq!(slots().slot_for(ch), SlotKind::HAnsi, "{ch:?} 应走 hAnsi");
    }
}

#[test]
fn hint_east_asia_overrides_ambiguous_chars() {
    // 这正是 Word 文档里破折号有时呈中文样式的原因：
    // hint="eastAsia" 把歧义字符划给中文字体。
    let mut s = slots();
    s.hint = FontHint::EastAsia;
    assert_eq!(s.slot_for('é'), SlotKind::EastAsia, "hint 应改变歧义区归属");
    // 而明确属于 CJK 的字符不受影响。
    assert_eq!(s.slot_for('中'), SlotKind::EastAsia);
}

#[test]
fn slot_selection_differs_from_single_family() {
    // 决定性的一条：同一个 FontSpec，中英文选出**不同**字体。
    // 若实现退化成「整段用一个 family」，这条必然失败。
    let font = FontSpec {
        slots: slots(),
        family: "Times New Roman".into(),
        size_half_points: 24,
        bold: false,
        italic: false,
        letter_spacing: 0,
        scale_pct: 100,
        kerning: false,
    };
    assert_eq!(font.family_for('A'), "Times New Roman");
    assert_eq!(font.family_for('中'), "SimSun");
    assert_eq!(font.family_for('م'), "Arial");
    assert_ne!(
        font.family_for('A'),
        font.family_for('中'),
        "中英文必须选出不同字体，否则槽规则没生效"
    );
}

#[test]
fn missing_slot_falls_back_to_family() {
    // 槽没写就退到 family；这不是 Word 的 fallback，只是「该槽未指定」。
    let font = FontSpec {
        slots: FontSlots { ascii: Some("Calibri".into()), ..FontSlots::default() },
        family: "Calibri".into(),
        size_half_points: 24,
        bold: false,
        italic: false,
        letter_spacing: 0,
        scale_pct: 100,
        kerning: false,
    };
    assert_eq!(font.family_for('中'), "Calibri", "eastAsia 未指定时退到 family");
}

#[test]
fn inherit_fills_only_missing_slots() {
    // 样式链与 docDefaults 靠它：已写的槽不被覆盖。
    let mut child = FontSlots { east_asia: Some("MS Mincho".into()), ..FontSlots::default() };
    child.inherit(&slots());
    assert_eq!(child.east_asia.as_deref(), Some("MS Mincho"), "自己写了就不继承");
    assert_eq!(child.ascii.as_deref(), Some("Times New Roman"), "没写的才继承");
}
