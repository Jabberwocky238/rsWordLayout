//! 一行一条记录，无论它由几个片段拼成。
//!
//! 绘制指令是按**片段**出的——一行里换字体、上标、分页符都会把它切开。
//! 把一条指令当一行，行这一层就被 run 切碎，比较器的行层配对必然对不上，
//! 而且失败**看起来像「引擎少排了行」，其实是记账粒度错了**，
//! 查起来会往分页的方向白跑一趟。
//!
//! 实测：MR1 夹具 Word 排 20 行，按指令数则是 29 条；差额正是一行里的多个 run。
//!
//! 另一半是**空行**：空行不产生片段，也就没有任何绘制指令，于是根本没有行记录。
//! 而 Word 是给空行一条行记录的（独占一行的分页符、空段落都有）。

use rsword_layout_core::{
    Color, Engine, FontSpec, LayoutRecord, PageSetup, Para, PlaceholderKind, Run, SimpleMetrics,
    paint_document,
};

fn run(text: &str, family: &str) -> Run {
    Run {
        text: text.to_string(),
        font: FontSpec::new(family, 24),
        color: Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
    }
}

fn record(paras: Vec<Para>) -> LayoutRecord {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    LayoutRecord::from_paint(&paint_document(&engine.layout(&paras), None, &[]))
}

fn lines(record: &LayoutRecord) -> Vec<usize> {
    record.pages.iter().map(|p| p.lines.len()).collect()
}

#[test]
fn several_runs_on_one_line_make_one_record() {
    // 三个 run（换字体）本该是**一行**，而不是三条记录。
    let para = Para {
        runs: vec![run("aaa", "A"), run("bbb", "B"), run("ccc", "C")],
        ..Para::default()
    };
    let r = record(vec![para]);
    assert_eq!(lines(&r), vec![1], "一行被 run 切碎成了多条记录");
}

#[test]
fn merged_record_keeps_the_whole_source_range() {
    // 合并后行的源区间是各片段的并集——比较器按读序配对时要靠它。
    //
    // 终点是 **5 而不是 4**：源文本是 `"abcd\r"`，段落标记占第 4 个位置，
    // 而 Word 为它画一个空格（§4），引擎也画。既然那个字形算这一行的，
    // 它的源字符就得在区间里——这与 Word 侧那一行的文本区间是同一个口径。
    let para = Para {
        runs: vec![run("ab", "A"), run("cd", "B")],
        ..Para::default()
    };
    let r = record(vec![para]);
    let line = &r.pages[0].lines[0];
    let source = line.source.expect("合并后应当仍有源区间");
    assert_eq!(
        (source.start, source.end),
        (0, 5),
        "源区间没有取并集，或没有覆盖到段落标记那一个字符"
    );
}

#[test]
fn merged_record_keeps_the_real_terminator() {
    // 终止符只有行末片段带真实值；合并时不能被行中片段的 `Wrapped` 盖掉。
    use rsword_layout_core::LineTerminator;
    let para = Para {
        runs: vec![run("ab", "A"), run("cd", "B")],
        ..Para::default()
    };
    let r = record(vec![para]);
    assert_eq!(
        r.pages[0].lines[0].terminator,
        LineTerminator::ParagraphMark,
        "合并把行末的终止符丢了"
    );
}

#[test]
fn empty_paragraph_still_gets_a_line_record() {
    // 空段落不产生片段。没有兜底的话它**一条记录都没有**，
    // 在比较器里表现为「引擎少排了行」。
    let r = record(vec![
        Para { runs: vec![run("x", "A")], ..Para::default() },
        Para { runs: vec![run("", "A")], ..Para::default() },
        Para { runs: vec![run("y", "A")], ..Para::default() },
    ]);
    assert_eq!(lines(&r), vec![3], "空段落没有行记录");
}

