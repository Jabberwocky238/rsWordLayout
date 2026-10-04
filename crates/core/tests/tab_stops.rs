//! 制表符与制表位：停到哪、占多宽、断在哪。
//!
//! 行界规则来自 Android Word 的实测（`word_analyse/reports/rsword-diff/tab.md`，
//! 窄路径 5329、纸页路径 10466）；这里用桩度量复现**规则本身**，不复现 Calibri 的宽度——
//! 桩里一个 `0` 是 120 twips，所以断点数与实测夹具不同，按桩宽度各自算出期望值。
//! 默认档大多直接写成 `Some(221)`，与平台无关；平台怎么补缺省值单有一条测试。
//! 用手机 Calibri 复现实测起点的在 `tab_stops_calibri.rs`（要设 `RSWORD_TEST_CALIBRI`）。
//!
//! 标「未实测」的断言（有的以 `assumed_` 开头）钉的是照规范／Word 桌面版行为、或为了守住
//! 「制表符不把后面的字推出行尾、不把放得下的词切开」而做的选择，不是 Android 的观测。

use rsword_layout_core::{
    Align, Color, Engine, FontMetrics, FontSpec, Fragment, LineTerminator, Margins, Page, PageSetup,
    Para, Platform, Run, SimpleMetrics, Size, TabAlign, TabLeader, TabStop, TextFragment, Twips, View,
    paras_from_document,
};

/// 左边距。片段的 x 减去它就是相对左页边距的位置。
const MARGIN: Twips = 720;

