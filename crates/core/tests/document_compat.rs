//! Document compatibility reaches the common formatter independently of platform/view.
//! These tests exercise OOXML switches and source accounting; they are not new Word captures.

use rsword::package::{Package, PartUri};
use rsword_layout_core::{
    DocumentCompatibility, Engine, LayoutRecord, LineTerminator as T, LoadedDocument, Page,
    PageBreakPosition as B, PageSetup, Platform, SimpleMetrics, View, document_from_json,
    load_document, paint_document,
};
use serde_json::json;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const BODY: &str =
    r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>x</w:t></w:r></w:p>"#;

fn load(body: &str, settings: Option<&str>, settings_name: &str) -> LoadedDocument {
    let bytes = rsword::save::blank_docx(None).unwrap();
    let mut package = Package::open(&bytes).unwrap();
    let document = format!(
        r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    );
    package
        .replace_part_xml(package.main_part(), &document)
        .unwrap();
    if let Some(settings) = settings {
        let content_type =
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
        package
            .register_new_part(
                PartUri::from_entry_name(&format!("word/{settings_name}")),
                content_type,
                settings,
            )
            .unwrap();
        for (name, close, addition) in [
            (
                "[Content_Types].xml",
                "</Types>",
                format!(
                    r#"<Override PartName="/word/{settings_name}" ContentType="{content_type}"/>"#
                ),
            ),
            (
                "word/_rels/document.xml.rels",
                "</Relationships>",
                format!(
                    r#"<Relationship Id="rIdSettings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="{settings_name}"/>"#
                ),
            ),
        ] {
            let part = package.find_name(name).unwrap();
            let xml = String::from_utf8(package.read_bytes(part).unwrap()).unwrap();
            package
                .replace_part_xml(part, &xml.replace(close, &format!("{addition}{close}")))
                .unwrap();
        }
    }
    load_document(&package.save().unwrap()).unwrap()
}

fn settings(flag: &str, prefix: &str, namespace: &str) -> String {
    format!(
        r#"<{prefix}:settings xmlns:{prefix}="{namespace}"><{prefix}:compat>{flag}</{prefix}:compat></{prefix}:settings>"#
    )
}

fn ranges(pages: &[Page]) -> Vec<(usize, u32, u32, T)> {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
        .pages
        .iter()
        .flat_map(|page| {
            page.lines.iter().map(move |line| {
                let source = line.source.expect("line retains its UTF-16 source range");
                (page.index, source.start, source.end, line.terminator)
            })
        })
        .collect()
}

fn expected(split: bool) -> Vec<(usize, u32, u32, T)> {
    if split {
        vec![
            (0, 0, 1, T::PageBreak(B::OwnLine)),
            (1, 1, 2, T::ParagraphMark),
            (1, 2, 4, T::ParagraphMark),
        ]
    } else {
        vec![
            (0, 0, 2, T::PageBreak(B::BeforeMark)),
            (1, 2, 4, T::ParagraphMark),
        ]
    }
}

#[test]
fn absent_enabled_and_disabled_switches_combine_with_all_platform_view_pairs() {
    let loaded = load(BODY, None, "settings.xml");
    for flag in [None, Some(true), Some(false)] {
        let compatibility = DocumentCompatibility {
            split_page_break_and_para_mark: flag,
        };
        let mut document = loaded.layout_document();
        document.compatibility = compatibility;
        for platform in [Platform::Desktop, Platform::Android] {
            for view in [View::Print, View::Mobile] {
                let engine =
                    Engine::new(&SimpleMetrics, PageSetup::a4()).with_platform(platform, view);
                let want = expected(flag == Some(true) || view == View::Mobile);
                assert_eq!(
                    ranges(&engine.layout_document(&document)),
                    want,
                    "document {flag:?}, {platform:?}, {view:?}"
                );
                assert_eq!(
                    ranges(
                        &engine
                            .with_compatibility(compatibility)
                            .layout(&document.paras)
                    ),
                    want,
                    "paragraph-only {flag:?}, {platform:?}, {view:?}"
                );
            }
        }
    }
}

