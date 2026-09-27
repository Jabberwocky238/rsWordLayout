//! Stories outside the main text are not laid out yet. They must be reported,
//! never dropped silently (docs/ACCEPTANCE-PANEL-2026-09-27.md found footnote32).

use rsword_layout_core::{document_from_json, load_document};
use serde_json::json;

fn text_block() -> serde_json::Value {
    json!({"kind": "text", "node": 1, "props": {}, "inlines": [
        {"kind": "run", "node": 2, "props": {}, "text": "Body"}]})
}

#[test]
fn user_notes_and_header_footer_references_are_reported() {
    let doc = json!({
        "main": [text_block()],
        "footnotes": [
            {"id": "-1", "kind": {"kind": "separator"}, "blocks": []},
            {"id": "0", "kind": {"kind": "continuationSeparator"}, "blocks": []},
            {"id": "1", "kind": {"kind": "normal"}, "blocks": []},
            {"id": "2", "kind": {"kind": "normal"}, "blocks": []},
        ],
        "endnotes": [{"id": "1", "kind": {"kind": "normal"}, "blocks": []}],
        "sections": [{"blockRange": [0, 1], "props": {
            "headerReferences": [{"type": "default"}],
            "footerReferences": [{"type": "default"}, {"type": "first"}],
        }}],
    });
    let doc = document_from_json(&doc);
    let has = |needle: &str| doc.diagnostics.iter().any(|d| d.contains(needle));
    assert!(has("2 footnotes are not laid out"), "{:?}", doc.diagnostics);
    assert!(has("1 endnotes are not laid out"), "{:?}", doc.diagnostics);
    assert!(has("section 0: 1 header and 2 footer references are not laid out"), "{:?}", doc.diagnostics);
}

#[test]
fn separators_alone_and_absent_stories_add_no_diagnostic() {
    let doc = json!({
        "main": [text_block()],
        "footnotes": [{"id": "-1", "kind": {"kind": "separator"}, "blocks": []}],
        "sections": [{"blockRange": [0, 1], "props": {"headerReferences": [], "footerReferences": []}}],
    });
    assert!(document_from_json(&doc).diagnostics.is_empty());
}

#[test]
#[ignore = "needs sibling word_analyse fixtures"]
fn real_footnote32_reports_its_note() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../word_analyse/fixtures/footnote32.docx");
    let doc = load_document(&std::fs::read(path).unwrap()).unwrap().layout_document();
    assert!(doc.diagnostics.iter().any(|d| d.contains("1 footnotes are not laid out")), "{:?}", doc.diagnostics);
}
