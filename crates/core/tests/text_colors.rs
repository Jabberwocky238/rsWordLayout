//! Explicit RGB projection and source/paint invariants, not new Word captures.

use rsword::package::Package;
use rsword_layout_core::{
    Color, DrawCmd, Engine, Fragment, LayoutRecord, LoadedDocument, Page, PageSetup,
    ParagraphMarkProperties, Platform, SimpleMetrics, TextFragment, View, document_from_json,
    load_document, paint_document,
};
use serde_json::{Value, json};

const RED: Color = Color { r: 0xaa, g: 0x11, b: 0x22 };
const BLUE: Color = Color { r: 0x11, g: 0x22, b: 0xaa };
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn load(body: &str, styles: &str) -> LoadedDocument {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    package.replace_part_xml(package.main_part(), &format!(
        r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#,
    )).unwrap();
    let part = package.find_name("word/styles.xml").unwrap();
    package.replace_part_xml(part, &format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#)).unwrap();
    load_document(&package.save().unwrap()).unwrap()
}

fn native(color: Value) -> rsword_layout_core::LayoutDocument {
    document_from_json(&json!({"main": [{"kind": "text", "node": 1,
        "props": {"spacing": {"lineRule": "exact", "line": 480, "after": 0}},
        "inlines": [{"kind": "run", "text": "ab", "props": {
            "fonts": {"ascii": "Body"}, "size": 24, "color": color}}]}]}))
}

fn layout(document: &rsword_layout_core::LayoutDocument) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(document)
}

fn texts(pages: &[Page]) -> Vec<&TextFragment> {
    pages.iter().flat_map(|p| &p.fragments).filter_map(|f| match f {
        Fragment::Text(text) => Some(text),
        _ => None,
    }).collect()
}

fn at(pages: &[Page], cp: u32) -> &TextFragment {
    texts(pages).into_iter().find(|text| !text.text.is_empty()
        && text.source.is_some_and(|(start, end)| start <= cp && cp < end))
        .expect("visible source character")
}

fn lines(pages: &[Page]) -> Vec<(usize, u32, u32)> {
    let record = LayoutRecord::from_paint(&paint_document(pages, None, &[]));
    record.pages.iter().flat_map(|page| page.lines.iter().map(move |line| {
        let source = line.source.unwrap();
        (page.index, source.start, source.end)
    })).collect()
}

#[test]
fn native_six_digit_rgb_is_consumed_by_body_and_paint_commands() {
    for spelling in ["AA1122", "aa1122", "Aa1122"] {
        let document = native(json!({"val": spelling}));
        assert_eq!(document.paras[0].runs[0].color, RED);
        let pages = layout(&document);
        assert_eq!(at(&pages, 0).color, RED);
        let paint = paint_document(&pages, None, &[]);
        assert!(paint.pages.iter().flat_map(|page| &page.cmds).any(|cmd| {
            matches!(cmd, DrawCmd::DrawGlyphs { text, paint, .. }
                if text.starts_with("ab") && paint.color == RED)
        }));
    }
}

#[test]
fn invalid_and_unresolved_theme_colors_keep_body_black_fallback() {
    for color in [
        Value::Null, json!("AA1122"), json!({}), json!({"val": 112233}),
        json!({"val": "#AA1122"}), json!({"val": " AA1122 "}),
        json!({"val": "AA112"}), json!({"val": "GG1122"}),
        json!({"val": "AA1122FF"}), json!({"val": "auto"}),
        json!({"val": "AA1122", "themeColor": "accent1"}),
        json!({"val": "AA1122", "themeTint": "80"}),
        json!({"val": "AA1122", "themeShade": "80"}),
        json!({"val": "AA1122", "themeColor": null}),
    ] {
        assert_eq!(native(color.clone()).paras[0].runs[0].color, Color::BLACK, "{color}");
    }
}

#[test]
fn real_docx_colors_inherit_and_direct_auto_resets_inherited_rgb() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Leaf"/><w:rPr><w:color w:val="AA1122"/></w:rPr></w:pPr><w:r><w:t>A</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Character"/></w:rPr><w:t>B</w:t></w:r><w:r><w:rPr><w:color w:val="auto"/></w:rPr><w:t>C</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:color w:val="AA1122"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"><w:rPr><w:color w:val="1122AA"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Leaf"><w:basedOn w:val="Base"/></w:style><w:style w:type="character" w:styleId="Character"><w:rPr><w:color w:val="AA1122"/></w:rPr></w:style>"#,
    );
    let raw = loaded.json.clone();
    let document = loaded.layout_document();
    assert_eq!(document.source_warnings, Vec::<Value>::new());
    let colors: Vec<_> = document.paras[0].runs.iter().map(|run| run.color).collect();
    assert_eq!(colors, [BLUE, RED, Color::BLACK]);
    assert_eq!(loaded.paragraphs().0[0].runs[0].color, BLUE);
    let pages = layout(&document);
    assert_eq!(at(&pages, 3).color, RED);
    assert_eq!(loaded.json, raw, "projection does not rewrite declared JSON");
}

