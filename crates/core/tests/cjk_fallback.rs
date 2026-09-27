//! CJK 回退链与名义 1 em。
//!
//! 实测来源（word_analyse，Android Word 16.0.20513，**实测的是行数**）：CJK 夹具的
//! eastAsia 槽写 SimSun，手机上没有 SimSun，Word 照样把汉字排成与 1 em 相容的宽度——
//! `han22` 30 个「汉」在窄路径 5329 twips 上排成 22 + 8，`han-24`（24pt）排成 11 + 11 + 8。
//! 原来的引擎只装 Calibri 时汉字没有 face、推进量为零，30 个字挤进 1 行。
//!
//! 下面凡是 Word 没量过的规则（回退链的查找顺序、不成字形的字符不查回退链、
//! 名义字形的纵向量、组合符号零宽、谚文 1 em）都在测试名或注释里标「假定」。
//!
//! 这里只用仓库里收录、可再分发的字体（Liberation / DejaVu / Droid Sans Fallback）；
//! 要手机 Calibri 的那一条 `#[ignore]`，设 `RSWORD_TEST_CALIBRI=<calibri.ttf>` 再加
//! `--include-ignored` 才跑。

#![cfg(feature = "fontenv")]

use rsword_layout_core::{
    Color, Engine, FontHint, FontMetrics, FontSlots, FontSpec, LayoutRecord, Margins, PageSetup,
    Para, RealMetrics, Run, TraceMeta, font::FontRegistry, paint_document, to_trace_json,
};

const SANS: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSans-Regular.ttf");
const SERIF: &[u8] = include_bytes!("../../../fixtures/fonts/LiberationSerif-Regular.ttf");
const DEJAVU: &[u8] = include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf");
const CJK: &[u8] = include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf");

/// 夹具的写法：西文槽是装了的字体，eastAsia 槽是装不上的 SimSun。
fn simsun_font(size_half_points: u32) -> FontSpec {
    let mut font = FontSpec::new("Liberation Sans", size_half_points);
    font.slots = FontSlots {
        ascii: Some("Liberation Sans".into()),
        h_ansi: Some("Liberation Sans".into()),
        east_asia: Some("SimSun".into()),
        cs: None,
        hint: FontHint::Default,
    };
    font
}

/// 窄路径：版心 5329 twips，A4 页宽余下的平分给左右边距。
fn narrow() -> PageSetup {
    let mut setup = PageSetup::a4();
    setup.margins = Margins::uniform(720);
    let slack = setup.size.width - 5329;
    setup.margins.left = slack / 2;
    setup.margins.right = slack - setup.margins.left;
    setup
}

