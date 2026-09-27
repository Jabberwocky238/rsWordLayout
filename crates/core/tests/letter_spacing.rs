//! `w:spacing`（run 级字符间距）与 `w:w`（横向缩放）从 docx 一路到断行与落位。
//!
//! 三段，各钉一件事：
//!
//! 1. **并排的 `w:rPr`**。解析器（rsWordParser 399e36a）只留最后一个，手机 Word 用上了
//!    第一个（word_analyse `latinspace.docx`：第一个 `w:rPr` 只有 `w:spacing w:val="20"`，
//!    Word 窄路径 24 行起点与「每字符 +1pt」逐项相同）。`load_document` 先把它们并起来。
//! 2. **桥接**：run 的 `spacing` / `scale` 进 `FontSpec`，不再写死 0 / 100。
//! 3. **加在哪些字符上**。空格也加是**实测**：`latinspace` 里「空格不加」不论行尾空格计不计宽，
//!    都在前几行就与 Word 错开。行尾最后一个可见字符也加是**假定**：只有「行尾空格不计宽」时
//!    `latinspace` 才否掉「末字不加」（第 4 行 130 对 124）；rsword 计行尾空格，这个口径下
//!    「末字不加」同样 24/24。按 cluster 而不是按字形计也是**假定**。
//!
//! 字体只用仓库自带的 `fixtures/fonts`；断行判据用桩度量，不依赖私有字体。

use rsword::bind::native::SessionTable;
use rsword_layout_core::{
    Engine, FontMetrics, FontSpec, Fragment, PageSetup, Para, Run, SimpleMetrics, load_document,
    merge_sibling_run_props, paras_from_document,
};

// ---- 1. 并排的 `w:rPr` -----------------------------------------------------------------------

/// 与 `latinspace.docx` 同形：第一个 `w:rPr` 只有字符间距，第二个只有字体与字号。
const DOUBLE_RPR: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    r#"<w:p><w:r><w:rPr><w:spacing w:val="20"/></w:rPr>"#,
    r#"<w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr>"#,
    r#"<w:t xml:space="preserve">alpha beta</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:rPr><w:w w:val="80"/></w:rPr><w:rPr><w:sz w:val="24"/></w:rPr>"#,
    r#"<w:t>gamma</w:t></w:r></w:p>"#,
    r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#,
);

/// 空白 docx 的主 part 换成 `xml`，存成字节。
fn docx_with_main_part(xml: &str) -> Vec<u8> {
    with_main_part(&rsword::save::blank_docx(None).expect("空白 docx"), xml)
}

/// `docx` 的主 part 换成 `xml`，存成字节。测试可以用会话的编辑接口，库里不用（见 load.rs）。
fn with_main_part(docx: &[u8], xml: &str) -> Vec<u8> {
    let mut sessions = SessionTable::default();
    let id = sessions.open(docx, None).unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
    let main = doc["mainPart"].as_u64().unwrap();
    let op = serde_json::json!({"op": "replacePartXml", "part": main, "xml": xml});
    sessions.apply(&id, &op.to_string(), None).unwrap();
    let bytes = sessions.save(&id, None).unwrap();
    sessions.close(&id);
    bytes
}

/// 直接经会话取的模型 JSON 与主 part 的 XML。
fn session_view(docx: &[u8]) -> (serde_json::Value, String) {
    let mut sessions = SessionTable::default();
    let id = sessions.open(docx, None).unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
    let main = doc["mainPart"].as_u64().unwrap() as u32;
    let xml = String::from_utf8(sessions.part_bytes(&id, main).unwrap()).unwrap();
    sessions.close(&id);
    (doc, xml)
}

fn run_props(doc: &serde_json::Value, para: usize) -> serde_json::Value {
    let main = doc["main"].as_array().unwrap();
    let text: Vec<_> = main.iter().filter(|b| b["kind"] == "text").collect();
    text[para]["inlines"][0]["props"].clone()
}

#[test]
fn sibling_run_props_merge_first_wins_per_property() {
    let (merged, n) = merge_sibling_run_props(
        r#"<w:r><w:rPr><w:spacing w:val="20"/><w:sz w:val="20"/></w:rPr><w:rPr><w:sz w:val="24"/></w:rPr><w:t>a</w:t></w:r>"#,
    )
    .expect("有并排的 w:rPr");
    assert_eq!(n, 1);
    // 拼成一个：子元素按原顺序接在一起。重复的 `w:sz` 由解析器按「先到先得」读。
    assert_eq!(
        merged,
        r#"<w:r><w:rPr><w:spacing w:val="20"/><w:sz w:val="20"/><w:sz w:val="24"/></w:rPr><w:t>a</w:t></w:r>"#
    );
}

