//! Grid input and Resolver regressions, without claiming a Word grid metric model.

use rsword::package::Package;
use rsword_layout_core::{
    DocumentGrid, Engine, GridKind, LayoutDocument, LoadedDocument, PageOverrides, PageSetup,
    Platform, SimpleMetrics, View, document_from_json, load_document,
};
use serde_json::{Value, json};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn paragraph(props: Value) -> Value {
    json!({"kind": "text", "props": props,
        "inlines": [{"kind": "run", "text": "Grid input", "props": {}}]})
}

fn input(grid: Option<Value>) -> Value {
    let mut props = json!({});
    if let Some(grid) = grid {
        props["docGrid"] = grid;
    }
    json!({"main": [paragraph(json!({}))],
        "sections": [{"blockRange": [0, 1], "props": props}]})
}

fn document(grid: Value) -> LayoutDocument {
    document_from_json(&input(Some(grid)))
}

fn has_note(document: &LayoutDocument, text: &str) -> bool {
    document.diagnostics.iter().any(|note| note.contains(text))
}

fn load(body: &str, section: &str, styles: &str) -> LoadedDocument {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    package.replace_part_xml(package.main_part(), &format!(
        r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr>{section}</w:sectPr></w:body></w:document>"#
    )).unwrap();
    let styles_part = package.find_name("word/styles.xml").unwrap();
    package
        .replace_part_xml(
            styles_part,
            &format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#),
        )
        .unwrap();
    load_document(&package.save().unwrap()).unwrap()
}

#[test]
fn absent_empty_default_and_malformed_declarations_remain_distinct() {
    let absent = document_from_json(&input(None));
    assert_eq!(absent.sections[0].grid, DocumentGrid::default());
    assert_eq!(absent.sections[0].grid.kind().unwrap(), None);
    assert!(absent.diagnostics.is_empty());
    assert!(absent.source_warnings.is_empty());
    assert_eq!(absent.trace_metadata()["sourceWarnings"], json!([]));
    assert_eq!(
        absent.trace_metadata()["sections"][0]["grid"]["present"],
        false
    );
    let empty = document(json!({}));
    assert_eq!(empty.sections[0].grid.declared(), Some(&json!({})));
    assert_eq!(empty.sections[0].grid.kind().unwrap(), None);
    assert!(has_note(&empty, "default behavior is unresolved"));
    let default = document(json!({"kind": "default"}));
    assert_eq!(
        default.sections[0].grid.kind().unwrap(),
        Some(GridKind::Default)
    );
    assert_eq!(default.sections[0].grid.line_pitch().unwrap(), None);
    for raw in [
        Value::Null,
        json!(true),
        json!(9),
        json!([]),
        json!("lines"),
    ] {
        let malformed = document(raw.clone());
        assert_eq!(malformed.sections[0].grid.declared(), Some(&raw));
        assert!(malformed.sections[0].grid.kind().is_err());
        assert!(has_note(&malformed, "docGrid must be an object"));
        let meta = malformed.trace_metadata();
        assert_eq!(meta["sections"][0]["grid"]["present"], true);
        assert_eq!(meta["sections"][0]["grid"]["declared"], raw);
        assert_eq!(meta["sections"][0]["grid"]["kind"]["state"], "invalid");
    }
}

#[test]
fn parser_kinds_are_retained_without_coercing_character_grids_to_lines() {
    for (name, kind) in [
        ("default", GridKind::Default),
        ("lines", GridKind::Lines),
        ("linesAndChars", GridKind::LinesAndChars),
        ("snapToChars", GridKind::SnapToChars),
    ] {
        let doc = document(json!({"kind": name, "linePitch": 297}));
        let grid = &doc.sections[0].grid;
        assert_eq!(grid.kind().unwrap(), Some(kind));
        assert_eq!(grid.line_pitch().unwrap(), Some(297));
        assert!(has_note(&doc, "input retained but not applied"));
        assert_eq!(
            has_note(&doc, "character-grid kind is unsupported"),
            matches!(kind, GridKind::LinesAndChars | GridKind::SnapToChars)
        );
        let meta = doc.trace_metadata();
        assert_eq!(meta["sections"][0]["grid"]["kind"]["value"], name);
        assert_eq!(meta["sections"][0]["grid"]["linePitchTwips"]["value"], 297);
        assert_eq!(meta["sections"][0]["grid"]["applied"], false);
    }
}

