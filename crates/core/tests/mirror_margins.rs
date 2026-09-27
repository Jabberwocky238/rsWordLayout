//! OOXML physical-page mirroring and synthetic flow invariants, not Word captures.

use rsword::package::{Package, PartUri};
use rsword_layout_core::{
    Engine, Fragment, LayoutDocument, LayoutRecord, Page, PageOverrides, PageSetup, Rect,
    SimpleMetrics, WrapContext, WrapRegion, document_from_json, load_document, paint_document,
};
use serde_json::{Value, json};

fn paragraph(text: &str) -> Value {
    json!({"kind": "text", "props": {
        "spacing": {"before": 0, "after": 0, "line": 480, "lineRule": "exact"},
        "widowControl": false,
    }, "inlines": [{"kind": "run", "text": text,
        "props": {"fonts": {"ascii": "Calibri", "hAnsi": "Calibri"}, "size": 24}}]})
}

fn section(start: usize, end: usize, kind: &str, capacity: i32) -> Value {
    json!({"blockRange": [start, end], "props": {
        "kind": kind, "pageSize": {"w": 3000, "h": capacity * 480 + 200},
        "pageMargins": {"top": 100, "right": 600, "bottom": 100, "left": 200, "gutter": 0},
    }})
}

fn input(paras: Vec<Value>, sections: Vec<Value>) -> Value {
    json!({"settings": {"mirrorMargins": true}, "main": paras, "sections": sections})
}

fn layout(value: &Value) -> Vec<Page> {
    Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document_from_json(value))
}

fn counts(pages: &[Page]) -> Vec<usize> {
    pages.iter().map(|page| page.line_columns.len()).collect()
}

fn sources(pages: &[Page]) -> Vec<(u32, u32)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .map(|line| {
            let source = line.source.unwrap();
            (source.start, source.end)
        })
        .collect()
}

fn first_x(page: &Page, line: u32) -> i32 {
    page.fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.line == line && !text.text.is_empty() => Some(text.x),
            _ => None,
        })
        .unwrap()
}

fn assert_x(pages: &[Page], expected: &[i32]) {
    assert_eq!(
        pages
            .iter()
            .map(|page| page.content_area.x)
            .collect::<Vec<_>>(),
        expected
    );
    for page in pages {
        assert_eq!(page.columns[0].x, page.content_area.x);
    }
}

#[test]
fn three_physical_pages_swap_declared_sides_without_mutating_template() {
    let value = input(vec![paragraph("x"); 5], vec![section(0, 5, "nextPage", 2)]);
    let document = document_from_json(&value);
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document);
    assert_eq!(counts(&pages), [2, 2, 1]);
    assert_x(&pages, &[200, 600, 200]);
    assert!(pages.iter().all(|page| page.content_area.width == 2200));
    assert_eq!(document.sections[0].setup.margins.left, 200);
    assert_eq!(document.sections[0].setup.margins.right, 600);
    assert_eq!(sources(&pages), [(0, 2), (2, 4), (4, 6), (6, 8), (8, 10)]);
    for page in &pages {
        assert_eq!(first_x(page, 0), page.content_area.x);
    }
}

#[test]
fn absent_false_and_invalid_inputs_remain_inactive_and_distinguishable() {
    for flag in [
        None,
        Some(json!(false)),
        Some(json!("true")),
        Some(json!(1)),
        Some(json!(null)),
    ] {
        let mut value = input(vec![paragraph("x"); 3], vec![section(0, 3, "nextPage", 1)]);
        if let Some(flag) = &flag {
            value["settings"]["mirrorMargins"] = flag.clone();
        } else {
            value["settings"]
                .as_object_mut()
                .unwrap()
                .remove("mirrorMargins");
        }
        let document = document_from_json(&value);
        assert_x(&layout(&value), &[200, 200, 200]);
        let invalid = flag.as_ref().is_some_and(|flag| !flag.is_boolean());
        assert_eq!(
            document
                .diagnostics
                .iter()
                .any(|note| note.contains("mirrorMargins requires a boolean")),
            invalid
        );
        assert_eq!(
            document.trace_metadata()["mirrorMargins"]["declared"],
            flag.filter(Value::is_boolean).unwrap_or(Value::Null)
        );
        assert_eq!(document.trace_metadata()["mirrorMargins"]["enabled"], false);
    }
}