#[test]
fn sibling_run_props_merge_handles_chains_empties_and_whitespace() {
    // 三个一串、中间有空白，都并成一个。
    let (merged, n) = merge_sibling_run_props(
        "<w:r><w:rPr><w:b/></w:rPr>\n  <w:rPr><w:i/></w:rPr><w:rPr><w:caps/></w:rPr><w:t>a</w:t></w:r>",
    )
    .unwrap();
    assert_eq!(n, 2);
    assert_eq!(merged, "<w:r><w:rPr><w:b/><w:i/><w:caps/></w:rPr><w:t>a</w:t></w:r>");

    // 第二个是空元素：删掉它，否则「只留最后一个」会把前面的属性清空。
    let (merged, n) =
        merge_sibling_run_props("<w:r><w:rPr><w:b/></w:rPr><w:rPr/><w:t>a</w:t></w:r>").unwrap();
    assert_eq!(n, 1);
    assert_eq!(merged, "<w:r><w:rPr><w:b/></w:rPr><w:t>a</w:t></w:r>");
}

#[test]
fn sibling_run_props_merge_leaves_legal_neighbours_alone() {
    for xml in [
        // 合法的单个 `w:rPr`。
        "<w:r><w:rPr><w:b/></w:rPr><w:t>a</w:t></w:r>",
        // OMML：`m:rPr` 后接 `w:rPr` 是正常写法，前缀不同，不动。
        "<m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><w:rPr><w:b/></w:rPr><m:t>a</m:t></m:r>",
        // 修订：`w:rPrChange` 里的 `w:rPr` 后面没有兄弟 `w:rPr`。
        "<w:r><w:rPr><w:b/><w:rPrChange w:id=\"1\"><w:rPr/></w:rPrChange></w:rPr><w:t>a</w:t></w:r>",
        // 段落标记的 `w:rPr` 后面是 `w:pPr` 的结束，不是兄弟 `w:rPr`。
        "<w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:rPr><w:i/></w:rPr></w:r></w:p>",
        // 第二个带属性：不是这里设想的情形，不猜。
        "<w:r><w:rPr><w:b/></w:rPr><w:rPr w:x=\"1\"><w:i/></w:rPr></w:r>",
    ] {
        assert_eq!(merge_sibling_run_props(xml), None, "{xml}");
    }
}

#[test]
fn parser_alone_keeps_only_the_last_run_props() {
    // 钉住**解析器的现状**。这条一旦失败，说明解析器自己会合并了——
    // 那时 `load_document` 的垫片可以删掉（见 crates/core/src/load.rs）。
    let bytes = docx_with_main_part(DOUBLE_RPR);
    let mut sessions = SessionTable::default();
    let id = sessions.open(&bytes, None).unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
    sessions.close(&id);
    let props = run_props(&doc, 0);
    assert_eq!(props["size"], 24);
    assert!(props.get("spacing").is_none(), "解析器已经保留第一个 w:rPr 了：{props}");
}

#[test]
fn load_document_keeps_properties_from_every_run_props() {
    let loaded = load_document(&docx_with_main_part(DOUBLE_RPR)).unwrap();
    assert_eq!(loaded.merged_run_props, 2);
    let first = run_props(&loaded.json, 0);
    assert_eq!(first["spacing"], 20);
    assert_eq!(first["size"], 24);
    assert_eq!(first["fonts"]["ascii"], "Calibri");
    let second = run_props(&loaded.json, 1);
    assert_eq!(second["scale"], 80);
    assert_eq!(second["size"], 24);

    // 进到桥接层：两个属性都到了 `FontSpec`。
    let (paras, _) = paras_from_document(&loaded.json);
    assert_eq!(paras[0].runs[0].font.letter_spacing, 20);
    assert_eq!(paras[0].runs[0].font.family, "Calibri");
    assert_eq!(paras[1].runs[0].font.scale_pct, 80);
}

