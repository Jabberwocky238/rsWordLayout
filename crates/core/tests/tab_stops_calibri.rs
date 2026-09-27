//! Android Word 的制表符实测起点，用手机 Word 自带的 Calibri 复现（feature `fontenv`）。
//!
//! 行起点的期望值是 `word_analyse/reports/rsword-diff/tab.md` 与 `findings/question-set.md` Q28
//! 抄下来的 Word 行起点（窄路径 `w3=5329`、纸页路径 `w3=10466`），不是从引擎反推的。
//! 但其中只有 4 条独立验证了制表位模型（`tab-stop-720`、`tab-stop-1440`、`tab-right-1440`、
//! `tab-right-fit`）；另 5 条（`tab-zeros`、`tab-i`、`tab-after-a`、`tab-paper` 两个宽度）
//! 依赖缺省制表位，而那个值（221）正是拿这 5 条拟合出来的——样本内，不算验证。
//! `tabone`（`X<TAB>Y`）制表符宽到约 5000 都是一行，对制表位模型没有分辨力。
//!
//! 夹具只有一个 run（Calibri 12pt）、没有 settings.xml，这里按 rsword 解析器的 JSON 形状
//! 原样构造，所以默认制表位走的是「文档没写」、由引擎按平台补的那一支——平台必须是
//! Android（桌面照规范补 720）。窄路径同时是移动视图，与记分器的调用一致。
//!
//! Calibri 是专有字体，**不进仓库**。设 `RSWORD_TEST_CALIBRI=<calibri.ttf 路径>` 才跑：
//! 两条都是 `#[ignore]`，用 `--include-ignored` 跑；没设时不会悄悄算通过。

use rsword_layout_core::font::FontRegistry;
use rsword_layout_core::{
    Engine, LayoutRecord, Margins, PageSetup, Platform, RealMetrics, Size, TextShaper, Twips,
    View, paint_document, paras_from_document,
};

fn calibri() -> Vec<u8> {
    let path = std::env::var_os("RSWORD_TEST_CALIBRI")
        .expect("设 RSWORD_TEST_CALIBRI=<手机 Word 的 calibri.ttf>");
    std::fs::read(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"))
}

/// 一段 Calibri 12pt 文字，可选 `w:tabs`，形状同解析器输出。
fn document(text: &str, tabs: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"main": [{
        "kind": "text",
        "props": {"tabs": {"tab": tabs}},
        "inlines": [{"kind": "run", "text": text,
                     "props": {"fonts": {"ascii": "Calibri", "hAnsi": "Calibri"}, "size": 24}}]
    }]})
}

/// 与 `layout-trace --content-width` 同一口径：A4 页宽，左右边距平分余量。
fn setup(content_width: Twips) -> PageSetup {
    let width = 11906;
    let left = (width - content_width) / 2;
    PageSetup {
        size: Size::new(width, 16838),
        margins: Margins { top: 1440, right: width - content_width - left, bottom: 1440, left },
    }
}

fn record(registry: &FontRegistry, doc: &serde_json::Value, content_width: Twips) -> LayoutRecord {
    let (paras, _) = paras_from_document(doc);
    let metrics = RealMetrics::new(registry);
    let view = if content_width == 5329 { View::Mobile } else { View::Print };
    let pages = Engine::new(&metrics, setup(content_width))
        .with_platform(Platform::Android, view)
        .layout(&paras);
    let shaper: &dyn TextShaper = registry;
    LayoutRecord::from_paint(&paint_document(&pages, Some(shaper), &registry.face_ids()))
}

fn starts(record: &LayoutRecord) -> Vec<u32> {
    record
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .map(|l| l.source.expect("行带源区间").start)
        .collect()
}

#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI"]
fn android_word_tab_line_starts_are_reproduced() {
    let mut registry = FontRegistry::new();
    registry.add(calibri(), 0).expect("Calibri 装得进");

    let zeros = |n: usize| "0".repeat(n);
    let none = serde_json::json!([]);
    let left = |pos: i64| serde_json::json!([{"pos": pos, "val": "left"}]);
    let right = serde_json::json!([{"pos": 1440, "val": "right"}]);
    // (夹具, 文字, 制表位, 版心宽, Word 的行起点前缀)
    let cases: Vec<(&str, String, serde_json::Value, Twips, Vec<u32>)> = vec![
        ("tab-zeros", format!("\t{}", zeros(80)), none.clone(), 5329, vec![0, 42]),
        ("tab-i", format!("\t{}", "i".repeat(120)), none.clone(), 5329, vec![0, 93]),
        ("tab-after-a", format!("A\t{}", zeros(80)), none.clone(), 5329, vec![0, 1, 43]),
        ("tab-stop-720", format!("A\t{}", zeros(80)), left(720), 5329, vec![0, 1, 39]),
        ("tab-stop-1440", format!("A\t{}", zeros(80)), left(1440), 5329, vec![0, 1, 33, 76]),
        ("tab-paper@narrow", format!("\t{}", zeros(100)), none.clone(), 5329, vec![0, 42, 85]),
        ("tab-paper@paper", format!("\t{}", zeros(100)), none.clone(), 10466, vec![0, 85]),
        ("tab-right-1440", format!("A\t{}", zeros(80)), right.clone(), 5329, vec![0, 1, 45]),
        ("tab-right-fit", format!("A\t00\t{}", zeros(50)), right.clone(), 5329, vec![0, 4, 48]),
        ("tabone", "X\tY".to_string(), none.clone(), 5329, vec![0]),
    ];
    for (name, text, tabs, width, word) in cases {
        let got = starts(&record(&registry, &document(&text, tabs), width));
        assert_eq!(&got[..word.len().min(got.len())], &word[..], "{name}: 引擎 {got:?}");
        if name == "tabone" {
            assert_eq!(got.len(), 1, "tabone 是一行，Word dcp=4");
        }
    }
}

#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI"]
fn tab_is_painted_as_one_space_glyph_with_the_stop_width() {
    let mut registry = FontRegistry::new();
    registry.add(calibri(), 0).expect("Calibri 装得进");

    // `tab-right-fit` 第一行：`A`、右对齐 1440 的制表符、`00`。
    // 这一行的行界（到 4 为止）是 Word 的实测；`00` 收在 1440 上是**假设**——Q28 没覆盖
    // 行内落位，下面那条断言钉的是规范的右对齐，不是 Word 的读数。
    let doc = document(&format!("A\t00\t{}", "0".repeat(50)), serde_json::json!([
        {"pos": 1440, "val": "right"}
    ]));
    let rec = record(&registry, &doc, 5329);
    let line = &rec.pages[0].lines[0];
    assert_eq!(line.glyphs.len(), 4, "A、制表符、0、0：制表符画一个字形");
    let origin = line.glyphs[0].origin_x_pt;
    let tab = &line.glyphs[1];
    assert_eq!(tab.source.map(|s| (s.start, s.end)), Some((1, 2)));
    // 空格字形，不是 U+0009 映到的 glyph 0。
    assert_ne!(tab.glyph_id, 0);
    let end_of_zeros = line.glyphs[3].origin_x_pt + line.glyphs[3].advance_x_pt - origin;
    assert!((end_of_zeros * 20.0 - 1440.0).abs() < 1e-6, "假设 `00` 收在 1440：{end_of_zeros}pt");
    assert!(
        (line.glyphs[2].origin_x_pt - tab.origin_x_pt - tab.advance_x_pt).abs() < 1e-9,
        "制表符字形的推进量就是它占的宽度"
    );
}
