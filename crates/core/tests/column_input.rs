//! Column geometry projection and validation, independent of page/column flow.
//! The two-column width 4873 has Android report evidence; other geometry cases
//! exercise host arithmetic and input handling rather than new Word captures.

use rsword::package::Package;
use rsword_layout_core::{
    ColumnLayout, ColumnSpec, LayoutDocument, PageOverrides, Rect, document_from_json,
    load_document,
};
use serde_json::{Value, json};

fn document(columns: Value) -> LayoutDocument {
    document_from_json(&json!({
        "main": [{"kind": "text", "props": {}, "inlines": []}],
        "sections": [{"blockRange": [0, 1], "props": {
            "pageSize": {"w": 11906, "h": 16838},
            "pageMargins": {"top": 720, "right": 720, "bottom": 720, "left": 720},
            "columns": columns,
        }}],
    }))
}

fn areas(document: &LayoutDocument) -> Vec<Rect> {
    let section = &document.sections[0];
    section.columns.areas(section.setup.content_area()).unwrap()
}

fn parsed(columns: &str, extra_section: &str) -> LayoutDocument {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>A</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720"/>{columns}{extra_section}</w:sectPr></w:body></w:document>"#
    );
    package.replace_part_xml(package.main_part(), &xml).unwrap();
    load_document(&package.save().unwrap())
        .unwrap()
        .layout_document()
}

#[test]
fn absent_and_empty_columns_preserve_single_body_area() {
    let absent = document_from_json(&json!({"main": []}));
    assert_eq!(absent.sections[0].columns, ColumnLayout::default());
    let empty = document(json!({}));
    assert_eq!(areas(&empty), [Rect::new(720, 720, 10466, 15398)]);
    assert!(empty.diagnostics.is_empty());
}