#[test]
fn a_lone_page_break_gets_its_own_line_record() {
    // 实测形状：MR1 的 `'\u{FFFC}B07 leading break'`——分页符独占一行。
    // Word 给它一条行记录（`'\x0c'` 自成一行），引擎也必须有，否则该页行数少一。
    let para = Para {
        runs: vec![Run {
            text: "\u{FFFC}后".to_string(),
            font: FontSpec::new("A", 24),
            color: Color::BLACK,
            placeholders: vec![PlaceholderKind::PageBreak],
        rise: 0,
        rise_fine: None,
        }],
        ..Para::default()
    };
    let r = record(vec![para]);
    // 第一页是那条空的分页行，第二页是「后」。
    assert_eq!(lines(&r), vec![1, 1], "独占的分页符没有自己的行记录");
    assert_eq!(r.pages[0].lines[0].glyph_count(), 0, "分页行不该有字形");
}

#[test]
fn line_records_survive_a_wrapped_paragraph() {
    // 换行产生的多行，每行仍是一条记录。
    let long = "word ".repeat(60);
    let r = record(vec![Para {
        runs: vec![run(long.trim_end(), "A")],
        ..Para::default()
    }]);
    let total: usize = lines(&r).iter().sum();
    assert!(total > 1, "这段应当换过行");
    // 每条记录的源区间首尾相接，不重叠也不留洞。
    let ranges: Vec<_> = r
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .filter_map(|l| l.source.map(|s| (s.start, s.end)))
        .collect();
    for pair in ranges.windows(2) {
        assert_eq!(pair[0].1, pair[1].0, "行与行之间的源区间不连续：{ranges:?}");
    }
}

/// 段落标记**自己也要画出一个字形**（一个空格），不能只在契约里声明。
///
/// 量具方法 §4 是从 Word 实测来的：段落标记画 1 个空格，软回车画 1 个，
/// 分节符画 0 个。`LineTerminator::expected_glyphs` 就是那张表。
///
/// 引擎原来只报数不画：每行的字形数比 Word 少 1，比较器一上来就
/// `GLYPH_COUNT_MISMATCH`，**结构对不上，几何一条都比不了**——
/// 在 accumulation 夹具上是 pairs=0，修好之后是 pairs=1340。
/// 所以这条挡的不是一个小偏差，是「整套比较能不能开工」。
#[test]
fn a_paragraph_mark_draws_its_own_space_glyph() {
    use rsword_layout_core::{DrawCmd, Fragment};

    let para = Para { runs: vec![run("ab", "A")], ..Para::default() };
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let pages = engine.layout(&[para]);

    let texts: Vec<String> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts.concat(),
        "ab ",
        "段落标记没有画出那个空格：{texts:?}"
    );

    // 声明与实际画出的必须一致——两者分家正是原来的毛病。
    let list = paint_document(&pages, None, &[]);
    for page in &list.pages {
        for cmd in &page.cmds {
            if let DrawCmd::DrawGlyphs { text, terminator, .. } = cmd {
                assert_eq!(
                    text.chars().rev().take_while(|c| *c == ' ').count(),
                    terminator.expected_glyphs(),
                    "自报 {} 个终止符字形，实际画了 {:?}",
                    terminator.expected_glyphs(),
                    text
                );
            }
        }
    }
}

/// 分节符**一个字形也不画**——同一张表的另一行，方向相反。
///
/// 只测「画了」不测「不该画的不画」，等于只锁住一半：把 `expected_glyphs`
/// 改成恒返回 1 也能全绿。
#[test]
fn a_section_break_draws_nothing_extra() {
    use rsword_layout_core::{Fragment, LineTerminator};

    let para = Para {
        runs: vec![run("ab", "A")],
        terminator: LineTerminator::SectionBreak,
        ..Para::default()
    };
    let metrics = SimpleMetrics;
    let pages = Engine::new(&metrics, PageSetup::a4()).layout(&[para]);
    let texts: Vec<String> = pages[0]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts.concat(), "ab", "分节符不该画字形：{texts:?}");
}
