//! Paragraph autospace switches through public input and measurement APIs.
//! Enabled widths retain the existing quarter-em policy; these are synthetic
//! invariants, not new Word measurements or cross-run/glyph-spacing claims.

use rsword::package::Package;
use rsword_layout_core::{
    Engine, FontMetrics, Fragment, LayoutRecord, LoadedDocument, Margins, PageSetup, Para,
    SimpleMetrics, Size, load_document, paint_document, paras_from_document,
};
use serde_json::{Value, json};

const MIXED: &str = "\u{6c49}0\u{6c49}";

fn raw_para(props: Value, text: &str) -> Para {
    let mut props = props;
    props["spacing"] = json!({"lineRule": "exact", "line": 240, "before": 0, "after": 0});
    let doc = json!({"main": [{"kind": "text", "props": props, "inlines": [
        {"kind": "run", "text": text, "props": {"size": 24}}
    ]}]});
    paras_from_document(&doc).0.remove(0)
}

fn loaded(body: &str, styles: &str) -> LoadedDocument {
    let bytes = rsword::save::blank_docx(None).unwrap();
    let mut package = Package::open(&bytes).unwrap();
    let ns = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    package
        .replace_part_xml(
            package.main_part(),
            &format!(r#"<w:document xmlns:w="{ns}"><w:body>{body}</w:body></w:document>"#),
        )
        .unwrap();
    let part = package.find_name("word/styles.xml").unwrap();
    package
        .replace_part_xml(
            part,
            &format!(r#"<w:styles xmlns:w="{ns}">{styles}</w:styles>"#),
        )
        .unwrap();
    let doc = load_document(&package.save().unwrap()).unwrap();
    assert!(doc.layout_document().source_warnings.is_empty());
    doc
}

fn xml_para(props: &str, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr>{props}<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#
    )
}

fn widths(paras: &[Para]) -> Vec<i32> {
    paras
        .iter()
        .map(|p| SimpleMetrics.measure(MIXED, &p.runs[0].font).advance)
        .collect()
}

fn layout(paras: &[Para], width: i32, height: i32) -> LayoutRecord {
    let setup = PageSetup {
        size: Size::new(width, height),
        margins: Margins::uniform(0),
    };
    let pages = Engine::new(&SimpleMetrics, setup).layout(paras);
    LayoutRecord::from_paint(&paint_document(&pages, None, &[]))
}

#[test]
fn native_false_changes_wrapping_and_pagination_without_changing_source() {
    // At 12pt, the built-in synthetic widths are 240 + 120 twips.
    // Disabling autospace fits exactly; the retained enabled policy adds 60.
    let text = "\u{6c49}0";
    let off = raw_para(json!({"autoSpaceDn": false}), text);
    let on = raw_para(json!({"autoSpaceDn": true}), text);
    let off = layout(&[off], 360, 240);
    let on = layout(&[on], 360, 240);
    assert_eq!(off.pages.len(), 1);
    assert_eq!(on.pages.len(), 2);
    let ranges = |record: &LayoutRecord| {
        record
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .map(|l| {
                let s = l.source.unwrap();
                (s.start, s.end)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(ranges(&off), [(0, 3)]);
    assert_eq!(ranges(&on), [(0, 1), (1, 3)]);
}

#[test]
fn docx_onoff_and_sparse_style_inheritance_reach_measurement() {
    let styles = r#"<w:docDefaults><w:pPrDefault><w:pPr><w:autoSpaceDN w:val="0"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"><w:name w:val="Base"/><w:pPr><w:autoSpaceDN/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Child"><w:name w:val="Child"/><w:basedOn w:val="Base"/></w:style>"#;
    let body = [
        "",
        r#"<w:pStyle w:val="Child"/>"#,
        r#"<w:pStyle w:val="Child"/><w:autoSpaceDN w:val="false"/>"#,
        r#"<w:autoSpaceDN w:val="true"/>"#,
        r#"<w:autoSpaceDN w:val="off"/>"#,
        r#"<w:autoSpaceDN w:val="on"/>"#,
    ]
    .map(|p| xml_para(p, MIXED))
    .join("");
    let doc = loaded(&body, styles);
    assert_eq!(doc.json["main"][2]["props"]["autoSpaceDn"], false);
    assert_eq!(widths(&doc.paragraphs().0), [600, 720, 600, 720, 600, 720]);
    assert_eq!(
        widths(&doc.layout_document().paras),
        [600, 720, 600, 720, 600, 720]
    );
}

#[test]
fn missing_default_and_neighboring_paragraphs_do_not_leak_the_switch() {
    let body = [
        r#"<w:autoSpaceDN w:val="0"/>"#,
        "",
        r#"<w:autoSpaceDN/>"#,
        r#"<w:autoSpaceDN w:val="false"/>"#,
    ]
    .map(|p| xml_para(p, MIXED))
    .join("");
    assert_eq!(
        widths(&loaded(&body, "").paragraphs().0),
        [600, 720, 720, 600]
    );
    assert_eq!(
        SimpleMetrics
            .measure(MIXED, &rsword_layout_core::FontSpec::new("synthetic", 24))
            .advance,
        720
    );
}

#[test]
fn empty_paragraphs_and_all_runs_receive_the_paragraph_setting() {
    let body = format!(
        r#"<w:p><w:pPr><w:autoSpaceDN w:val="false"/></w:pPr></w:p>{}"#,
        xml_para(r#"<w:autoSpaceDN w:val="false"/>"#, MIXED).replace(
            "</w:p>",
            r#"<w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>tail</w:t></w:r></w:p>"#
        )
    );
    let paras = loaded(&body, "").paragraphs().0;
    assert_eq!(paras[0].runs.len(), 1);
    assert!(paras[0].runs[0].text.is_empty());
    for para in paras {
        for run in para.runs {
            // The synthesized empty run may use a host size; remove that
            // variable by comparing independent single-character requests.
            let plain = ["\u{6c49}", "0", "\u{6c49}"]
                .iter()
                .map(|t| SimpleMetrics.measure(t, &run.font).advance)
                .sum::<i32>();
            assert_eq!(SimpleMetrics.measure(MIXED, &run.font).advance, plain);
        }
    }
}

#[test]
fn native_key_is_independent_of_xml_spelling_de_and_run_properties() {
    for props in [
        json!({}),
        json!({"autoSpaceDN": false}),
        json!({"autoSpaceDe": false}),
        json!({"autoSpace": false}),
    ] {
        assert_eq!(widths(&[raw_para(props, MIXED)]), [720]);
    }
    assert_eq!(
        widths(&[raw_para(
            json!({"autoSpaceDn": false, "autoSpaceDe": true}),
            MIXED
        )]),
        [600]
    );
    let doc = json!({"main": [{"kind": "text", "props": {}, "inlines": [
        {"kind": "run", "text": MIXED, "props": {"size": 24, "autoSpaceDn": false}}
    ]}]});
    assert_eq!(widths(&paras_from_document(&doc).0), [720]);
}

#[test]
fn ordinary_and_emergency_fitting_use_the_disabled_width() {
    let off = raw_para(json!({"autoSpaceDn": false}), MIXED);
    let font = &off.runs[0].font;
    let fitted = SimpleMetrics.fit(MIXED, font, 600).unwrap();
    assert_eq!((fitted.0, fitted.1.advance), (MIXED.len(), 600));
    let fitted = SimpleMetrics.fit_clusters(MIXED, font, 600);
    assert_eq!((fitted.0, fitted.1.advance), (MIXED.len(), 600));
    assert_eq!(SimpleMetrics.advance_pt(MIXED, font), 30.0);
}

#[test]
fn disabling_autospace_keeps_character_spacing_and_horizontal_scale() {
    let doc = json!({"main": [{"kind": "text", "props": {"autoSpaceDn": false}, "inlines": [
        {"kind": "run", "text": MIXED, "props": {"size": 24, "spacing": 20, "scale": 200}}
    ]}]});
    let p = paras_from_document(&doc).0.remove(0);
    assert_eq!(SimpleMetrics.measure(MIXED, &p.runs[0].font).advance, 1260);
    assert_eq!(SimpleMetrics.advance_pt(MIXED, &p.runs[0].font), 63.0);
}

#[test]
fn aligned_tabs_measure_the_following_text_with_the_paragraph_switch() {
    let setup = PageSetup {
        size: Size::new(3000, 1000),
        margins: Margins::uniform(0),
    };
    for (align, off_x, on_x) in [
        ("right", 840, 780),
        ("center", 1140, 1110),
        ("decimal", 1080, 1020),
    ] {
        for (enabled, expected) in [(false, off_x), (true, on_x)] {
            let p = raw_para(
                json!({"autoSpaceDn": enabled,
                "tabs": {"tab": [{"pos": 1440, "val": align}]}}),
                "A\t\u{6c49}0.5",
            );
            let pages = Engine::new(&SimpleMetrics, setup).layout(&[p]);
            let text = pages[0]
                .fragments
                .iter()
                .find_map(|f| match f {
                    Fragment::Text(t) if t.text.starts_with('\u{6c49}') => Some(t),
                    _ => None,
                })
                .unwrap();
            assert_eq!(text.x, expected, "{align} enabled={enabled}");
            assert_eq!(text.x_pt, f64::from(expected) / 20.0);
        }
    }
}

#[cfg(feature = "fontenv")]
#[test]
fn real_metrics_disabled_width_matches_unadjusted_shaped_glyph_total() {
    use rsword_layout_core::{RealMetrics, TextShaper, font::FontRegistry};
    let mut registry = FontRegistry::new();
    registry
        .add(
            include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf").to_vec(),
            0,
        )
        .unwrap();
    registry
        .add(
            include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf").to_vec(),
            0,
        )
        .unwrap();
    let metrics = RealMetrics::new(&registry);
    let mut off = raw_para(json!({"autoSpaceDn": false}), MIXED)
        .runs
        .remove(0)
        .font;
    off.family = "Liberation Sans".into();
    off.slots.ascii = Some("Liberation Sans".into());
    off.slots.east_asia = Some("Droid Sans Fallback".into());
    let glyphs = registry.shape(MIXED, &off);
    assert_eq!(glyphs.len(), 3);
    let twips: i32 = glyphs.iter().map(|g| g.x_advance).sum();
    let points: f64 = glyphs.iter().map(|g| g.x_advance_pt).sum();
    assert_eq!(metrics.measure(MIXED, &off).advance, twips);
    assert!((metrics.advance_pt(MIXED, &off) - points).abs() < 1e-9);
    let mut on = raw_para(json!({}), MIXED).runs.remove(0).font;
    on.family.clone_from(&off.family);
    on.slots.clone_from(&off.slots);
    assert_eq!(
        metrics.measure(MIXED, &on).advance - metrics.measure(MIXED, &off).advance,
        120
    );
    assert!((metrics.advance_pt(MIXED, &on) - metrics.advance_pt(MIXED, &off) - 6.0).abs() < 1e-9);
}

#[test]
fn default_paragraph_style_is_inherited_and_direct_true_reenables_spacing() {
    let styles = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:autoSpaceDN w:val="false"/></w:pPr></w:style>"#;
    let body = format!(
        "{}{}",
        xml_para("", MIXED),
        xml_para("<w:autoSpaceDN/>", MIXED)
    );
    assert_eq!(widths(&loaded(&body, styles).paragraphs().0), [600, 720]);
}

#[test]
fn custom_metric_provider_receives_the_request_and_keeps_its_own_widths() {
    use rsword_layout_core::{BreakOpportunity, FontSpec, TextMetrics};
    use std::cell::RefCell;

    struct IndependentMetrics(RefCell<Vec<bool>>);
    impl FontMetrics for IndependentMetrics {
        fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
            self.0.borrow_mut().push(font.auto_space_dn);
            TextMetrics {
                advance: text.chars().count() as i32 * 100,
                ascent: 80,
                descent: 20,
                line_gap: 0,
            }
        }

        fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
            SimpleMetrics.break_opportunities(text)
        }
    }

    let metrics = IndependentMetrics(RefCell::new(Vec::new()));
    let setup = PageSetup {
        size: Size::new(200, 240),
        margins: Margins::uniform(0),
    };
    let engine = Engine::new(&metrics, setup);
    // This provider defines a 100-twip advance per character, independently
    // of the built-in quarter-em policy. Neither layout nor fitting may
    // subtract a presumed autospace amount from its returned widths.
    for enabled in [false, true, false] {
        let para = raw_para(
            json!({"autoSpaceDn": enabled}),
            "\u{6c49}0\u{6c49}0\u{6c49}0",
        );
        let font = &para.runs[0].font;
        assert_eq!(metrics.fit(MIXED, font, 200).unwrap().1.advance, 200);
        assert_eq!(metrics.fit_clusters(MIXED, font, 200).1.advance, 200);
        assert_eq!(metrics.advance_pt(MIXED, font), 15.0);
        let pages = engine.layout(&[para]);
        let record = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
        let sources = record
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .map(|l| {
                let source = l.source.unwrap();
                (source.start, source.end)
            })
            .collect::<Vec<_>>();
        assert_eq!(sources, [(0, 2), (2, 4), (4, 7)]);
        assert_eq!(pages.len(), 3);
        assert!(!metrics.0.borrow().is_empty());
        assert!(metrics.0.borrow().iter().all(|&request| request == enabled));
        metrics.0.borrow_mut().clear();

        let tab = raw_para(
            json!({"autoSpaceDn": enabled,
            "tabs": {"tab": [{"pos": 1440, "val": "right"}]}}),
            "A\t\u{6c49}0.5",
        );
        let setup = PageSetup {
            size: Size::new(2000, 240),
            margins: Margins::uniform(0),
        };
        let pages = Engine::new(&metrics, setup).layout(&[tab]);
        let x = pages[0]
            .fragments
            .iter()
            .find_map(|f| match f {
                Fragment::Text(t) if t.text.starts_with('\u{6c49}') => Some(t.x),
                _ => None,
            })
            .unwrap();
        assert_eq!(x, 1040);
        assert!(!metrics.0.borrow().is_empty());
        assert!(metrics.0.borrow().iter().all(|&request| request == enabled));
        metrics.0.borrow_mut().clear();
    }
}
