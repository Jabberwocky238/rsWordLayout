//! `w:caps` / `w:smallCaps`：桥接读取、度量、断行与源偏移（桩度量，不需要字体）。
//!
//! 钉的是**机制**，不是 Calibri 的几何：桩给西文一律 0.5 em，大写与小写同宽，
//! 所以 `w:caps` 在桩下看不出宽度变化，只有小型大写的缩小字号看得出来。
//! 真字体上的 Word 实测（`caps-on` 75 / 38，`smallcaps` 95 / 132 / 96）在 `caps_shaping.rs`。
//!
//! 大小写映射是一对一（`font::caps::upper_one_to_one`）：`ß` 不变。这是**假定**，
//! 只有手机 Word 导入的大写函数名做静态依据，`ß` 夹具待测。

use rsword_layout_core::{
    Caps, Color, Engine, FontMetrics, FontSpec, LayoutRecord, Margins, PageSetup, Para, Run,
    SimpleMetrics, Size, paint_document, paras_from_document,
};
use serde_json::json;

fn doc_with_run(props: serde_json::Value, text: &str) -> serde_json::Value {
    json!({"main": [{"kind": "text", "props": {},
        "inlines": [{"kind": "run", "text": text, "props": props}]}]})
}

fn run(text: &str, caps: Caps) -> Run {
    let mut font = FontSpec::new("Test", 24);
    font.caps = caps;
    Run {
        text: text.into(),
        font,
        color: Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

/// 版心宽 `width` twips 的页面。
fn setup(width: i32) -> PageSetup {
    PageSetup {
        size: Size::new(width + 1440, 16838),
        margins: Margins::new(720, 720, 720, 720),
    }
}

/// 各行的源区间（UTF-16，全篇偏移），与 `layout-trace` 的 `sourceStart/sourceEnd` 同一来源。
fn line_ranges(paras: &[Para], width: i32) -> Vec<(u32, u32)> {
    let pages = Engine::new(&SimpleMetrics, setup(width)).layout(paras);
    LayoutRecord::from_paint(&paint_document(&pages, None, &[]))
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| {
            let s = l.source.expect("每行都有源区间");
            (s.start, s.end)
        })
        .collect()
}

fn starts(text: &str, caps: Caps, width: i32) -> Vec<u32> {
    line_ranges(&[Para { runs: vec![run(text, caps)], ..Para::default() }], width)
        .iter()
        .map(|r| r.0)
        .collect()
}

#[test]
fn bridge_reads_caps_small_caps_and_explicit_off() {
    for (props, want) in [
        (json!({}), Caps::None),
        (json!({"caps": true}), Caps::All),
        // `<w:caps w:val="0"/>`：解析器给 false，必须关得掉。
        (json!({"caps": false}), Caps::None),
        (json!({"smallCaps": true}), Caps::Small),
        (json!({"smallCaps": false}), Caps::None),
        (json!({"caps": false, "smallCaps": true}), Caps::Small),
        // 两者互斥；同时出现时取全大写（与解析器兼容层同向，Word 未测）。
        (json!({"caps": true, "smallCaps": true}), Caps::All),
    ] {
        let (paras, _) = paras_from_document(&doc_with_run(props.clone(), "text"));
        assert_eq!(paras[0].runs[0].font.caps, want, "{props}");
        // 变换不改原文。
        assert_eq!(paras[0].runs[0].text, "text");
    }
}

/// 假定（一对一映射）：`ß` 在 `w:caps` 与 `w:smallCaps` 下都原样、全字号，不展开成 `SS`。
#[test]
fn assumed_sharp_s_keeps_one_full_size_glyph_under_caps_and_small_caps() {
    let m = SimpleMetrics;
    let plain = run("", Caps::None).font;
    let caps = run("", Caps::All).font;
    let small = run("", Caps::Small).font;
    // 12pt 桩：一个西文字符 0.5 em = 120 twips。展开成 `SS` 会是 240。
    assert_eq!(m.measure("ß", &plain).advance, 120);
    assert_eq!(m.measure("ß", &caps).advance, 120);
    assert_eq!(m.advance_pt("ß", &caps), 6.0);
    // 小型大写下 `ß` 没有一对一的大写，不缩小；`a` 缩到 9.5pt。
    assert_eq!(m.measure("ß", &small).advance, 120);
    assert_eq!(m.measure("a", &small).advance, 95);

    // 断行按显示宽度、源区间按原文：`ßß ` 在 caps 下仍是 120 + 120 + 60 = 300，
    // 版心 2400 放 8 个词、24 个源单位，与不变换相同。
    let text = "ßß ".repeat(10);
    let caps_lines =
        line_ranges(&[Para { runs: vec![run(&text, Caps::All)], ..Para::default() }], 2400);
    assert_eq!(caps_lines.iter().map(|r| r.0).collect::<Vec<_>>(), [0, 24]);
    assert_eq!(caps_lines.last().unwrap().1, 31, "末行含段落标记");
    assert_eq!(starts(&text, Caps::None, 2400), [0, 24]);
}

#[test]
fn caps_keep_letter_spacing_slots_on_source_clusters() {
    // 字符间距每个源字符簇一次（G3 的口径），大小写变换不改源字符，也就不改位置数。
    let m = SimpleMetrics;
    for caps in [Caps::None, Caps::All, Caps::Small] {
        let mut font = run("", caps).font;
        font.letter_spacing = 20;
        let base = {
            let mut f = font.clone();
            f.letter_spacing = 0;
            m.measure("aße\u{301}", &f).advance
        };
        // a、ß、e + U+0301：三个簇，三次间距。
        assert_eq!(m.measure("aße\u{301}", &font).advance, base + 60, "{caps:?}");
    }
}

#[test]
fn small_caps_shrink_only_characters_with_an_uppercase_form() {
    let m = SimpleMetrics;
    let small = run("", Caps::Small).font;
    // 12pt → 9.5pt（80% 向下取整到半点）：`a` 0.5 × 9.5pt = 95 twips。
    assert_eq!(m.measure("a", &small).advance, 95);
    // 大写字母、数字、空格、CJK 保持原字号。
    assert_eq!(m.measure("A", &small).advance, 120);
    assert_eq!(m.measure("1", &small).advance, 120);
    assert_eq!(m.measure(" ", &small).advance, 60);
    assert_eq!(m.measure("汉", &small).advance, 240);
    assert_eq!(m.measure("aA", &small).advance, 215);
    assert!((m.advance_pt("aA", &small) - 10.75).abs() < 1e-12);

    // 断点仍按原文找（空格后）。`aaaa ` 小型大写 4 × 95 + 60 = 440：版心 2000 放 4 个词；
    // 不变换时 4 × 120 + 60 = 540，只放 3 个。
    let text = "aaaa ".repeat(8);
    assert_eq!(starts(&text, Caps::Small, 2000), [0, 20]);
    assert_eq!(starts(&text, Caps::None, 2000), [0, 15, 30]);
}

#[test]
fn caps_off_run_keeps_original_widths_next_to_a_small_caps_run() {
    // 同一段里直接格式 smallCaps=false 的 run 不受相邻小型大写 run 影响。
    let doc = json!({"main": [{"kind": "text", "props": {}, "inlines": [
        {"kind": "run", "text": "aa", "props": {"smallCaps": true}},
        {"kind": "run", "text": "aa", "props": {"smallCaps": false}},
        {"kind": "run", "text": "aa", "props": {"caps": true}},
    ]}]});
    let (paras, _) = paras_from_document(&doc);
    let m = SimpleMetrics;
    let widths: Vec<i32> = paras[0]
        .runs
        .iter()
        .map(|r| m.measure(&r.text, &r.font).advance)
        .collect();
    // 桩里 `A` 与 `a` 同宽，全大写那个 run 看不出变化，只看得出没被缩小。
    assert_eq!(widths, [190, 240, 240]);
}

/// 依赖 G0（行首没有断点的长词在放得下的最后一个字符处硬断）：紧急断行量的也是显示宽度。
#[test]
fn unbreakable_small_caps_word_breaks_at_the_last_character_that_fits() {
    // `a` × 30：小型大写每个 95 twips，版心 1000 放 10 个（950）；不变换每个 120，放 8 个。
    let text = "a".repeat(30);
    assert_eq!(starts(&text, Caps::Small, 1000), [0, 10, 20]);
    assert_eq!(starts(&text, Caps::None, 1000), [0, 8, 16, 24]);
}

/// 假定：上标 + 小型大写，对上标缩出来的精确字号（12pt × 0.66 = 7.92pt）取 80% 再向下取整，得 6pt。
#[test]
fn assumed_small_caps_on_superscript_shrink_the_exact_size() {
    let (paras, _) = paras_from_document(&doc_with_run(
        json!({"smallCaps": true, "vertAlign": "superscript"}),
        "aA",
    ));
    let font = &paras[0].runs[0].font;
    assert_eq!(font.effective_size_centipoints(), 792);
    // `a`：0.5 em × 6pt = 60 twips；`A`：0.5 em × 7.92pt = 79.2 twips。
    assert_eq!(SimpleMetrics.measure("a", font).advance, 60);
    assert!((SimpleMetrics.advance_pt("aA", font) - (3.0 + 3.96)).abs() < 1e-12);
}
