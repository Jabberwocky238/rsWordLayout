//! `w:caps` / `w:smallCaps` 在真字体上：整形、字形记录、度量与绘制一致（feature `fontenv`）。
//!
//! 仓库字体（Liberation Sans / Serif、DejaVu Sans）钉机制：大写字形、缩小的字号、原文上的源区间、
//! 断行量到的宽度等于画出来的推进量、字体槽按源字符选、大写画不出时退回源字符。
//!
//! Word 实测的几何只在 Calibri 上有（`word_analyse/reports/rsword-diff/caps.md`）。
//! Calibri 是专有字体，不进仓库：那一条 `#[ignore]`，设 `RSWORD_TEST_CALIBRI=<calibri.ttf>`
//! 再加 `--include-ignored` 才跑。

#![cfg(feature = "fontenv")]

use rsword_layout_core::{
    Caps, Color, DrawCmd, Engine, FontMetrics, FontSlots, FontSpec, LayoutRecord, Margins,
    PageSetup, Para, PositionedGlyph, RealMetrics, Run, RustybuzzShaper, Size, TextShaper,
    font::FontRegistry, paint_document, paras_from_document,
};
use serde_json::json;

const SANS: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf");
const SERIF: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");
const DEJAVU: &[u8] = include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf");

fn sans() -> FontRegistry {
    let mut r = FontRegistry::new();
    r.add(SANS.to_vec(), 0).unwrap();
    r
}

fn font(caps: Caps) -> FontSpec {
    let mut f = FontSpec::new("Liberation Sans", 24);
    f.caps = caps;
    f
}

