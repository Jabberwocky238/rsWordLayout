//! Independent mark inputs retain native declarations and Resolver results.
//! These tests establish projection and compatibility, not Word mark layout rules.

use rsword::package::{Package, PartUri};
use rsword_layout_core::{
    Engine, Fragment, LayoutDocument, LayoutRecord, LoadedDocument, PageSetup,
    ParagraphMarkProperties, Platform, SimpleMetrics, View, document_from_json, load_document,
    paint_document,
};
use serde_json::{Value, json};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn package(body: &str, styles: &str) -> Package {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let document = format!(
        r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    );
    package
        .replace_part_xml(package.main_part(), &document)
        .unwrap();
    let styles_part = package.find_name("word/styles.xml").unwrap();
    package
        .replace_part_xml(
            styles_part,
            &format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#),
        )
        .unwrap();
    package
}

fn load(body: &str, styles: &str) -> LoadedDocument {
    load_document(&package(body, styles).save().unwrap()).unwrap()
}

fn assert_projection_views_agree(loaded: &LoadedDocument) -> LayoutDocument {
    let before = loaded.json.clone();
    let (paras, _) = loaded.paragraphs();
    let document = loaded.layout_document();
    assert_eq!(paras.len(), document.paras.len());
    for (from_paragraphs, from_document) in paras.iter().zip(&document.paras) {
        assert_eq!(from_paragraphs.source_node, from_document.source_node);
        assert_eq!(from_paragraphs.mark, from_document.mark);
    }
    assert_eq!(
        loaded.json, before,
        "projection must not rewrite declared JSON"
    );
    document
}

fn text_block(node: u32, text: &str, mark: Option<Value>) -> Value {
    let mut props = json!({
        "spacing": {"lineRule": "exact", "line": 100, "before": 0, "after": 0},
    });
    if let Some(mark) = mark {
        props["rpr"] = mark;
    }
    json!({"kind": "text", "node": node, "props": props,
        "inlines": [{"kind": "run", "text": text,
            "props": {"fonts": {"ascii": "Body Font"}, "size": 24}}]})
}

#[test]
fn native_json_retains_absent_empty_null_and_malformed_mark_states_in_trace() {
    let states = [
        None,
        Some(json!({})),
        Some(Value::Null),
        Some(json!(false)),
        Some(json!(17)),
        Some(json!(["not", "properties"])),
        Some(json!("raw mark")),
        Some(json!({"bold": false, "position": 0, "size": {"raw": "bad"}})),
    ];
    let mut blocks = vec![json!({"kind": "table", "node": 999})];
    blocks.extend(
        states
            .iter()
            .enumerate()
            .map(|(index, state)| text_block(100 - index as u32, "A", state.clone())),
    );
    let input = json!({"main": blocks});
    let document = document_from_json(&input);
    assert_eq!(document.skipped_blocks, 1);
    assert_eq!(document.paras.len(), states.len());
    let trace = document.trace_metadata();
    assert!(trace["paragraphMarks"]["layoutPolicy"].is_string());
    for (index, (para, expected)) in document.paras.iter().zip(&states).enumerate() {
        assert_eq!(para.mark.declared(), expected.as_ref());
        assert_eq!(para.mark.effective(), None);
        let row = &trace["paragraphMarks"]["paragraphs"][index];
        assert_eq!(row["paragraph"], index);
        assert_eq!(row["sourceNode"], 100 - index);
        assert_eq!(row["declaredPresent"], expected.is_some());
        assert_eq!(
            row["declared"],
            expected.as_ref().unwrap_or(&Value::Null).clone()
        );
        assert_eq!(row["effectiveAvailable"], false);
        assert_eq!(row["effective"], Value::Null);
    }
}