#[test]
fn section_number_labels_and_restarts_do_not_control_physical_parity() {
    let mut sections = vec![
        section(0, 1, "nextPage", 2),
        section(1, 2, "nextPage", 2),
        section(2, 3, "nextPage", 2),
    ];
    for (section, start) in sections.iter_mut().zip([2, 1, 42]) {
        section["props"]["pageNumbers"] = json!({"start": start, "fmt": "decimal"});
    }
    let pages = layout(&input(vec![paragraph("x"); 3], sections));
    assert_eq!(counts(&pages), [1, 1, 1]);
    assert_x(&pages, &[200, 600, 200]);
}

#[test]
fn inserted_blank_and_following_page_have_their_own_physical_geometry() {
    for (first_count, kind, expected_counts, xs) in [
        (1, "oddPage", vec![1, 0, 1], vec![200, 600, 200]),
        (2, "evenPage", vec![1, 1, 0, 1], vec![200, 600, 200, 600]),
    ] {
        let value = input(
            vec![paragraph("x"); first_count + 1],
            vec![
                section(0, first_count, "nextPage", 1),
                section(first_count, first_count + 1, kind, 1),
            ],
        );
        let pages = layout(&value);
        assert_eq!(counts(&pages), expected_counts);
        assert_x(&pages, &xs);
        assert_eq!(sources(&pages).len(), first_count + 1);
        assert!(pages[pages.len() - 2].fragments.is_empty());
    }
}

#[test]
fn first_section_parity_retains_no_leading_blank_host_policy() {
    for kind in ["oddPage", "evenPage"] {
        let pages = layout(&input(
            vec![paragraph("x"); 2],
            vec![section(0, 2, kind, 1)],
        ));
        assert_eq!(counts(&pages), [1, 1]);
        assert_x(&pages, &[200, 600]);
    }
}

#[test]
fn next_page_section_reuses_the_page_opened_by_an_explicit_page_break() {
    let mut first = paragraph("A\u{fffc}");
    first["inlines"][0]["segments"] = json!([{"kind": {"kind": "br", "breakKind": "page"}}]);
    let pages = layout(&input(
        vec![first, paragraph("B")],
        vec![section(0, 1, "nextPage", 2), section(1, 2, "nextPage", 2)],
    ));
    assert_eq!(counts(&pages), [1, 1]);
    assert_x(&pages, &[200, 600]);
    assert_eq!(sources(&pages), [(0, 3), (3, 5)]);
}

#[test]
fn next_column_section_changes_parity_only_when_it_opens_a_physical_page() {
    let mut sections = vec![
        section(0, 1, "nextPage", 2),
        section(1, 2, "nextColumn", 2),
        section(2, 3, "nextColumn", 2),
    ];
    for s in &mut sections {
        s["props"]["columns"] = json!({"equalWidth": true, "num": 2, "space": 200});
    }
    let pages = layout(&input(vec![paragraph("x"); 3], sections));
    assert_eq!(counts(&pages), [2, 1]);
    assert_x(&pages, &[200, 600]);
    assert_eq!(pages[0].line_columns, [0, 1]);
    assert_eq!(pages[1].line_columns, [0]);
    assert_eq!(first_x(&pages[0], 1), 1400);
    assert_eq!(sources(&pages), [(0, 2), (2, 4), (4, 6)]);
}

#[test]
fn columns_advance_left_to_right_and_mirror_only_at_physical_page_boundaries() {
    let mut s = section(0, 9, "nextPage", 2);
    s["props"]["columns"] = json!({"equalWidth": false, "num": 2,
        "col": [{"w": 800, "space": 200}, {"w": 1200}]});
    let pages = layout(&input(vec![paragraph("x"); 9], vec![s]));
    assert_eq!(counts(&pages), [4, 4, 1]);
    assert_x(&pages, &[200, 600, 200]);
    assert_eq!(pages[0].line_columns, [0, 0, 1, 1]);
    assert_eq!(pages[1].line_columns, [0, 0, 1, 1]);
    for page in pages {
        assert_eq!(page.columns[0].width, 800);
        assert_eq!(page.columns[1].width, 1200);
        assert_eq!(page.columns[1].x, page.content_area.x + 1000);
    }
}

