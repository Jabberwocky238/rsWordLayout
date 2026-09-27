//! Shared paragraph pagination constraints.
//!
//! SimpleMetrics cases pin inferred invariants, not new Word measurements.
//! The optional Calibri replay checks reported Android Word page boundaries in
//! word_analyse/reports/rsword-diff/{widow,keep-next,keep-lines}.md. The original
//! raw captures for those reports are not present in the source project.

use rsword::package::Package;
use rsword_layout_core::{
    Color, Engine, FontSpec, LayoutRecord, LayoutSection, LineRule, Margins, Page, PageSetup, Para,
    PlaceholderKind, Platform, Rect, Run, SectionStart, SimpleMetrics, Size, View, WrapContext,
    WrapRegion, document_from_json, load_document, paint_document,
};
use serde_json::json;

const LINE: i32 = 480;
const MODES: [(Platform, View); 4] = [
    (Platform::Desktop, View::Print),
    (Platform::Desktop, View::Mobile),
    (Platform::Android, View::Print),
    (Platform::Android, View::Mobile),
];

fn para(lines: usize) -> Para {
    assert!(lines > 0);
    Para {
        runs: vec![Run {
            text: vec!["x"; lines].join("\u{fffc}"),
            font: FontSpec::new("synthetic", 24),
            color: Color::BLACK,
            placeholders: vec![PlaceholderKind::LineBreak; lines - 1],
            rise: 0,
            rise_fine: None,
            hidden: false,
        }],
        line_rule: LineRule::Exact,
        line_value: LINE,
        ..Para::default()
    }
}

fn setup(height: i32) -> PageSetup {
    PageSetup {
        size: Size::new(10000, height),
        margins: Margins::uniform(0),
    }
}

fn counts(pages: &[Page]) -> Vec<usize> {
    record(pages)
        .pages
        .iter()
        .map(|page| page.lines.len())
        .collect()
}

fn record(pages: &[Page]) -> LayoutRecord {
    LayoutRecord::from_paint(&paint_document(pages, None, &[]))
}

fn assert_sources(pages: &[Page], paras: &[Para]) {
    let record = record(pages);
    let sources: Vec<_> = record
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .map(|line| line.source.expect("line keeps source range"))
        .collect();
    let units: usize = paras
        .iter()
        .map(|p| {
            p.runs
                .iter()
                .map(|r| r.text.encode_utf16().count())
                .sum::<usize>()
                + 1
        })
        .sum();
    assert_eq!(sources.first().unwrap().start, 0);
    assert_eq!(sources.last().unwrap().end, units as u32);
    assert!(sources.windows(2).all(|pair| pair[0].end == pair[1].start));
    assert!(sources.iter().all(|source| source.start < source.end));
}

fn assert_counts(paras: &[Para], height: i32, expected: &[usize]) {
    for (platform, view) in MODES {
        let pages = Engine::new(&SimpleMetrics, setup(height))
            .with_platform(platform, view)
            .layout(paras);
        assert_eq!(counts(&pages), expected, "{platform:?} {view:?}");
        assert_sources(&pages, paras);
    }
}

#[test]
fn assumed_widow_control_keeps_two_lines_together() {
    let block = Para {
        widow_control: true,
        ..para(2)
    };
    assert_counts(&[para(3), block], 4 * LINE, &[3, 2]);
    assert_counts(&[para(3), para(2)], 4 * LINE, &[4, 1]);
}

#[test]
fn negative_spacing_cannot_hide_an_intermediate_keep_paragraphs_bottom() {
    let lead = Para {
        keep_next: true,
        ..para(1)
    };
    let middle = Para {
        keep_next: true,
        space_after: -2 * LINE,
        ..para(2)
    };
    // The chain's final cursor is only two lines below its start, but its
    // middle paragraph reaches three. With two slots left, the chain must move.
    assert_counts(&[para(2), lead, middle, para(1)], 4 * LINE, &[2, 4]);
}

#[test]
fn assumed_three_line_paragraph_cannot_split_one_two_or_two_one() {
    for prefix in [2, 3] {
        let block = Para {
            widow_control: true,
            ..para(3)
        };
        assert_counts(&[para(prefix), block], 4 * LINE, &[prefix, 3]);
    }
}

#[test]
fn assumed_widow_control_retreats_a_line_to_leave_two_on_next_page() {
    let block = Para {
        widow_control: true,
        ..para(4)
    };
    assert_counts(&[para(1), block], 4 * LINE, &[3, 2]);
}

#[test]
fn assumed_long_widow_paragraph_balances_the_final_two_pages() {
    let block = Para {
        widow_control: true,
        ..para(7)
    };
    assert_counts(&[block], 3 * LINE, &[3, 2, 2]);
}

