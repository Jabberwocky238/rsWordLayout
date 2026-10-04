//! 文档会话：各入口共用的装载 → 排版 → 绘制（R02）。
//!
//! 全部走真实 DOCX 字节与公开 API。与手拼的 `Engine` / `paint_document` 比，是为了钉住
//! 「会话就是原来那条路」：入口换成会话之后，同一份输入排出来逐字节相同。

use rsword::package::Package;
use rsword_layout_core::{
    DiagnosticCode, DocumentSession, Engine, LayoutOptions, LayoutRecord, PaintList, Platform,
    PreparedDocument, SessionErrorKind, SimpleMetrics, TraceMeta, View, WrapPolicy, paint_document,
    to_trace_json,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx(body: &str) -> Vec<u8> {
    docx_with(body, &[])
}

/// `parts`：整个替换掉的部件，如 `("word/settings.xml", "<w:settings …/>")`。
fn docx_with(body: &str, parts: &[(&str, &str)]) -> Vec<u8> {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    for (name, xml) in parts {
        let part = package.find_name(name).unwrap();
        package.replace_part_xml(part, xml).unwrap();
    }
    package.save().unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn para(text: &str) -> String {
    format!(r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

/// 规范化的布局结果：行、源区间、字形与字体指纹全在里面。
fn trace(list: &PaintList, fingerprint: Option<&str>) -> String {
    let meta = TraceMeta {
        engine: "test".into(),
        metrics: "test".into(),
        glyph_origin_method: "test".into(),
        source: "test".into(),
        font_fingerprint: fingerprint.map(str::to_string),
    };
    to_trace_json(&LayoutRecord::from_paint(list), &meta)
}

fn session_trace(session: &DocumentSession) -> String {
    trace(&session.paint(), session.font_fingerprint())
}

fn codes(session: &DocumentSession) -> Vec<DiagnosticCode> {
    session.diagnostics().iter().map(|d| d.code).collect()
}

fn approximate(bytes: &[u8], options: LayoutOptions) -> DocumentSession {
    PreparedDocument::load(bytes).unwrap().layout_approximate(&options).unwrap()
}

#[test]
fn approximate_session_is_the_engine_path_and_says_so() {
    let bytes = docx(&format!("{}{}", para("alpha beta"), para("gamma")));
    let session = approximate(&bytes, LayoutOptions::default());

    let document = PreparedDocument::load(&bytes).unwrap().document().clone();
    let pages = Engine::new(&SimpleMetrics, document.sections[0].setup).layout_document(&document);
    assert_eq!(session_trace(&session), trace(&paint_document(&pages, None, &[]), None));

    assert!(session.is_approximate());
    assert!(session.faces().is_empty());
    assert_eq!(session.font_fingerprint(), None);
    assert!(!session.paint().pages[0].cmds.is_empty());
    // 近似模式必须自己说出来，不能冒充精确结果。
    assert_eq!(session.layout_diagnostics().len(), 1);
    assert_eq!(session.layout_diagnostics()[0].code, DiagnosticCode::MetricsApproximate);
}

#[test]
fn platform_and_view_reach_the_engine() {
    // 两处各看一个开关：
    // - 没写 `w:defaultTabStop` 时缺省制表位按平台补（桌面 720，Android 221），看平台；
    // - 段末手动分页符：移动视图把段落标记另起一行，分页视图收进分页符那一行，看视图。
    let body = concat!(
        r#"<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>one</w:t><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>"#,
    );
    // 空白模板没有 settings.xml，也就没有 `w:defaultTabStop`。
    let bytes = docx(body);
    let with = |platform, view| LayoutOptions { platform, view, wrap: WrapPolicy::None };
    let options = with(Platform::Android, View::Mobile);
    let android_mobile = approximate(&bytes, options);

    let document = PreparedDocument::load(&bytes).unwrap().document().clone();
    assert_eq!(document.paras[0].default_tab_stop, None, "缺省制表位要由平台补");
    let pages = Engine::new(&SimpleMetrics, document.sections[0].setup)
        .with_platform(Platform::Android, View::Mobile)
        .layout_document(&document);
    // 近似模式的轨迹没有字形，制表位的横向位置只在页的片段里：两样都比。
    assert_eq!(session_trace(&android_mobile), trace(&paint_document(&pages, None, &[]), None));
    assert_eq!(format!("{:?}", android_mobile.pages()), format!("{pages:?}"));
    let desktop_mobile = approximate(&bytes, with(Platform::Desktop, View::Mobile));
    let android_print = approximate(&bytes, with(Platform::Android, View::Print));
    assert_ne!(format!("{:?}", android_mobile.pages()), format!("{:?}", desktop_mobile.pages()), "平台没传到引擎");
    assert_ne!(session_trace(&android_mobile), session_trace(&android_print), "视图没传到引擎");
    assert_eq!(*android_mobile.options(), options);
}

#[test]
fn layout_diagnostics_follow_the_input_diagnostics_exactly_once() {
    // CLI 先打装载诊断、排完再打排版诊断：两段必须拼得回全体，且不重叠。
    for (bytes, layout_code) in [
        (docx(TABLE), DiagnosticCode::MetricsApproximate),
        (fixture("wrap.docx"), DiagnosticCode::WrapApproximate),
    ] {
        let prepared = PreparedDocument::load(&bytes).unwrap();
        let input = prepared.diagnostics();
        let options = LayoutOptions { wrap: WrapPolicy::Anchors, ..LayoutOptions::default() };
        let session = prepared.layout_approximate(&options).unwrap();
        let joined: Vec<_> = input.iter().chain(session.layout_diagnostics()).cloned().collect();
        assert_eq!(session.diagnostics(), &joined[..]);
        assert!(session.layout_diagnostics().iter().all(|d| d.code != DiagnosticCode::DocumentInput));
        assert!(session.layout_diagnostics().iter().any(|d| d.code == layout_code), "{:?}", session.layout_diagnostics());
    }
    assert!(PreparedDocument::load(&docx(TABLE)).unwrap().diagnostics().iter()
        .any(|d| d.code == DiagnosticCode::DocumentInput), "表后缺尾段是装载阶段的诊断");
}

const TABLE: &str = concat!(
    r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="nil"/>"#,
    r#"<w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/>"#,
    r#"<w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/>"#,
    r#"<w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/>"#,
    r#"</w:tblCellMar></w:tblPr>"#,
    r#"<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>cell one</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc><w:p><w:r><w:t>cell two</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"</w:tbl>"#,
);

#[test]
fn layout_json_carries_mode_tables_and_diagnostics() {
    let bytes = docx(TABLE);
    let session = approximate(&bytes, LayoutOptions::default());
    let value: serde_json::Value = serde_json::from_str(&session.layout_json()).unwrap();
    assert_eq!(value["schema"], "rsword-layout-result/1");
    assert_eq!(value["metrics"], "approximate");
    assert_eq!(value["verticalGrid"], serde_json::Value::Null);
    assert_eq!(value["tableLayout"], session.table_layout());
    assert_eq!(value["layoutInput"], session.document().trace_metadata());
    assert_eq!(value["diagnostics"].as_array().unwrap().len(), session.diagnostics().len());
    assert_eq!(value["pages"][0]["lines"].as_array().unwrap().len(), 3, "两行表格，加排版补的尾段");
    // 同输入再建一次，逐字节相同。
    assert_eq!(approximate(&bytes, LayoutOptions::default()).layout_json(), session.layout_json());
}

#[test]
fn a_table_without_a_tail_paragraph_is_laid_out_and_still_reported() {
    // Word 要求表后有一个段落；缺了照样排表，诊断照留。源文档的段落不改，
    // 排版时接上 Word 补的缺省空段（`LayoutDocument::implied_final_para`）。
    let bytes = docx(TABLE);
    let prepared = PreparedDocument::load(&bytes).unwrap();
    assert!(prepared.document().paras.is_empty());
    assert!(prepared.has_layout_content(), "只看 paras 会把只有表格的文档当成空的");

    assert!(prepared.document().implied_final_para.as_ref().is_some_and(|para| para.source_node.is_none()));

    let session = prepared.layout_approximate(&LayoutOptions::default()).unwrap();
    assert_eq!(session.pages()[0].table_rows.len(), 2);
    let record = LayoutRecord::from_paint(&session.paint());
    let tail = record.pages[0].lines.last().unwrap();
    let source = tail.source.as_ref().unwrap();
    assert_eq!((source.start, source.end), (20, 21), "尾段接在表格的 20 个源单位之后");
    assert!(session.diagnostics().iter().any(|d| d.code == DiagnosticCode::DocumentInput
        && d.message.contains("table has no following paragraph")), "{:?}", session.diagnostics());
}

#[test]
fn nothing_layable_is_an_error_that_keeps_its_diagnostics() {
    // 一行两格的表格本版不支持：没排，也没有别的内容。
    let unsupported = TABLE.replace(
        "<w:tc><w:p><w:r><w:t>cell one</w:t></w:r></w:p></w:tc>",
        "<w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc>",
    );
    let bytes = docx(&unsupported);
    let prepared = PreparedDocument::load(&bytes).unwrap();
    assert!(!prepared.has_layout_content());
    let error = prepared.layout_approximate(&LayoutOptions::default()).err().unwrap();
    assert_eq!(error.kind, SessionErrorKind::NoLayoutContent);
    assert_eq!(error.to_string(), "没有可排版的段落");
    let codes: Vec<_> = error.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&DiagnosticCode::BlocksSkipped), "{:?}", error.diagnostics);
    assert!(error.diagnostics.iter().any(|d| d.message.contains("table not laid out")));
}

#[test]
fn anchors_wrap_only_when_asked() {
    let bytes = fixture("wrap.docx");
    let plain = approximate(&bytes, LayoutOptions::default());
    assert!(plain.anchors().is_none());
    assert!(!codes(&plain).contains(&DiagnosticCode::WrapApproximate));

    let options = LayoutOptions { wrap: WrapPolicy::Anchors, ..LayoutOptions::default() };
    let wrapped = approximate(&bytes, options);
    let anchors = wrapped.anchors().unwrap();
    assert_eq!(anchors.regions, 1);
    assert!(codes(&wrapped).contains(&DiagnosticCode::WrapApproximate), "环绕是近似定位的，要说出来");
    assert_ne!(session_trace(&plain), session_trace(&wrapped), "环绕区改变了行的可用宽度");
}

#[test]
fn page_overrides_apply_before_layout() {
    let bytes = docx(&para(&"word ".repeat(80)));
    let mut prepared = PreparedDocument::load(&bytes).unwrap();
    prepared
        .apply_page_overrides(rsword_layout_core::PageOverrides {
            content_width: Some(5329),
            ..Default::default()
        })
        .unwrap();
    let narrow = prepared.layout_approximate(&LayoutOptions::default()).unwrap();
    assert_eq!(narrow.document().sections[0].setup.content_area().width, 5329);
    assert_eq!(narrow.pages()[0].content_area.width, 5329);
}

#[cfg(feature = "fontenv")]
mod fonts {
    use super::*;
    use rsword_layout_core::font::FontRegistry;
    use rsword_layout_core::{RealMetrics, TextShaper, VerticalGrid};

    fn font(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../fixtures/fonts/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn sans_with_droid() -> FontRegistry {
        let mut fonts = FontRegistry::new();
        fonts.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        fonts.add_fallback(font("DroidSansFallbackFull.ttf"), 0).unwrap();
        fonts
    }

    fn run(text: &str, fonts: &str) -> String {
        format!(r#"<w:r><w:rPr><w:rFonts {fonts}/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
    }

    #[test]
    fn real_session_is_the_hand_built_pipeline() {
        let bytes = fixture("cjk-plain.docx");
        let options = LayoutOptions { platform: Platform::Android, view: View::Print, wrap: WrapPolicy::None };
        let grid = VerticalGrid::MacWordThreeHundredthsInch;
        let session = PreparedDocument::load(&bytes)
            .unwrap()
            .layout_with_fonts(sans_with_droid(), grid, &options)
            .unwrap();

        // 原来 layout-trace 的装法：同一个注册表量宽、整形、出字形记录。
        let registry = sans_with_droid();
        let document = PreparedDocument::load(&bytes).unwrap().document().clone();
        let real = RealMetrics::new(&registry)
            .with_vertical_grid(grid)
            .with_east_asian_line_scale(true);
        let pages = Engine::new(&real, document.sections[0].setup)
            .with_platform(Platform::Android, View::Print)
            .layout_document(&document);
        let shaper: &dyn TextShaper = &registry;
        let expected = trace(&paint_document(&pages, Some(shaper), &registry.face_ids()), registry.fingerprint());
        assert_eq!(session_trace(&session), expected);

        assert!(!session.is_approximate());
        assert_eq!(session.faces(), registry.face_ids());
        assert_eq!(session.font_fingerprint(), registry.fingerprint());
        assert_eq!(session.vertical_grid(), Some(grid));
        assert_eq!(session.fonts().unwrap().fallback_faces(), registry.fallback_faces());
        assert!(!codes(&session).contains(&DiagnosticCode::MetricsApproximate));
    }

    #[test]
    fn pages_paint_the_same_one_at_a_time() {
        let session = PreparedDocument::load(&fixture("cjk-plain.docx"))
            .unwrap()
            .layout_with_fonts(sans_with_droid(), VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        let whole = session.paint();
        let pages = PaintList { pages: (0..session.pages().len()).map(|i| session.paint_page(i).unwrap()).collect() };
        assert_eq!(trace(&pages, None), trace(&whole, None));
        assert!(session.paint_page(session.pages().len()).is_none());
    }

    #[test]
    fn same_fonts_tells_roles_apart_where_the_fingerprint_cannot() {
        let session = PreparedDocument::load(&fixture("cjk-plain.docx"))
            .unwrap()
            .layout_with_fonts(sans_with_droid(), VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        assert!(session.same_fonts(&sans_with_droid()));
        // 同样两份字体，Droid 当正文字体：指纹相同，角色不同。
        let mut primaries = FontRegistry::new();
        primaries.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        primaries.add(font("DroidSansFallbackFull.ttf"), 0).unwrap();
        assert_eq!(primaries.fingerprint(), session.font_fingerprint());
        assert!(!session.same_fonts(&primaries));
        let approximate = approximate(&fixture("cjk-plain.docx"), LayoutOptions::default());
        assert!(!approximate.same_fonts(&sans_with_droid()));
    }

    #[test]
    fn font_sources_replay_an_identical_registry() {
        let mut sources = rsword_layout_core::FontSources::new();
        sources.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        sources.add_fallback(font("DroidSansFallbackFull.ttf"), 0).unwrap();
        sources.add(font("DejaVuSans.ttf"), 0).unwrap();
        let built = sources.build();
        assert_eq!(built.face_ids(), sources.registry().face_ids());
        assert_eq!(built.fallback_faces(), sources.registry().fallback_faces());
        assert_eq!(built.fingerprint(), sources.registry().fingerprint());
        let session = PreparedDocument::load(&fixture("cjk-plain.docx"))
            .unwrap()
            .layout_with_fonts(built, VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        assert!(session.same_fonts(sources.registry()));
        assert_eq!(sources.add(b"garbage".to_vec(), 0), Err("FONT_INVALID"));
        assert!(session.same_fonts(sources.registry()), "装不进去的字体不改字体集");
    }

    #[test]
    fn painted_glyphs_come_from_the_session_fonts() {
        let session = PreparedDocument::load(&fixture("cjk-plain.docx"))
            .unwrap()
            .layout_with_fonts(sans_with_droid(), VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        let record = LayoutRecord::from_paint(&session.paint());
        assert!(record.glyph_count() > 0);
        let faces = session.faces();
        for glyph in record.pages.iter().flat_map(|p| &p.lines).flat_map(|l| &l.glyphs) {
            assert!(faces.contains(&glyph.face), "字形的 face 不在会话的字体里：{}", glyph.face);
            assert!(session.fonts().unwrap().face_data(&glyph.face).is_some());
        }
    }

    #[test]
    fn fallback_fonts_alone_are_rejected() {
        let mut fonts = FontRegistry::new();
        fonts.add_fallback(font("DroidSansFallbackFull.ttf"), 0).unwrap();
        let error = PreparedDocument::load(&docx(&para("alpha")))
            .unwrap()
            .layout_with_fonts(fonts, VerticalGrid::None, &LayoutOptions::default())
            .err()
            .unwrap();
        assert_eq!(error.kind, SessionErrorKind::NoPrimaryFont);
    }

    #[test]
    fn missing_glyphs_and_substituted_families_are_reported() {
        // 只装 Liberation Sans：汉字谁都画不出（名义 .notdef），泰文也画不出（跳过）；
        // eastAsia 槽点名的 SimSun 没装。
        let latin = r#"w:ascii="Liberation Sans" w:hAnsi="Liberation Sans" w:cs="Liberation Sans""#;
        let body = format!(
            "<w:p>{}{}</w:p>",
            run("Hello ", latin),
            run("汉字 ก", &format!(r#"{latin} w:eastAsia="SimSun""#)),
        );
        let mut fonts = FontRegistry::new();
        fonts.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        let session = PreparedDocument::load(&docx(&body))
            .unwrap()
            .layout_with_fonts(fonts, VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        let find = |code| session.layout_diagnostics().iter().find(|d| d.code == code);

        let nominal = find(DiagnosticCode::GlyphNominal).expect("汉字按名义画");
        assert!(nominal.message.starts_with("2 个 CJK 字符（2 种）"), "{}", nominal.message);
        // 与画出来的 .notdef 个数同一口径（这份文档没有大小写变换）。
        assert_eq!(LayoutRecord::from_paint(&session.paint()).notdef_glyph_count(), 2);

        let dropped = find(DiagnosticCode::GlyphDropped).expect("泰文没画");
        assert!(dropped.message.starts_with("1 个字符（1 种）"), "{}", dropped.message);
        assert!(dropped.message.contains("U+0E01"), "{}", dropped.message);

        let family = find(DiagnosticCode::FontFamilySubstituted).expect("SimSun 没装");
        assert!(family.message.ends_with("「SimSun」"), "{}", family.message);
    }

    #[test]
    fn coverage_counts_table_cells_and_skips_hidden_runs() {
        // 单元格里的汉字画了、要数；隐藏的没画、不数。与画出的 .notdef 个数同一口径。
        let latin = r#"w:ascii="Liberation Sans" w:hAnsi="Liberation Sans" w:eastAsia="Liberation Sans""#;
        let cell = TABLE.replace("<w:t>cell one</w:t>", "<w:t>表格</w:t>");
        let hidden = format!(
            r#"<w:r><w:rPr><w:rFonts {latin}/><w:vanish/></w:rPr><w:t>隐藏</w:t></w:r>"#
        );
        let body = format!("{cell}<w:p>{}{hidden}</w:p>", run("正文", latin));
        let mut fonts = FontRegistry::new();
        fonts.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        let session = PreparedDocument::load(&docx(&body))
            .unwrap()
            .layout_with_fonts(fonts, VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        let nominal = session.layout_diagnostics().iter()
            .find(|d| d.code == DiagnosticCode::GlyphNominal).expect("汉字按名义画");
        assert!(nominal.message.starts_with("4 个 CJK 字符（4 种）"), "{}", nominal.message);
        assert_eq!(LayoutRecord::from_paint(&session.paint()).notdef_glyph_count(), 4);
    }

    #[test]
    fn a_missing_bold_face_is_reported_and_an_installed_one_is_not() {
        let serif = r#"w:ascii="Liberation Serif" w:hAnsi="Liberation Serif""#;
        let body = format!(r#"<w:p><w:r><w:rPr><w:rFonts {serif}/><w:b/></w:rPr><w:t>Bold</w:t></w:r></w:p>"#);
        let bytes = docx(&body);
        let lay_out = |faces: &[&str]| {
            let mut fonts = FontRegistry::new();
            for face in faces {
                fonts.add(font(face), 0).unwrap();
            }
            PreparedDocument::load(&bytes)
                .unwrap()
                .layout_with_fonts(fonts, VerticalGrid::None, &LayoutOptions::default())
                .unwrap()
        };
        let regular_only = lay_out(&["LiberationSerif-Regular.ttf"]);
        let restyled = regular_only.layout_diagnostics().iter()
            .find(|d| d.code == DiagnosticCode::FontStyleSubstituted).expect("粗体 face 没装");
        assert!(restyled.message.ends_with("「Liberation Serif」粗体"), "{}", restyled.message);
        assert!(!regular_only.layout_diagnostics().iter().any(|d| d.code == DiagnosticCode::FontFamilySubstituted));

        let both = lay_out(&["LiberationSerif-Regular.ttf", "LiberationSerif-Bold.ttf"]);
        assert!(both.layout_diagnostics().is_empty(), "{:?}", both.layout_diagnostics());
    }

    #[test]
    fn the_host_default_family_is_not_blamed_on_the_document() {
        // 没有 rFonts、也没有 docDefaults：`family` 是桥接层的宿主缺省，不是文档点名的。
        let styles = format!(r#"<w:styles xmlns:w="{W}"/>"#);
        let bytes = docx_with(&para("Hello"), &[("word/styles.xml", &styles)]);
        let document = PreparedDocument::load(&bytes).unwrap().document().clone();
        let font = &document.paras[0].runs[0].font;
        assert_eq!(font.slots.ascii, None);
        assert_eq!(font.family, "Times New Roman, SimSun, serif");

        let mut fonts = FontRegistry::new();
        fonts.add(super::fonts::font("LiberationSans-Regular.ttf"), 0).unwrap();
        let session = PreparedDocument::load(&bytes)
            .unwrap()
            .layout_with_fonts(fonts, VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        assert!(session.layout_diagnostics().is_empty(), "{:?}", session.layout_diagnostics());
    }

    #[test]
    fn a_fully_covered_document_adds_no_font_diagnostic() {
        let latin = r#"w:ascii="Liberation Sans" w:hAnsi="Liberation Sans" w:eastAsia="Liberation Sans" w:cs="Liberation Sans""#;
        let mut fonts = FontRegistry::new();
        fonts.add(font("LiberationSans-Regular.ttf"), 0).unwrap();
        let session = PreparedDocument::load(&docx(&format!("<w:p>{}</w:p>", run("Hello world", latin))))
            .unwrap()
            .layout_with_fonts(fonts, VerticalGrid::None, &LayoutOptions::default())
            .unwrap();
        assert!(session.layout_diagnostics().is_empty(), "{:?}", session.layout_diagnostics());
    }
}