#[test]
fn even_page_continuous_group_balances_by_replay_in_the_resolved_body() {
    let mut two = section(1, 5, "nextPage", 4);
    two["props"]["columns"] = json!({"equalWidth": true, "num": 2, "space": 200});
    let pages = layout(&input(
        vec![paragraph("x"); 6],
        vec![
            section(0, 1, "nextPage", 4),
            two,
            section(5, 6, "continuous", 4),
        ],
    ));
    assert_eq!(counts(&pages), [1, 5]);
    assert_x(&pages, &[200, 600]);
    assert_eq!(pages[1].line_columns, [0, 0, 1, 1, 2]);
    assert_eq!(
        pages[1].columns,
        [
            Rect::new(600, 100, 1000, 960),
            Rect::new(1800, 100, 1000, 960),
            Rect::new(600, 1060, 2200, 960)
        ]
    );
    assert_eq!(
        sources(&pages),
        [(0, 2), (2, 4), (4, 6), (6, 8), (8, 10), (10, 12)]
    );
}

#[test]
fn changed_continuous_setup_keeps_actual_body_until_the_next_physical_page() {
    let mut changed = section(1, 5, "continuous", 2);
    changed["props"]["pageSize"]["w"] = json!(3400);
    changed["props"]["pageMargins"]["left"] = json!(300);
    changed["props"]["pageMargins"]["right"] = json!(900);
    let pages = layout(&input(
        vec![paragraph("x"); 5],
        vec![section(0, 1, "nextPage", 2), changed],
    ));
    assert_eq!(counts(&pages), [2, 2, 1]);
    assert_x(&pages, &[200, 900, 300]);
    assert_eq!(
        pages.iter().map(|page| page.size.width).collect::<Vec<_>>(),
        [3000, 3400, 3400]
    );
    assert_eq!(sources(&pages).last(), Some(&(8, 10)));
}

#[test]
fn keep_lines_reformats_against_absolute_wrap_on_the_mirrored_page() {
    let value = input(
        vec![paragraph("A"), paragraph(&"0".repeat(26))],
        vec![section(0, 2, "nextPage", 2)],
    );
    let mut document = document_from_json(&value);
    document.paras[1].keep_lines = true;
    let mut wrap = WrapContext::new();
    wrap.add(WrapRegion::rect(Rect::new(200, 100, 1000, 480), 0));
    let pages = Engine::with_wrap(&SimpleMetrics, PageSetup::a4(), wrap).layout_document(&document);
    assert_eq!(counts(&pages), [1, 2]);
    assert_x(&pages, &[200, 600]);
    assert_eq!(first_x(&pages[1], 0), 1200);
    assert_eq!(first_x(&pages[1], 1), 600);
    assert_eq!(sources(&pages), [(0, 2), (2, 15), (15, 29)]);
}

#[test]
fn gutter_at_top_is_ignored_when_mirroring_and_rtl_composition_is_diagnosed() {
    for rtl in [false, true] {
        let mut value = input(vec![paragraph("x"); 3], vec![section(0, 3, "nextPage", 2)]);
        value["settings"]["gutterAtTop"] = json!(true);
        value["sections"][0]["props"]["pageMargins"]["gutter"] = json!(80);
        value["sections"][0]["props"]["rtlGutter"] = json!(rtl);
        let document = document_from_json(&value);
        let pages = layout(&value);
        assert_eq!(counts(&pages), [2, 1]);
        assert_x(&pages, if rtl { &[200, 680] } else { &[280, 600] });
        assert!(
            pages
                .iter()
                .all(|page| page.content_area.y == 100 && page.content_area.width == 2120)
        );
        assert_eq!(
            document
                .diagnostics
                .iter()
                .any(|note| note.contains("mirrorMargins with rtlGutter")),
            rtl
        );
    }
}