#[test]
fn assumed_empty_page_relaxes_impossible_widow_constraints_to_advance() {
    let block = Para {
        widow_control: true,
        keep_lines: true,
        ..para(3)
    };
    assert_counts(std::slice::from_ref(&block), LINE, &[1, 1, 1]);
    assert_counts(&[block], LINE - 1, &[1, 1, 1]);
}

#[test]
fn assumed_manual_page_break_takes_precedence_over_widow_and_keep_lines() {
    let mut block = Para {
        widow_control: true,
        keep_lines: true,
        ..para(3)
    };
    block.runs[0].placeholders[0] = PlaceholderKind::PageBreak;
    assert_counts(&[para(3), block], 4 * LINE, &[4, 2]);
}

#[test]
fn assumed_keep_next_chain_moves_before_any_member_is_placed() {
    let a = Para {
        keep_next: true,
        ..para(1)
    };
    let b = Para {
        keep_next: true,
        ..para(1)
    };
    assert_counts(&[para(2), a, b, para(1)], 4 * LINE, &[2, 3]);
}

#[test]
fn assumed_multiline_keep_next_paragraph_moves_with_its_following_line() {
    let a = Para {
        keep_next: true,
        ..para(2)
    };
    assert_counts(&[para(2), a, para(1)], 4 * LINE, &[2, 3]);
}

#[test]
fn assumed_keep_next_only_requires_a_legal_prefix_of_terminal_paragraph() {
    let a = Para {
        keep_next: true,
        ..para(2)
    };
    assert_counts(&[para(1), a, para(3)], 4 * LINE, &[4, 2]);
}

#[test]
fn assumed_keep_next_respects_terminal_widow_and_keep_lines() {
    for terminal in [
        Para {
            widow_control: true,
            ..para(3)
        },
        Para {
            keep_lines: true,
            ..para(3)
        },
    ] {
        let a = Para {
            keep_next: true,
            ..para(1)
        };
        assert_counts(&[para(1), a, terminal], 4 * LINE, &[1, 4]);
    }
    let a = Para {
        keep_next: true,
        ..para(1)
    };
    let terminal = Para {
        widow_control: true,
        ..para(4)
    };
    assert_counts(&[para(1), a, terminal], 4 * LINE, &[4, 2]);
}

#[test]
fn assumed_keep_next_rechecks_terminal_widow_after_wrapped_tail_reflow() {
    let a = Para {
        keep_next: true,
        ..para(1)
    };
    let mut b = Para {
        widow_control: true,
        ..para(1)
    };
    b.runs[0].text = "0".repeat(20);
    let paras = [para(1), a, b];
    let page_setup = PageSetup {
        size: Size::new(1200, 4 * LINE),
        margins: Margins::uniform(0),
    };
    let mut wrap = WrapContext::new();
    wrap.add(WrapRegion::rect(Rect::new(0, 2 * LINE, 600, 10000), 0));
    for (platform, view) in MODES {
        let pages = Engine::with_wrap(&SimpleMetrics, page_setup, wrap.clone())
            .with_platform(platform, view)
            .layout(&paras);
        // At the old position B has four five-character lines. Reserving two
        // leaves ten characters, which become a forbidden single line at the
        // next page top. Moving A first lets B use 10 + 5 + 5 on the same page.
        assert_eq!(counts(&pages), [1, 4], "{platform:?} {view:?}");
        assert_sources(&pages, &paras);
        let trace = record(&pages);
        assert_eq!(trace.pages[1].lines[0].source.unwrap().start, 2);
        assert_eq!(trace.pages[1].lines[1].source.unwrap().start, 4);
    }
}

#[test]
fn assumed_widow_rechecks_the_tail_at_the_successor_sections_page_width() {
    for linked in [false, true] {
        let mut document = document_from_json(&json!({"main": []}));
        document.paras = vec![para(1)];
        if linked {
            document.paras.push(Para {
                keep_next: true,
                ..para(1)
            });
        }
        let mut block = Para {
            widow_control: true,
            ..para(1)
        };
        block.runs[0].text = "0".repeat(40);
        document.paras.push(block);
        document.sections = vec![
            LayoutSection {
                block_range: 0..1,
                para_range: 0..1,
                setup: PageSetup {
                    size: Size::new(1200, 4 * LINE),
                    margins: Margins::uniform(0),
                },
                kind: SectionStart::NextPage,
                fallback_fields: vec![],
            },
            LayoutSection {
                block_range: 1..document.paras.len(),
                para_range: 1..document.paras.len(),
                setup: PageSetup {
                    size: Size::new(2400, 4 * LINE),
                    margins: Margins::uniform(0),
                },
                kind: SectionStart::Continuous,
                fallback_fields: vec![],
            },
        ];
        for (platform, view) in MODES {
            let pages = Engine::new(&SimpleMetrics, setup(4 * LINE))
                .with_platform(platform, view)
                .layout_document(&document);
            // Two narrow cached tail lines would become one wide-page line.
            // The common formatter must reject that otherwise legal 2/2 split.
            assert_eq!(counts(&pages), [1, if linked { 3 } else { 2 }]);
            assert_sources(&pages, &document.paras);
        }
    }
}