#[test]
fn effective_mark_color_is_independent_of_required_font_and_size_fields() {
    let mut document = native(json!({"val": "1122AA"}));
    for effective in [
        json!({"color": {"val": "AA1122"}}),
        json!({"fonts": {}, "size": 0, "position": "bad", "color": {"val": "AA1122"}}),
    ] {
        document.paras[0].mark = ParagraphMarkProperties::from_json(None, Some(&effective));
        let pages = layout(&document);
        assert_eq!(at(&pages, 0).color, BLUE);
        assert_eq!(at(&pages, 2).color, RED);
        assert_eq!(at(&pages, 2).font, document.paras[0].runs[0].font);
        assert_eq!(document.trace_metadata()["paragraphMarks"]["paragraphs"][0]["paintStyleAvailable"], false);
        assert_eq!(document.trace_metadata()["paragraphMarks"]["paragraphs"][0]["paintColorAvailable"], true);
    }
}

#[test]
fn mark_auto_uses_host_black_and_does_not_inherit_the_colored_tail() {
    let loaded = load(
        r#"<w:p><w:pPr><w:rPr><w:color w:val="auto"/></w:rPr></w:pPr><w:r><w:t>A</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:color w:val="AA1122"/></w:rPr></w:rPrDefault></w:docDefaults>"#,
    );
    let document = loaded.layout_document();
    assert_eq!(document.paras[0].mark.effective().unwrap()["color"]["val"], "auto");
    let pages = layout(&document);
    assert_eq!(at(&pages, 0).color, RED);
    assert_eq!(at(&pages, 1).color, Color::BLACK);
}

#[test]
fn unavailable_declared_only_and_unsupported_mark_colors_retain_legacy_color() {
    let mut document = native(json!({"val": "1122AA"}));
    for effective in [None, Some(Value::Null), Some(json!({})),
        Some(json!({"color": {"val": "bad"}})),
        Some(json!({"color": {"val": "AA1122", "themeColor": "accent1"}})),
        Some(json!({"color": {"val": "auto", "themeShade": "80"}})),
    ] {
        let declared = json!({"color": {"val": "AA1122"}});
        document.paras[0].mark = ParagraphMarkProperties::from_json(Some(&declared), effective.as_ref());
        assert_eq!(at(&layout(&document), 2).color, BLUE, "{effective:?}");
        assert_eq!(document.trace_metadata()["paragraphMarks"]["paragraphs"][0]["paintColorAvailable"], false);
    }
}

#[test]
fn empty_and_fully_hidden_body_keep_visible_marks_with_their_effective_color() {
    let loaded = load(
        r#"<w:p><w:pPr><w:rPr><w:color w:val="AA1122"/></w:rPr></w:pPr></w:p><w:p><w:pPr><w:rPr><w:color w:val="1122AA"/></w:rPr></w:pPr><w:r><w:rPr><w:vanish/><w:color w:val="AA1122"/></w:rPr><w:t>H</w:t></w:r></w:p>"#,
        "",
    );
    let document = loaded.layout_document();
    assert_eq!(document.paras[0].runs[0].color, RED);
    assert!(document.paras[1].runs[0].hidden);
    let pages = layout(&document);
    assert_eq!(at(&pages, 0).color, RED);
    assert_eq!(at(&pages, 2).color, BLUE);
    assert!(texts(&pages).iter().all(|text| !text.text.contains('H')));
    assert_eq!(lines(&pages), [(0, 0, 1), (0, 1, 3)]);
}

#[test]
fn mark_color_does_not_replace_manual_break_colors_or_change_flow() {
    for kind in ["textWrapping", "page", "column"] {
        let loaded = load(&format!(
            r#"<w:p><w:pPr><w:rPr><w:color w:val="AA1122"/></w:rPr></w:pPr><w:r><w:rPr><w:color w:val="1122AA"/></w:rPr><w:t>A</w:t><w:br w:type="{kind}"/></w:r></w:p>"#,
        ), "");
        let document = loaded.layout_document();
        let mut baseline = document.clone();
        baseline.paras[0].mark = ParagraphMarkProperties::default();
        for view in [View::Print, View::Mobile] {
            let engine = Engine::new(&SimpleMetrics, PageSetup::a4())
                .with_platform(Platform::Desktop, view);
            let before = engine.layout_document(&baseline);
            let after = engine.layout_document(&document);
            assert_eq!(at(&after, 0).color, BLUE);
            // Some hard controls retain a source-only fragment with no painted
            // replacement space. Color ownership does not imply a glyph count.
            let control = texts(&after).into_iter().find(|text| {
                text.source.is_some_and(|(start, end)| start <= 1 && 1 < end)
            }).expect("control source owner");
            assert_eq!(control.color, BLUE, "{kind}/{view:?}");
            assert_eq!(at(&after, 2).color, RED, "{kind}/{view:?}");
            assert_eq!(lines(&after), lines(&before));
            assert_eq!(after.iter().map(|p| &p.line_placements).collect::<Vec<_>>(),
                before.iter().map(|p| &p.line_placements).collect::<Vec<_>>());
        }
    }
}

#[test]
fn section_terminator_does_not_apply_paragraph_mark_color() {
    let mut document = native(json!({"val": "1122AA"}));
    document.paras[0].terminator = rsword_layout_core::LineTerminator::SectionBreak;
    document.paras[0].mark = ParagraphMarkProperties::from_json(None,
        Some(&json!({"color": {"val": "AA1122"}})));
    let pages = layout(&document);
    let tail = texts(&pages).into_iter().find(|text| text.source == Some((2, 3)))
        .expect("nonpainting section source owner");
    assert_eq!(tail.color, BLUE);
    assert_eq!(lines(&pages), [(0, 0, 3)]);
}