#[test]
fn overrides_change_the_template_before_parity_and_rejected_overrides_are_atomic() {
    let value = input(vec![paragraph("x"); 3], vec![section(0, 3, "nextPage", 2)]);
    let mut document = document_from_json(&value);
    document
        .apply_page_overrides(PageOverrides {
            page_width: Some(3200),
            ..Default::default()
        })
        .unwrap();
    let engine = Engine::new(&SimpleMetrics, PageSetup::a4());
    assert_x(&engine.layout_document(&document), &[200, 600]);
    document
        .apply_page_overrides(PageOverrides {
            content_width: Some(2201),
            ..Default::default()
        })
        .unwrap();
    assert_x(&engine.layout_document(&document), &[499, 500]);
    let before = document.trace_metadata();
    assert!(
        document
            .apply_page_overrides(PageOverrides {
                content_width: Some(4000),
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(document.trace_metadata(), before);
    let mut symmetric = document_from_json(&value);
    symmetric
        .apply_page_overrides(PageOverrides {
            margin: Some(100),
            ..Default::default()
        })
        .unwrap();
    assert_x(&engine.layout_document(&symmetric), &[100, 100]);
}

#[test]
fn invalid_section_falls_back_without_losing_the_document_mirror_setting() {
    let mut para = paragraph("x");
    para["props"]["spacing"]["line"] = json!(10000);
    let mut value = input(vec![para; 3], vec![section(0, 3, "nextPage", 2)]);
    value["sections"][0]["props"]["pageMargins"]["left"] = json!(4000);
    let mut document = document_from_json(&value);
    assert_eq!(document.sections[0].setup, PageSetup::a4());
    assert_eq!(document.trace_metadata()["mirrorMargins"]["enabled"], true);
    assert!(
        document
            .diagnostics
            .iter()
            .any(|note| note.contains("invalid content geometry"))
    );
    document
        .apply_page_overrides(PageOverrides {
            content_width: Some(10001),
            ..Default::default()
        })
        .unwrap();
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document);
    assert_eq!(counts(&pages), [1, 1, 1]);
    assert_x(&pages, &[952, 953, 952]);
    assert!(pages.iter().all(|page| page.content_area.width == 10001));
    assert_eq!(sources(&pages), [(0, 2), (2, 4), (4, 6)]);
}

fn parsed(settings: &str) -> LayoutDocument {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let p = r#"<w:p><w:pPr><w:widowControl w:val="0"/><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>x</w:t></w:r></w:p>"#;
    let document = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}<w:sectPr><w:pgSz w:w="3000" w:h="1160"/><w:pgMar w:top="100" w:right="600" w:bottom="100" w:left="200" w:gutter="0"/><w:pgNumType w:start="2"/></w:sectPr></w:body></w:document>"#,
        p.repeat(3)
    );
    package
        .replace_part_xml(package.main_part(), &document)
        .unwrap();
    let content_type =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
    package
        .register_new_part(
            PartUri::from_entry_name("word/mirror.xml"),
            content_type,
            settings,
        )
        .unwrap();
    for (part, close, addition) in [
        ("[Content_Types].xml", "</Types>", format!(r#"<Override PartName="/word/mirror.xml" ContentType="{content_type}"/>"#)),
        ("word/_rels/document.xml.rels", "</Relationships>", r#"<Relationship Id="rIdMirror" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="mirror.xml"/>"#.into()),
    ] {
        let part = package.find_name(part).unwrap();
        let xml = String::from_utf8(package.read_bytes(part).unwrap()).unwrap();
        package.replace_part_xml(part, &xml.replace(close, &format!("{addition}{close}"))).unwrap();
    }
    load_document(&package.save().unwrap())
        .unwrap()
        .layout_document()
}

#[test]
fn actual_docx_onoff_and_namespace_prefixes_reach_physical_page_flow() {
    for prefix in ["w", "x"] {
        for (flag, expected) in [
            (None, None),
            (Some(""), Some(true)),
            (Some("true"), Some(true)),
            (Some("1"), Some(true)),
            (Some("on"), Some(true)),
            (Some("false"), Some(false)),
            (Some("0"), Some(false)),
            (Some("off"), Some(false)),
        ] {
            let child = flag
                .map(|flag| {
                    if flag.is_empty() {
                        format!("<{prefix}:mirrorMargins/>")
                    } else {
                        format!(r#"<{prefix}:mirrorMargins {prefix}:val="{flag}"/>"#)
                    }
                })
                .unwrap_or_default();
            let document = parsed(&format!(
                r#"<{prefix}:settings xmlns:{prefix}="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{child}</{prefix}:settings>"#
            ));
            assert!(
                document.source_warnings.is_empty(),
                "{prefix}, {flag:?}: {:?}",
                document.source_warnings
            );
            assert_eq!(
                document.trace_metadata()["mirrorMargins"]["declared"],
                json!(expected)
            );
            let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document);
            assert_eq!(counts(&pages), [2, 1]);
            assert_x(
                &pages,
                if expected == Some(true) {
                    &[200, 600]
                } else {
                    &[200, 200]
                },
            );
        }
    }
}