#[test]
fn load_document_is_a_pass_through_for_well_formed_files() {
    let xml = DOUBLE_RPR
        .replace(
            r#"<w:rPr><w:spacing w:val="20"/></w:rPr><w:rPr>"#,
            r#"<w:rPr><w:spacing w:val="20"/>"#,
        )
        .replace(r#"<w:rPr><w:w w:val="80"/></w:rPr><w:rPr>"#, r#"<w:rPr><w:w w:val="80"/>"#);
    let bytes = docx_with_main_part(&xml);
    let loaded = load_document(&bytes).unwrap();
    assert_eq!(loaded.merged_run_props, 0);

    let (direct, _) = session_view(&bytes);
    assert_eq!(loaded.json, direct, "没有可合并的 w:rPr 时 JSON 必须原样");
    assert_eq!(run_props(&direct, 0)["spacing"], 20);
}

#[test]
fn load_document_merge_matches_the_session_on_a_merged_file() {
    // 合并那一支不经会话的编辑接口（load.rs `document_with_main_part`），自己拼
    // 「开包 → 换主 part → 重建 → 投影」。这条钉住它与会话出口**同形**：
    // 与「主 part 本来就是合并后 XML」的同一份文件经 `SessionTable::document()` 取的 JSON 整份相同
    // ——顶层键（`totalBlocks`/`truncated`）、媒体句柄、修订编号、节点号都在比对之内。
    // 底子是仓库的 `fixtures/wrap.docx`（带一张图）；正文末尾加两段：一段带修订、带并排 `w:rPr`，
    // 一段的修订在合并点**之后**（节点号变了，沿用第一次打开的修订编号就会换号，与新开的不同）。
    let wrap = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wrap.docx"))
        .expect("fixtures/wrap.docx");
    let (_, xml) = session_view(&wrap);
    let at = xml.rfind("<w:sectPr").expect("正文末尾的 w:sectPr");
    let extra = concat!(
        r#"<w:p><w:ins w:id="901" w:author="t" w:date="2026-01-01T00:00:00Z"><w:r>"#,
        r#"<w:rPr><w:spacing w:val="20"/></w:rPr><w:rPr><w:sz w:val="24"/></w:rPr>"#,
        r#"<w:t>x</w:t></w:r></w:ins></w:p>"#,
        r#"<w:p><w:ins w:id="902" w:author="t" w:date="2026-01-01T00:00:00Z"><w:r>"#,
        r#"<w:t>y</w:t></w:r></w:ins></w:p>"#,
    );
    let doubled = with_main_part(&wrap, &format!("{}{extra}{}", &xml[..at], &xml[at..]));

    let loaded = load_document(&doubled).unwrap();
    assert_eq!((loaded.merged_run_props, loaded.merge_error.as_deref()), (1, None));

    let (_, doubled_xml) = session_view(&doubled);
    let (merged_xml, _) = merge_sibling_run_props(&doubled_xml).expect("有并排的 w:rPr");
    let (by_hand, _) = session_view(&with_main_part(&doubled, &merged_xml));
    // 比对要有东西可比：这几样确实在。
    assert!(!by_hand["media"].as_array().unwrap().is_empty(), "{}", by_hand["media"]);
    assert!(by_hand["media"][0]["mediaId"].is_number());
    assert!(by_hand["totalBlocks"].is_number() && by_hand["truncated"] == false);
    let revisions = by_hand["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 2, "两处修订都要在投影里：{}", by_hand["revisions"]);
    let x = by_hand["main"].as_array().unwrap().iter().find(|b| b["inlines"][0]["text"] == "x");
    let x = x.expect("加的那一段").to_string();
    assert!(x.contains(r#""spacing":20"#), "合并后第一个 w:rPr 的间距要在：{x}");

    assert_eq!(loaded.json, by_hand);
}

// ---- 2. 桥接 ---------------------------------------------------------------------------------

fn bridged_font(props: serde_json::Value) -> FontSpec {
    let doc = serde_json::json!({"main": [{"kind": "text",
        "inlines": [{"kind": "run", "text": "text", "props": props}]}]});
    paras_from_document(&doc).0.remove(0).runs.remove(0).font
}

#[test]
fn bridge_reads_run_spacing_and_scale() {
    let font = bridged_font(serde_json::json!({"spacing": 20, "scale": 80}));
    assert_eq!((font.letter_spacing, font.scale_pct), (20, 80));
    // `ST_SignedTwipsMeasure`：可负（紧缩）。
    let font = bridged_font(serde_json::json!({"spacing": -15}));
    assert_eq!((font.letter_spacing, font.scale_pct), (-15, 100));
    // 缺省：不加间距、不缩放。
    let font = bridged_font(serde_json::json!({}));
    assert_eq!((font.letter_spacing, font.scale_pct), (0, 100));
    // 解析器原样保留的非法值不是数：按缺省，不在桥接层猜。
    let font = bridged_font(serde_json::json!({"spacing": "20px", "scale": "big"}));
    assert_eq!((font.letter_spacing, font.scale_pct), (0, 100));
}

// ---- 3. 加在哪些字符上 ------------------------------------------------------------------------

fn run(text: &str, letter_spacing: i32) -> Run {
    let mut font = FontSpec::new("Test", 24); // 桩：字母 6pt = 120 twips，空格 3pt = 60 twips
    font.letter_spacing = letter_spacing;
    Run {
        text: text.into(),
        font,
        color: rsword_layout_core::Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

/// 各行的起点（UTF-16 源偏移），与 Word 的 `cpFirst` 同一口径。
fn line_starts(para: Para, content_width: i32) -> Vec<u32> {
    let mut setup = PageSetup::a4();
    let slack = setup.size.width - content_width;
    setup.margins.left = slack / 2;
    setup.margins.right = slack - setup.margins.left;
    let pages = Engine::new(&SimpleMetrics, setup).layout(&[para]);
    let mut starts: Vec<u32> = Vec::new();
    for f in &pages[0].fragments {
        if let Fragment::Text(t) = f
            && let Some((start, _)) = t.source
            && starts.len() <= t.line as usize
        {
            starts.push(start);
        }
    }
    starts
}

#[test]
fn spacing_counts_spaces_and_the_last_glyph_on_the_line() {
    // 钉的是 **rsword 选定的口径**（每个字符都加，空格与行尾末字也加），不全是 Word 的实测：
    // `latinspace` 只证实了「空格也加」。「末字不加」在 rsword 的 fit（计行尾空格）之下与
    // Word 的 24 行同样相容，只在「行尾空格不计宽」时才被否掉——见模块文档第 3 条。
    //
    // "aaaa aaaa" 带 20 twips 间距：8 × (120 + 20) + (60 + 20) = 1200 twips。
    // 版心 1199：按这个口径放不下，要断在空格后。另两种口径都会算成 1180 而放下：
    //   - 空格不加（`latinspace` 否掉了）：8 × 140 + 60 = 1180；
    //   - 行尾末字不加（`latinspace` 在 rsword 的口径下否不掉）：7 × 140 + 120 + 80 = 1180。
    let text = "aaaa aaaa";
    assert_eq!(SimpleMetrics.measure(text, &run(text, 20).font).advance, 1200);
    let para = |spacing| Para { runs: vec![run(text, spacing)], ..Para::default() };
    assert_eq!(line_starts(para(20), 1199), [0, 5]);
    assert_eq!(line_starts(para(20), 1200), [0]);
    // 对照：没有间距时 1199 绰绰有余。
    assert_eq!(line_starts(para(0), 1199), [0]);
}

#[test]
fn negative_spacing_condenses_the_line() {
    // 紧缩：-20 twips 让 9 个字符少占 180 twips，原本放不下的整行放下了。
    let text = "aaaa aaaa";
    let para = |spacing| Para { runs: vec![run(text, spacing)], ..Para::default() };
    assert_eq!(line_starts(para(0), 950), [0, 5]);
    assert_eq!(SimpleMetrics.measure(text, &run(text, -20).font).advance, 840);
    assert_eq!(line_starts(para(-20), 950), [0]);
}

#[test]
fn stub_spacing_is_counted_once_per_source_cluster() {
    // 桩不整形，但间距的位置数按源字符簇数：`x` + U+0301 是一簇，只加一次——与 `RealMetrics`
    // 按整形 cluster 计同一个数（两者对照见 `letter_spacing_shaped.rs`）。按 cluster 计是假定。
    let spaced =
        |text: &str, spacing| SimpleMetrics.measure(text, &run(text, spacing).font).advance;
    assert_eq!(spaced("x\u{301}", 20) - spaced("x\u{301}", 0), 20);
    assert_eq!(spaced("ab c", 20) - spaced("ab c", 0), 4 * 20, "一字符一簇：空格也算一次");
    // 在簇边界切开，两段的间距之和等于整段：紧急断行按同一张表切，宽度可加。
    assert_eq!(spaced("ax\u{301}", 20), spaced("a", 20) + spaced("x\u{301}", 20));
}