#[test]
fn invalid_kind_is_an_error_and_raw_value_survives() {
    for kind in [
        json!({"raw": "chars"}),
        json!("chars"),
        json!(false),
        json!(0),
        Value::Null,
    ] {
        let raw = json!({"kind": kind, "linePitch": 297});
        let doc = document(raw.clone());
        assert!(doc.sections[0].grid.kind().is_err());
        assert!(has_note(&doc, "docGrid.kind"));
        assert_eq!(doc.trace_metadata()["sections"][0]["grid"]["declared"], raw);
    }
    let wrong_key = document(json!({"type": "lines", "linePitch": 297}));
    assert_eq!(wrong_key.sections[0].grid.kind().unwrap(), None);
    assert!(has_note(&wrong_key, "default behavior is unresolved"));
}

#[test]
fn pitch_validation_retains_zero_negative_missing_and_invalid_inputs() {
    for kind in ["lines", "linesAndChars"] {
        assert!(has_note(
            &document(json!({"kind": kind})),
            "linePitch absent"
        ));
    }
    for pitch in [0, -1, i32::MIN] {
        let doc = document(json!({"kind": "lines", "linePitch": pitch}));
        assert_eq!(doc.sections[0].grid.line_pitch().unwrap(), Some(pitch));
        assert!(has_note(&doc, "is nonpositive"));
    }
    for pitch in [
        json!({"raw": "oops"}),
        json!("297"),
        json!(true),
        json!(1.5),
        json!(2147483648_i64),
        json!(-2147483649_i64),
        Value::Null,
    ] {
        let doc = document(json!({"kind": "lines", "linePitch": pitch}));
        assert!(doc.sections[0].grid.line_pitch().is_err());
        assert!(has_note(&doc, "requires a signed 32-bit integer"));
        assert_eq!(
            doc.trace_metadata()["sections"][0]["grid"]["declared"]["linePitch"],
            pitch
        );
    }
    let max = document(json!({"kind": "lines", "linePitch": i32::MAX}));
    assert_eq!(max.sections[0].grid.line_pitch().unwrap(), Some(i32::MAX));
}

#[test]
fn char_space_keeps_signed_raw_integer_without_twip_conversion() {
    for space in [i32::MIN, -17, 0, 12345, i32::MAX] {
        let doc = document(json!({"charSpace": space}));
        assert_eq!(doc.sections[0].grid.char_space().unwrap(), Some(space));
        assert!(has_note(&doc, "character-grid spacing is unsupported"));
        assert_eq!(
            doc.trace_metadata()["sections"][0]["grid"]["charSpaceRaw"]["value"],
            space
        );
    }
    for space in [
        json!({"raw": "2147483648"}),
        json!(2147483648_i64),
        json!(true),
        json!("0"),
    ] {
        let doc = document(json!({"charSpace": space}));
        assert!(doc.sections[0].grid.char_space().is_err());
        assert!(has_note(&doc, "docGrid.charSpace requires"));
        assert_eq!(doc.sections[0].grid.declared().unwrap()["charSpace"], space);
    }
}

#[test]
fn section_ranges_keep_their_own_grid_across_unsupported_blocks() {
    let doc = document_from_json(&json!({
        "main": [paragraph(json!({})), {"kind": "table"},
            paragraph(json!({"sectPr": {"docGrid": {"kind": "lines", "linePitch": 999}}})),
            paragraph(json!({}))],
        "sections": [
            {"blockRange": [0, 2], "props": {"docGrid": {"kind": "lines", "linePitch": 139}}},
            {"blockRange": [2, 3], "props": {"kind": "continuous", "docGrid": {"kind": "lines", "linePitch": 297}}},
            {"blockRange": [3, 4], "props": {}}
        ]
    }));
    assert_eq!(doc.skipped_blocks, 1);
    assert_eq!(
        doc.sections
            .iter()
            .map(|s| s.para_range.clone())
            .collect::<Vec<_>>(),
        [0..1, 1..2, 2..3]
    );
    assert_eq!(doc.sections[0].grid.line_pitch().unwrap(), Some(139));
    assert_eq!(doc.sections[1].grid.line_pitch().unwrap(), Some(297));
    assert_eq!(doc.sections[2].grid, DocumentGrid::default());
    assert!(has_note(&doc, "changes continuous docGrid declarations"));
    assert!(!doc.trace_metadata().to_string().contains("999"));
}

