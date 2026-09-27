//! 平台与视图开关：[`Platform`] × [`View`]。
//!
//! 同一份最小 docx，Mac Word 与 Android Word 在几条规则上实测相反，文档里又没有属性能区分，
//! 所以由调用方说明在模拟谁（见 `Platform` 的说明）。这里只钉开关本身：
//!
//! - 库的默认是 Desktop + Print——svg / wasm / cffi 都走 `Engine::new` / `Engine::with_wrap`，
//!   不说明平台，行为不能因为多了这个开关而变；
//! - 光拨开关不动排版。下面那组段落（拉丁字折行、两端对齐、汉字、段中分页符后面还有字）
//!   在四种组合下排出来逐字节相同。以后读开关的规则（缺省制表位、行末标点挂出、
//!   段末分页符拆行……）各自只管自己那一类输入，这组段落碰不到它们；
//!   真碰到了，是那条规则的作用域写宽了，先查规则，别改这里的期望。
//!
//! 全用桩度量，不依赖字体。

use rsword_layout_core::{
    Align, Color, Engine, FontSpec, PageSetup, Para, PlaceholderKind, Platform, Run, SimpleMetrics,
    View, WrapContext,
};

fn run(text: &str, placeholders: &[PlaceholderKind]) -> Run {
    Run {
        text: text.to_owned(),
        font: FontSpec::new("synthetic", 20),
        color: Color::BLACK,
        placeholders: placeholders.to_vec(),
        rise: 0,
        rise_fine: None,
        hidden: false,
    }
}

fn paras() -> Vec<Para> {
    let words = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod ".repeat(8);
    vec![
        Para { runs: vec![run(&words, &[])], ..Para::default() },
        Para { runs: vec![run(&words, &[])], align: Align::Justify, ..Para::default() },
        Para { runs: vec![run(&"汉".repeat(90), &[])], ..Para::default() },
        Para {
            runs: vec![run("before\u{FFFC}after", &[PlaceholderKind::PageBreak])],
            ..Para::default()
        },
    ]
}

const ALL: [(Platform, View); 4] = [
    (Platform::Desktop, View::Print),
    (Platform::Desktop, View::Mobile),
    (Platform::Android, View::Print),
    (Platform::Android, View::Mobile),
];

#[test]
fn the_library_default_is_desktop_print() {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    assert_eq!((engine.platform(), engine.view()), (Platform::Desktop, View::Print));
    let engine = Engine::with_wrap(&metrics, PageSetup::a4(), WrapContext::new());
    assert_eq!((engine.platform(), engine.view()), (Platform::Desktop, View::Print));
    assert_eq!(Platform::default(), Platform::Desktop);
    assert_eq!(View::default(), View::Print);
}

#[test]
fn with_platform_sets_both() {
    let metrics = SimpleMetrics;
    for (platform, view) in ALL {
        let engine = Engine::new(&metrics, PageSetup::a4()).with_platform(platform, view);
        assert_eq!((engine.platform(), engine.view()), (platform, view));
    }
}

#[test]
fn the_switch_alone_moves_nothing() {
    let metrics = SimpleMetrics;
    let paras = paras();
    let base = Engine::new(&metrics, PageSetup::a4()).layout(&paras);
    // 夹具本身要真的有东西可比：多行、多页。
    assert!(base.len() >= 2, "分页符没有翻页");
    assert!(base[0].fragments.len() > 10, "段落没有折行");
    let base = format!("{base:?}");
    for (platform, view) in ALL {
        let pages = Engine::new(&metrics, PageSetup::a4())
            .with_platform(platform, view)
            .layout(&paras);
        assert_eq!(format!("{pages:?}"), base, "{platform:?} + {view:?}");
    }
}
