//! `w:contextualSpacing`：带开关的段，前一段同样式时不要自己的段前，后一段同样式时不要自己的段后。
//!
//! Word 实测（Android Word 打印视图，2026-10-04；版心高 15398，每段一行、精确行距 480，读第 1 页的段数；
//! `docs/WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md` 补测第 8 条）：
//!
//! | 写法 | 本段管本段 | 任一段的开关管两侧 | 不实现 | Word |
//! | --- | ---: | ---: | ---: | ---: |
//! | 全部带开关，段后 240 | 32 | 32 | 21 | 32 |
//! | 全部带开关，段前 240 | 31 | 31 | 21 | 31 |
//! | 单数段带开关段后 0，双数段不带段后 240 | 25 | 32 | 25 | 25 |
//! | 双数段带开关段前 0，单数段不带段前 240 | 25 | 32 | 25 | 25 |
//! | 全部带开关段后 240，两种样式交替 | 21 | 21 | 21 | 21 |
//!
//! 去掉的段后仍按原值抵下一段的段前（`both-phase`：行距 400、段前 300、段后 100，双数段带开关，
//! Word 27 段；「段后清零再取 max」给 25）。`w:pStyle` 指向不存在的样式、或样式 part 不由主文档
//! 关系引到时，Word 按缺省样式比（word_analyse `context-mixed` 42 行；补测 `sr-root`）。

use rsword::package::Package;
use rsword_layout_core::{LayoutOptions, LayoutRecord, Platform, PreparedDocument, View, WrapPolicy};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// (样式名后缀, 带不带开关, 段前, 段后)
type Spec = (&'static str, bool, u32, u32);

fn docx(specs: &[Spec]) -> Vec<u8> {
    docx_with(specs, 480, true)
}

/// `styles_related`：样式 part 由不由主文档关系引到（不引到时只留在包里）。
fn docx_with(specs: &[Spec], line: u32, styles_related: bool) -> Vec<u8> {
    let paras: String = specs
        .iter()
        .enumerate()
        .map(|(i, &(style, contextual, before, after))| {
            let flag = if contextual { "<w:contextualSpacing/>" } else { r#"<w:contextualSpacing w:val="0"/>"# };
            format!(
                r#"<w:p><w:pPr><w:pStyle w:val="Probe{style}"/>{flag}<w:spacing w:before="{before}" w:after="{after}" w:line="{line}" w:lineRule="exact"/></w:pPr><w:r><w:t>P{i:03}</w:t></w:r></w:p>"#
            )
        })
        .collect();
    let sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let styles: String = ['A', 'B']
        .iter()
        .map(|x| format!(r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="Probe{x}"><w:name w:val="Probe{x}"/></w:style>"#))
        .collect();
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{paras}{sect}</w:body></w:document>"#))
        .unwrap();
    let styles_part = package.find_name("word/styles.xml").expect("空白模板带 styles.xml");
    package.replace_part_xml(styles_part, &format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#)).unwrap();
    if !styles_related {
        let rels = package.find_name("word/_rels/document.xml.rels").expect("主文档有关系 part");
        let xml = String::from_utf8(package.read_bytes(rels).unwrap()).unwrap();
        let kept = drop_styles_relationship(&xml);
        assert_ne!(kept, xml, "前提：去掉了样式关系");
        package.replace_part_bytes(rels, kept.into_bytes());
    }
    package.save().unwrap()
}

/// 去掉 `Type` 以 `/styles"` 结尾的那条 `<Relationship .../>`。
fn drop_styles_relationship(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<Relationship ") {
        let end = at + rest[at..].find("/>").unwrap() + 2;
        out.push_str(&rest[..at]);
        if !rest[at..end].contains("/relationships/styles\"") {
            out.push_str(&rest[at..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn first_page(specs: &[Spec]) -> usize {
    first_page_of(&docx(specs))
}

fn first_page_of(bytes: &[u8]) -> usize {
    let prepared = PreparedDocument::load(bytes).unwrap();
    let options = LayoutOptions { platform: Platform::Android, view: View::Print, wrap: WrapPolicy::None };
    LayoutRecord::from_paint(&prepared.layout_approximate(&options).unwrap().paint()).pages[0].lines.len()
}

const N: usize = 70;

#[test]
fn a_paragraph_drops_its_own_spacing_next_to_the_same_style() {
    assert_eq!(first_page(&[("A", true, 0, 240); N]), 32);
    assert_eq!(first_page(&[("A", true, 240, 0); N]), 31, "第一段的段前在页顶照算");
}

#[test]
fn a_neighbors_switch_does_not_drop_this_paragraphs_spacing() {
    let next: Vec<Spec> = (0..N).map(|i| ("A", i % 2 == 1, 0, if i % 2 == 0 { 240 } else { 0 })).collect();
    assert_eq!(first_page(&next), 25);
    let prev: Vec<Spec> = (0..N).map(|i| ("A", i % 2 == 0, if i % 2 == 1 { 240 } else { 0 }, 0)).collect();
    assert_eq!(first_page(&prev), 25);
}

#[test]
fn different_styles_keep_their_spacing() {
    let alternating: Vec<Spec> = (0..N).map(|i| (if i % 2 == 0 { "A" } else { "B" }, true, 0, 240)).collect();
    assert_eq!(first_page(&alternating), 21);
}

#[test]
fn a_dropped_space_after_still_offsets_the_next_space_before() {
    let both_phase: Vec<Spec> = (0..N).map(|i| ("A", i % 2 == 0, 300, 100)).collect();
    assert_eq!(first_page_of(&docx_with(&both_phase, 400, true)), 27);
}

#[test]
fn undefined_styles_compare_as_the_default_style() {
    // ProbeX / ProbeY 都没定义：两段都是缺省样式，照同样式去段后。
    let undefined: Vec<Spec> = (0..N).map(|i| (if i % 2 == 0 { "X" } else { "Y" }, true, 0, 240)).collect();
    assert_eq!(first_page(&undefined), 32);
}

#[test]
fn a_styles_part_outside_the_document_relationships_is_not_loaded() {
    let alternating: Vec<Spec> = (0..N).map(|i| (if i % 2 == 0 { "A" } else { "B" }, true, 0, 240)).collect();
    assert_eq!(first_page_of(&docx_with(&alternating, 480, true)), 21);
    assert_eq!(first_page_of(&docx_with(&alternating, 480, false)), 32, "ProbeA / ProbeB 都没加载");
}
