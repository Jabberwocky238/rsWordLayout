//! DOCX style resolution reaches the common formatter without rewriting declared JSON.
//! These are projection regressions against the pinned parser's Resolver semantics;
//! they do not claim new Word measurements for Android toggle inheritance.

use rsword::package::{Package, PartUri};
use rsword_layout_core::{
    Align, Caps, Engine, Fragment, LineRule, PageSetup, SimpleMetrics, TabAlign, load_document,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn package(body: &str, styles: &str) -> Package {
    let bytes = rsword::save::blank_docx(None).unwrap();
    let mut package = Package::open(&bytes).unwrap();
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

fn load(body: &str, styles: &str) -> rsword_layout_core::LoadedDocument {
    load_document(&package(body, styles).save().unwrap()).unwrap()
}

#[test]
fn doc_defaults_supply_run_and_paragraph_metrics() {
    let loaded = load(
        r#"<w:p><w:r><w:t>body</w:t></w:r></w:p>"#,
        r#"<w:docDefaults>
            <w:rPrDefault><w:rPr><w:rFonts w:ascii="Default Latin" w:hAnsi="Default Latin" w:eastAsia="Default CJK"/><w:sz w:val="30"/><w:spacing w:val="17"/><w:w w:val="90"/></w:rPr></w:rPrDefault>
            <w:pPrDefault><w:pPr><w:spacing w:before="37" w:after="91" w:line="350" w:lineRule="exact"/><w:ind w:left="140" w:right="80" w:firstLine="30"/><w:jc w:val="center"/><w:keepLines/><w:overflowPunct w:val="0"/></w:pPr></w:pPrDefault>
        </w:docDefaults>"#,
    );
    let document = loaded.layout_document();
    let paras = document.paras;
    assert_eq!(document.skipped_blocks, 0);
    let para = &paras[0];
    let font = &para.runs[0].font;
    assert_eq!(font.family, "Default Latin");
    assert_eq!(font.slots.east_asia.as_deref(), Some("Default CJK"));
    assert_eq!(font.size_half_points, 30);
    assert_eq!(font.letter_spacing, 17);
    assert_eq!(font.scale_pct, 90);
    assert_eq!(para.align, Align::Center);
    assert_eq!(
        (para.indent_left, para.indent_right, para.indent_first_line),
        (140, 80, 30)
    );
    assert_eq!(
        (para.space_before, para.space_after, para.line_value),
        (37, 91, 350)
    );
    assert_eq!(para.line_rule, LineRule::Exact);
    assert!(para.keep_lines);
    assert!(!para.overflow_punct);
    assert!(loaded.json["main"][0]["props"]["spacing"].is_null());
    assert!(loaded.json["main"][0]["inlines"][0]["props"]["size"].is_null());
}

#[test]
fn default_paragraph_style_is_selected_without_pstyle() {
    let loaded = load(
        r#"<w:p><w:r><w:t>default</w:t></w:r></w:p>"#,
        r#"<w:style w:type="paragraph" w:default="1" w:styleId="BodyCustom"><w:name w:val="Body"/><w:pPr><w:spacing w:after="63"/></w:pPr><w:rPr><w:rFonts w:ascii="Chosen Default"/><w:sz w:val="34"/><w:vanish/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    assert!(!paras.is_empty(), "{}", loaded.json["main"]);
    assert_eq!(paras[0].space_after, 63);
    assert_eq!(paras[0].runs[0].font.size_half_points, 34);
    assert_eq!(paras[0].runs[0].font.family, "Chosen Default");
    assert!(paras[0].runs[0].hidden);
}

#[test]
fn paragraph_chain_merges_components_and_direct_false_overrides() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Leaf"/><w:keepNext w:val="0"/><w:keepLines w:val="0"/><w:pageBreakBefore w:val="0"/><w:spacing w:after="0"/><w:ind w:right="75"/></w:pPr><w:r><w:t>body</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:line="321" w:lineRule="atLeast"/><w:ind w:left="100"/></w:pPr></w:pPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:styleId="Base"><w:name w:val="Base"/><w:pPr><w:keepNext/><w:keepLines/><w:pageBreakBefore/><w:spacing w:before="220" w:after="160"/><w:ind w:hanging="40"/></w:pPr><w:rPr><w:sz w:val="36"/><w:b/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="Leaf"><w:name w:val="Leaf"/><w:basedOn w:val="Base"/><w:pPr><w:jc w:val="right"/></w:pPr><w:rPr><w:i/></w:rPr></w:style>"#,
    );
    let doc = loaded.layout_document();
    let para = &doc.paras[0];
    assert!(!para.keep_next);
    assert!(!para.keep_lines);
    assert!(!para.page_break_before);
    assert_eq!(para.align, Align::Right);
    assert_eq!((para.space_before, para.space_after), (220, 0));
    assert_eq!((para.line_rule, para.line_value), (LineRule::AtLeast, 321));
    assert_eq!(
        (para.indent_left, para.indent_right, para.indent_first_line),
        (100, 75, -40)
    );
    assert_eq!(para.runs[0].font.size_half_points, 36);
    assert!(para.runs[0].font.bold);
    assert!(para.runs[0].font.italic);
}

#[test]
fn explicit_heading_id_uses_its_real_style_without_name_heuristics() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>small</w:t></w:r></w:p>"#,
        r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:rPr><w:sz w:val="18"/><w:b w:val="0"/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    assert_eq!(paras[0].runs[0].font.size_half_points, 18);
    assert!(!paras[0].runs[0].font.bold);
    assert!(!paras[0].keep_next);
    assert_eq!((paras[0].space_before, paras[0].space_after), (0, 0));
}