#[test]
fn real_docx_onoff_values_and_namespace_prefixes_reach_layout() {
    for (namespace, prefix) in [
        (W, "w"),
        (W, "s"),
        ("http://purl.oclc.org/ooxml/wordprocessingml/main", "s"),
    ] {
        for (value, enabled) in [
            (None, true),
            (Some("1"), true),
            (Some("true"), true),
            (Some("on"), true),
            (Some("0"), false),
            (Some("false"), false),
            (Some("off"), false),
        ] {
            let attr =
                value.map_or_else(String::new, |value| format!(r#" {prefix}:val="{value}""#));
            let flag = format!("<{prefix}:splitPgBreakAndParaMark{attr}/>");
            let loaded = load(
                BODY,
                Some(&settings(&flag, prefix, namespace)),
                "settings.xml",
            );
            let document = loaded.layout_document();
            assert_eq!(
                document.compatibility.split_page_break_and_para_mark,
                Some(enabled)
            );
            // Native parser JSON does not expose raw_unmodeled compatibility flags.
            assert!(
                loaded.json["settings"]["compat"]
                    .get("splitPgBreakAndParaMark")
                    .is_none()
            );
            assert_eq!(
                document.trace_metadata()["compatibility"]["splitPgBreakAndParaMark"],
                enabled
            );
            for platform in [Platform::Desktop, Platform::Android] {
                for view in [View::Print, View::Mobile] {
                    let pages = Engine::new(&SimpleMetrics, PageSetup::a4())
                        .with_platform(platform, view)
                        .layout_document(&document);
                    assert_eq!(ranges(&pages), expected(enabled || view == View::Mobile));
                }
            }
        }
    }
}

#[test]
fn absent_settings_and_absent_flag_preserve_the_default_and_metadata_absence() {
    for xml in [None, Some(settings("", "w", W))] {
        let loaded = load(BODY, xml.as_deref(), "settings.xml");
        let document = loaded.layout_document();
        assert_eq!(document.compatibility, DocumentCompatibility::default());
        assert!(document.trace_metadata()["compatibility"]["splitPgBreakAndParaMark"].is_null());
        assert_eq!(
            ranges(&Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document)),
            expected(false)
        );
    }
}

#[test]
fn document_switches_do_not_leak_between_layout_calls_or_from_engine_defaults() {
    let enabled = load(
        BODY,
        Some(&settings("<w:splitPgBreakAndParaMark/>", "w", W)),
        "settings.xml",
    )
    .layout_document();
    let absent = load(BODY, None, "settings.xml").layout_document();
    let disabled = load(
        BODY,
        Some(&settings(
            r#"<w:splitPgBreakAndParaMark w:val="0"/>"#,
            "w",
            W,
        )),
        "settings.xml",
    )
    .layout_document();
    let engine =
        Engine::new(&SimpleMetrics, PageSetup::a4()).with_compatibility(enabled.compatibility);
    for document in [&enabled, &absent, &disabled, &enabled, &absent] {
        assert_eq!(
            ranges(&engine.layout_document(document)),
            expected(document.compatibility.split_page_break_and_para_mark == Some(true))
        );
    }
    assert_eq!(ranges(&engine.layout(&absent.paras)), expected(true));
}

#[test]
fn settings_relationship_resolves_a_nonstandard_part_name() {
    let loaded = load(
        BODY,
        Some(&settings("<w:splitPgBreakAndParaMark/>", "w", W)),
        "custom-settings.xml",
    );
    assert_eq!(
        loaded
            .layout_document()
            .compatibility
            .split_page_break_and_para_mark,
        Some(true)
    );
}

#[test]
fn flags_outside_word_compat_or_in_another_namespace_do_not_enable_splitting() {
    for xml in [
        format!(r#"<w:settings xmlns:w="{W}"><w:splitPgBreakAndParaMark/></w:settings>"#),
        settings(r#"<a:splitPgBreakAndParaMark xmlns:a="urn:test"/>"#, "w", W),
        settings(
            r#"<w:compatSetting w:name="splitPgBreakAndParaMark" w:val="1"/>"#,
            "w",
            W,
        ),
    ] {
        let loaded = load(BODY, Some(&xml), "settings.xml");
        assert_eq!(
            loaded.layout_document().compatibility,
            DocumentCompatibility::default()
        );
    }
}

#[test]
fn document_json_keeps_compatibility_unset_until_explicitly_supplied() {
    let loaded = load(
        BODY,
        Some(&settings("<w:splitPgBreakAndParaMark/>", "w", W)),
        "settings.xml",
    );
    assert_eq!(
        document_from_json(&loaded.json).compatibility,
        DocumentCompatibility::default()
    );
    assert_eq!(
        loaded
            .layout_document()
            .compatibility
            .split_page_break_and_para_mark,
        Some(true)
    );
}

#[test]
fn split_switch_survives_sibling_run_property_merge() {
    let body = r#"<w:p><w:r><w:rPr><w:spacing w:val="10"/></w:rPr><w:rPr><w:sz w:val="24"/></w:rPr><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>x</w:t></w:r></w:p>"#;
    let loaded = load(
        body,
        Some(&settings("<w:splitPgBreakAndParaMark/>", "w", W)),
        "settings.xml",
    );
    assert_eq!(loaded.merged_run_props, 1);
    assert!(loaded.merge_error.is_none());
    assert_eq!(
        ranges(
            &Engine::new(&SimpleMetrics, PageSetup::a4())
                .layout_document(&loaded.layout_document())
        ),
        expected(true)
    );
}

#[test]
fn compatibility_does_not_split_section_terminators_or_change_midparagraph_breaks() {
    for body in [
        r#"<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>x</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>a</w:t><w:br w:type="page"/><w:t>b</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:br/></w:r></w:p>"#,
        r#"<w:p><w:r><w:br w:type="column"/></w:r></w:p>"#,
    ] {
        let absent = load(body, None, "settings.xml").layout_document();
        let enabled = load(
            body,
            Some(&settings("<w:splitPgBreakAndParaMark/>", "w", W)),
            "settings.xml",
        )
        .layout_document();
        let engine = Engine::new(&SimpleMetrics, PageSetup::a4());
        assert_eq!(
            format!("{:?}", engine.layout_document(&absent)),
            format!("{:?}", engine.layout_document(&enabled)),
            "{body}"
        );
    }
}

#[test]
fn trace_metadata_distinguishes_absence_from_explicit_false() {
    let mut document = document_from_json(&json!({"main": []}));
    assert!(document.trace_metadata()["compatibility"]["splitPgBreakAndParaMark"].is_null());
    document.compatibility.split_page_break_and_para_mark = Some(false);
    assert_eq!(
        document.trace_metadata()["compatibility"]["splitPgBreakAndParaMark"],
        false
    );
}