#[test]
fn assumed_keep_next_reservation_includes_interparagraph_spacing() {
    let a = Para {
        keep_next: true,
        space_after: 120,
        ..para(1)
    };
    let b = Para {
        space_before: 120,
        ..para(1)
    };
    assert_counts(&[para(2), a, b], 4 * LINE, &[2, 2]);
}

#[test]
fn assumed_next_paragraph_page_break_stops_keep_chain() {
    let a = Para {
        keep_next: true,
        ..para(1)
    };
    let b = Para {
        page_break_before: true,
        ..para(1)
    };
    assert_counts(&[para(3), a, b], 4 * LINE, &[4, 1]);
}

#[test]
fn assumed_manual_page_break_ends_keep_next_reservation() {
    let mut a = Para {
        keep_next: true,
        ..para(1)
    };
    a.runs[0].text.push('\u{fffc}');
    a.runs[0].placeholders.push(PlaceholderKind::PageBreak);
    // Android print combines the page break and paragraph mark. Other modes
    // deliberately keep a paragraph-mark line, so they need separate counts.
    let paras = [para(3), a, para(1)];
    let pages = Engine::new(&SimpleMetrics, setup(4 * LINE))
        .with_platform(Platform::Android, View::Print)
        .layout(&paras);
    assert_eq!(counts(&pages), [4, 1]);
    assert_sources(&pages, &paras);
}

#[test]
fn assumed_next_page_section_stops_keep_chain() {
    let paras = vec![
        para(3),
        Para {
            keep_next: true,
            ..para(1)
        },
        para(1),
    ];
    let mut document = document_from_json(&json!({"main": []}));
    document.paras = paras;
    document.sections = vec![
        LayoutSection {
            block_range: 0..2,
            para_range: 0..2,
            setup: setup(4 * LINE),
            kind: SectionStart::NextPage,
            fallback_fields: vec![],
        },
        LayoutSection {
            block_range: 2..3,
            para_range: 2..3,
            setup: setup(4 * LINE),
            kind: SectionStart::NextPage,
            fallback_fields: vec![],
        },
    ];
    for (platform, view) in MODES {
        let pages = Engine::new(&SimpleMetrics, setup(4 * LINE))
            .with_platform(platform, view)
            .layout_document(&document);
        assert_eq!(counts(&pages), [4, 1], "{platform:?} {view:?}");
        assert_sources(&pages, &document.paras);
    }
}

#[test]
fn assumed_oversized_keep_chain_preserves_content_and_makes_progress() {
    let paras: Vec<_> = (0..9)
        .map(|_| Para {
            keep_next: true,
            ..para(1)
        })
        .collect();
    for (platform, view) in MODES {
        let pages = Engine::new(&SimpleMetrics, setup(3 * LINE))
            .with_platform(platform, view)
            .layout(&paras);
        assert!(pages.len() >= 3 && pages.len() <= paras.len());
        assert!(counts(&pages).iter().all(|&n| (1..=3).contains(&n)));
        assert_sources(&pages, &paras);
    }
}

#[test]
fn assumed_oversized_keep_lines_relaxes_without_breaking_a_satisfiable_keep_next_link() {
    // Synthetic policy: a keepLines block taller than a fresh page relaxes that
    // constraint instead of moving away from an already satisfiable keepNext.
    // No new Word capture establishes this oversized-constraint priority.
    let a = Para {
        keep_next: true,
        ..para(1)
    };
    let b = Para {
        keep_lines: true,
        ..para(5)
    };
    let paras = [para(1), a, b];
    assert_counts(&paras, 4 * LINE, &[4, 3]);
    for (platform, view) in MODES {
        let pages = Engine::new(&SimpleMetrics, setup(4 * LINE))
            .with_platform(platform, view)
            .layout(&paras);
        let trace = record(&pages);
        assert_eq!(trace.pages[0].lines[1].source.unwrap().start, 2);
        assert_eq!(trace.pages[0].lines[2].source.unwrap().start, 4);
    }
}