fn record(registry: &FontRegistry, text: &str, font: FontSpec) -> LayoutRecord {
    let para = Para {
        runs: vec![Run {
            text: text.into(),
            font,
            color: Color::BLACK,
            placeholders: Vec::new(),
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        ..Para::default()
    };
    let metrics = RealMetrics::new(registry);
    let pages = Engine::new(&metrics, narrow()).layout(&[para]);
    LayoutRecord::from_paint(&paint_document(
        &pages,
        Some(registry),
        &registry.face_ids(),
    ))
}

/// 每行的（基线 y，字形数）：行距与字形数一起比。
fn pitch_and_glyphs(record: &LayoutRecord) -> Vec<(i64, usize)> {
    record
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .map(|l| {
            (
                l.glyphs.first().map_or(0, |g| g.origin_y_fine),
                l.glyphs.len(),
            )
        })
        .collect()
}

/// `hint="eastAsia"`：歧义区与 ASCII 的字符都划给 eastAsia 槽。
fn east_asia_hint(font: FontSpec) -> FontSpec {
    let mut font = font;
    font.slots.hint = FontHint::EastAsia;
    font
}

fn starts(record: &LayoutRecord) -> Vec<u32> {
    record
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .map(|l| l.source.expect("行有源区间").start)
        .collect()
}

fn han(n: usize) -> String {
    "汉".repeat(n)
}

#[test]
fn uncovered_cjk_takes_one_em_notdef_instead_of_zero_width() {
    let mut registry = FontRegistry::new();
    let sans = registry.add(SANS.to_vec(), 0).unwrap();
    let font = simsun_font(24);

    let shaped = registry.shape_text("汉）。", &font);
    assert_eq!(shaped.len(), 3, "每个缺字一个 .notdef，不能跳过");
    for (i, glyph) in shaped.iter().enumerate() {
        assert_eq!(glyph.glyph_id, 0);
        assert_eq!(registry.face_ids()[glyph.face_index], sans);
        assert_eq!(glyph.source, Some((i as u32, i as u32 + 1)));
        assert_eq!(glyph.x_advance, 240, "12pt 的 1 em 是 240 twips");
        assert_eq!(glyph.x_advance_pt, 12.0);
    }

    let metrics = RealMetrics::new(&registry);
    let m = metrics.measure("汉汉汉", &font);
    assert_eq!(m.advance, 720);
    assert_eq!(metrics.advance_pt("汉汉汉", &font), 36.0);
    // 纵向量借名义 face 的：一段全是缺字的汉字不能行高为零。借的是哪个 face 的升部、
    // 降部是**占位**，没有 Word 的数（jsonl 的行高为空；Mac 的 N7 ×1.3 在 Android 上未知），
    // 所以这里只钉「不为零」，不钉具体值。
    assert!(m.ascent > 0 && m.descent > 0);
}

#[test]
fn nominal_em_follows_the_exact_size_without_drift() {
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();

    // 24pt：1 em 是 480 twips。`han-24` 实测的是 11 字一行，与 1 em 相容
    // （只把字宽夹到 444.1–484.5 twips），480 本身不是 Word 的数。
    let big = registry.shape_text("汉汉", &simsun_font(48));
    assert_eq!(
        big.iter().map(|g| g.x_advance).collect::<Vec<_>>(),
        vec![480, 480]
    );

    // 7.92pt = 158.4 twips 落不到整 twips 上：单个推进量取相邻取整位置之差，
    // 前缀和始终等于精确累计值取整，不随字数漂。
    let small = simsun_font(24).with_size_centipoints(792);
    let glyphs = registry.shape_text("汉汉汉", &small);
    assert_eq!(
        glyphs.iter().map(|g| g.x_advance).collect::<Vec<_>>(),
        vec![158, 159, 158]
    );
    assert!(glyphs.iter().all(|g| g.x_advance_pt == 7.92));
}

#[test]
fn non_cjk_uncovered_chars_are_still_skipped() {
    // 名义宽度只给 CJK：其余字符在手机上的画法没有实测，维持原样（跳过、零宽），
    // 且源位置照占——代理对仍占两个 UTF-16 单位。
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    let font = simsun_font(24);

    assert!(registry.face_for_char(&font, '\u{10ffff}').is_none());
    let shaped = registry.shape_text("A\u{10ffff}汉", &font);
    assert_eq!(
        shaped
            .iter()
            .map(|g| (g.source, g.glyph_id == 0))
            .collect::<Vec<_>>(),
        vec![(Some((0, 1)), false), (Some((3, 4)), true)]
    );
}

#[test]
fn han22_without_a_cjk_font_breaks_like_word() {
    // Word 窄路径：30 个「汉」起点 0、22；24pt 起点 0、11、22。
    let mut nominal = FontRegistry::new();
    nominal.add(SANS.to_vec(), 0).unwrap();
    let mut fallback = FontRegistry::new();
    fallback.add(SANS.to_vec(), 0).unwrap();
    let droid = fallback.add_fallback(CJK.to_vec(), 0).unwrap();

    for registry in [&nominal, &fallback] {
        assert_eq!(
            starts(&record(registry, &han(30), simsun_font(24))),
            vec![0, 22]
        );
        assert_eq!(
            starts(&record(registry, &han(30), simsun_font(48))),
            vec![0, 11, 22]
        );
    }

    let meta = TraceMeta::default();
    let by_fallback = record(&fallback, &han(30), simsun_font(24));
    assert!(
        by_fallback.pages[0].lines[0]
            .glyphs
            .iter()
            .all(|g| g.face == droid && g.glyph_id != 0),
        "回退链盖得住就用回退字体的真字形"
    );
    assert!(to_trace_json(&by_fallback, &meta).contains("\"notdefGlyphs\": 0,"));

    let by_nominal = record(&nominal, &han(30), simsun_font(24));
    assert_eq!(by_nominal.notdef_glyph_count(), 30);
    let json = to_trace_json(&by_nominal, &meta);
    assert!(json.contains("\"notdefGlyphs\": 30,"), "{json}");
    // 未归行的字形是另一回事，不能混进这一栏。
    assert!(json.contains("\"unassignedGlyphs\": 0,"), "{json}");
}

#[test]
fn fallback_faces_never_capture_what_primary_faces_cover() {
    // fontenv 的「任意覆盖」一步按内容哈希排序。回退字体若混进去，哈希排前的
    // 那一份会抢走本该落在正文字体上的字符（`breakme` 没有 rFonts，Noto 当正文字体装
    // 时拉丁字母全落到 Noto 上，每行 45 字而不是 Word 的 51）。
    // 两种先后都试：总有一种是回退字体的哈希排在前面。
    for (primary, fallback) in [(SANS, DEJAVU), (DEJAVU, SANS)] {
        let mut registry = FontRegistry::new();
        let want = registry.add(primary.to_vec(), 0).unwrap();
        registry.add_fallback(fallback.to_vec(), 0).unwrap();
        for ch in ['A', '0', ' ', 'é'] {
            assert_eq!(
                registry.select_face("Times New Roman, SimSun, serif", ch, false, false),
                Some(want.clone()),
                "{ch:?}"
            );
        }
    }
}

#[test]
fn fallback_chain_is_searched_in_declared_order() {
    // 正文字体是 Droid：它不含拉丁字母。`hint="eastAsia"` 把 'A' 划进 eastAsia 槽，
    // 槽里的 SimSun 没装，'A' 只能从回退链上找，按声明的先后。
    for (first, second) in [(SANS, DEJAVU), (DEJAVU, SANS)] {
        let mut registry = FontRegistry::new();
        registry.add(CJK.to_vec(), 0).unwrap();
        let want = registry.add_fallback(first.to_vec(), 0).unwrap();
        let other = registry.add_fallback(second.to_vec(), 0).unwrap();
        assert_eq!(registry.fallback_faces(), [want.clone(), other]);
        let font = east_asia_hint(simsun_font(24));
        assert_eq!(registry.select_face_for(&font, 'A'), Some(want));
        // 回退链只接 eastAsia 槽：没有 hint 时 'A' 在 ascii 槽，槽里的字体没装，回退链也不查，
        // 结果与没有回退链时一样（正文字体都画不出就是 None）。
        let mut latin_missing = FontSpec::new("missing", 24);
        latin_missing.slots.east_asia = Some("SimSun".into());
        assert_eq!(registry.select_face_for(&latin_missing, 'A'), None);
        assert_eq!(registry.select_face("missing", 'A', false, false), None);
    }
}

#[test]
fn a_family_named_by_the_document_still_selects_a_fallback_face() {
    // 槽规则先于回退：文档点名的字体装了（哪怕是作为回退装的）就用它。
    let mut registry = FontRegistry::new();
    let sans = registry.add(SANS.to_vec(), 0).unwrap();
    let dejavu = registry.add_fallback(DEJAVU.to_vec(), 0).unwrap();
    assert_eq!(
        registry.select_face("DejaVu Sans", 'A', false, false),
        Some(dejavu)
    );
    assert_eq!(
        registry.select_face("missing", 'A', false, false),
        Some(sans)
    );
}

#[test]
fn a_fallback_face_changes_nothing_that_primary_faces_already_cover() {
    // 装了 CJK 正文字体时（han22 / mix-cjk / kinsoku / breakme 的现状），再挂一个画不出
    // 这些 CJK 字符的回退字体，结果必须不变：西文字符（ascii / hAnsi 槽）不查回退链，
    // eastAsia 槽的字符查了也落空、回到正文字体。
    let mut plain = FontRegistry::new();
    plain.add(SANS.to_vec(), 0).unwrap();
    plain.add(CJK.to_vec(), 0).unwrap();
    let mut chained = FontRegistry::new();
    chained.add(SANS.to_vec(), 0).unwrap();
    chained.add(CJK.to_vec(), 0).unwrap();
    chained.add_fallback(DEJAVU.to_vec(), 0).unwrap();

    let font = simsun_font(24);
    let text = "A汉é）\u{3000}，6汉";
    for ch in text.chars() {
        assert_eq!(
            plain.select_face_for(&font, ch),
            chained.select_face_for(&font, ch),
            "{ch:?}"
        );
    }
    assert_eq!(
        plain.shape_text(text, &font),
        chained.shape_text(text, &font)
    );
    assert_eq!(
        starts(&record(&plain, &han(30), font.clone())),
        starts(&record(&chained, &han(30), font))
    );
}

#[test]
fn registering_as_primary_promotes_a_fallback_face() {
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    let dejavu = registry.add_fallback(DEJAVU.to_vec(), 0).unwrap();
    assert_eq!(registry.fallback_faces(), std::slice::from_ref(&dejavu));
    assert_eq!(registry.add(DEJAVU.to_vec(), 0).unwrap(), dejavu);
    assert!(registry.fallback_faces().is_empty());
    // 已是正文字体的不降级。
    registry.add_fallback(DEJAVU.to_vec(), 0).unwrap();
    assert!(registry.fallback_faces().is_empty());
}

#[test]
fn uncovered_chars_ignore_controls_and_placeholders() {
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    let font = simsun_font(24);
    let text = "\tA汉\u{fffc}\u{b}）";
    assert_eq!(
        registry.uncovered_chars(text, &font).collect::<Vec<_>>(),
        vec!['汉', '）']
    );

    registry.add_fallback(CJK.to_vec(), 0).unwrap();
    assert_eq!(registry.uncovered_chars(text, &font).count(), 0);
}

/// 会查回退链的字符与选 face 的第 2 步同一个判据：eastAsia 槽、成字形、槽里点名的字体
/// 画不出——别的正文字体画得出也算（`hint="eastAsia"` 下的 `“`、`A`、空格）。
/// [`FontRegistry::uncovered_chars`] 只是其中谁都画不出的那些。
#[test]
fn fallback_candidates_follow_the_slot_rule_not_global_coverage() {
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    let hinted = east_asia_hint(simsun_font(24));
    let text = "\u{201C}A 汉\u{202A}\t";
    assert_eq!(
        registry
            .fallback_candidates(text, &hinted)
            .collect::<Vec<_>>(),
        vec!['\u{201C}', 'A', ' ', '汉']
    );
    assert_eq!(
        registry.uncovered_chars(text, &hinted).collect::<Vec<_>>(),
        vec!['汉']
    );
    // 没有 hint：`“`、`A`、空格在西文槽，槽里的 Liberation Sans 画得出。
    assert_eq!(
        registry
            .fallback_candidates(text, &simsun_font(24))
            .collect::<Vec<_>>(),
        vec!['汉']
    );
    // eastAsia 槽点名的字体画得出就不算。
    let mut named = hinted.clone();
    named.slots.east_asia = Some("Liberation Sans".into());
    assert_eq!(
        registry
            .fallback_candidates(text, &named)
            .collect::<Vec<_>>(),
        vec!['汉']
    );
    // 候选里链上 face 盖得住的，注册之后都改用它；盖不住的「汉」仍谁都画不出。
    let dejavu = registry.add_fallback(DEJAVU.to_vec(), 0).unwrap();
    for ch in ['\u{201C}', 'A', ' '] {
        assert_eq!(
            registry.select_face_for(&hinted, ch),
            Some(dejavu.clone()),
            "{ch:?}"
        );
    }
    assert_eq!(registry.select_face_for(&hinted, '汉'), None);
}

#[test]
fn nominal_glyphs_borrow_the_run_s_own_latin_face() {
    // 名义 `.notdef` 借 run 自己的西文 face，不是注册顺序里的第一个。
    let mut registry = FontRegistry::new();
    let sans = registry.add(SANS.to_vec(), 0).unwrap();
    let serif = registry.add(SERIF.to_vec(), 0).unwrap();
    let mut font = simsun_font(24);
    font.slots.ascii = Some("Liberation Serif".into());
    font.slots.h_ansi = Some("Liberation Serif".into());
    let shaped = registry.shape_text("汉", &font);
    assert_eq!(registry.face_ids()[shaped[0].face_index], serif);
    assert_eq!(registry.face_for_char(&font, '汉'), Some((serif, true)));
    assert_eq!(
        registry.face_for_char(&simsun_font(24), '汉'),
        Some((sans, true))
    );
}

#[test]
fn surrogate_pairs_keep_two_utf16_units_under_the_nominal_em() {
    // 扩展 B 的 U+20000 是代理对：名义字形的源区间占两个 UTF-16 单位，推进量仍是 1 em。
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    let shaped = registry.shape_text("\u{20000}汉", &simsun_font(24));
    assert_eq!(
        shaped
            .iter()
            .map(|g| (g.source, g.x_advance))
            .collect::<Vec<_>>(),
        vec![(Some((0, 2)), 240), (Some((2, 3)), 240)]
    );
    // 30 个扩展 B 字：22 字一行，第二行从第 44 个 UTF-16 单位起。
    let text = "\u{20000}".repeat(30);
    assert_eq!(
        starts(&record(&registry, &text, simsun_font(24))),
        vec![0, 44]
    );
}

/// 假定：名义字形里的组合符号（U+3099 浊音符）零宽、并进前一个字的 cluster——
/// Noto Sans CJK 里它是零宽（字体表，不是 Word 的数）。谚文音节照样 1 em，
/// 这是已知的偏差（Noto 的谚文是 0.92 em），钉在这里，改了会看见。
#[test]
fn assumed_combining_marks_are_zero_width_and_hangul_is_one_em_under_the_nominal_glyph() {
    let mut registry = FontRegistry::new();
    registry.add(SANS.to_vec(), 0).unwrap();
    let font = simsun_font(24);
    let shaped = registry.shape_text("か\u{3099}한", &font);
    assert_eq!(
        shaped
            .iter()
            .map(|g| (g.source, g.x_advance, g.x_advance_pt))
            .collect::<Vec<_>>(),
        vec![
            (Some((0, 2)), 240, 12.0),
            (Some((0, 2)), 0, 0.0),
            (Some((2, 3)), 240, 12.0)
        ]
    );
    assert!(shaped.iter().all(|g| g.glyph_id == 0));
    let metrics = RealMetrics::new(&registry);
    assert_eq!(metrics.measure("か\u{3099}한", &font).advance, 480);
    // 开头就是组合符号（前面没有字）：自成一簇，仍是零宽。
    let lone = registry.shape_text("\u{3099}", &font);
    assert_eq!((lone[0].source, lone[0].x_advance), (Some((0, 1)), 0));
}

/// 假定：控制字符、格式字符（Cf）与默认可忽略码位不查回退链、不给名义字形。
/// Droid 映射了 U+0000 与 U+3164（都是 1 em）、U+202A–U+202D（零宽，但带进 Droid 的纵向量）；
/// 挂上 Droid 之后它们仍与没有回退链时一样：没有字形、零宽。
#[test]
fn assumed_controls_and_format_chars_never_take_a_fallback_or_nominal_glyph() {
    let mut plain = FontRegistry::new();
    plain.add(SANS.to_vec(), 0).unwrap();
    let mut chained = FontRegistry::new();
    chained.add(SANS.to_vec(), 0).unwrap();
    let droid = chained.add_fallback(CJK.to_vec(), 0).unwrap();

    // U+3164 谚文填充符、U+115F 初声填充符（在 `is_cjk` 里）都是默认可忽略码位。
    let font = simsun_font(24);
    let shaped = chained.shape_text("汉\u{3164}\u{115F}汉", &font);
    assert_eq!(
        shaped.iter().map(|g| g.source).collect::<Vec<_>>(),
        vec![Some((0, 1)), Some((3, 4))]
    );
    assert!(
        shaped
            .iter()
            .all(|g| chained.face_ids()[g.face_index] == droid)
    );
    let metrics = RealMetrics::new(&chained);
    assert_eq!(metrics.measure("汉\u{3164}\u{115F}汉", &font).advance, 480);
    for ch in [
        '\u{3164}', '\u{115F}', '\u{0}', '\u{202A}', '\u{FE0F}', '\u{200B}',
    ] {
        assert_eq!(
            chained.face_for_char(&font, ch),
            plain.face_for_char(&font, ch),
            "{ch:?}"
        );
    }

    // `hint="eastAsia"`：U+0000 与 U+202A / U+202C 进了 eastAsia 槽，SimSun 没装。
    // 回退链不接它们：U+0000 谁都画不出、跳过；U+202A / U+202C 照旧由正文字体
    // （Liberation 映射了它们）画——与没有回退链时逐位相同。
    let hinted = east_asia_hint(simsun_font(24));
    let text = "A\u{0}\u{202A}B\u{202C}";
    assert_eq!(
        chained.shape_text(text, &hinted),
        plain.shape_text(text, &hinted)
    );
    assert!(chained.uncovered_chars(text, &hinted).next().is_none());
    let (with, without) = (RealMetrics::new(&chained), RealMetrics::new(&plain));
    assert_eq!(with.measure(text, &hinted), without.measure(text, &hinted));

    // 一段西文里夹双向格式符，西文槽的字体没装（全靠正文字体的「任意覆盖」）：
    // 行距与字形数与没有回退链时相同——ascii / hAnsi 槽的字符不查回退链。
    // （`hint="eastAsia"` 的整段西文不在此列：空格也进了 eastAsia 槽，Droid 画得出空格，
    // 按槽规则就该用它。）
    let mut latin_missing = FontSpec::new("missing", 24);
    latin_missing.slots.east_asia = Some("SimSun".into());
    let latin = "Hello \u{202A}world\u{202C} ".repeat(12);
    let base = pitch_and_glyphs(&record(&plain, &latin, latin_missing.clone()));
    assert!(base.len() > 1);
    assert_eq!(
        pitch_and_glyphs(&record(&chained, &latin, latin_missing)),
        base
    );
}

/// 假定（槽规则推的，Word 未测）：`hint="eastAsia"` 下的 `“`（U+201C）在 eastAsia 槽，
/// 槽里的 SimSun 没装，就用回退链的字体，哪怕西文正文字体也画得出。
/// 这正是能分辨回退顺序的那份待测夹具（Noto SC 1000、Noto JP 474、Calibri 418 per 1000 em）。
#[test]
fn assumed_hint_east_asia_quote_takes_the_fallback_face_not_the_latin_primary() {
    let mut registry = FontRegistry::new();
    let sans = registry.add(SANS.to_vec(), 0).unwrap();
    let dejavu = registry.add_fallback(DEJAVU.to_vec(), 0).unwrap();
    let hinted = east_asia_hint(simsun_font(24));
    assert_eq!(
        registry.select_face_for(&hinted, '\u{201C}'),
        Some(dejavu.clone())
    );
    let shaped = registry.shape_text("\u{201C}", &hinted);
    assert_eq!(registry.face_ids()[shaped[0].face_index], dejavu);
    // 没有 hint：U+201C 在 hAnsi 槽，槽里的 Liberation Sans 画得出，回退链轮不到。
    assert_eq!(
        registry.select_face_for(&simsun_font(24), '\u{201C}'),
        Some(sans)
    );
}

// ---- Word 的字体（Calibri，需 RSWORD_TEST_CALIBRI）----

/// 手机 Word 自带的 Calibri 不进仓库，设 `RSWORD_TEST_CALIBRI=<calibri.ttf>` 才跑；
/// 设了却装不进或不是 Calibri 就失败，不静默跳过。
fn calibri() -> Vec<u8> {
    let path = std::env::var_os("RSWORD_TEST_CALIBRI")
        .expect("设 RSWORD_TEST_CALIBRI=<手机 Word 的 calibri.ttf>");
    std::fs::read(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"))
}

/// G4 审查的 `latin-cf.docx` 同构：Calibri 西文里夹 U+202A / U+202C（Calibri 不映射它们，
/// Droid 映射）。挂上 Droid 回退之后，行距与字形数必须与只装 Calibri 时相同——
/// 原型里它们拿到 Droid 的字形，行距涨了 3.5–3.7pt、每行多 2 个字形。
/// 对照是「没有回退链」的引擎自身，不是 Word 的数。
#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI"]
fn a_latin_paragraph_with_bidi_format_chars_keeps_its_pitch_on_calibri() {
    let mut plain = FontRegistry::new();
    plain.add(calibri(), 0).unwrap();
    assert!(
        plain.covers_family("Calibri"),
        "RSWORD_TEST_CALIBRI 不是 Calibri"
    );
    let mut chained = FontRegistry::new();
    chained.add(calibri(), 0).unwrap();
    chained.add_fallback(CJK.to_vec(), 0).unwrap();
    let mut font = FontSpec::new("Calibri", 24);
    font.slots.h_ansi = Some("Calibri".into());
    font.slots.east_asia = Some("SimSun".into());
    let text = "The quick \u{202A}brown\u{202C} fox jumps over the lazy dog. ".repeat(4);
    for ch in ['\u{202A}', '\u{202C}'] {
        assert_eq!(
            plain.select_face_for(&font, ch),
            None,
            "Calibri 不映射 {ch:?}"
        );
        assert_eq!(chained.select_face_for(&font, ch), None, "{ch:?}");
    }
    let base = pitch_and_glyphs(&record(&plain, &text, font.clone()));
    assert!(base.len() > 1);
    assert_eq!(pitch_and_glyphs(&record(&chained, &text, font)), base);
}