#[test]
fn real_docx_section_terminator_belongs_to_the_grid_it_ends() {
    let loaded = load(
        r#"<w:p><w:pPr><w:sectPr><w:docGrid w:type="lines" w:linePitch="139"/></w:sectPr></w:pPr><w:r><w:t>First</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p>"#,
        "",
        "",
    );
    let doc = loaded.layout_document();
    assert_eq!(doc.sections.len(), 2);
    assert_eq!(doc.sections[0].para_range, 0..1);
    assert_eq!(doc.sections[1].para_range, 1..2);
    assert_eq!(doc.sections[0].grid.kind().unwrap(), Some(GridKind::Lines));
    assert_eq!(doc.sections[0].grid.line_pitch().unwrap(), Some(139));
    assert_eq!(doc.sections[1].grid, DocumentGrid::default());
}

#[test]
fn real_docx_invalid_grid_uses_parser_raw_projection_and_diagnostics() {
    let loaded = load(
        r#"<w:p><w:r><w:t>A</w:t></w:r></w:p>"#,
        r#"<w:docGrid w:type="chars" w:linePitch="oops" w:charSpace="2147483648"/>"#,
        "",
    );
    let expected = json!({"kind": {"raw": "chars"}, "linePitch": {"raw": "oops"},
        "charSpace": {"raw": "2147483648"}});
    assert_eq!(loaded.json["sections"][0]["props"]["docGrid"], expected);
    assert!(loaded.json["warnings"].as_array().unwrap().len() >= 3);
    let doc = loaded.layout_document();
    assert_eq!(doc.sections[0].grid.declared(), Some(&expected));
    assert!(has_note(&doc, "docGrid.kind cannot be interpreted"));
    assert!(has_note(&doc, "docGrid.linePitch requires"));
    assert!(has_note(&doc, "docGrid.charSpace requires"));
}

#[test]
fn direct_snap_preserves_absence_false_and_true_in_paras_and_trace() {
    let doc = document_from_json(&json!({"main": [paragraph(json!({})),
        paragraph(json!({"snapToGrid": false})), paragraph(json!({"snapToGrid": true}))]}));
    assert_eq!(
        doc.paras.iter().map(|p| p.snap_to_grid).collect::<Vec<_>>(),
        [None, Some(false), Some(true)]
    );
    assert_eq!(
        doc.trace_metadata()["grid"]["paragraphSnapToGrid"],
        json!([null, false, true])
    );
    assert_eq!(doc.trace_metadata()["grid"]["applied"], false);
}

#[test]
fn malformed_direct_snap_is_diagnosed_instead_of_coerced_to_false() {
    for raw in [
        Value::Null,
        json!(0),
        json!(1),
        json!("false"),
        json!({"raw": "bad"}),
    ] {
        let doc = document_from_json(&json!({"main": [paragraph(json!({"snapToGrid": raw}))]}));
        assert_eq!(doc.paras[0].snap_to_grid, None);
        assert!(has_note(&doc, "block 0: snapToGrid requires a boolean"));
    }
}