#[test]
fn available_empty_or_null_resolution_remains_distinct_from_unavailable() {
    let mut document = document_from_json(&json!({"main": [
        text_block(3, "A", None), text_block(7, "B", None), text_block(9, "C", None),
    ]}));
    document.paras[1].mark = ParagraphMarkProperties::from_json(None, Some(&json!({})));
    document.paras[2].mark = ParagraphMarkProperties::from_json(None, Some(&Value::Null));
    let trace = document.trace_metadata();
    let rows = &trace["paragraphMarks"]["paragraphs"];
    assert_eq!(rows[0]["effectiveAvailable"], false);
    assert_eq!(rows[1]["effectiveAvailable"], true);
    assert_eq!(rows[1]["effective"], json!({}));
    assert_eq!(rows[2]["effectiveAvailable"], true);
    assert_eq!(rows[2]["effective"], Value::Null);
    assert_ne!(document.paras[0].mark, document.paras[2].mark);
    assert!(
        document
            .paras
            .iter()
            .all(|para| para.mark.declared().is_none())
    );
}

#[test]
fn nonempty_docx_mark_retains_its_own_font_color_and_position() {
    let loaded = load(
        r#"<w:p><w:pPr><w:rPr><w:rFonts w:ascii="Mark Font"/><w:color w:val="AA1122"/><w:position w:val="-3"/><w:sz w:val="48"/></w:rPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Body Font"/><w:position w:val="2"/><w:sz w:val="24"/></w:rPr><w:t>body</w:t></w:r></w:p>"#,
        "",
    );
    let document = assert_projection_views_agree(&loaded);
    let para = &document.paras[0];
    let direct = para.mark.declared().unwrap();
    assert_eq!(direct, &loaded.json["main"][0]["props"]["rpr"]);
    for props in [direct, para.mark.effective().unwrap()] {
        assert_eq!(props["fonts"]["ascii"], "Mark Font");
        assert_eq!(props["size"], 48);
        assert_eq!(props["position"], -3);
        assert_eq!(props["color"]["val"], "AA1122");
    }
    assert_eq!(para.runs.len(), 1);
    assert_eq!(para.runs[0].font.family, "Body Font");
    assert_eq!(para.runs[0].font.size_half_points, 24);
    assert_eq!(para.runs[0].effective_rise_fine(), 100);
}

#[test]
fn defaults_and_style_chains_resolve_without_becoming_direct_declarations() {
    let loaded = load(
        r#"<w:p><w:r><w:t>inherited</w:t></w:r></w:p><w:p><w:pPr><w:rPr><w:b w:val="0"/><w:i w:val="0"/><w:caps w:val="0"/><w:spacing w:val="0"/><w:position w:val="0"/></w:rPr></w:pPr><w:r><w:t>direct</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:hAnsi="Default Latin" w:cs="Default CS"/><w:spacing w:val="17"/><w:sz w:val="30"/></w:rPr></w:rPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:styleId="Base"><w:name w:val="Base"/><w:rPr><w:rFonts w:ascii="Base Mark"/><w:b/><w:position w:val="3"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:default="1" w:styleId="Leaf"><w:name w:val="Leaf"/><w:basedOn w:val="Base"/><w:rPr><w:i/><w:caps/></w:rPr></w:style>"#,
    );
    let document = assert_projection_views_agree(&loaded);
    assert!(document.paras[0].mark.declared().is_none());
    let inherited = document.paras[0].mark.effective().unwrap();
    assert_eq!(inherited["fonts"]["ascii"], "Base Mark");
    assert_eq!(inherited["fonts"]["cs"], "Default CS");
    assert_eq!(inherited["size"], 30);
    assert_eq!(inherited["bold"], true);
    assert_eq!(inherited["italic"], true);
    assert_eq!(inherited["caps"], true);
    assert_eq!(inherited["position"], 3);
    assert_eq!(inherited["spacing"], 17);
    let direct = document.paras[1].mark.declared().unwrap();
    assert!(direct.get("fonts").is_none());
    assert!(direct.get("size").is_none());
    let overridden = document.paras[1].mark.effective().unwrap();
    for key in ["bold", "italic", "caps"] {
        assert_eq!(direct[key], false);
        assert_eq!(overridden[key], false);
    }
    assert_eq!(overridden["position"], 0);
    assert_eq!(overridden["spacing"], 0);
    assert_eq!(overridden["fonts"]["ascii"], "Base Mark");
    assert_eq!(overridden["size"], 30);
    let trace = document.trace_metadata();
    assert_eq!(
        trace["paragraphMarks"]["paragraphs"][0]["declaredPresent"],
        false
    );
    assert_eq!(
        trace["paragraphMarks"]["paragraphs"][0]["effectiveAvailable"],
        true
    );
}

