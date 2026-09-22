//! The on/off fixtures must remain distinct through parser, bridge and layout.

use rsword::bind::native::SessionTable;
use rsword_layout_core::{Engine, Fragment, PageSetup, SimpleMetrics, paras_from_document};

fn document(name: &str) -> serde_json::Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join("fixtures").join(name)).unwrap();
    let mut sessions = SessionTable::default();
    let id = sessions.open(&bytes, None).unwrap();
    let result = serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
    sessions.close(&id);
    result
}

#[test]
fn archived_overflow_switch_reaches_layout() {
    for (name, enabled, first_line_chars) in
        [("kinsoku.docx", true, 38), ("kinsoku2.docx", false, 36)]
    {
        let (paras, _) = paras_from_document(&document(name));
        let text_paras: Vec<_> = paras
            .iter()
            .filter(|p| {
                p.runs.iter().any(|r| {
                    r.text.contains('\u{3002}')
                        || r.text.contains('\u{ff0c}')
                        || r.text.contains('\u{ff09}')
                        || r.text.contains('\u{3001}')
                })
            })
            .collect();
        assert_eq!(text_paras.len(), 4);
        assert!(text_paras.iter().all(|p| p.overflow_punct == enabled));
        // SimpleMetrics provides exactly one em per CJK character. This checks
        // policy transport and ordering, not the captured Songti geometry.
        for para in text_paras {
            let pages =
                Engine::new(&SimpleMetrics, PageSetup::a4()).layout(std::slice::from_ref(para));
            let first: String = pages[0]
                .fragments
                .iter()
                .filter_map(|f| match f {
                    Fragment::Text(t) if t.line == 0 => Some(t.text.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(first.chars().count(), first_line_chars, "{name}");
        }
    }
}

#[test]
fn bridge_preserves_explicit_on_off_and_default() {
    for (props, enabled) in [
        (serde_json::json!({}), true),
        (serde_json::json!({"overflowPunct": true}), true),
        (serde_json::json!({"overflowPunct": false}), false),
    ] {
        let doc = serde_json::json!({"main": [{"kind": "text", "props": props,
            "inlines": [{"kind": "run", "text": "text"}]}]});
        assert_eq!(paras_from_document(&doc).0[0].overflow_punct, enabled);
    }
}