#[test]
fn character_chain_inherits_and_direct_false_disables_toggle_properties() {
    let loaded = load(
        r#"<w:p><w:r><w:rPr><w:rStyle w:val="CharLeaf"/></w:rPr><w:t>hidden</w:t></w:r><w:r><w:rPr><w:rStyle w:val="CharLeaf"/><w:vanish w:val="0"/><w:b w:val="0"/><w:caps w:val="0"/><w:i w:val="0"/></w:rPr><w:t>visible</w:t></w:r></w:p>"#,
        r#"<w:style w:type="character" w:styleId="CharBase"><w:name w:val="Base"/><w:rPr><w:vanish/><w:b/><w:caps/><w:sz w:val="28"/></w:rPr></w:style>
        <w:style w:type="character" w:styleId="CharLeaf"><w:name w:val="Leaf"/><w:basedOn w:val="CharBase"/><w:rPr><w:i/><w:spacing w:val="19"/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    let inherited = &paras[0].runs[0];
    assert!(inherited.hidden);
    assert!(inherited.font.bold);
    assert!(inherited.font.italic);
    assert_eq!(inherited.font.caps, Caps::All);
    let explicit = &paras[0].runs[1];
    assert!(!explicit.hidden);
    assert!(!explicit.font.bold);
    assert!(!explicit.font.italic);
    assert_eq!(explicit.font.caps, Caps::None);
    assert_eq!(
        (explicit.font.size_half_points, explicit.font.letter_spacing),
        (28, 19)
    );
}

#[test]
fn pinned_resolver_combines_paragraph_and_character_toggle_layers() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="ParaHidden"/></w:pPr><w:r><w:rPr><w:rStyle w:val="CharHidden"/></w:rPr><w:t>visible</w:t></w:r></w:p>"#,
        r#"<w:style w:type="paragraph" w:styleId="ParaHidden"><w:name w:val="P"/><w:rPr><w:vanish/><w:b/></w:rPr></w:style>
        <w:style w:type="character" w:styleId="CharHidden"><w:name w:val="C"/><w:rPr><w:vanish/><w:b/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    assert!(!paras[0].runs[0].hidden);
    assert!(!paras[0].runs[0].font.bold);
}