#[test]
fn resolver_snap_chain_and_direct_false_reach_the_layout_document() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Leaf"/></w:pPr><w:r><w:t>Inherited</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Leaf"/><w:snapToGrid w:val="0"/></w:pPr><w:r><w:t>False</w:t></w:r></w:p><w:p><w:r><w:t>DocDefault</w:t></w:r></w:p>"#,
        r#"<w:docGrid w:type="lines" w:linePitch="297"/>"#,
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:snapToGrid w:val="false"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"><w:name w:val="Base"/><w:pPr><w:snapToGrid/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Leaf"><w:name w:val="Leaf"/><w:basedOn w:val="Base"/></w:style>"#,
    );
    assert!(loaded.json["main"][0]["props"].get("snapToGrid").is_none());
    let doc = loaded.layout_document();
    assert_eq!(
        doc.paras.iter().map(|p| p.snap_to_grid).collect::<Vec<_>>(),
        [Some(true), Some(false), Some(false)]
    );
    assert_eq!(
        doc.trace_metadata()["grid"]["paragraphSnapToGrid"],
        json!([true, false, false])
    );
    assert_eq!(loaded.paragraphs().0[0].snap_to_grid, Some(true));
}

#[test]
fn resolver_default_style_and_missing_style_keep_existing_boundaries() {
    let loaded = load(
        r#"<w:p><w:r><w:t>DefaultStyle</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Missing"/></w:pPr><w:r><w:t>Unknown</w:t></w:r></w:p>"#,
        "",
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:snapToGrid w:val="0"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Body"><w:name w:val="Body"/><w:pPr><w:snapToGrid/></w:pPr></w:style>"#,
    );
    let doc = loaded.layout_document();
    assert_eq!(doc.paras[0].snap_to_grid, Some(true));
    assert_eq!(doc.paras[1].snap_to_grid, Some(false));
    let all_absent = load(r#"<w:p><w:r><w:t>A</w:t></w:r></w:p>"#, "", "");
    assert_eq!(all_absent.layout_document().paras[0].snap_to_grid, None);
    assert!(all_absent.layout_document().source_warnings.is_empty());
}

#[test]
fn real_docx_paragraph_on_off_forms_are_preserved() {
    for (attribute, expected) in [
        ("", true),
        (r#" w:val="1""#, true),
        (r#" w:val="true""#, true),
        (r#" w:val="on""#, true),
        (r#" w:val="0""#, false),
        (r#" w:val="false""#, false),
        (r#" w:val="off""#, false),
    ] {
        let body = format!(
            r#"<w:p><w:pPr><w:snapToGrid{attribute}/></w:pPr><w:r><w:t>A</w:t></w:r></w:p>"#
        );
        assert_eq!(
            load(&body, "", "").layout_document().paras[0].snap_to_grid,
            Some(expected)
        );
    }
}

#[test]
fn run_snap_does_not_replace_paragraph_snap() {
    let loaded = load(
        r#"<w:p><w:pPr><w:snapToGrid w:val="0"/></w:pPr><w:r><w:rPr><w:snapToGrid/></w:rPr><w:t>A</w:t></w:r></w:p>"#,
        "",
        "",
    );
    assert_eq!(loaded.layout_document().paras[0].snap_to_grid, Some(false));
    assert!(
        loaded.json["main"][0]["inlines"][0]["props"]
            .get("snapToGrid")
            .is_none()
    );
}

#[test]
fn invalid_xml_snap_retains_the_pinned_parsers_warning_and_fallback() {
    let loaded = load(
        r#"<w:p><w:pPr><w:snapToGrid w:val="invalid-grid-toggle"/></w:pPr><w:r><w:t>A</w:t></w:r></w:p>"#,
        "",
        "",
    );
    // OnOff has no Val::Raw representation. The parser fallback must not be
    // confused with a validated literal true in the unchanged source JSON.
    assert_eq!(loaded.json["main"][0]["props"]["snapToGrid"], true);
    assert!(
        loaded.json["warnings"]
            .to_string()
            .contains("invalid-grid-toggle")
    );
    let source_json = loaded.json.clone();
    let warnings = source_json["warnings"].clone();
    let mut document = loaded.layout_document();
    assert_eq!(document.paras[0].snap_to_grid, Some(true));
    assert_eq!(json!(document.source_warnings), warnings);
    assert_eq!(document.trace_metadata()["sourceWarnings"], warnings);
    document
        .apply_page_overrides(PageOverrides {
            margin: Some(720),
            page_width: Some(10000),
            content_width: Some(7000),
        })
        .unwrap();
    assert_eq!(document.trace_metadata()["sourceWarnings"], warnings);
    assert!(
        document
            .apply_page_overrides(PageOverrides {
                margin: Some(-1),
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(json!(document.source_warnings), warnings);
    assert_eq!(document.trace_metadata()["sourceWarnings"], warnings);
    assert_eq!(loaded.json, source_json);
}

#[test]
fn parser_warning_objects_preserve_all_source_fields_in_trace() {
    let mut source = input(None);
    source["warnings"] = json!([{
        "code": "PROP_BAD_VALUE", "part": 4, "range": [10, 31],
        "message": "original warning", "futureField": {"raw": "unchanged"},
    }]);
    let expected = source["warnings"].clone();
    let doc = document_from_json(&source);
    source["warnings"][0]["message"] = json!("caller changed source");
    assert_eq!(json!(doc.source_warnings), expected);
    assert_eq!(doc.trace_metadata()["sourceWarnings"], expected);
    assert!(doc.diagnostics.is_empty());
}

#[test]
fn page_overrides_do_not_change_grid_declarations_or_snap_inputs() {
    let mut source = input(Some(
        json!({"kind": "linesAndChars", "linePitch": 139, "charSpace": -17}),
    ));
    source["main"][0]["props"]["snapToGrid"] = json!(false);
    let mut doc = document_from_json(&source);
    let grid = doc.sections[0].grid.clone();
    let before = doc.trace_metadata()["sections"][0]["grid"].clone();
    doc.apply_page_overrides(PageOverrides {
        margin: Some(720),
        page_width: Some(10000),
        content_width: Some(7000),
    })
    .unwrap();
    assert_eq!(doc.sections[0].grid, grid);
    assert_eq!(doc.trace_metadata()["sections"][0]["grid"], before);
    assert_eq!(doc.paras[0].snap_to_grid, Some(false));
    assert!(
        doc.apply_page_overrides(PageOverrides {
            content_width: Some(-1),
            ..Default::default()
        })
        .is_err()
    );
    assert_eq!(doc.sections[0].grid, grid);
}

/// 网格只在 Android 打印视图、有效的行网格、没写 `snapToGrid=false` 时改排版（`grid_lines.rs`）；
/// 其余组合照旧。
#[test]
fn retaining_grid_inputs_does_not_change_current_layout_in_any_mode() {
    let mut source = input(None);
    source["main"] = json!((0..90).map(|_| paragraph(json!({}))).collect::<Vec<_>>());
    source["sections"][0]["blockRange"] = json!([0, 90]);
    let baseline = document_from_json(&source);
    for (platform, view) in [
        (Platform::Desktop, View::Print),
        (Platform::Desktop, View::Mobile),
        (Platform::Android, View::Print),
        (Platform::Android, View::Mobile),
    ] {
        let engine = Engine::new(&SimpleMetrics, PageSetup::a4()).with_platform(platform, view);
        let pages = format!("{:?}", engine.layout_document(&baseline));
        for grid in [
            json!({"kind": "lines", "linePitch": 139}),
            json!({"kind": "linesAndChars", "linePitch": 297, "charSpace": -17}),
            json!({"kind": {"raw": "bad"}, "linePitch": 0}),
        ] {
            source["sections"][0]["props"]["docGrid"] = grid.clone();
            for snap in [Value::Null, json!(false), json!(true)] {
                for para in source["main"].as_array_mut().unwrap() {
                    if snap.is_null() {
                        para["props"].as_object_mut().unwrap().remove("snapToGrid");
                    } else {
                        para["props"]["snapToGrid"] = snap.clone();
                    }
                }
                let applies = platform == Platform::Android
                    && view == View::Print
                    && grid["kind"].is_string()
                    && snap != json!(false);
                let laid = format!("{:?}", engine.layout_document(&document_from_json(&source)));
                assert_eq!(laid != pages, applies, "{platform:?} {view:?} {grid} {snap}");
            }
        }
    }
}