fn setup(content_width: Twips) -> PageSetup {
    PageSetup {
        size: Size::new(content_width + 2 * MARGIN, 16838),
        margins: Margins::uniform(MARGIN),
    }
}

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        // 12pt：桩里一个 `0`／`A` 是 120 twips，空格 60。
        font: FontSpec::new("Calibri", 24),
        color: Color::BLACK,
        placeholders: Vec::new(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn para(text: &str, tabs: Vec<TabStop>, default_tab_stop: Twips) -> Para {
    Para {
        runs: vec![run(text)],
        tabs,
        default_tab_stop: Some(default_tab_stop),
        ..Para::default()
    }
}

fn stop(pos: Twips, align: TabAlign) -> TabStop {
    TabStop { pos, align, leader: TabLeader::None }
}

fn layout(paras: &[Para], content_width: Twips) -> Vec<Page> {
    Engine::new(&SimpleMetrics, setup(content_width)).layout(paras)
}

/// 每行的源起点（段内 UTF-16，与实测的 `cpFirst` 同一口径）。
fn starts(pages: &[Page]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut last_line = None;
    for page in pages {
        for f in &page.fragments {
            if let Fragment::Text(t) = f {
                let key = (page as *const Page, t.line);
                if last_line != Some(key) {
                    out.push(t.source.expect("每个片段都带源区间").0);
                    last_line = Some(key);
                }
            }
        }
    }
    out
}

/// 从 `source_start` 起的那个片段的 x，相对左页边距。
fn x_of(pages: &[Page], source_start: u32) -> Twips {
    pages
        .iter()
        .flat_map(|p| &p.fragments)
        .find_map(|f| match f {
            Fragment::Text(t) if t.source.map(|s| s.0) == Some(source_start) && !t.text.is_empty() => {
                Some(t.x - MARGIN)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("没有从 {source_start} 起的片段"))
}

fn zeros(n: usize) -> String {
    "0".repeat(n)
}

#[test]
fn default_tab_at_line_start_goes_to_the_first_default_stop() {
    // 对应 `tab-zeros`：行首制表符停在默认档上，不是 720，也不是 U+0009 的字宽。
    // 桩：(5329 − 221) / 120 = 42.6 → 制表符后 42 个 `0`，下一行从 43 起。
    let pages = layout(&[para(&format!("\t{}", zeros(80)), vec![], 221)], 5329);
    assert_eq!(starts(&pages), [0, 43]);
    assert_eq!(x_of(&pages, 1), 221, "制表符后的文字从默认档起排");
}

#[test]
fn a_tab_breaks_before_itself_not_after() {
    // 对应 `tab-after-a`（Word 起点 0、1、43）：`A` 单独一行，制表符跟着后面的 `0` 下去，
    // 到新行行首重新停到默认档。若断在制表符之后，第二行应从 2 起。
    let pages = layout(&[para(&format!("A\t{}", zeros(80)), vec![], 221)], 5329);
    assert_eq!(starts(&pages), [0, 1, 44]);
    assert_eq!(x_of(&pages, 2), 221, "挪下来的制表符在新行行首重新落位");
}

#[test]
fn a_tab_that_fits_stays_and_stops_on_the_grid() {
    // `A` 占到 120，下一个默认档是 221：制表符宽 101，而不是固定 221。
    // **未实测**：Android 只量过行首制表符，默认档是栅格还是固定宽分不开（见 bridge.rs）。
    let pages = layout(&[para("A\tB", vec![], 221)], 5329);
    assert_eq!(starts(&pages), [0]);
    assert_eq!(x_of(&pages, 2), 221);
}

#[test]
fn custom_left_stop_is_honoured_and_hides_nearer_default_stops() {
    // 对应 `tab-stop-720` / `tab-stop-1440`（Word 起点 0、1、39 与 0、1、33、76）：
    // 行首制表符停在写下的位置，没有先停在更近的默认档（221）上。
    // 桩：(5329 − 720) / 120 = 38.4 → 38 个；(5329 − 1440) / 120 = 32.4 → 32 个。
    let text = format!("A\t{}", zeros(80));
    let p720 = layout(&[para(&text, vec![stop(720, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&p720), [0, 1, 40]);
    assert_eq!(x_of(&p720, 2), 720);
    let p1440 = layout(&[para(&text, vec![stop(1440, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&p1440), [0, 1, 34, 78]);
    assert_eq!(x_of(&p1440, 2), 1440);
}

#[test]
fn default_stops_resume_after_the_last_custom_stop_on_the_margin_grid() {
    // 第一个制表符停在 720；第二个没有自定义制表位可去，落到 720 之后的默认档：
    // 从左页边距起每 500 一档，所以是 1000，不是 720 + 500。**栅格锚点未实测**，照规范。
    let pages = layout(&[para("\t\tX", vec![stop(720, TabAlign::Left)], 500)], 5329);
    assert_eq!(x_of(&pages, 2), 1000);
}

#[test]
fn assumed_a_stop_exactly_at_the_pen_is_skipped() {
    // 两个 `0` 正好到 240：制表位 240 不能再用，去下一个（480）。**未实测**：
    // 没有夹具让笔位恰好落在制表位上而结果有别（「严格大于」是假设）。
    let pages = layout(
        &[para("00\tX", vec![stop(240, TabAlign::Left), stop(480, TabAlign::Left)], 221)],
        5329,
    );
    assert_eq!(x_of(&pages, 3), 480);
}

#[test]
fn right_tab_that_fits_ends_the_text_at_the_stop() {
    // 对应 `tab-right-fit` 的第一行（`A`、右对齐 1440、`00`，Word 这一行到 4 为止）：
    // `00` 收在 1440 上。行内落位 **未实测**，只有行界可对照。
    let text = format!("A\t00\t{}", zeros(50));
    let pages = layout(&[para(&text, vec![stop(1440, TabAlign::Right)], 221)], 5329);
    assert_eq!(x_of(&pages, 2), 1440 - 240);
    // 第二个制表符后面的 50 个 `0` 放不下，断在它之前；它到新行行首又遇上右对齐 1440，
    // 那段比 1440 宽，于是不占宽度：新行是制表符加 44 个 `0`（桩里每行 44 个）。
    assert_eq!(starts(&pages), [0, 4, 49]);
}

#[test]
fn right_tab_whose_text_does_not_fit_takes_no_width() {
    // 对应 `tab-right-1440`（Word 起点 0、1、45 = 制表符加 43 个 `0`，与没有制表符时相同）。
    let pages = layout(
        &[para(&format!("A\t{}", zeros(80)), vec![stop(1440, TabAlign::Right)], 221)],
        5329,
    );
    assert_eq!(starts(&pages), [0, 1, 46]);
    assert_eq!(x_of(&pages, 2), 0);
}

#[test]
fn center_and_decimal_tabs_align_the_following_text() {
    // **未实测**，照规范：居中对齐那段的中点；小数点对齐第一个 `.`，没有 `.` 时同右对齐。
    let center = layout(&[para("\tABCD", vec![stop(3000, TabAlign::Center)], 221)], 5329);
    assert_eq!(x_of(&center, 1), 3000 - 240);
    let decimal = layout(&[para("\t12.5", vec![stop(3000, TabAlign::Decimal)], 221)], 5329);
    assert_eq!(x_of(&decimal, 1), 3000 - 240, "`12` 在小数点之前");
    let integer = layout(&[para("\t125", vec![stop(3000, TabAlign::Decimal)], 221)], 5329);
    assert_eq!(x_of(&integer, 1), 3000 - 360, "没有小数点时整段收在制表位上");
}

#[test]
fn right_tab_segment_ends_at_the_next_tab_and_spans_runs() {
    // 那段文字到下一个制表符为止，跨 run 累加：`1` 在一个 run、`2.` 在另一个 run。
    let mut p = para("\t1", vec![stop(3000, TabAlign::Right), stop(4000, TabAlign::Left)], 221);
    p.runs.push(run("2\tX"));
    let pages = layout(&[p], 5329);
    assert_eq!(x_of(&pages, 1), 3000 - 240);
    assert_eq!(x_of(&pages, 4), 4000);
}

#[test]
fn bar_stops_are_not_stopping_points() {
    // 竖线制表位只画线（本版不画），制表符越过它。
    let pages = layout(
        &[para("\tX", vec![stop(500, TabAlign::Bar), stop(1000, TabAlign::Left)], 221)],
        5329,
    );
    assert_eq!(x_of(&pages, 1), 1000);
}

#[test]
fn stops_are_measured_from_the_margin_not_the_indent() {
    // 左缩进 300：行首在 300，默认档仍从页边距起算，所以停在 720（制表符宽 420）。
    let mut p = para("\tX", vec![], 720);
    p.indent_left = 300;
    let pages = layout(&[p], 5329);
    assert_eq!(x_of(&pages, 1), 720);
}

#[test]
fn hanging_indent_is_an_implicit_stop_on_the_first_line() {
    // **未实测**，照 Word 桌面版：首行悬挂时左缩进处有一个隐含制表位（列表编号靠它）。
    // 首行起点 360，`1.` 到 600，下一个默认档本是 663，隐含制表位 720 更近却排在自定义一侧。
    let mut p = para(&format!("1.\tX {}", "y ".repeat(60)), vec![], 221);
    p.indent_left = 720;
    p.indent_first_line = -360;
    let pages = layout(&[p], 5329);
    assert_eq!(x_of(&pages, 3), 720);
}

#[test]
fn assumed_a_left_stop_beyond_the_line_keeps_the_tab_and_wraps_the_text() {
    // **未实测**：左对齐制表位超出行宽时，制表符只推到行尾、留在本行，后面的字另起一行，
    // 前面不带制表符。原型在这里把制表符挪下去，于是中间多出一条只有制表符的行（0、2、3）。
    // 待测：左对齐 6480 的 `A<TAB>000` @5329。
    let text = format!("AB\t{}", zeros(3));
    let pages = layout(&[para(&text, vec![stop(9000, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&pages), [0, 3]);
    assert_eq!(x_of(&pages, 3), 0, "另起一行的字从行首排");
    // 制表符停在行首时也一样够不着：它自成一行，`000` 整个另起一行——只有制表符的行
    // 唯一的来处（见 `assumed_no_tab_strands_or_splits_the_text_after_it`）。原先把行首的
    // 制表符行当空行、至少收一个字，于是第一个 `0` 画在行尾之外（0、2）。
    let text = format!("\t{}", zeros(3));
    let pages = layout(&[para(&text, vec![stop(9000, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&pages), [0, 1]);
    assert_eq!(x_of(&pages, 1), 0);
    // 后面是放不下任何一行的长串时也不例外：制表符之后一个字都塞不进去，就不塞；
    // 长串到下一行（空行）上照紧急断行切，每行 44 个。
    let text = format!("\t{}", zeros(80));
    let pages = layout(&[para(&text, vec![stop(9000, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&pages), [0, 1, 45]);
}

#[test]
fn assumed_a_word_that_fits_an_empty_line_is_not_split_after_a_tab() {
    // **未实测**：制表符后面的词在这一行放不下、在空行上放得下时，不在词中间切开，
    // 也不把它的第一个字推出行尾。照 `tab.md` 的几条实测规则硬推（制表符之前可断、之后不可断，
    // 行首之后没有断点就按剩余宽度切），Word 会切开它；实测夹具的后面全是放不下任何一行的长串，
    // 覆盖不到这里。待测：`\t` + 94 个 `i` @5329（照这里 0、1，硬推 0、93），
    // 左对齐 5040 的 `Name:<TAB>Date: today` @5329（照这里 0、6，硬推 0、5、8）。
    //
    // 行首制表符的停靠点在行外（5760）或离行尾太近（5040）：制表符自成一行，词整个下去。
    // 原先给 `\tS` | `incerely,` 与 `\tSi` | `ncerely,`，`S` 画在 5329 上。
    for pos in [5760, 5040] {
        let pages = layout(&[para("\tSincerely,", vec![stop(pos, TabAlign::Left)], 221)], 5329);
        assert_eq!(starts(&pages), [0, 1], "左对齐 {pos}");
        assert_eq!(x_of(&pages, 1), 0, "左对齐 {pos}");
    }
    // 前面有字时：挪下去制表符还停在同一个 5040，词照样上不来，所以不挪——制表符留在
    // `Name:` 之后，`Date: today` 从下一行行首起。原先给 `Name:` | `\tDa` | `te: today`。
    let pages =
        layout(&[para("Name:\tDate: today", vec![stop(5040, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&pages), [0, 6]);
    assert_eq!(x_of(&pages, 6), 0);
    // 停靠点在行内（5300）、只差一点：`A<TAB>` | `B C`。原先给 `A` | `\tB ` | `C`，`B` 越出行尾。
    let pages = layout(&[para("A\tB C", vec![stop(5300, TabAlign::Left)], 221)], 5329);
    assert_eq!(starts(&pages), [0, 2]);
    // 挪下去有地方时照旧断在制表符之前（实测的方向）：下一行行首的制表符停在更近的 1440，
    // `Date:` 放得下。
    let pages = layout(
        &[para(
            "Name: John Smith Jr\tDate:",
            vec![stop(1440, TabAlign::Left), stop(5040, TabAlign::Left)],
            221,
        )],
        5329,
    );
    assert_eq!(starts(&pages), [0, 19]);
    assert_eq!(x_of(&pages, 20), 1440);
    // 桌面缺省档 720、行宽 9026：第 13 个制表符的停靠点 9360 在行外，`Sincerely,` 整个下去。
    // 原先给 13 个制表符加 `S` | `incerely,`。Android 的缺省档 221 之下一行放得下。
    let p = Para { runs: vec![run(&format!("{}Sincerely,", "\t".repeat(13)))], ..Para::default() };
    let on = |platform| {
        Engine::new(&SimpleMetrics, setup(9026))
            .with_platform(platform, View::Print)
            .layout(std::slice::from_ref(&p))
    };
    assert_eq!(starts(&on(Platform::Desktop)), [0, 13]);
    assert_eq!(starts(&on(Platform::Android)), [0]);
    // 词后还跟着空格与别的字，行宽只比词宽出不到一个空格：空行上这个词照样不切开
    // （`fit` 连「词 + 空格」都放不下，按字符收，收下整个词、空格吃掉），所以「空行上放得下」
    // 去掉词后的空格再比，制表符之后也不切。桩里 `Sincerely,` 1200、`Sincerely, ` 1260，
    // 缺省档 221。原先给 `\tSincerel` | `y, x` 与 `A` | `\tSincerel` | `y, x`：后面多一截字，
    // 前面的词就被切开。现在与不带制表符的同一截字断法一致。
    for width in [1200, 1230, 1250, 1259] {
        let plain = layout(&[para("Sincerely, x", vec![], 221)], width);
        assert_eq!(starts(&plain), [0, 11], "@{width}");
        let pages = layout(&[para("\tSincerely, x", vec![], 221)], width);
        assert_eq!(starts(&pages), [0, 1, 12], "@{width}");
        let pages = layout(&[para("A\tSincerely, x", vec![], 221)], width);
        assert_eq!(starts(&pages), [0, 2, 13], "@{width}");
    }
    // 一个空格也放得下时本来就对：`\t` | `Sincerely, ` | `x`。
    let pages = layout(&[para("\tSincerely, x", vec![], 221)], 1260);
    assert_eq!(starts(&pages), [0, 1, 12]);
}

#[test]
fn justification_does_not_move_text_before_the_last_tab() {
    // 两端对齐只拉开最后一个制表符之后的部分。**未实测**，照 Word 的已知行为。
    let mut p = para(&format!("A\t{}", "B ".repeat(60)), vec![], 221);
    p.align = Align::Justify;
    let pages = layout(&[p], 5329);
    assert_eq!(x_of(&pages, 0), 0);
    assert_eq!(x_of(&pages, 2), 221, "制表符后的文字仍从制表位起");

    // 单 run 的行本来就不拉伸（空隙只摊在片段交界上），上面那条几乎是空的。这里让制表符
    // 之后有多个 run：`A`、制表符、`BB `、`CC ` 排进 1000 宽的一行，行宽 821、余 179。
    // `A` 与制表符之间是断点（制表符之前可断），不排除的话它也分一份空隙，制表符后的字
    // 就离开了 221；实际只有 `BB ` | `CC ` 那一条交界拿到全部 179。
    let mut p = para("A\tBB ", vec![], 221);
    p.runs.extend([run("CC "), run("DD "), run("EEEEEEEE")]);
    p.align = Align::Justify;
    let pages = layout(&[p], 1000);
    assert_eq!(starts(&pages)[..2], [0, 8]);
    assert_eq!(x_of(&pages, 0), 0);
    assert_eq!(x_of(&pages, 2), 221, "制表符后的第一片不动");
    assert_eq!(x_of(&pages, 5), 221 + 300 + 179, "余量全摊在制表符之后的交界上");
}

#[test]
fn shared_break_rule_puts_the_opportunity_before_a_tab() {
    let offsets: Vec<usize> =
        SimpleMetrics.break_opportunities("A\t0").iter().map(|b| b.offset).collect();
    assert_eq!(offsets, [1, 3], "制表符之前可断，之后不可断");
}

#[test]
fn bridge_merges_style_tabs_and_reads_the_default_tab_stop() {
    let doc = serde_json::json!({
        "settings": {"defaultTabStop": 420},
        "styles": {"styles": [
            {"kind": "paragraph", "styleId": "Normal", "isDefault": true,
             "ppr": {"tabs": {"tab": [{"pos": 2000, "val": "left"}]}}},
            {"kind": "paragraph", "styleId": "S1", "basedOn": "Normal",
             "ppr": {"tabs": {"tab": [{"pos": 2000, "val": "clear"},
                                      {"pos": 5000, "val": "right", "leader": "dot"}]}}}
        ]},
        "main": [
            {"kind": "text", "props": {},
             "inlines": [{"kind": "run", "text": "a\tb"}]},
            {"kind": "text", "styleId": "S1",
             "props": {"tabs": {"tab": [{"pos": 3000, "val": "center"},
                                        {"pos": 100, "val": "bar"}]}},
             "inlines": [{"kind": "run", "text": "a\tb"}]}
        ]
    });
    let (paras, _) = paras_from_document(&doc);
    assert_eq!(paras[0].default_tab_stop, Some(420));
    assert_eq!(paras[0].tabs, [stop(2000, TabAlign::Left)], "默认段落样式的制表位");
    assert_eq!(
        paras[1].tabs,
        [
            stop(100, TabAlign::Bar),
            stop(3000, TabAlign::Center),
            TabStop { pos: 5000, align: TabAlign::Right, leader: TabLeader::Dot },
        ],
        "clear 删掉继承来的 2000，直接格式叠在样式链之上"
    );
}

#[test]
fn bridge_leaves_a_missing_default_tab_stop_to_the_engine() {
    // 没写 `w:defaultTabStop`：桥接层不知道在模拟哪个平台，不填数，由引擎按平台补。
    let doc = serde_json::json!({"main": [
        {"kind": "text", "props": {}, "inlines": [{"kind": "run", "text": "\tx"}]}
    ]});
    let (paras, _) = paras_from_document(&doc);
    assert_eq!(paras[0].default_tab_stop, None);
    assert!(paras[0].tabs.is_empty());
    // 写成 0 按没写处理（**假设**，规范没说）。
    let doc = serde_json::json!({"settings": {"defaultTabStop": 0}, "main": [
        {"kind": "text", "props": {}, "inlines": [{"kind": "run", "text": "\tx"}]}
    ]});
    let (paras, _) = paras_from_document(&doc);
    assert_eq!(paras[0].default_tab_stop, None);
}

#[test]
fn the_missing_default_tab_stop_depends_on_the_platform() {
    // 文档没写 `w:defaultTabStop`（`None`）：桌面照规范 720（Mac 上**未实测**），
    // Android 用拟合值 221（见 `layout.rs` 的 `ANDROID_MISSING_DEFAULT_TAB_STOP`：
    // 记分器分不出 221 与 248，钉住这个数的只有这里）。
    let p = Para { runs: vec![run("\tX")], ..Para::default() };
    assert_eq!(p.default_tab_stop, None);
    let x = |p: &Para, engine: Engine<'_, SimpleMetrics>| x_of(&engine.layout(std::slice::from_ref(p)), 1);
    let on = |platform, view| Engine::new(&SimpleMetrics, setup(5329)).with_platform(platform, view);
    assert_eq!(x(&p, Engine::new(&SimpleMetrics, setup(5329))), 720, "库的默认是桌面");
    assert_eq!(x(&p, on(Platform::Desktop, View::Print)), 720);
    assert_eq!(x(&p, on(Platform::Android, View::Print)), 221);
    // 视图不参与：窄路径（移动视图）与纸页路径用同一个值（**假设**，见常数的说明）。
    assert_eq!(x(&p, on(Platform::Android, View::Mobile)), 221);
    // 文档写了就照用，两个平台一样（Android 认不认它**未实测**）。
    let written = Para { default_tab_stop: Some(420), ..p.clone() };
    assert_eq!(x(&written, on(Platform::Desktop, View::Print)), 420);
    assert_eq!(x(&written, on(Platform::Android, View::Mobile)), 420);
}

#[test]
fn assumed_a_right_stop_beyond_the_line_is_clamped_to_the_line_end() {
    // **未实测**：右对齐制表位写在纸页右边距（10466）上，窄路径（5329）上它在行外。
    // 停靠点夹到行尾：`12` 收在行尾，整段一行——与没有制表位模型之前的一行相同。
    // 原型不夹，`12` 怎么也放不下，排成 (0,5)、(5,6)、(6,9) 三行，中间一行只有制表符。
    // 待测：右对齐 10466 的 `Title<TAB>12` @5329。
    let pages = layout(&[para("Title\t12", vec![stop(10466, TabAlign::Right)], 221)], 5329);
    assert_eq!(starts(&pages), [0]);
    assert_eq!(x_of(&pages, 6), 5329 - 240, "`12` 收在行尾");
    // 居中的那段也不越过行尾：照 5233 居中 `Center` 会越出 5329，改成收在行尾。`Right` 那一截
    // 放不下，断在第二个制表符之前；它到新行行首先遇上的是居中 5233，同样收在行尾。
    // 原型不夹，给 5 行（没有制表位模型之前是 1 行）。
    let pages = layout(
        &[para(
            "Left\tCenter\tRight",
            vec![stop(5233, TabAlign::Center), stop(10466, TabAlign::Right)],
            221,
        )],
        5329,
    );
    assert_eq!(starts(&pages), [0, 11]);
    assert_eq!(x_of(&pages, 5), 5329 - 720, "`Center` 收在行尾");
    assert_eq!(x_of(&pages, 12), 5329 - 600, "`Right` 收在行尾");
}

/// 文字片段按行分组，按排出的顺序。
fn lines_of(pages: &[Page]) -> Vec<Vec<&TextFragment>> {
    let mut lines: Vec<(usize, u32, Vec<&TextFragment>)> = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        for f in &page.fragments {
            if let Fragment::Text(t) = f {
                match lines.last_mut() {
                    Some((p, l, frags)) if *p == page_index && *l == t.line => frags.push(t),
                    _ => lines.push((page_index, t.line, vec![t])),
                }
            }
        }
    }
    lines.into_iter().map(|(_, _, frags)| frags).collect()
}

/// 段内 UTF-16 偏移上的那个字符在哪个 run 的哪个字节上；段末给 `None`。
fn locate(p: &Para, utf16: u32) -> Option<(usize, usize)> {
    let mut at = 0u32;
    for (index, r) in p.runs.iter().enumerate() {
        for (byte, c) in r.text.char_indices() {
            if at == utf16 {
                return Some((index, byte));
            }
            at += c.len_utf16() as u32;
        }
    }
    None
}

/// 一个词（到断点为止的一截）在宽 `empty` 的空行上放不放得下：去掉词后的空格再比。
/// 空行上 `fit` 连「词 + 空格」都放不下时，引擎按字符收，词照收、空格随后吃掉，
/// 所以词本身放得下就不会被切开。
fn fits_empty_line<M: FontMetrics>(metrics: &M, word: &str, font: &FontSpec, empty: Twips) -> bool {
    metrics.measure(word.trim_end_matches(' '), font).advance <= empty
}

/// 制表符与后面那截字之间守着的三条（**假设**，见 `layout.rs` 里 `Engine::shortfall`
/// 的（丙）（丁））。违反的逐条列出。
///
/// 1. 只有制表符的行（段末、显式换行之前的不算）只出现在行首的制表符后面一个字都上不来的
///    时候：下一行开头那截字在空行上放得下，它到第一个断点的前缀就比制表符之后剩下的宽；
///    放不下，它的第一个字符就比剩下的宽。
/// 2. 各 run 同一字体的段：换行不落在空行上放得下的词中间（词以断点与制表符为界）。词可以跨 run——
///    跨 run 回退之后拆不拆 run 断法都一样，所以把各 run 的文字拼起来查。
/// 3. 紧跟在制表符之后的字不越出行尾（容 1 twip 的取整）。
///
/// 1、2 两条里的「那截字」都拼上后面的 run 再截（到下一个制表符为止），用第一个 run 的字体量——
/// 样本里各 run 字体相同。
///
/// 比较宽度时容 1 twip：引擎比的是整 twips，这里的「剩下」按精确落位算。
///
/// 两种口径的宽度不要混用。「空行上放不放得下」去掉词后的空格（[`fits_empty_line`]）：
/// 空行上词照收、空格随后吃掉。「制表符之后上不上得来」（第 1 条的 `room`）含词后的空格：
/// 制表符之后不是空行，照有内容的行的口径，空格放不下整词就换行。
///
/// `spaces_hang`：行尾空格挂在行宽之外（Android，`Engine::trailing_spaces_hang`）时，「越出行尾」只量
/// 片段去掉末尾空格之后的部分——挂出去的空格本来就在行尾之外。
fn tab_violations<M: FontMetrics>(
    metrics: &M, p: &Para, width: Twips, pages: &[Page], spaces_hang: bool,
) -> Vec<String> {
    // 区间宽不小于 0（`Span::width`）；空行（不是首行）的宽度与引擎判断「空行上放不放得下」同一口径。
    let span = (width - p.indent_left - p.indent_right).max(0);
    let empty = span.max(1);
    // 行尾照引擎的算法：行首 = 区间起点 + 首行缩进，行宽 = 区间宽 − 首行缩进（至少 1）。
    // 缩进比行宽还大时区间是空的，首行会伸出右边距——那是既有的几何，与制表符无关。
    let line_end = |index: usize| {
        let first = if index == 0 { p.indent_first_line } else { 0 };
        f64::from(MARGIN + p.indent_left + first + (span - first).max(1))
    };
    let lines = lines_of(pages);
    let mut bad = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let line_end = line_end(index);
        for pair in line.windows(2) {
            let (tab, text) = (pair[0], pair[1]);
            if tab.tab_advance_pt.is_some()
                && text.tab_advance_pt.is_none()
                && !text.text.is_empty()
                && text.terminator == LineTerminator::Wrapped
            {
                let visible = if spaces_hang { text.text.trim_end_matches(' ') } else { text.text.as_str() };
                let end = (text.x_pt + metrics.advance_pt(visible, &text.font)) * 20.0;
                if !visible.is_empty() && end > line_end + 1.0 {
                    bad.push(format!("制表符后的 {:?} 越出行尾：{end} > {line_end}", text.text));
                }
            }
        }
        let wrapped = line.iter().all(|t| t.terminator == LineTerminator::Wrapped);
        let only_tabs = line.iter().any(|t| t.tab_advance_pt.is_some())
            && line.iter().all(|t| t.text.chars().all(|c| c == '\t'));
        if !(wrapped && only_tabs) {
            continue;
        }
        let tabs_end = line
            .iter()
            .filter_map(|t| t.tab_advance_pt.map(|a| (t.x_pt + a) * 20.0))
            .fold(f64::MIN, f64::max);
        let room = line_end - tabs_end;
        let next = lines.get(index + 1).and_then(|l| l.first()).and_then(|t| t.source);
        let Some((r, byte)) = next.and_then(|(start, _)| locate(p, start)) else {
            bad.push(format!("只有制表符的行 {:?} 之后没有字", line[0].source));
            continue;
        };
        let run = &p.runs[r];
        let after: String = std::iter::once(&run.text[byte..])
            .chain(p.runs[r + 1..].iter().map(|r| r.text.as_str()))
            .collect();
        let chunk = &after[..after.find('\t').unwrap_or(after.len())];
        if chunk.is_empty() {
            bad.push(format!("只有制表符的行 {:?} 断在两个制表符之间", line[0].source));
            continue;
        }
        let end = metrics
            .break_opportunities(chunk)
            .iter()
            .map(|o| o.offset)
            .find(|&o| o > 0)
            .unwrap_or(chunk.len());
        let unit = if fits_empty_line(metrics, &chunk[..end], &run.font, empty) {
            &chunk[..end]
        } else {
            &chunk[..chunk.chars().next().map_or(0, char::len_utf8)]
        };
        let w = f64::from(metrics.measure(unit, &run.font).advance);
        if w <= room - 1.0 {
            bad.push(format!("只有制表符的行之后的 {unit:?}（{w}）放得进制表符后剩下的 {room}"));
        }
    }
    if p.runs.iter().all(|r| r.font == p.runs[0].font) {
        let run = &p.runs[0];
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        let text = text.as_str();
        let opportunities: Vec<usize> =
            metrics.break_opportunities(text).iter().map(|o| o.offset).collect();
        for line in lines.iter().skip(1) {
            let Some((r, b)) = line.first().and_then(|t| t.source).and_then(|(s, _)| locate(p, s)) else {
                continue;
            };
            // 拼起来之后的字节偏移。
            let b = p.runs[..r].iter().map(|r| r.text.len()).sum::<usize>() + b;
            if b == 0 || text[..b].ends_with('\t') || text[b..].starts_with('\t') || opportunities.contains(&b) {
                continue;
            }
            let from = opportunities
                .iter()
                .copied()
                .filter(|&o| o < b)
                .chain(text[..b].rfind('\t').map(|i| i + 1))
                .max()
                .unwrap_or(0);
            let to = opportunities
                .iter()
                .copied()
                .filter(|&o| o > b)
                .chain(text[b..].find('\t').map(|i| b + i))
                .min()
                .unwrap_or(text.len());
            let word = &text[from..to];
            if fits_empty_line(metrics, word, &run.font, empty) {
                bad.push(format!("空行上放得下（≤ {empty}）的 {word:?} 在字节 {b} 处被切开"));
            }
        }
    }
    bad
}

/// 线性同余，够用就行：测试不引外部 crate。
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// `tab_violations` 用的手写样本：审查与原型排坏过的形状。
fn hand_tab_cases() -> Vec<(Para, Twips)> {
    vec![
        (para(&format!("\t\t{}", zeros(40)), vec![], 221), 2400),
        (para(&format!("\t\t{}", zeros(40)), vec![], 221), 5329),
        (para("A\t000", vec![stop(10000, TabAlign::Left)], 221), 2400),
        (para("\t000", vec![stop(10000, TabAlign::Left)], 221), 2400),
        (para("Name:\tDate:", vec![stop(6480, TabAlign::Left)], 221), 5329),
        (para("A\tB C", vec![stop(5300, TabAlign::Left)], 221), 5329),
        (para(&format!("A\t\t{}", zeros(60)), vec![], 221), 5329),
        (para(&format!("\t\t\t{}", zeros(60)), vec![stop(5329, TabAlign::Left)], 221), 5329),
        (para("\tSincerely,", vec![stop(5760, TabAlign::Left)], 221), 5329),
        (para("\tSincerely,", vec![stop(5040, TabAlign::Left)], 221), 5329),
        (para("Name:\tDate: today", vec![stop(5040, TabAlign::Left)], 221), 5329),
        (para("Name: John Smith\tDate:", vec![stop(8800, TabAlign::Left)], 221), 9026),
        (para(&format!("{}Sincerely,", "\t".repeat(13)), vec![], 720), 9026),
        (para(&format!("\t{}", "i".repeat(94)), vec![], 221), 5329),
        // 词后跟空格：行宽只比词宽出不到一个空格（`Sincerely,` 1200、加空格 1260）。
        (para("\tSincerely, x", vec![], 221), 1230),
        (para("\tSincerely, x", vec![], 221), 1259),
        (para("A\tSincerely, x", vec![], 221), 1230),
        (para("A\tSincerely, x", vec![], 221), 1259),
        (para(&format!("\t{} x", zeros(44)), vec![], 221), 5300),
    ]
}

/// `tab_violations` 用的随机组合（种子固定）：几种字、几种制表位（含行外、负位置、竖线）、
/// 几种行宽、跨 run、悬挂缩进。
fn random_tab_cases() -> Vec<(Para, Twips)> {
    let mut cases: Vec<(Para, Twips)> = Vec::new();
    let tokens =
        ["\t", "\t\t", "A", "ab ", " ", "12.5", "\u{6c49}\u{6c49}", "Title", "Sincerely,", "Date: "];
    let positions = [-100, 300, 720, 1440, 2600, 5000, 5040, 5300, 5329, 6480, 9000, 10466, 20000];
    let aligns = [TabAlign::Left, TabAlign::Right, TabAlign::Center, TabAlign::Decimal, TabAlign::Bar];
    let widths = [300, 600, 1500, 2400, 5329, 10466];
    let mut rng = Lcg(0x5eed);
    for _ in 0..800 {
        let mut text = String::new();
        for _ in 0..1 + rng.below(8) {
            if rng.below(3) == 0 {
                text.push_str(&zeros(1 + rng.below(90)));
            } else {
                text.push_str(tokens[rng.below(tokens.len())]);
            }
        }
        let tabs = (0..rng.below(4))
            .map(|_| stop(positions[rng.below(positions.len())], aligns[rng.below(aligns.len())]))
            .collect::<Vec<_>>();
        let mut tabs_sorted = tabs;
        tabs_sorted.sort_by_key(|t| t.pos);
        tabs_sorted.dedup_by_key(|t| t.pos);
        let mut p = para(&text, tabs_sorted, [221, 720, 4000][rng.below(3)]);
        if rng.below(4) == 0 {
            p.default_tab_stop = None;
        }
        if rng.below(4) == 0 {
            p.indent_left = 720;
            p.indent_first_line = -360;
        }
        // 在随机字符边界上拆成两个 run。
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).filter(|&i| i > 0).collect();
        if !bounds.is_empty() && rng.below(2) == 0 {
            let at = bounds[rng.below(bounds.len())];
            p.runs = vec![run(&text[..at]), run(&text[at..])];
        }
        cases.push((p, widths[rng.below(widths.len())]));
    }
    cases
}

/// 行宽卡在「词」与「词 + 空格」之间的随机样本（种子固定）：制表符后面跟一个词、再跟空格与
/// 别的字。空行上这个词不切开（按字符收下词、空格吃掉），制表符之后也不该切开（第 2 条）。
/// 固定行宽的 `random_tab_cases` 碰不到这不足一个空格宽的窗口，所以行宽按 `metrics` 现算，
/// 桩度量与真字体各生成各的；字体一并写进 run。
fn near_fit_tab_cases<M: FontMetrics>(metrics: &M, font: &FontSpec) -> Vec<(Para, Twips)> {
    let heads = ["", "A", "Name:", "\t", "A B"];
    let words = ["Sincerely,", "Title", "Date:", "12.5", "\u{6c49}\u{6c49}"];
    let tails = [" x", "  x", " Date: today", " ab ab ab", " \t0", " "];
    let positions = [300, 1440, 5040, 5300, 9000];
    let aligns = [TabAlign::Left, TabAlign::Right, TabAlign::Center, TabAlign::Decimal];
    let space = metrics.measure(" ", font).advance.max(1);
    let mut rng = Lcg(0xf17);
    let mut cases = Vec::new();
    for _ in 0..200 {
        let word = if rng.below(3) == 0 {
            zeros(1 + rng.below(90))
        } else {
            words[rng.below(words.len())].to_string()
        };
        let text = format!("{}\t{word}{}", heads[rng.below(heads.len())], tails[rng.below(tails.len())]);
        let width = metrics.measure(&word, font).advance + rng.below(space as usize) as Twips;
        let mut tabs: Vec<TabStop> = (0..rng.below(3))
            .map(|_| stop(positions[rng.below(positions.len())], aligns[rng.below(aligns.len())]))
            .collect();
        tabs.sort_by_key(|t| t.pos);
        tabs.dedup_by_key(|t| t.pos);
        let mut p = para(&text, tabs, [221, 720][rng.below(2)]);
        p.runs[0].font = font.clone();
        cases.push((p, width.max(1)));
    }
    cases
}

/// 在两个平台上排 `cases`，收集 `tab_violations`。
fn check_tab_cases<M: FontMetrics>(metrics: &M, cases: &[(Para, Twips)]) {
    for (p, width) in cases {
        for platform in [Platform::Desktop, Platform::Android] {
            let pages = Engine::new(metrics, setup(*width))
                .with_platform(platform, View::Print)
                .layout(std::slice::from_ref(p));
            let bad = tab_violations(metrics, p, *width, &pages, platform == Platform::Android);
            let text: Vec<&str> = p.runs.iter().map(|r| r.text.as_str()).collect();
            assert!(bad.is_empty(), "{text:?} tabs {:?} @{width} {platform:?}: {bad:?}", p.tabs);
        }
    }
}

#[test]
fn assumed_no_tab_strands_or_splits_the_text_after_it() {
    // 不变式（**假设**，见 `tab_violations`）。原型与上一版在这里都排坏过：`\t\t` + 长串断在两个
    // 制表符之间、左对齐 10000 的 `A<TAB>000` @2400 中间一行只有制表符（原型）；行首制表符停在
    // 5760／5040 时 `Sincerely,` 被切开、`S` 画在行尾上，`Name:<TAB>Date: today` 切成 `\tDa`（上一版）。
    check_tab_cases(&SimpleMetrics, &hand_tab_cases());
    check_tab_cases(&SimpleMetrics, &random_tab_cases());
    // 上一版还在这里排坏过：`\tSincerely, x` @1230 切成 `\tSincerel` | `y, x`（空行上放不放得下
    // 问的是含空格的 `fit`）。
    check_tab_cases(&SimpleMetrics, &near_fit_tab_cases(&SimpleMetrics, &run("").font));
}

#[test]
fn assumed_a_tab_and_its_word_split_across_runs_break_like_one_run() {
    // 制表符与后面的词粘在一起，拆不拆 run 断法都一样——`A`、制表符挪下去（0、1、20、40）。
    // 粘着的词跨 run 量（`layout.rs` 里 `Engine::shortfall` 的（丙））。跨 run 回退落地之前
    // 看不到这层粘连：末片是 `00` 不是制表符，于是断在 run 边界上（0、4、24）。Word 未测。
    let whole = para(&format!("A\t{}", zeros(42)), vec![], 221);
    let mut split = para("A\t00", vec![], 221);
    split.runs.push(run(&zeros(40)));
    assert_eq!(starts(&layout(&[whole], 2400)), [0, 1, 20, 40]);
    assert_eq!(starts(&layout(&[split], 2400)), [0, 1, 20, 40]);
    // 词在空行上放得下时也一样：行首制表符停在 5040，`Sin` | `cerely,` 拆在两个 run 里、
    // `Sin` 放得进制表符之后，整个词仍然挪下去（`\t` | `Sincerely,`）。
    let whole = para("\tSincerely,", vec![stop(4900, TabAlign::Left)], 221);
    let mut split = para("\tSin", vec![stop(4900, TabAlign::Left)], 221);
    split.runs.push(run("cerely,"));
    assert_eq!(starts(&layout(&[whole], 5329)), [0, 1]);
    assert_eq!(starts(&layout(&[split], 5329)), [0, 1]);
}

#[test]
fn assumed_no_break_between_an_opening_bracket_and_a_tab() {
    // **假设**：制表符之前的断点也守行尾禁则——`（` 不留在行尾，`（<TAB>` 之间不断。
    // 行尾禁则那一组（`font::linebreak::is_no_line_end`）随禁则一步落地，「制表符之前」
    // 一条也查它。没有夹具把开括号放在制表符前面。
    // 这里只查断点表；排版层照它退回，见下面两条。
    let offsets: Vec<usize> =
        SimpleMetrics.break_opportunities("\u{ff08}\t").iter().map(|b| b.offset).collect();
    assert_eq!(offsets, [4], "`（` 之后不该可断：{offsets:?}");
    // 不是行尾禁则的字照旧在制表符之前可断。
    let offsets: Vec<usize> =
        SimpleMetrics.break_opportunities("\u{6c49}\t").iter().map(|b| b.offset).collect();
    assert_eq!(offsets, [3, 4], "{offsets:?}");
}

// 下面几条钉的是排版层：`（`、制表符与后面的字之间都不断（前一处是上面的**假设**，后一处实测），
// 三者粘成一截，放不下就退回 `（` 之前的断点（`layout.rs` 里 `Engine::shortfall` 的（丙）（丁），
// 制表符前的交界照断点表判断）。跨 run 回退落地之前，挪粘着的制表符时不问制表符之前那一处交界，
// 照样断在 `（` 与制表符之间、把 `（` 留在行尾。Word 未测。
// 默认档 221 与 720 各钉一份：720（桌面的缺省，也是 Word 几乎总写进文档的值）让本行的制表符
// 停到行尾之外（`past_line_end`），在（丙）里走另一条路。

#[test]
fn assumed_an_opening_bracket_goes_down_with_its_glued_tab() {
    // 桩：20 个汉字 + `（` = 5040，制表符停到 5083，`Sincerely`（9 × 120）放不下。
    // 退到 `汉|（`（20），第二行 `（` 240、制表符停到 442、词到 1522，放得下。
    // 跨 run 回退落地之前：`（` 留在第一行行尾（0、21），制表符与词挪下去。
    let text = format!("{}\u{ff08}\tSincerely", "\u{6c49}".repeat(20));
    assert_eq!(starts(&layout(&[para(&text, vec![], 221)], 5329)), [0, 20]);
}

#[test]
fn assumed_an_opening_bracket_goes_down_with_its_glued_tab_on_the_720_grid() {
    // 默认档 720：本行的制表符从 5040 起，下一档 5760 在行尾之外，只推到行尾（`past_line_end`）。
    // 断点紧挨在制表符之前时制表符就此留在本行、字另起一行（本条最后的对照）；这里制表符之前
    // 不断，那样排 `（` 就留在行尾。所以照样在下一行试排：`（` 240、制表符停到 720、词到 1800，
    // 放得下，退到 `汉|（`（20）。两个平台一样，桌面文档没写默认档（缺省 720）时也一样。
    // 修正之前（与 G6 相同）：0、22，`（` 与制表符留在第一行行尾。**假设**，Word 未测
    // （待测：`w:defaultTabStop` 720 的 `20汉（<TAB>Sincerely` @5329）。
    let lay = |p: &Para, platform| {
        starts(
            &Engine::new(&SimpleMetrics, setup(5329))
                .with_platform(platform, View::Print)
                .layout(std::slice::from_ref(p)),
        )
    };
    let text = format!("{}\u{ff08}\tSincerely", "\u{6c49}".repeat(20));
    for platform in [Platform::Desktop, Platform::Android] {
        assert_eq!(lay(&para(&text, vec![], 720), platform), [0, 20], "{platform:?}");
        // 拆在 `汉|（`、`（|<TAB>`、`<TAB>|S` 上断法不变。
        for at in [60, 63, 64] {
            let mut split = para(&text[..at], vec![], 720);
            split.runs.push(run(&text[at..]));
            assert_eq!(lay(&split, platform), [0, 20], "{platform:?} 拆在字节 {at}");
        }
    }
    let unset = Para { default_tab_stop: None, ..para(&text, vec![], 720) };
    assert_eq!(lay(&unset, Platform::Desktop), [0, 20], "桌面缺省 720");
    let pages = layout(&[para(&text, vec![], 720)], 5329);
    assert_eq!(x_of(&pages, 22), 720, "第二行的制表符停到 720");
    // 别的行尾禁则字一样：41 个 `A` + 空格 + `(` = 5100（`“` 5220），制表符推到行尾；退到空格
    // 之后（42），第二行制表符停到 720、词到 1800。修正之前 0、44。
    for open in ['(', '$', '\u{201c}'] {
        let latin = format!("{} {open}\tSincerely", "A".repeat(41));
        for platform in [Platform::Desktop, Platform::Android] {
            assert_eq!(lay(&para(&latin, vec![], 720), platform), [0, 42], "{open:?} {platform:?}");
        }
    }
    // 后面跟一长串 `0`：第二行 `（` 240、制表符停到 720、(5329 − 720) / 120 = 38.4 → 38 个 `0`，
    // 第三行从 20 + 2 + 38 = 60 起。修正之前 0、22、66。
    let long = format!("{}\u{ff08}\t{}", "\u{6c49}".repeat(20), zeros(60));
    assert_eq!(lay(&para(&long, vec![], 720), Platform::Android), [0, 20, 60]);
    // 对照：断点紧挨在制表符之前（`汉|<TAB>`）时不退，制表符留在本行行尾，`Sincerely` 另起一行、
    // 从行首排（`assumed_a_left_stop_beyond_the_line_keeps_the_tab_and_wraps_the_text` 那条假设）。
    let plain = format!("{}\tSincerely", "\u{6c49}".repeat(21));
    for platform in [Platform::Desktop, Platform::Android] {
        assert_eq!(lay(&para(&plain, vec![], 720), platform), [0, 22], "{platform:?}");
    }
    assert_eq!(x_of(&layout(&[para(&plain, vec![], 720)], 5329), 22), 0);
}

#[test]
fn known_deviation_an_opening_bracket_stays_before_a_tab_that_cannot_land_on_the_next_line() {
    // **已知偏差**：左对齐停靠点在行外（6480 @5329），制表符在哪一行都只推到行尾。试排里它到
    // 下一行仍然够不着，不退：`（` 与制表符留在第一行行尾，`Sincerely` 另起一行、从行首排（0、22）。
    // 退下去 `（` 照样停在第二行行尾，还多出一行，所以禁则在这里让步。**假设**，Word 未测
    // （待测：左对齐 6480 的 `20汉（<TAB>Sincerely` @5329）。
    let text = format!("{}\u{ff08}\tSincerely", "\u{6c49}".repeat(20));
    for default_tab_stop in [221, 720] {
        let pages = layout(&[para(&text, vec![stop(6480, TabAlign::Left)], default_tab_stop)], 5329);
        assert_eq!(starts(&pages), [0, 22], "默认档 {default_tab_stop}");
        assert_eq!(x_of(&pages, 22), 0);
    }
}

#[test]
fn assumed_an_opening_bracket_is_not_stranded_before_a_glued_tab() {
    // `汉（<TAB>` + 60 个 `0`：唯一的断点是 `汉|（`，第一行只有 `汉`。第二行从 `（` 起没有断点，
    // 照紧急断行填满：`（` 240、制表符停到 442、(5329 − 442) / 120 = 40.7 → 40 个 `0`，
    // 第三行从 1 + 2 + 40 = 43 起。跨 run 回退落地之前：0、2、45（`汉（` 一行）。
    let got = starts(&layout(&[para(&format!("\u{6c49}\u{ff08}\t{}", zeros(60)), vec![], 221)], 5329));
    assert_eq!(got, [0, 1, 43]);
    // `（<TAB>` + 60 个 `0`：从行首起一个断点都没有，第一行照紧急断行填满（0、42）。
    // 跨 run 回退落地之前：`（` 独占一行（0、1、44）。
    let got = starts(&layout(&[para(&format!("\u{ff08}\t{}", zeros(60)), vec![], 221)], 5329));
    assert_eq!(got, [0, 42]);
}

#[cfg(feature = "fontenv")]
mod real {
    use super::*;
    use rsword_layout_core::font::FontRegistry;
    use rsword_layout_core::{LayoutRecord, RealMetrics, TextShaper, paint_document};

    /// 仓库自带的字体。
    fn repo_font(name: &str) -> Vec<u8> {
        std::fs::read(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(name))
            .expect("字体读得到")
    }

    /// 拉丁三款加 Droid（汉字走它）。
    fn registry() -> FontRegistry {
        let mut registry = FontRegistry::new();
        for name in [
            "LiberationSans-Regular.ttf",
            "LiberationSerif-Regular.ttf",
            "DejaVuSans.ttf",
            "DroidSansFallbackFull.ttf",
        ] {
            registry.add(repo_font(name), 0).expect("字体装得进");
        }
        registry
    }

    fn para_in(texts: &[&str], family: &str, half_points: u32, tabs: Vec<TabStop>) -> Para {
        Para {
            runs: texts
                .iter()
                .map(|t| Run { font: FontSpec::new(family, half_points), ..run(t) })
                .collect(),
            tabs,
            default_tab_stop: Some(720),
            ..Para::default()
        }
    }

    #[test]
    fn an_aligned_tab_at_the_line_end_keeps_text_split_across_runs_or_faces_on_the_line() {
        // 右对齐制表位在行尾（页眉页脚、目录、表单的「标题<TAB>页码」）：推进量按那段的精确宽倒推，
        // 断行却逐片比整 twips，那段跨 run 或跨 face 时整 twips 之和比精确值多 1～2 twips，
        // 最后一片放不下（审查复现：Liberation Serif 11pt `Name<TAB>` `John ` `Smith` @9026 给 0、10；
        // Liberation Sans 11pt 的 `x<TAB>12汉字ab`，拉丁与 Droid 两个 face，给 0、6）。
        // 桩度量的宽度全是整 twips，碰不到这一条。
        let registry = registry();
        let metrics = RealMetrics::new(&registry);
        let right = |pos| vec![stop(pos, TabAlign::Right)];
        let p = para_in(&["Name\t", "John ", "Smith"], "Liberation Serif", 22, right(9026));
        let pages = Engine::new(&metrics, setup(9026)).layout(std::slice::from_ref(&p));
        assert_eq!(starts(&pages), [0]);
        // 那段在精确落位上仍收在行尾。
        let smith = pages[0]
            .fragments
            .iter()
            .find_map(|f| match f {
                Fragment::Text(t) if t.source.map(|s| s.0) == Some(10) => Some(t),
                _ => None,
            })
            .expect("`Smith` 那一片");
        let end = smith.x_pt * 20.0 + metrics.advance_pt("Smith", &smith.font) * 20.0 - f64::from(MARGIN);
        assert!((end - 9026.0).abs() < 1e-6, "{end}");
        let p = para_in(&["x\t12\u{6c49}\u{5b57}ab"], "Liberation Sans", 22, right(9026));
        assert_eq!(starts(&Engine::new(&metrics, setup(9026)).layout(&[p])), [0]);

        // 小范围扫一遍：三种对齐、停在行尾或行外（夹到行尾）、几种字体字号行宽，全是一行。
        let texts: [&[&str]; 5] = [
            &["Name\t", "John ", "Smith"],
            &["Title\t1", "2"],
            &["Total\t1", "2.5", "0"],
            &["x\t12\u{6c49}\u{5b57}ab"],
            &["x\t3.", "14\u{6c49}"],
        ];
        let mut wrapped = Vec::new();
        for family in ["Liberation Sans", "Liberation Serif", "DejaVu Sans"] {
            for half_points in [19, 21, 22, 24, 28] {
                for width in [5329, 8306, 9026] {
                    for align in [TabAlign::Right, TabAlign::Center, TabAlign::Decimal] {
                        for pos in [width, width + 1000] {
                            for texts in texts {
                                let p = para_in(texts, family, half_points, vec![stop(pos, align)]);
                                let got = starts(&Engine::new(&metrics, setup(width)).layout(&[p]));
                                if got != [0] {
                                    wrapped.push(format!("{family} {half_points} @{width} {align:?} {pos} {texts:?}: {got:?}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(wrapped.is_empty(), "{} 行被拆开：{wrapped:#?}", wrapped.len());
    }

    #[test]
    fn assumed_no_tab_strands_or_splits_the_text_after_it_with_real_fonts() {
        // 与桩度量那一条同一组不变式，换成仓库字体（Liberation Sans，汉字落到 Droid）：
        // 宽度不再是整 twips，逐片取整与精确落位之间的差也一起查。手写样本全用，随机样本
        // 每 32 个取一个——未优化构建里整形慢，全量要三分钟。
        let registry = registry();
        let metrics = RealMetrics::new(&registry);
        let cases: Vec<(Para, Twips)> = hand_tab_cases()
            .into_iter()
            .chain(random_tab_cases().into_iter().step_by(32))
            .map(|(mut p, width)| {
                for r in &mut p.runs {
                    r.font = FontSpec::new("Liberation Sans", 24);
                }
                (p, width)
            })
            .collect();
        check_tab_cases(&metrics, &cases);
        check_tab_cases(&metrics, &near_fit_tab_cases(&metrics, &FontSpec::new("Liberation Sans", 24)));
    }

    #[test]
    fn assumed_a_word_one_space_short_of_the_line_is_not_split_after_a_tab_with_real_fonts() {
        // **未实测**（见桩度量那一条 `assumed_a_word_that_fits_an_empty_line_is_not_split_after_a_tab`）。
        // 审查复现（手机 Calibri @5329）：`\t` + 43 个 `0` 加 `i` 给 0、1，后面再跟 ` x` 就给 0、42——
        // 行宽只比词宽出不到一个空格时，「空行上放得下」问的是含空格的 `fit`，词被当成放不下切开。
        // 这里用仓库字体（`RealMetrics` 覆盖了按字符收的 `fit_clusters`），行宽取词宽到
        // 「词 + 空格」宽之间的几个值；期望与不带制表符的同一截字断法一致。
        let registry = registry();
        let metrics = RealMetrics::new(&registry);
        let font = FontSpec::new("Liberation Sans", 24);
        let word = format!("{}i", zeros(43));
        let bare = metrics.measure(&word, &font).advance;
        let spaced = metrics.measure(&format!("{word} "), &font).advance;
        assert!(spaced > bare + 1, "空格有宽度：{bare} / {spaced}");
        let lay = |text: &str, width: Twips| {
            let p = para_in(&[text], "Liberation Sans", 24, vec![]);
            starts(&Engine::new(&metrics, setup(width)).layout(&[p]))
        };
        for width in [bare, (bare + spaced) / 2, spaced - 1] {
            assert_eq!(lay(&format!("{word} x"), width), [0, 45], "@{width}");
            assert_eq!(lay(&format!("\t{word} x"), width), [0, 1, 46], "@{width}");
            assert_eq!(lay(&format!("A\t{word} x"), width), [0, 2, 47], "@{width}");
        }
    }

    /// 制表符画成**一个空格字形**，推进量就是它占的宽度（仓库自带的 Liberation Sans；
    /// 手机 Calibri 那一条在 `tab_stops_calibri.rs`）。「画 1 个空格」是量具方法 §4 的
    /// Windows 计数约定，Mac 与 Android 上**未测**。
    #[test]
    fn a_tab_is_painted_as_one_space_glyph_with_its_advance() {
        use skrifa::{FontRef, MetadataProvider};
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/fonts/LiberationSans-Regular.ttf"),
        )
        .expect("字体读得到");
        let space = FontRef::new(&bytes).unwrap().charmap().map(' ').unwrap().to_u32();
        let mut registry = FontRegistry::new();
        registry.add(bytes, 0).expect("字体装得进");
        let metrics = RealMetrics::new(&registry);
        let mut r = run("A\t00\tB");
        r.font = FontSpec::new("Liberation Sans", 24);
        let p = Para {
            runs: vec![r],
            tabs: vec![stop(1440, TabAlign::Right)],
            default_tab_stop: Some(221),
            ..Para::default()
        };
        let pages = Engine::new(&metrics, setup(5329)).layout(&[p]);
        let shaper: &dyn TextShaper = &registry;
        let rec = LayoutRecord::from_paint(&paint_document(&pages, Some(shaper), &registry.face_ids()));
        let line = &rec.pages[0].lines[0];
        // A、制表符、0、0、制表符、B、段落标记的空格：一个制表符一个字形。
        assert_eq!(line.glyphs.len(), 7, "{:?}", line.glyphs);
        let origin = line.glyphs[0].origin_x_pt;
        for (tab, next) in [(1, 2), (4, 5)] {
            let t = &line.glyphs[tab];
            assert_eq!(t.glyph_id, space, "制表符画成空格字形，不是 U+0009 映到的字形");
            assert_eq!(t.source.map(|s| (s.start, s.end)), Some((tab as u32, tab as u32 + 1)));
            assert!(
                (line.glyphs[next].origin_x_pt - t.origin_x_pt - t.advance_x_pt).abs() < 1e-9,
                "制表符字形的推进量就是它占的宽度"
            );
        }
        // 右对齐 1440：`00` 收在 1440；第二个制表符落到 1440 之后的默认档 1547（7 × 221）。
        let end_of_zeros = line.glyphs[3].origin_x_pt + line.glyphs[3].advance_x_pt - origin;
        assert!((end_of_zeros * 20.0 - 1440.0).abs() < 1e-6, "{end_of_zeros}pt");
        assert!((line.glyphs[5].origin_x_pt - origin - 1547.0 / 20.0).abs() < 1e-6);
    }
}