#[test]
fn empty_paragraph_uses_effective_paragraph_mark_font() {
    let loaded = load(
        r#"<w:p><w:pPr><w:rPr><w:sz w:val="26"/><w:b w:val="0"/></w:rPr></w:pPr></w:p>"#,
        r#"<w:style w:type="paragraph" w:default="1" w:styleId="Body"><w:name w:val="Body"/><w:rPr><w:rFonts w:ascii="Mark Font"/><w:sz w:val="40"/><w:b/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    assert_eq!(paras[0].runs.len(), 1);
    let mark = &paras[0].runs[0];
    assert!(mark.text.is_empty());
    assert_eq!(mark.font.family, "Mark Font");
    assert_eq!(mark.font.size_half_points, 26);
    assert!(!mark.font.bold);
}

#[test]
fn inherited_hidden_text_retains_source_offsets_through_layout() {
    let loaded = load(
        r#"<w:p><w:r><w:t>A</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Hidden"/></w:rPr><w:t>🦀X</w:t></w:r><w:r><w:t>B</w:t></w:r></w:p>"#,
        r#"<w:style w:type="character" w:styleId="Hidden"><w:name w:val="Hidden"/><w:rPr><w:vanish/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout(&paras);
    let runs = pages
        .iter()
        .flat_map(|p| &p.fragments)
        .filter_map(|f| match f {
            Fragment::Text(run) if !run.text.is_empty() => Some(run),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        runs.iter()
            .all(|run| !run.text.contains('X') && !run.text.contains('🦀'))
    );
    let b = runs.iter().find(|run| run.text.starts_with('B')).unwrap();
    assert_eq!(b.source.unwrap().0, 4);
}

#[test]
fn tabs_merge_doc_defaults_styles_and_direct_clears_by_position() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Leaf"/><w:tabs><w:tab w:val="clear" w:pos="100"/><w:tab w:val="decimal" w:pos="300"/></w:tabs></w:pPr><w:r><w:t>a</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:tabs><w:tab w:val="left" w:pos="100"/><w:tab w:val="left" w:pos="200"/></w:tabs></w:pPr></w:pPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:styleId="Base"><w:name w:val="Base"/><w:pPr><w:tabs><w:tab w:val="center" w:pos="200"/><w:tab w:val="left" w:pos="400"/></w:tabs></w:pPr></w:style>
        <w:style w:type="paragraph" w:styleId="Leaf"><w:name w:val="Leaf"/><w:basedOn w:val="Base"/><w:pPr><w:tabs><w:tab w:val="right" w:pos="400"/></w:tabs></w:pPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    assert_eq!(
        paras[0]
            .tabs
            .iter()
            .map(|t| (t.pos, t.align))
            .collect::<Vec<_>>(),
        vec![
            (200, TabAlign::Center),
            (300, TabAlign::Decimal),
            (400, TabAlign::Right)
        ]
    );
}

#[test]
fn sibling_rpr_repair_rebuilds_effective_nodes_for_the_repaired_model() {
    let loaded = load(
        r#"<w:p><w:r><w:rPr><w:rStyle w:val="Hidden"/><w:spacing w:val="20"/></w:rPr><w:rPr><w:vanish w:val="0"/></w:rPr><w:t>A</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Hidden"/></w:rPr><w:t>B</w:t></w:r></w:p>"#,
        r#"<w:style w:type="character" w:styleId="Hidden"><w:name w:val="Hidden"/><w:rPr><w:sz w:val="38"/><w:vanish/></w:rPr></w:style>"#,
    );
    assert_eq!(loaded.merged_run_props, 1);
    assert!(loaded.merge_error.is_none());
    let (paras, _) = loaded.paragraphs();
    assert!(!paras[0].runs[0].hidden);
    assert_eq!(paras[0].runs[0].font.letter_spacing, 20);
    assert_eq!(paras[0].runs[0].font.size_half_points, 38);
    assert!(paras[0].runs[1].hidden);
    assert_eq!(paras[0].runs[1].font.size_half_points, 38);
}

#[test]
fn theme_font_references_reach_concrete_layout_slots() {
    let mut package = package(
        r#"<w:p><w:r><w:t>body</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Ignored Literal" w:asciiTheme="minorHAnsi" w:hAnsiTheme="majorHAnsi" w:eastAsiaTheme="minorEastAsia"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>"#,
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
    let (paras, _) = loaded.paragraphs();
    let slots = &paras[0].runs[0].font.slots;
    assert_eq!(slots.ascii.as_deref(), Some("Theme Minor"));
    assert_eq!(slots.h_ansi.as_deref(), Some("Theme Major"));
    assert_eq!(slots.east_asia.as_deref(), Some("Minor CJK"));
}

#[test]
fn layout_document_keeps_inherited_page_break_before() {
    let loaded = load(
        r#"<w:p><w:r><w:t>first</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="NewPage"/></w:pPr><w:r><w:t>second</w:t></w:r></w:p>"#,
        r#"<w:style w:type="paragraph" w:styleId="NewPage"><w:name w:val="New page"/><w:pPr><w:pageBreakBefore/></w:pPr></w:style>"#,
    );
    let document = loaded.layout_document();
    assert!(!document.paras[0].page_break_before);
    assert!(document.paras[1].page_break_before);
}

#[test]
fn style_hidden_whole_paragraph_keeps_its_utf16_length_and_section_membership() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Hidden"/></w:pPr><w:r><w:t>🦀X</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p>"#,
        r#"<w:style w:type="paragraph" w:styleId="Hidden"><w:name w:val="Hidden"/><w:rPr><w:vanish/></w:rPr></w:style>"#,
    );
    assert_eq!(loaded.json["main"][0]["protectedKind"]["kind"], "invisible");
    let document = loaded.layout_document();
    assert_eq!(document.skipped_blocks, 0);
    assert_eq!(document.paras.len(), 2);
    assert_eq!(document.sections[0].para_range, 0..2);
    assert!(document.paras[0].runs[0].hidden);
    let pages = Engine::new(&SimpleMetrics, PageSetup::a4()).layout_document(&document);
    let runs = pages
        .iter()
        .flat_map(|p| &p.fragments)
        .filter_map(|f| match f {
            Fragment::Text(run) => Some(run),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        runs.iter()
            .all(|run| !run.text.contains('X') && !run.text.contains('🦀'))
    );
    let b = runs.iter().find(|run| run.text.starts_with('B')).unwrap();
    assert_eq!(b.source.unwrap().0, 4);
}

#[test]
fn default_style_choice_and_tabs_follow_the_same_resolver_chain() {
    for (styles, expected_size, expected_stop) in [
        (
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Old"><w:name w:val="Old"/><w:pPr><w:tabs><w:tab w:val="left" w:pos="100"/></w:tabs></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
            <w:style w:type="paragraph" w:default="1" w:styleId="New"><w:name w:val="New"/><w:pPr><w:tabs><w:tab w:val="left" w:pos="200"/></w:tabs></w:pPr><w:rPr><w:sz w:val="30"/></w:rPr></w:style>"#,
            30,
            200,
        ),
        (
            r#"<w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="left" w:pos="300"/></w:tabs></w:pPr><w:rPr><w:sz w:val="40"/></w:rPr></w:style>"#,
            40,
            300,
        ),
    ] {
        let loaded = load(r#"<w:p><w:r><w:t>x</w:t></w:r></w:p>"#, styles);
        let (paras, _) = loaded.paragraphs();
        assert_eq!(paras[0].runs[0].font.size_half_points, expected_size);
        assert_eq!(paras[0].tabs.len(), 1);
        assert_eq!(paras[0].tabs[0].pos, expected_stop);
    }
}

#[test]
fn font_slots_inherit_independently_across_defaults_styles_and_direct_props() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Body"/></w:pPr><w:r><w:rPr><w:rFonts w:hAnsi="Direct Latin"/></w:rPr><w:t>x</w:t></w:r></w:p>"#,
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Default Ascii" w:eastAsia="Default CJK"/></w:rPr></w:rPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:styleId="Body"><w:name w:val="Body"/><w:rPr><w:rFonts w:eastAsia="Styled CJK" w:cs="Styled CS"/></w:rPr></w:style>"#,
    );
    let (paras, _) = loaded.paragraphs();
    let slots = &paras[0].runs[0].font.slots;
    assert_eq!(slots.ascii.as_deref(), Some("Default Ascii"));
    assert_eq!(slots.h_ansi.as_deref(), Some("Direct Latin"));
    assert_eq!(slots.east_asia.as_deref(), Some("Styled CJK"));
    assert_eq!(slots.cs.as_deref(), Some("Styled CS"));
}

#[test]
fn absent_heading_style_does_not_invent_heading_metrics() {
    let loaded = load(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#,
        "",
    );
    let (paras, _) = loaded.paragraphs();
    assert_eq!(paras[0].runs[0].font.size_half_points, 24);
    assert!(!paras[0].runs[0].font.bold);
    assert!(!paras[0].keep_next);
    assert_eq!((paras[0].space_before, paras[0].space_after), (0, 0));
}
