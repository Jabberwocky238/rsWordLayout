//! `w:fitText`：一个 run 的文字合起来恰好占 `w:val` twips，整截不可拆。
//!
//! Word 实测（Android Word 纸页 10466，Calibri 12pt，word_analyse `reports/rsword-diff/fittext.md`）：
//! `fittext`（40 个 `0` 压到 2000，后接 80 个 `0`）下一行从 109 起；`fittext-wide`（拉到 8000）
//! 从 60 起；`fittext-over`（10 个 `0`，40 个 `0` 设成 12000，再 80 个 `0`）是 0、10、50。
//! 三份都是「整截当一个 cluster、按紧急断行收」：第一份的 69 个 `0` 由紧急断行切出；第三份
//! 第一行在第 10 个 `0` 之后那一刀也是紧急断行切的（整截放不下、行首之后没有断点），
//! 整截随后独占一行、溢出也收（空行至少收一个 cluster）。
//!
//! 引擎侧的规则用桩度量钉（拉丁字 0.5 em、空格 0.25 em，10pt 即 100 / 50 twips），不依赖字体；
//! 桥接层走真实 DOCX 字节；`real` 模块（feature `fontenv`）钉绘制：字形推进量合计恰为目标宽。
//! 截内字形怎么摊（这里均摊在 cluster 之间）是**假设**，Word 只有断行读数。