fn para(text: &str, caps: Caps) -> Para {
    Para {
        runs: vec![Run {
            text: text.into(),
            font: font(caps),
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        ..Para::default()
    }
}

fn painted(registry: &FontRegistry, paras: &[Para], setup: PageSetup) -> Vec<PositionedGlyph> {
    let real = RealMetrics::new(registry);
    let pages = Engine::new(&real, setup).layout(paras);
    paint_document(&pages, Some(registry), &registry.face_ids())
        .pages
        .into_iter()
        .flat_map(|p| p.cmds)
        .filter_map(|cmd| match cmd {
            DrawCmd::DrawGlyphs { glyphs, text, .. } if text.trim() != "" => Some(glyphs),
            _ => None,
        })
        .flatten()
        .collect()
}

fn glyph_id(registry: &FontRegistry, ch: char) -> u32 {
    registry.shape_text(&ch.to_string(), &font(Caps::None))[0].glyph_id
}

#[test]
fn caps_shape_uppercase_glyphs_on_original_source_spans() {
    let r = sans();
    let shaped = r.shape_text("aéß", &font(Caps::All));
    let ids: Vec<u32> = shaped.iter().map(|g| g.glyph_id).collect();
    // 一对一映射（假定）：`ß` 没有单字符的大写，原样画。
    assert_eq!(ids, [glyph_id(&r, 'A'), glyph_id(&r, 'É'), glyph_id(&r, 'ß')]);
    let sources: Vec<_> = shaped.iter().map(|g| g.source).collect();
    assert_eq!(sources, [Some((0, 1)), Some((1, 2)), Some((2, 3))]);
    assert!(shaped.iter().all(|g| g.size_centipoints == Some(1200)));

    // 度量走同一个整形：caps 下 `a` 就是 `A` 的宽度，`ß` 仍是 `ß` 的宽度。
    let m = RealMetrics::new(&r);
    assert_eq!(m.measure("a", &font(Caps::All)).advance, m.measure("A", &font(Caps::None)).advance);
    assert_ne!(m.measure("a", &font(Caps::All)).advance, m.measure("a", &font(Caps::None)).advance);
    assert_eq!(m.measure("ß", &font(Caps::All)).advance, m.measure("ß", &font(Caps::None)).advance);

    // 不经注册表、直接用 RustybuzzShaper 的整形入口，也做同一个变换。
    let mut direct = RustybuzzShaper::new();
    direct.add_face("sans", SANS.to_vec(), 0);
    let alone = direct.shape("aéß", &font(Caps::All));
    assert_eq!(
        alone.iter().map(|g| (g.glyph_id, g.source)).collect::<Vec<_>>(),
        shaped.iter().map(|g| (g.glyph_id, g.source)).collect::<Vec<_>>()
    );
    let alone = direct.shape("aA", &font(Caps::Small));
    assert_eq!(
        alone.iter().map(|g| g.size_centipoints).collect::<Vec<_>>(),
        [Some(950), Some(1200)]
    );
}

#[test]
fn small_caps_shape_lowercase_as_smaller_capitals() {
    let r = sans();
    let shaped = r.shape_text("aA", &font(Caps::Small));
    assert_eq!(shaped[0].glyph_id, glyph_id(&r, 'A'));
    assert_eq!(shaped[1].glyph_id, glyph_id(&r, 'A'));
    assert_eq!(shaped[0].size_centipoints, Some(950));
    assert_eq!(shaped[1].size_centipoints, Some(1200));
    assert_eq!(shaped[0].source, Some((0, 1)));
    assert_eq!(shaped[1].source, Some((1, 2)));
    // 9.5pt 的 `A` 与 12pt 的 `A` 同一字形、推进量按字号成比例。
    let ratio = shaped[0].x_advance_pt / shaped[1].x_advance_pt;
    assert!((ratio - 9.5 / 12.0).abs() < 1e-12, "{ratio}");
}

#[test]
fn small_caps_keep_a_combining_mark_in_its_base_cluster() {
    // `e` + U+0301：附加符号跟基字一个字号，整形不被切成两段，与预组合的 `é` 画成同样的字形。
    let r = sans();
    let decomposed = r.shape_text("e\u{301}", &font(Caps::Small));
    assert!(!decomposed.is_empty());
    assert!(decomposed.iter().all(|g| g.size_centipoints == Some(950)), "{decomposed:?}");
    assert!(decomposed.iter().all(|g| g.source == Some((0, 2))), "一个 cluster：{decomposed:?}");
    let composed = r.shape_text("é", &font(Caps::Small));
    assert_eq!(
        decomposed.iter().map(|g| g.glyph_id).collect::<Vec<_>>(),
        composed.iter().map(|g| g.glyph_id).collect::<Vec<_>>()
    );
    assert_eq!(composed[0].glyph_id, glyph_id(&r, 'É'));
    assert_eq!(composed[0].size_centipoints, Some(950));
}

/// 假定：字体槽按**源字符**选。`ı` → `I` 从 hAnsi 落进 ascii 区，仍用 hAnsi 槽的字体。
#[test]
fn assumed_caps_pick_the_font_slot_from_the_source_character() {
    let mut r = FontRegistry::new();
    r.add(SANS.to_vec(), 0).unwrap();
    r.add(SERIF.to_vec(), 0).unwrap();
    let mut f = FontSpec::new("Liberation Sans", 24);
    f.slots = FontSlots {
        ascii: Some("Liberation Sans".into()),
        h_ansi: Some("Liberation Serif".into()),
        ..FontSlots::default()
    };
    let serif = FontSpec::new("Liberation Serif", 24);
    let face_of = |text: &str, spec: &FontSpec| r.shape_text(text, spec)[0].face_index;
    let serif_face = face_of("é", &f);
    let sans_face = face_of("a", &f);
    assert_ne!(serif_face, sans_face);

    f.caps = Caps::All;
    // `ı` → `I`：画的是衬线体的 `I`，不是无衬线体的。
    let dotless = r.shape_text("ı", &f);
    assert_eq!(dotless[0].face_index, serif_face);
    assert_eq!(dotless[0].glyph_id, r.shape_text("I", &serif)[0].glyph_id);
    // `ß` 不变，仍在 hAnsi；`a` → `A` 本来就在 ascii。
    assert_eq!(face_of("ß", &f), serif_face);
    assert_eq!(face_of("a", &f), sans_face);
    // 纵向量也按源字符选 face：变换前后同一个行高。
    let m = RealMetrics::new(&r);
    let mut plain = f.clone();
    plain.caps = Caps::None;
    assert_eq!(m.natural_height_fine("ı", &f), m.natural_height_fine("ı", &plain));
}

/// Liberation Sans 在前、DejaVu Sans 作回退。仓库字体的覆盖：
///
/// | 源字符 | 大写 | Liberation Sans | DejaVu Sans |
/// | --- | --- | --- | --- |
/// | `ɐ` U+0250 | `Ɐ` U+2C6F | 只有 `ɐ` | 都有 |
/// | `ʇ` U+0287 | `Ʇ` U+A7B1 | 只有 `ʇ` | 只有 `ʇ` |
/// | `ა` U+10D0（格鲁吉亚文） | `Ა` U+1C90 | 都没有 | 只有 `ა` |
fn sans_and_dejavu() -> FontRegistry {
    let mut r = sans();
    r.add(DEJAVU.to_vec(), 0).unwrap();
    r
}

/// (face 下标, glyph id, 源区间, 字号)。
type ShapedId = (usize, u32, Option<(u32, u32)>, Option<u64>);

fn shaped_ids(r: &FontRegistry, text: &str, caps: Caps) -> Vec<ShapedId> {
    r.shape_text(text, &font(caps))
        .iter()
        .map(|g| (g.face_index, g.glyph_id, g.source, g.size_centipoints))
        .collect()
}

/// 假定：哪个 face 都没有的大写退回源字符，小型大写下全字号——与不做 caps 画得一样，
/// 而不是因为没有 face 被丢掉（宽度为零、什么都不画）。
#[test]
fn assumed_uppercase_missing_from_every_face_draws_the_source_char() {
    let r = sans_and_dejavu();
    let m = RealMetrics::new(&r);
    let georgian = "საქართველო";
    for caps in [Caps::All, Caps::Small] {
        for text in ["ʇ", georgian, "ʇ\u{301}"] {
            assert_eq!(shaped_ids(&r, text, caps), shaped_ids(&r, text, Caps::None), "{caps:?} {text}");
            assert_eq!(m.measure(text, &font(caps)).advance, m.measure(text, &font(Caps::None)).advance);
        }
        assert_eq!(r.shape_text(georgian, &font(caps)).len(), 10, "{caps:?}");
        // 同一个词里画得出的照常变：`a` 缩小成 `A`，`ʇ` 原样、全字号。
        let mixed = r.shape_text("aʇ", &font(caps));
        assert_eq!(mixed[0].glyph_id, glyph_id(&r, 'A'));
        assert_eq!(mixed[1].glyph_id, glyph_id(&r, 'ʇ'));
        assert_eq!(mixed[1].size_centipoints, Some(1200));
        // 绘制与字形记录也一样：一个字形都不少。
        let setup = PageSetup::a4();
        let shown: Vec<_> =
            painted(&r, &[para(georgian, caps)], setup).iter().map(|g| (g.glyph_id, g.source)).collect();
        let plain: Vec<_> =
            painted(&r, &[para(georgian, Caps::None)], setup).iter().map(|g| (g.glyph_id, g.source)).collect();
        assert_eq!(shown, plain, "{caps:?}");
    }
    // 不经注册表的整形器同一条规则：它的 face 里没有 `Ʇ`，画 `ʇ` 而不是 .notdef。
    let mut direct = RustybuzzShaper::new();
    direct.add_face("sans", SANS.to_vec(), 0);
    for caps in [Caps::All, Caps::Small] {
        let got = direct.shape("aʇɐ", &font(caps));
        let ids: Vec<u32> = got.iter().map(|g| g.glyph_id).collect();
        assert_eq!(ids, [glyph_id(&r, 'A'), glyph_id(&r, 'ʇ'), glyph_id(&r, 'ɐ')], "{caps:?}");
        assert_eq!(got[1].size_centipoints, Some(1200), "{caps:?}");
    }
}

/// 槽里的字体缺大写字形、别的 face 有：从有的那个 face 画大写（槽仍按源字符定）。
#[test]
fn caps_draw_an_uppercase_missing_from_the_slot_font_from_a_covering_face() {
    let r = sans_and_dejavu();
    let plain = shaped_ids(&r, "ɐ", Caps::None);
    let turned = shaped_ids(&r, "Ɐ", Caps::None);
    assert_ne!(plain[0].0, turned[0].0, "`ɐ` 在 Liberation、`Ɐ` 回退到 DejaVu");
    for (caps, size) in [(Caps::All, 1200), (Caps::Small, 950)] {
        let got = shaped_ids(&r, "ɐ", caps);
        assert_eq!(got, [(turned[0].0, turned[0].1, Some((0, 1)), Some(size))], "{caps:?}");
    }
}

#[test]
fn measured_width_equals_painted_advances_and_glyph_records_keep_sizes() {
    let r = sans();
    let m = RealMetrics::new(&r);
    let text = "Straße aA";
    for caps in [Caps::None, Caps::All, Caps::Small] {
        let glyphs = painted(&r, &[para(text, caps)], PageSetup::a4());
        // 段落标记画成末尾的一个空格（源区间 9..10），不属于正文宽度。
        let (body, mark): (Vec<_>, Vec<_>) =
            glyphs.iter().partition(|g| g.source.is_some_and(|(a, _)| a < 9));
        assert_eq!(mark.len(), 1, "{caps:?}");
        let painted_pt: f64 = body.iter().map(|g| g.advance_x_pt).sum();
        let measured_pt = m.advance_pt(text, &font(caps));
        assert!((painted_pt - measured_pt).abs() < 1e-9, "{caps:?}: {painted_pt} vs {measured_pt}");
        // 每个字形的源位置都在原文里（9 个 UTF-16 单位）。
        assert!(body.iter().all(|g| g.source.is_some_and(|(a, b)| a < b && b <= 9)), "{caps:?}");
        // 一对一映射：一字一形，`ß` 也是一个。
        assert_eq!(body.len(), 9, "{caps:?}");
    }
    // 小型大写的字形记录带缩小后的字号，栅格化与比较器读的就是它。
    let glyphs = painted(&r, &[para("aA", Caps::Small)], PageSetup::a4());
    let sizes: Vec<_> = glyphs.iter().map(|g| (g.size_half_points, g.size_centipoints)).collect();
    // 末尾是段落标记的空格：没有大写形式，原字号。
    assert_eq!(sizes, [(19, 950), (24, 1200), (24, 1200)]);
}

#[test]
fn trace_line_ranges_stay_on_source_text_under_caps() {
    // 变换后的字符、分段的字号、组合序列：行的源区间与字形的 sourceChar 仍按原文数。
    let r = sans();
    let real = RealMetrics::new(&r);
    let text = "Stra\u{df}e e\u{301}\u{131} ".repeat(20);
    let units = text.encode_utf16().count() as u32;
    let setup = PageSetup {
        size: Size::new(5329 + 1440, 16838),
        margins: Margins::new(720, 720, 720, 720),
    };
    for caps in [Caps::All, Caps::Small] {
        let pages = Engine::new(&real, setup).layout(&[para(&text, caps)]);
        let record = LayoutRecord::from_paint(&paint_document(&pages, Some(&r), &r.face_ids()));
        let lines: Vec<_> = record.pages.iter().flat_map(|p| p.lines.iter()).collect();
        assert!(lines.len() > 1, "{caps:?}");
        let mut expect_start = 0;
        for line in &lines {
            let s = line.source.unwrap();
            assert_eq!(s.start, expect_start, "{caps:?}: 行首接着上一行的行尾");
            expect_start = s.end;
            for g in &line.glyphs {
                let gs = g.source.expect("字形有源区间");
                assert!(s.start <= gs.start && gs.end <= s.end, "{caps:?}: 字形 {gs:?} 在行 {s:?} 内");
            }
        }
        assert_eq!(expect_start, units + 1, "{caps:?}: 源单位加段落标记");
    }
}

// ---- Word 实测（Calibri，需 RSWORD_TEST_CALIBRI）----

/// 手机 Word 自带的 Calibri 不进仓库，设 `RSWORD_TEST_CALIBRI=<calibri.ttf>` 才跑；
/// 设了却装不进或不是 Calibri 就失败，不静默跳过。
fn calibri() -> FontRegistry {
    let path = std::env::var_os("RSWORD_TEST_CALIBRI")
        .expect("设 RSWORD_TEST_CALIBRI=<手机 Word 的 calibri.ttf>");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
    let mut r = FontRegistry::new();
    r.add(bytes, 0).expect("字体装得进");
    assert!(r.covers_family("Calibri"), "{path:?} 不是 Calibri");
    r
}

/// 与 `word_analyse/fixtures/{caps-on,smallcaps*}.docx` 同构：一个 run，120 或 160 个 `a`。
fn fixture_paras(n: usize, props: serde_json::Value) -> Vec<Para> {
    let mut props = props;
    props["fonts"] = json!({"ascii": "Calibri", "hAnsi": "Calibri"});
    props["size"] = json!(24);
    let doc = json!({"main": [{"kind": "text", "props": {},
        "inlines": [{"kind": "run", "text": "a".repeat(n), "props": props}]}]});
    paras_from_document(&doc).0
}

fn line_starts(r: &FontRegistry, paras: &[Para], page_width: i32, content: i32) -> Vec<u32> {
    let slack = page_width - content;
    let setup = PageSetup {
        size: Size::new(page_width, 16838),
        margins: Margins::new(720, slack - slack / 2, 720, slack / 2),
    };
    let real = RealMetrics::new(r);
    let pages = Engine::new(&real, setup).layout(paras);
    LayoutRecord::from_paint(&paint_document(&pages, Some(r as &dyn TextShaper), &r.face_ids()))
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| l.source.unwrap().start)
        .collect()
}

/// 实测：六个 Word 的数（`caps.md`、`caps-on.word.narrow.jsonl`）。每个夹具都是一个没有断点的
/// 长词，所以它们同时依赖紧急断行（G0）；`caps-plain` 是 G0 自己的对照，不经 caps。
#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI"]
fn word_caps_and_small_caps_line_starts_on_calibri() {
    let r = calibri();
    // (夹具, 字数, 属性, 页宽, 版心, Word 的行起点前缀)
    let cases = [
        ("caps-plain", 120, json!({}), 11906, 10466, vec![0, 91]),
        ("caps-on", 120, json!({"caps": true}), 11906, 10466, vec![0, 75]),
        // caps-on 窄路径 jsonl：0–38、38–76、76–114、114–121。
        ("caps-on narrow", 120, json!({"caps": true}), 11906, 5329, vec![0, 38, 76, 114]),
        ("smallcaps", 120, json!({"smallCaps": true}), 11906, 10466, vec![0, 95]),
        ("smallcaps-wide", 160, json!({"smallCaps": true}), 16000, 14560, vec![0, 132]),
        ("smallcaps-12000", 120, json!({"smallCaps": true}), 12000, 10560, vec![0, 96]),
    ];
    for (name, n, props, page, content, want) in cases {
        let got = line_starts(&r, &fixture_paras(n, props), page, content);
        assert_eq!(&got[..want.len()], &want[..], "{name}: {got:?}");
    }
}