#[test]
fn assumed_oversized_keep_lines_retains_satisfiable_widow_and_keep_next_constraints() {
    // Relax only the impossible whole-paragraph constraint. A needs at least two
    // B lines beside it; the fresh page fits three and leaves a legal two-line tail.
    // This is the same inferred priority as the preceding synthetic test.
    let a = Para {
        keep_next: true,
        ..para(1)
    };
    let b = Para {
        keep_lines: true,
        widow_control: true,
        ..para(5)
    };
    let paras = [para(2), a, b];
    assert_counts(&paras, 4 * LINE, &[2, 4, 2]);
    for (platform, view) in MODES {
        let pages = Engine::new(&SimpleMetrics, setup(4 * LINE))
            .with_platform(platform, view)
            .layout(&paras);
        let trace = record(&pages);
        assert_eq!(trace.pages[1].lines[0].source.unwrap().start, 4);
        assert_eq!(trace.pages[1].lines[1].source.unwrap().start, 6);
    }
}

fn loaded(body: &str, styles: &str) -> rsword_layout_core::LoadedDocument {
    let bytes = rsword::save::blank_docx(None).unwrap();
    let mut package = Package::open(&bytes).unwrap();
    let namespace = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    package
        .replace_part_xml(
            package.main_part(),
            &format!(r#"<w:document xmlns:w="{namespace}"><w:body>{body}</w:body></w:document>"#,),
        )
        .unwrap();
    let styles_part = package.find_name("word/styles.xml").unwrap();
    package
        .replace_part_xml(
            styles_part,
            &format!(r#"<w:styles xmlns:w="{namespace}">{styles}</w:styles>"#,),
        )
        .unwrap();
    load_document(&package.save().unwrap()).unwrap()
}

#[test]
fn widow_projection_preserves_missing_true_and_explicit_false() {
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r></w:p><w:p><w:pPr><w:widowControl/></w:pPr><w:r><w:t>b</w:t></w:r></w:p><w:p><w:pPr><w:widowControl w:val="0"/></w:pPr><w:r><w:t>c</w:t></w:r></w:p>"#;
    let direct = loaded(body, "").layout_document();
    assert_eq!(
        direct
            .paras
            .iter()
            .map(|p| p.widow_control)
            .collect::<Vec<_>>(),
        [false, true, false]
    );
    let defaults = loaded(body, r#"<w:docDefaults><w:pPrDefault><w:pPr><w:widowControl/></w:pPr></w:pPrDefault></w:docDefaults>"#).layout_document();
    assert_eq!(
        defaults
            .paras
            .iter()
            .map(|p| p.widow_control)
            .collect::<Vec<_>>(),
        [true, true, false]
    );
}

#[test]
fn widow_projection_uses_style_chain_and_direct_false_override() {
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Child"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Child"/><w:widowControl w:val="0"/></w:pPr><w:r><w:t>b</w:t></w:r></w:p>"#;
    let document = loaded(body, r#"<w:style w:type="paragraph" w:styleId="Base"><w:name w:val="Base"/><w:pPr><w:widowControl/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Child"><w:name w:val="Child"/><w:basedOn w:val="Base"/></w:style>"#).layout_document();
    assert!(document.paras[0].widow_control);
    assert!(!document.paras[1].widow_control);
}

#[cfg(feature = "fontenv")]
#[test]
#[ignore = "needs RSWORD_TEST_CALIBRI and the sibling word_analyse fixtures"]
fn reported_android_calibri_widow_and_keep_page_boundaries() {
    use rsword_layout_core::{RealMetrics, font::FontRegistry};
    let font = std::env::var_os("RSWORD_TEST_CALIBRI").expect("set RSWORD_TEST_CALIBRI");
    let mut registry = FontRegistry::new();
    registry.add(std::fs::read(font).unwrap(), 0).unwrap();
    let metrics = RealMetrics::new(&registry);
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../word_analyse/fixtures");
    for (file, expected_counts, second_start) in [
        ("widow-split.docx", vec![32, 1], 1933),
        ("widow-on.docx", vec![31, 2], 1834),
        ("keep-next.docx", vec![31, 2], 1054),
        ("keep-lines.docx", vec![31, 2], 1054),
    ] {
        let loaded = load_document(&std::fs::read(fixtures.join(file)).unwrap()).unwrap();
        let document = loaded.layout_document();
        assert_eq!(document.skipped_blocks, 0, "{file}");
        let pages = Engine::new(&metrics, PageSetup::a4())
            .with_platform(Platform::Android, View::Print)
            .layout_document(&document);
        assert_eq!(counts(&pages), expected_counts, "{file}");
        assert_eq!(
            record(&pages).pages[1].lines[0].source.unwrap().start,
            second_start,
            "{file}"
        );
        assert_sources(&pages, &document.paras);
    }
}
