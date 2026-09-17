//! 序列化出口的检验。
//!
//! 这层只负责把 `LayoutRecord` 搬出进程，所以断言钉的是**口径**，不是布局对不对：
//! 单位换没换、缺失有没有变成 0、终止符名字与量具的约定表对不对得上。
//! 口径错了，比较器那边的差值全部没有意义，而且看不出来。

use rsword_layout_core::{
    GlyphRecord, LayoutRecord, LineRecord, LineTerminator, PageBreakPosition, PageRecord,
    SourceRange, TraceMeta, to_trace_json,
};

fn meta() -> TraceMeta {
    TraceMeta {
        engine: "test".into(),
        metrics: "test".into(),
        glyph_origin_method: "test".into(),
        source: "test.docx".into(),
        font_fingerprint: Some("abc123".into()),
    }
}

fn glyph(x: i32, y: i32) -> GlyphRecord {
    GlyphRecord {
        origin_x: x,
        origin_y: y,
        origin_x_pt: f64::from(x) / 20.0,
        origin_y_fine: i64::from(y) * rsword_layout_core::font::FINE_PER_TWIP,
        advance_x: 120,
        advance_x_pt: 6.0,
        advance_y: 0,
        face: "face".into(),
        glyph_id: 7,
        size_half_points: 24,
        source: Some(SourceRange::new(3, 4)),
    }
}

fn record_with(line: LineRecord) -> LayoutRecord {
    LayoutRecord {
        pages: vec![PageRecord {
            index: 0,
            width: 11906,
            height: 16838,
            lines: vec![line],
        }],
        unassigned_glyphs: 0,
    }
}

#[test]
fn twips_become_points() {
    // 契约写的是点，记录里是 twips。这一步换算错了，比较器会拿 20 倍的数去比 0 容差。
    let json = to_trace_json(&record_with(LineRecord {
        glyphs: vec![glyph(1440, 1660)],
        source: Some(SourceRange::new(0, 5)),
        terminator: LineTerminator::ParagraphMark,
        box_top: 0,
        box_height: 0,
    }), &meta());

    assert!(json.contains("\"unit\": \"pt\""));
    // 1440 twips = 72pt（1 英寸），1660 twips = 83pt，120 twips = 6pt。
    assert!(json.contains("\"origin\": [72.0, 83.0]"), "{json}");
    assert!(json.contains("\"advance\": [6.0, 0.0]"), "{json}");
    // 页面尺寸同样换算：11906 twips = 595.3pt。
    assert!(json.contains("\"width\": 595.300000"), "{json}");
}

#[test]
fn missing_source_stays_null_not_zero() {
    // 源区间缺失时必须是 null。写成 0 会让比较器把「不知道」读成「从第 0 个字符起」，
    // 那是编出来的读数，比报「判不了」糟得多。
    let json = to_trace_json(&record_with(LineRecord {
        glyphs: vec![GlyphRecord { source: None, ..glyph(0, 0) }],
        source: None,
        terminator: LineTerminator::Wrapped,
        box_top: 0,
        box_height: 0,
    }), &meta());

    assert!(json.contains("\"sourceStart\": null"), "{json}");
    assert!(json.contains("\"sourceEnd\": null"), "{json}");
    assert!(json.contains("\"sourceChar\": null"), "{json}");
    assert!(!json.contains("\"sourceChar\": 0"), "缺失被写成了 0：{json}");
}

#[test]
fn terminator_names_match_the_counting_table() {
    // 这些名字是量具 §4 约定表的键，两边对不上就没法按约定排除某类字形。
    let cases = [
        (LineTerminator::ParagraphMark, "PARAGRAPH_MARK", 1),
        (LineTerminator::LineBreak, "SOFT_RETURN", 1),
        (LineTerminator::SectionBreak, "SECTION_BREAK", 0),
        (LineTerminator::PageBreak(PageBreakPosition::OwnLine), "PAGE_BREAK_OWN_LINE", 0),
        (LineTerminator::PageBreak(PageBreakPosition::BeforeMark), "PAGE_BREAK_BEFORE_MARK", 1),
        (LineTerminator::PageBreak(PageBreakPosition::MidParagraph), "PAGE_BREAK_MID_PARAGRAPH", 0),
        (LineTerminator::Wrapped, "WRAP", 0),
    ];
    for (terminator, name, expected) in cases {
        let json = to_trace_json(&record_with(LineRecord {
            glyphs: vec![glyph(0, 0)],
            source: None,
            terminator,
            box_top: 0,
            box_height: 0,
        }), &meta());
        assert!(json.contains(&format!("\"terminator\": \"{name}\"")), "{terminator:?}: {json}");
        // 应产出几个字形也带出去——那是比较器排除该类字形时的分母。
        assert!(
            json.contains(&format!("\"terminatorExpectedGlyphs\": {expected}")),
            "{terminator:?} 应产出 {expected} 个：{json}"
        );
    }
}

#[test]
fn line_box_is_labelled_diagnostic() {
    // 行盒与行基线在现有通道上**不可测**，所以它们不能叫得像验收量。
    // 字段名带 Diagnostic 后缀，是为了让下游没法顺手拿它当判据。
    let json = to_trace_json(&record_with(LineRecord {
        glyphs: vec![glyph(0, 0)],
        source: None,
        terminator: LineTerminator::Wrapped,
        box_top: 100,
        box_height: 200,
    }), &meta());
    assert!(json.contains("\"boxTopDiagnostic\""), "{json}");
    assert!(json.contains("\"boxHeightDiagnostic\""), "{json}");
    assert!(!json.contains("\"baseline\""), "不该出现像验收量的 baseline 字段：{json}");
}

#[test]
fn empty_glyph_line_is_still_well_formed() {
    // 没接整形器时字形序列为空——JSON 仍必须合法，否则比较器连「字形数为 0」
    // 这个事实都读不到。
    let json = to_trace_json(&record_with(LineRecord {
        glyphs: Vec::new(),
        source: Some(SourceRange::new(0, 1)),
        terminator: LineTerminator::ParagraphMark,
        box_top: 0,
        box_height: 0,
    }), &meta());
    assert!(json.contains("\"glyphs\": []"), "{json}");
    assert!(json.trim_end().ends_with('}'));
}

#[test]
fn output_is_ascii_and_stable() {
    // 轨迹要能逐字节 diff：非 ASCII 一律转义，且同一输入给同一输出。
    let mut m = meta();
    m.metrics = "近似桩".into();
    let record = record_with(LineRecord {
        glyphs: vec![glyph(0, 0)],
        source: None,
        terminator: LineTerminator::Wrapped,
        box_top: 0,
        box_height: 0,
    });
    let a = to_trace_json(&record, &m);
    assert!(a.is_ascii(), "输出含非 ASCII，逐字节 diff 会受终端编码影响");
    assert_eq!(a, to_trace_json(&record, &m), "同一输入两次输出不同");
}

#[test]
fn unassigned_glyph_count_survives() {
    // 8.5% 的字形不进行划分是实测现象；这个分母丢了，验收范围就说不清。
    let json = to_trace_json(
        &LayoutRecord { pages: Vec::new(), unassigned_glyphs: 2988 },
        &meta(),
    );
    assert!(json.contains("\"unassignedGlyphs\": 2988"), "{json}");
}