#[test]
fn observed_two_column_width_uses_missing_equal_width_as_equal() {
    let doc = parsed(r#"<w:cols w:num="2" w:space="720"/>"#, "");
    assert_eq!(
        areas(&doc),
        [
            Rect::new(720, 720, 4873, 15398),
            Rect::new(6313, 720, 4873, 15398),
        ]
    );
    assert!(doc.diagnostics.is_empty());
    let meta = doc.trace_metadata();
    assert_eq!(meta["sections"][0]["columns"]["declared"]["num"], 2);
    assert!(
        meta["sections"][0]["columns"]["declared"]
            .get("equalWidth")
            .is_none()
    );
    assert_eq!(meta["sections"][0]["columns"]["areas"][1]["x"], 6313);
    assert!(meta["sections"][0]["columns"]["flow"].as_str().unwrap().contains("sequential document end"));
    assert!(meta["sections"][0]["columns"]["flowEvidence"].as_str().unwrap().contains("other platforms and views is inferred"));
    assert!(meta["sections"][0]["columns"]["areasScope"].as_str().unwrap().contains("section body templates"));
}

#[test]
fn default_equal_gap_is_720_and_explicit_zero_is_retained() {
    let default_gap = document(json!({"num": 2}));
    assert_eq!(areas(&default_gap)[0].width, 4873);
    let no_gap = document(json!({"num": 2, "space": 0}));
    assert_eq!(
        areas(&no_gap),
        [
            Rect::new(720, 720, 5233, 15398),
            Rect::new(5953, 720, 5233, 15398),
        ]
    );
}

#[test]
fn assumed_integer_remainder_distribution_preserves_exact_body_width() {
    let columns = ColumnLayout::equal(3, 1).unwrap();
    let result = columns.areas(Rect::new(100, 200, 13, 300)).unwrap();
    assert_eq!(
        result,
        [
            Rect::new(100, 200, 4, 300),
            Rect::new(105, 200, 4, 300),
            Rect::new(110, 200, 3, 300),
        ]
    );
    assert_eq!(result.last().unwrap().right(), 113);
}

#[test]
fn explicit_columns_use_child_widths_and_gaps_with_last_gap_ignored() {
    let doc = parsed(
        r#"<w:cols w:equalWidth="0" w:num="8" w:space="999"><w:col w:w="3000" w:space="720"/><w:col w:w="6746" w:space="888"/></w:cols>"#,
        "",
    );
    assert_eq!(
        areas(&doc),
        [
            Rect::new(720, 720, 3000, 15398),
            Rect::new(4440, 720, 6746, 15398),
        ]
    );
    assert_eq!(doc.sections[0].columns.count(), 2);
    assert!(
        doc.diagnostics
            .iter()
            .any(|note| note.contains("num=8 ignored"))
    );
    let meta = doc.trace_metadata();
    assert_eq!(
        meta["sections"][0]["columns"]["declared"]["col"][1]["space"],
        888
    );
    assert_eq!(
        meta["sections"][0]["columns"]["effective"]["columns"][1]["gapAfter"],
        0
    );
}

#[test]
fn explicit_list_without_num_and_without_gaps_does_not_inherit_container_space() {
    let doc = document(json!({"equalWidth": false, "space": 720,
        "col": [{"w": 3000}, {"w": 7466}]}));
    assert_eq!(
        areas(&doc),
        [
            Rect::new(720, 720, 3000, 15398),
            Rect::new(3720, 720, 7466, 15398),
        ]
    );
}

#[test]
fn explicit_underfilled_body_is_not_rescaled_and_is_diagnosed() {
    let doc = document(json!({"equalWidth": false,
        "col": [{"w": 2000, "space": 360}, {"w": 6666}]}));
    assert_eq!(areas(&doc)[1], Rect::new(3080, 720, 6666, 15398));
    assert!(
        doc.diagnostics
            .iter()
            .any(|note| note.contains("1440 twips unused"))
    );
}

#[test]
fn explicit_entries_with_equal_width_are_ignored_with_diagnostic() {
    for equal in [None, Some(true)] {
        let mut input = json!({"num": 2, "space": 720, "col": [{"w": 1}, {"w": 9999}]});
        if let Some(equal) = equal {
            input["equalWidth"] = json!(equal);
        }
        let doc = document(input);
        assert_eq!(areas(&doc)[0].width, 4873);
        assert!(
            doc.diagnostics
                .iter()
                .any(|note| note.contains("explicit column entries are ignored"))
        );
    }
}

#[test]
fn invalid_native_column_inputs_fall_back_with_original_metadata_and_reason() {
    for input in [
        Value::Null,
        json!([]),
        json!({"num": 0}),
        json!({"num": -1}),
        json!({"num": 46}),
        json!({"num": true}),
        json!({"num": 1.5}),
        json!({"num": {"raw": "two"}}),
        json!({"num": 2, "space": -1}),
        json!({"num": 2, "space": 10466}),
        json!({"num": 2, "space": 2147483648_i64}),
        json!({"equalWidth": "false"}),
        json!({"sep": 1}),
        json!({"col": 4}),
        json!({"equalWidth": false, "col": []}),
        json!({"equalWidth": false, "col": [1]}),
        json!({"equalWidth": false, "col": [{}]}),
        json!({"equalWidth": false, "col": [{"w": 0}]}),
        json!({"equalWidth": false, "col": [{"w": -1}]}),
        json!({"equalWidth": false, "col": [{"w": {"raw": "wide"}}]}),
        json!({"equalWidth": false, "col": [{"w": 10467}]}),
        json!({"equalWidth": false, "col": [{"w": 8000, "space": 2500}, {"w": 1}]}),
        json!({"equalWidth": false, "col": [{"w": 100, "space": -1}]}),
    ] {
        let doc = document(input.clone());
        assert_eq!(areas(&doc), [Rect::new(720, 720, 10466, 15398)], "{input}");
        assert!(
            doc.diagnostics
                .iter()
                .any(|note| note.contains("invalid columns")),
            "{input}"
        );
        assert_eq!(
            doc.trace_metadata()["sections"][0]["columns"]["declared"],
            input
        );
    }
}

#[test]
fn actual_parser_raw_values_are_not_silently_defaulted() {
    for xml in [
        r#"<w:cols w:num="two"/>"#,
        r#"<w:cols w:num="2" w:space="not-a-measure"/>"#,
        r#"<w:cols w:equalWidth="0"><w:col w:w="invalid"/></w:cols>"#,
    ] {
        let doc = parsed(xml, "");
        assert_eq!(doc.sections[0].columns.count(), 1);
        assert!(
            doc.diagnostics
                .iter()
                .any(|note| note.contains("invalid columns"))
        );
        assert!(
            doc.trace_metadata()["sections"][0]["columns"]["declared"]
                .to_string()
                .contains("raw")
        );
    }
}

#[test]
fn public_constructors_bound_column_counts_widths_and_gaps() {
    assert!(ColumnLayout::equal(0, 0).is_err());
    assert!(ColumnLayout::equal(46, 0).is_err());
    assert!(ColumnLayout::equal(2, -1).is_err());
    assert!(ColumnLayout::explicit(vec![]).is_err());
    assert!(
        ColumnLayout::explicit(vec![
            ColumnSpec {
                width: 1,
                gap_after: 0
            };
            46
        ])
        .is_err()
    );
    assert!(
        ColumnLayout::explicit(vec![ColumnSpec {
            width: 0,
            gap_after: 0
        }])
        .is_err()
    );
    let maximum = ColumnLayout::equal(45, 0).unwrap();
    assert_eq!(maximum.areas(Rect::new(0, 0, 45, 100)).unwrap().len(), 45);
    assert!(maximum.areas(Rect::new(0, 0, 44, 100)).is_err());
}

#[test]
fn rectangle_arithmetic_rejects_overflow_and_nonpositive_dimensions() {
    for columns in [
        ColumnLayout::default(),
        ColumnLayout::equal(2, i32::MAX).unwrap(),
    ] {
        for body in [
            Rect::new(i32::MAX, 0, 1, 1),
            Rect::new(0, i32::MAX, 1, 1),
            Rect::new(0, 0, 0, 1),
            Rect::new(0, 0, 1, 0),
        ] {
            assert!(columns.areas(body).is_err());
        }
    }
    assert!(
        ColumnLayout::equal(45, i32::MAX)
            .unwrap()
            .areas(Rect::new(0, 0, i32::MAX, 10))
            .is_err()
    );
    let explicit = ColumnLayout::explicit(vec![
        ColumnSpec {
            width: i32::MAX,
            gap_after: i32::MAX
        };
        2
    ])
    .unwrap();
    assert!(explicit.areas(Rect::new(0, 0, i32::MAX, 10)).is_err());
}

#[test]
fn equal_layout_equality_ignores_declaration_and_unused_single_column_gap() {
    let first = document(json!({"num": 2, "space": 720}));
    let second = document(json!({"num": 2, "space": 720, "equalWidth": true, "sep": false}));
    assert_eq!(first.sections[0].columns, second.sections[0].columns);
    assert_eq!(
        ColumnLayout::default(),
        ColumnLayout::equal(1, 720).unwrap()
    );
}

#[test]
fn supported_page_overrides_rederive_equal_columns_and_keep_declared_inputs() {
    let mut doc = document(json!({"num": 2, "space": 720}));
    let original = doc.trace_metadata()["sections"][0]["columns"]["declared"].clone();
    doc.apply_page_overrides(PageOverrides {
        content_width: Some(9000),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        areas(&doc),
        [
            Rect::new(1453, 720, 4140, 15398),
            Rect::new(6313, 720, 4140, 15398)
        ]
    );
    assert_eq!(
        doc.trace_metadata()["sections"][0]["columns"]["declared"],
        original
    );
    assert_eq!(
        doc.trace_metadata()["sections"][0]["columns"]["areas"][0]["width"],
        4140
    );
}

#[test]
fn failed_page_override_does_not_change_any_section_or_accumulated_overrides() {
    let mut doc = document(json!({"num": 2, "space": 720}));
    let mut second = doc.sections[0].clone();
    second.columns = ColumnLayout::explicit(vec![ColumnSpec {
        width: 8000,
        gap_after: 0,
    }])
    .unwrap();
    doc.sections.push(second);
    doc.apply_page_overrides(PageOverrides {
        margin: Some(500),
        ..Default::default()
    })
    .unwrap();
    let before = doc.trace_metadata();
    assert!(
        doc.apply_page_overrides(PageOverrides {
            content_width: Some(7000),
            ..Default::default()
        })
        .is_err()
    );
    assert_eq!(doc.trace_metadata(), before);
    assert!(
        doc.apply_page_overrides(PageOverrides {
            content_width: Some(720),
            ..Default::default()
        })
        .is_err()
    );
    assert_eq!(doc.trace_metadata(), before);
}

#[test]
fn unsupported_separator_rtl_and_vertical_flow_are_explicit_diagnostics() {
    let doc = parsed(
        r#"<w:cols w:num="2" w:sep="1"/>"#,
        r#"<w:bidi/><w:textDirection w:val="tbRl"/>"#,
    );
    assert_eq!(doc.sections[0].columns.count(), 2);
    for phrase in [
        "separator lines",
        "right-to-left column order",
        "textDirection",
    ] {
        assert!(
            doc.diagnostics.iter().any(|note| note.contains(phrase)),
            "{phrase}: {:?}",
            doc.diagnostics
        );
    }
}

#[test]
fn same_setup_continuous_column_changes_retain_inputs_without_an_unsupported_diagnostic() {
    let doc = document_from_json(&json!({
        "main": [{"kind": "text"}, {"kind": "text"}],
        "sections": [
            {"blockRange": [0, 1], "props": {}},
            {"blockRange": [1, 2], "props": {"kind": "continuous", "columns": {"num": 2}}},
        ],
    }));
    assert_eq!(
        (
            doc.sections[0].columns.count(),
            doc.sections[1].columns.count()
        ),
        (1, 2)
    );
    assert!(doc.diagnostics.is_empty());
}

#[test]
fn next_column_and_changed_page_setup_retain_the_pending_column_diagnostic() {
    for (kind, changed_setup) in [("nextColumn", false), ("continuous", true)] {
        let mut next_props = json!({"kind": kind, "columns": {"num": 2}});
        if changed_setup {
            next_props["pageSize"] = json!({"w": 10000, "h": 16838});
        }
        let doc = document_from_json(&json!({
            "main": [{"kind": "text"}, {"kind": "text"}],
            "sections": [
                {"blockRange": [0, 1], "props": {}},
                {"blockRange": [1, 2], "props": next_props},
            ],
        }));
        assert!(doc.diagnostics.iter().any(|note| note.contains(
            "current page retains its columns; new columns start on the next page"
        )), "{kind}: {:?}", doc.diagnostics);
        assert_eq!(doc.diagnostics.iter().any(|note| note.contains(
            "no balancing or new same-page column group"
        )), changed_setup);
    }
}

#[test]
fn keep_next_diagnostic_is_limited_to_linked_same_setup_continuous_multicolumn_groups() {
    for (first_columns, next_columns) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
        for (kind, keep_next, page_break, changed_setup) in [
            ("continuous", true, false, false),
            ("continuous", false, false, false),
            ("continuous", true, true, false),
            ("continuous", true, false, true),
            ("nextColumn", true, false, false),
        ] {
            let mut next_props = json!({"kind": kind, "columns": {"num": next_columns}});
            if changed_setup {
                next_props["pageSize"] = json!({"w": 10000, "h": 16838});
            }
            let doc = document_from_json(&json!({
                "main": [
                    {"kind": "text", "props": {"keepNext": keep_next}},
                    {"kind": "text", "props": {"pageBreakBefore": page_break}},
                ],
                "sections": [
                    {"blockRange": [0, 1], "props": {"columns": {"num": first_columns}}},
                    {"blockRange": [1, 2], "props": next_props},
                ],
            }));
            let expected = kind == "continuous" && keep_next && !page_break && !changed_setup
                && (first_columns > 1 || next_columns > 1);
            assert_eq!(doc.diagnostics.iter().any(|note| note.contains(
                "keepNext across same-setup continuous column groups"
            )), expected, "{first_columns}/{next_columns}/{kind}/{keep_next}/{page_break}/{changed_setup}");
        }
    }
}