use rsword::package::Package;
use rsword_layout_core::{
    Color, Engine, FontSpec, Fragment, LayoutRecord, Margins, Page, PageSetup, Para, Run,
    SimpleMetrics, Size, Twips, load_document, paint_document,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn run(text: &str, fit: Option<Twips>) -> Run {
    let mut font = FontSpec::new("synthetic", 20);
    font.fit_text = fit;
    Run {
        text: text.to_owned(),
        font,
        color: Color::BLACK,
        placeholders: vec![],
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn setup(width: Twips) -> PageSetup {
    PageSetup { size: Size::new(width, 1_000_000), margins: Margins::uniform(0) }
}

fn pages(width: Twips, runs: Vec<Run>) -> Vec<Page> {
    Engine::new(&SimpleMetrics, setup(width)).layout(&[Para { runs, ..Para::default() }])
}

/// 各行的源区间（UTF-16，含段落标记那一格）。
fn lines(width: Twips, runs: Vec<Run>) -> Vec<(u32, u32)> {
    let record = LayoutRecord::from_paint(&paint_document(&pages(width, runs), None, &[]));
    record
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| {
            let s = l.source.expect("每行都有源区间");
            (s.start, s.end)
        })
        .collect()
}

/// 第 `line` 行有字的片段：(文字, 起点 x)。
fn pieces(pages: &[Page], line: u32) -> Vec<(String, Twips)> {
    pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) if t.line == line && !t.text.is_empty() => Some((t.text.clone(), t.x)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_compressed_run_takes_its_fit_width_and_the_rest_is_cut_by_emergency_break() {
    // `fittext` 的形状：86.5 个字宽的行，40 个字压到 2000，再接 80 个字。
    // 2000 + 66 × 100 = 8600 放得下，第 67 个超出：下一行从 40 + 66 = 106 起。
    let zeros = |n: usize| "0".repeat(n);
    assert_eq!(
        lines(8650, vec![run(&zeros(40), Some(2000)), run(&zeros(80), None)]),
        [(0, 106), (106, 121)]
    );
}

#[test]
fn an_expanded_run_takes_its_fit_width() {
    // `fittext-wide`：拉到 8000，余下 650 只放得下 6 个字。
    let zeros = |n: usize| "0".repeat(n);
    assert_eq!(
        lines(8650, vec![run(&zeros(40), Some(8000)), run(&zeros(80), None)]),
        [(0, 46), (46, 121)]
    );
}

#[test]
fn a_run_wider_than_the_line_moves_whole_onto_its_own_line() {
    // `fittext-over`：前面 10 个字之后整截放不下、行首之后没有断点，紧急断行切在第 10 个字之后；
    // 整截随后独占一行（溢出也收），剩下的字另起一行。Word 是 0、10、50。
    let zeros = |n: usize| "0".repeat(n);
    assert_eq!(
        lines(
            8650,
            vec![run(&zeros(10), None), run(&zeros(40), Some(12000)), run(&zeros(80), None)]
        ),
        [(0, 10), (10, 50), (50, 131)]
    );
}

#[test]
fn breaks_inside_a_fit_run_are_not_used() {
    // 截内有空格（断点）也不在里面断：放不下就在它之前的交界断，整截挪到下一行。
    assert_eq!(
        lines(4000, vec![run("xx ", None), run("aa aa aa aa", Some(4000))]),
        [(0, 3), (3, 15)]
    );
}

#[test]
fn retreating_to_an_earlier_break_never_lands_inside_a_fit_run() {
    // 后面一长串 `b` 放不下、交界不可断：退回本行更早的断点。截内的空格不算，
    // 退到截之前的交界（`x ` 之后），截与 `b` 一起到下一行，`b` 再按紧急断行切。
    assert_eq!(
        lines(3000, vec![run("x ", None), run("a a a", Some(1000)), run(&"b".repeat(30), None)]),
        [(0, 2), (2, 27), (27, 38)]
    );
}

#[test]
fn the_next_piece_starts_after_the_fit_width() {
    let zeros = |n: usize| "0".repeat(n);
    let laid = pages(8650, vec![run(&zeros(40), Some(2000)), run(&zeros(80), None)]);
    assert_eq!(
        pieces(&laid, 0).iter().map(|(_, x)| *x).collect::<Vec<_>>(),
        [0, 2000]
    );
}

#[test]
fn the_paragraph_mark_is_not_merged_into_a_fit_fragment() {
    // 段落标记那个空格不并进带宽的片段，否则绘制会把它一起摊开。
    let laid = pages(8650, vec![run("0000", Some(2000))]);
    let texts: Vec<String> = pieces(&laid, 0).into_iter().map(|(text, _)| text).collect();
    assert_eq!(texts, ["0000", " "]);
}

fn docx(body: &str) -> Vec<u8> {
    let mut package = Package::open(&rsword::save::blank_docx(None).unwrap()).unwrap();
    let main = package.main_part();
    package
        .replace_part_xml(main, &format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#))
        .unwrap();
    package.save().unwrap()
}

fn fit_widths(body: &str) -> Vec<Option<Twips>> {
    let (paras, _) = load_document(&docx(body)).unwrap().paragraphs();
    paras[0].runs.iter().map(|run| run.font.fit_text).collect()
}

fn fit_run(text: &str, fit: &str) -> String {
    format!(r#"<w:r><w:rPr>{fit}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

#[test]
fn the_loader_reads_fit_text_the_parser_does_not_model() {
    // 钉住的解析器只建模单元格的 `w:tcFitText`；run 级的留在 raw_unmodeled，由 load.rs 补读。
    let body = format!(
        "<w:p>{}{}</w:p>",
        fit_run("0000", r#"<w:fitText w:val="2000"/>"#),
        fit_run("1111", ""),
    );
    assert_eq!(fit_widths(&body), [Some(2000), None]);
}

#[test]
fn runs_sharing_an_id_split_the_width_by_length() {
    // 同一 `w:id` 的 run 合成一个区，总宽按 UTF-16 长度分摊（近似，未测），合计恰为 `w:val`。
    let fit = r#"<w:fitText w:val="1000" w:id="5"/>"#;
    let body = format!("<w:p>{}{}</w:p>", fit_run("aaaa", fit), fit_run("bbbbbb", fit));
    assert_eq!(fit_widths(&body), [Some(400), Some(600)]);

    let body = format!(
        "<w:p>{}{}{}</w:p>",
        fit_run("a", fit),
        fit_run("b", fit),
        fit_run("c", fit)
    );
    let widths: Vec<Twips> = fit_widths(&body).into_iter().map(Option::unwrap).collect();
    assert_eq!(widths.iter().sum::<Twips>(), 1000);
    assert!(widths.iter().all(|&w| (333..=334).contains(&w)), "{widths:?}");
}

#[test]
fn runs_without_an_id_are_separate_regions() {
    let fit = r#"<w:fitText w:val="1000"/>"#;
    let body = format!("<w:p>{}{}</w:p>", fit_run("aaaa", fit), fit_run("bbbbbb", fit));
    assert_eq!(fit_widths(&body), [Some(1000), Some(1000)]);
}

#[test]
fn a_region_with_a_tab_is_not_fitted() {
    // 制表符把 run 切成几截，一个宽度分不到几截上：整区不按 fitText 排。
    let body = r#"<w:p><w:r><w:rPr><w:fitText w:val="2000"/></w:rPr><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p>"#;
    assert_eq!(fit_widths(body), [None]);
}

#[cfg(feature = "fontenv")]
mod real {
    use super::*;
    use rsword_layout_core::{DrawCmd, RealMetrics, font::FontRegistry};
    use std::path::PathBuf;

    fn liberation() -> FontRegistry {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/fonts/LiberationSans-Regular.ttf");
        let mut registry = FontRegistry::new();
        registry.add(std::fs::read(path).expect("字体读得到"), 0).expect("字体装得进");
        registry
    }

    /// 一个画出来的片段：(片段文字, 片段起点 x_pt, 各字形 (x_pt, advance_x_pt))。
    type PaintedFragment = (String, f64, Vec<(f64, f64)>);

    fn painted(fit: Twips) -> Vec<PaintedFragment> {
        let registry = liberation();
        let metrics = RealMetrics::new(&registry);
        let mut font = FontSpec::new("Liberation Sans", 24);
        font.fit_text = Some(fit);
        let runs = vec![Run { font, ..run("0000", None) }];
        let laid = Engine::new(&metrics, setup(10_000)).layout(&[Para { runs, ..Para::default() }]);
        paint_document(&laid, Some(&registry), &registry.face_ids()).pages[0]
            .cmds
            .iter()
            .filter_map(|cmd| match cmd {
                DrawCmd::DrawGlyphs { glyphs, origin_x_pt, text, .. } => Some((
                    text.clone(),
                    *origin_x_pt,
                    glyphs.iter().map(|g| (g.x_pt, g.advance_x_pt)).collect(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn glyph_advances_add_up_to_the_fit_width() {
        for fit in [600, 2000, 8000] {
            let frags = painted(fit);
            let (_, start, glyphs) = frags.iter().find(|(text, ..)| text == "0000").expect("带宽的片段");
            let total: f64 = glyphs.iter().map(|&(_, advance)| advance).sum();
            assert!((total - f64::from(fit) / 20.0).abs() < 1e-9, "fit {fit}: {total}");
            // 差额摊在字间：首字形从片段起点画，末字形的推进量是它的自然宽度。
            assert!((glyphs[0].0 - start).abs() < 1e-9);
            // 段落标记另成片段，从目标宽的终点起画。
            let (_, mark_x, _) = frags.iter().find(|(text, ..)| text == " ").expect("段落标记");
            assert!((mark_x - (start + f64::from(fit) / 20.0)).abs() < 1e-9, "{mark_x}");
        }
    }
}