#[test]
fn docx_without_style_definitions_distinguishes_absent_and_empty_direct_mark() {
    let loaded = load(
        r#"<w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:pPr><w:rPr/></w:pPr><w:r><w:t>B</w:t></w:r></w:p>"#,
        "",
    );
    let document = assert_projection_views_agree(&loaded);
    assert_eq!(document.paras[0].mark.declared(), None);
    assert_eq!(document.paras[1].mark.declared(), Some(&json!({})));
    for para in &document.paras {
        let effective = para
            .mark
            .effective()
            .expect("resolver ran even without style definitions");
        assert!(effective.is_object());
        assert!(effective.get("size").is_none());
        assert!(effective["fonts"].get("ascii").is_none());
    }
    assert_eq!(
        document.paras[0].mark.effective(),
        document.paras[1].mark.effective()
    );
}

#[test]
fn mark_theme_references_and_resolved_slots_are_retained_separately() {
    let mut package = package(
        r#"<w:p><w:pPr><w:rPr><w:rFonts w:ascii="Ignored Literal" w:asciiTheme="minorHAnsi" w:hAnsiTheme="majorHAnsi" w:eastAsiaTheme="minorEastAsia"/><w:sz w:val="28"/></w:rPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Body Only"/><w:sz w:val="24"/></w:rPr><w:t>A</w:t></w:r></w:p>"#,
        "",
    );
    let theme_type = "application/vnd.openxmlformats-officedocument.theme+xml";
    let theme = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Test"><a:themeElements><a:fontScheme name="Test"><a:majorFont><a:latin typeface="Theme Major"/><a:ea typeface="Major CJK"/><a:cs typeface="Major CS"/></a:majorFont><a:minorFont><a:latin typeface="Theme Minor"/><a:ea typeface="Minor CJK"/><a:cs typeface="Minor CS"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;
    package
        .register_new_part(
            PartUri::from_entry_name("word/theme/theme1.xml"),
            theme_type,
            theme,
        )
        .unwrap();
    for (name, close, addition) in [
        ("[Content_Types].xml", "</Types>", format!(r#"<Override PartName="/word/theme/theme1.xml" ContentType="{theme_type}"/>"#)),
        ("word/_rels/document.xml.rels", "</Relationships>", r#"<Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>"#.into()),
    ] {
        let part = package.find_name(name).unwrap();
        let xml = String::from_utf8(package.read_bytes(part).unwrap()).unwrap();
        package.replace_part_xml(part, &xml.replace(close, &format!("{addition}{close}"))).unwrap();
    }
    let loaded = load_document(&package.save().unwrap()).unwrap();
    let document = assert_projection_views_agree(&loaded);
    let mark = &document.paras[0].mark;
    assert_eq!(
        mark.declared().unwrap()["fonts"]["ascii"],
        "Ignored Literal"
    );
    assert_eq!(
        mark.declared().unwrap()["fonts"]["asciiTheme"],
        "minorHAnsi"
    );
    let resolved = &mark.effective().unwrap()["fonts"];
    assert_eq!(resolved["ascii"], "Theme Minor");
    assert_eq!(resolved["hAnsi"], "Theme Major");
    assert_eq!(resolved["eastAsia"], "Minor CJK");
    assert_eq!(document.paras[0].runs[0].font.family, "Body Only");
}

#[test]
fn empty_nonempty_and_directly_hidden_paragraphs_keep_independent_mark_inputs() {
    let loaded = load(
        r#"<w:p><w:pPr><w:rPr><w:rFonts w:ascii="Empty Mark"/><w:position w:val="3"/><w:sz w:val="40"/></w:rPr></w:pPr></w:p>
        <w:p><w:pPr><w:rPr><w:rFonts w:ascii="Text Mark"/><w:sz w:val="52"/></w:rPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Text Body"/><w:sz w:val="20"/></w:rPr><w:t>A</w:t></w:r></w:p>
        <w:p><w:pPr><w:rPr><w:rFonts w:ascii="Hidden Mark"/><w:vanish w:val="0"/><w:sz w:val="64"/></w:rPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Hidden Body"/><w:vanish/><w:sz w:val="22"/></w:rPr><w:t>H</w:t></w:r></w:p>"#,
        "",
    );
    let document = assert_projection_views_agree(&loaded);
    for (para, family) in document
        .paras
        .iter()
        .zip(["Empty Mark", "Text Mark", "Hidden Mark"])
    {
        assert_eq!(para.mark.declared().unwrap()["fonts"]["ascii"], family);
        assert_eq!(para.mark.effective().unwrap()["fonts"]["ascii"], family);
    }
    let empty = &document.paras[0].runs[0];
    assert!(empty.text.is_empty());
    assert_eq!(empty.font.family, "Empty Mark");
    assert_eq!(empty.font.size_half_points, 40);
    assert_eq!(empty.effective_rise_fine(), 150);
    assert_eq!(document.paras[1].runs[0].font.family, "Text Body");
    assert_eq!(document.paras[2].runs[0].font.family, "Hidden Body");
    assert!(document.paras[2].runs[0].hidden);
    assert_eq!(document.paras[2].mark.effective().unwrap()["vanish"], false);
}

#[test]
fn recovered_hidden_paragraph_marks_use_original_resolver_and_current_nodes() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Hidden"/><w:rPr><w:rFonts w:ascii="Recovered Mark"/><w:sz w:val="40"/></w:rPr></w:pPr><w:r><w:t>&#x1F980;X</w:t></w:r></w:p><w:p><w:pPr><w:rPr><w:rFonts w:ascii="Following Mark"/><w:sz w:val="22"/></w:rPr></w:pPr><w:r><w:t>B</w:t></w:r></w:p>"#,
        r#"<w:style w:type="paragraph" w:styleId="Hidden"><w:name w:val="Hidden"/><w:rPr><w:vanish/></w:rPr></w:style>"#,
    );
    assert_eq!(loaded.json["main"][0]["protectedKind"]["kind"], "invisible");
    let document = assert_projection_views_agree(&loaded);
    assert_eq!(document.skipped_blocks, 0);
    assert_eq!(document.paras.len(), 2);
    let first = &document.paras[0];
    assert_eq!(
        first.mark.declared().unwrap()["fonts"]["ascii"],
        "Recovered Mark"
    );
    assert_eq!(first.mark.effective().unwrap()["vanish"], true);
    assert!(first.runs[0].hidden);
    assert_eq!(
        document.paras[1].mark.effective().unwrap()["fonts"]["ascii"],
        "Following Mark"
    );
    let trace = document.trace_metadata();
    for index in 0..2 {
        assert_eq!(
            trace["paragraphMarks"]["paragraphs"][index]["sourceNode"],
            loaded.json["main"][index]["node"]
        );
    }
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document);
    let b = pages
        .iter()
        .flat_map(|page| &page.fragments)
        .find_map(|fragment| match fragment {
            Fragment::Text(text) if text.text.starts_with('B') => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(b.source.unwrap(), (4, 5));
    let records = LayoutRecord::from_paint(&paint_document(&pages, None, &[]));
    let last = records.pages.last().unwrap().lines.last().unwrap();
    let source = last.source.unwrap();
    assert_eq!((source.start, source.end), (4, 6));
}

#[test]
fn sibling_rpr_repair_keeps_each_paragraph_mark_on_its_rebuilt_node() {
    let body = r#"<w:p><w:pPr><w:rPr><w:position w:val="1"/><w:sz w:val="32"/></w:rPr></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:rPr><w:sz w:val="20"/></w:rPr><w:t>A</w:t></w:r></w:p><w:p><w:pPr><w:rPr><w:position w:val="-2"/><w:sz w:val="48"/></w:rPr></w:pPr><w:r><w:t>B</w:t></w:r></w:p>"#;
    let loaded = load(body, "");
    assert_eq!(loaded.merged_run_props, 1);
    assert!(loaded.merge_error.is_none());
    let document = assert_projection_views_agree(&loaded);
    let already_repaired = load(&body.replace("<w:b/></w:rPr><w:rPr>", "<w:b/>"), "");
    assert_eq!(already_repaired.merged_run_props, 0);
    let repaired = already_repaired.layout_document();
    for (index, (size, position)) in [(32, 1), (48, -2)].into_iter().enumerate() {
        let para = &document.paras[index];
        assert_eq!(para.mark, repaired.paras[index].mark);
        assert_eq!(para.mark.declared().unwrap()["size"], size);
        assert_eq!(para.mark.effective().unwrap()["size"], size);
        assert_eq!(para.mark.effective().unwrap()["position"], position);
        assert_eq!(json!(para.source_node), loaded.json["main"][index]["node"]);
    }
    assert_ne!(document.paras[0].source_node, document.paras[1].source_node);
}

#[test]
fn independent_marks_preserve_body_placement_pagination_and_source_ranges() {
    let blocks: Vec<_> = (0..5)
        .map(|index| text_block(index + 10, "A", None))
        .collect();
    let input = json!({"main": blocks,
    "sections": [{"blockRange": [0, 5], "props": {
        "pageSize": {"w": 1200, "h": 250},
        "pageMargins": {"top": 0, "right": 0, "bottom": 0, "left": 0},
    }}]});
    let original = document_from_json(&input);
    let variants = [
        Value::Null,
        json!({}),
        json!({"fonts": {"ascii": "Very Tall Mark"}, "size": 200, "position": 90,
            "vanish": true, "color": {"val": "FF0000"}}),
    ];
    for (platform, view) in [
        (Platform::Desktop, View::Print),
        (Platform::Desktop, View::Mobile),
        (Platform::Android, View::Print),
        (Platform::Android, View::Mobile),
    ] {
        let engine = Engine::new(&SimpleMetrics, PageSetup::a4()).with_platform(platform, view);
        let before = engine.layout_document(&original);
        assert_eq!(before.len(), 3);
        let expected_paint = paint_document(&before, None, &[]);
        let records = LayoutRecord::from_paint(&expected_paint);
        let body_positions = |pages: &[rsword_layout_core::Page]| {
            pages.iter().enumerate().flat_map(|(page, p)| {
                p.fragments.iter().filter_map(move |fragment| match fragment {
                    Fragment::Text(t) if t.text.starts_with('A') => Some((
                        page, t.line, t.x, t.x_pt, t.baseline_fine,
                        t.font.clone(), t.color, t.source.map(|s| s.0),
                    )),
                    _ => None,
                })
            }).collect::<Vec<_>>()
        };
        let mut cursor = 0;
        for line in records.pages.iter().flat_map(|page| &page.lines) {
            let source = line.source.unwrap();
            assert_eq!(source.start, cursor);
            cursor = source.end;
        }
        assert_eq!(cursor, 10);
        for raw in &variants {
            let mut changed_input = input.clone();
            for block in changed_input["main"].as_array_mut().unwrap() {
                block["props"]["rpr"] = raw.clone();
            }
            let mut changed = document_from_json(&changed_input);
            for para in &mut changed.paras {
                para.mark = ParagraphMarkProperties::from_json(para.mark.declared(), Some(raw));
            }
            let after = engine.layout_document(&changed);
            assert_eq!(after.len(), before.len());
            let paint = paint_document(&after, None, &[]);
            assert_eq!(LayoutRecord::from_paint(&paint), records);
            assert_eq!(body_positions(&after), body_positions(&before));
            if raw.get("size").is_none() {
                assert_eq!(paint, expected_paint);
            }
        }
    }
}
