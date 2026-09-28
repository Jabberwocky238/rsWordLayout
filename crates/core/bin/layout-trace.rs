//! `layout-trace [选项] <input.docx> [output.json]`
//!
//! 出具量具比较器要的引擎轨迹（schema `rsword-layout-trace/1`）。
//! 用法与限定见 `tools/measure/README.md`；契约见 [`rsword_layout_core::oracle`]。
//!
//! ```text
//! --font <path>          装这个字体文件，可重复。不给就只有桩度量，没有字形级记录
//! --require <family>     要求该族必须装上，可重复
//! --metrics simple|real  度量来源。默认 real（读字体）；simple 是近似桩
//! --vertical-grid mac|none  纵向量化，默认 none。**含回测规则，先读 VerticalGrid 的限定**
//! --margin <twips>       四边页边距。默认 1440（1 英寸）
//! --content-width <twips>  用页宽减去左右边距得到这个版心宽。优先于 --margin 的左右值
//! --page-width <twips>   页宽。默认 A4 的 11906；先于 --content-width 生效
//! --platform mac|android 模拟哪个平台的 Word，默认 mac。见 `Platform`
//! --view print|mobile    模拟哪种视图，默认 print（分页视图）。见 `View`
//! --fallback-font <path>[#index]  CJK 回退链上的字体，可重复，按给出的顺序查。
//!                        `#index` 只装 TTC 的那一个 face（如 Noto CJK 的 SC 是 `#2`）
//! --no-fallback          不装回退字体（连默认的也不装）；缺字的 CJK 按名义 1 em
//! ```
//!
//! 环境变量 `RSWORD_FALLBACK_FONT=<path>[#index]`（多个按系统的路径分隔符隔开，Unix 是 `:`）
//! 替换 `android` 下的默认回退字体；`--fallback-font` 比它优先，`mac` 下不读它。
//!
//! `--platform` / `--view` 不给就是 `mac` + `print`，排版与没有这两个选项之前完全相同
//! （只是 `metrics` 栏多记了这两项）：`tools/measure` 的 Mac 回放（`sweep.py`）不传它们，
//! 靠的就是这个默认。
//! Android Word 的读数（word_analyse）要显式传 `--platform android`；它的窄路径
//! （`w3=5329`）是移动视图，再加 `--view mobile`，纸页路径（`w3=10466`）仍是 `print`。
//! 两个取值都记进轨迹的 `metrics` 栏。
//!
//! 目前读平台的断行规则有两条（CJK 回退字体装不装也看平台，见下文「回退字体」）：
//!
//! - 文档没写 `w:defaultTabStop` 时的默认制表位——`mac` 照规范补 720（Mac 上未测），
//!   `android` 补 221（拟合值，不是测量值，见 `layout.rs` 的 `ANDROID_MISSING_DEFAULT_TAB_STOP`）；
//! - 行末标点挂出——`mac` 照段落的 `w:overflowPunct`（没写即开）把单个越界的 `。，）、`
//!   挂出版心（Mac Word 实测），`android` 从不挂出、退回前一个合法断点（Android Word 实测
//!   `）`、`。`，其余是假设）。没有单独的开关：挂不挂由平台定，记进 `metrics` 栏。
//!
//! 读视图的规则有一条：段末手动分页符之后段内再无内容、段落以段落标记结束时，
//! `print` 把段落标记收进分页符那一行（Mac Word 实测），`mobile` 让段落标记另起一行
//! （Android Word 窄路径 `br-page` 实测 `(340,341)`、`(341,342)`）。只看视图、不看平台。
//! 移动视图没有页，引擎照样分页，所以 `--view mobile` 轨迹里的页下标是合成的：
//! 段落标记那行落到下一页顶、页行数（`br-page` 是 11 / 6）都不对应 Word，只有行的码元区间对应。
//!
//! Mac 回放用默认的 `mac`，所以挂出照旧；行首 / 行尾禁则两个平台共用一套。禁则表扩充后
//! `mac` 下也有几处 Mac 未测的变化（`％。`、`。〉` 不再挂出等；拆在两个 run 里时跨 run 回退
//! 退回更早的断点，与一个 run 里相同。见 `Platform` 的说明），Mac 回放与仓库夹具里都没有这些序列。
//!
//! # 回退字体
//!
//! 手机上的 Word 遇到装不上的 eastAsia 字体（CJK 夹具都写 SimSun）不会把汉字排成零宽：
//! `han22` 30 个「汉」排 22 + 8，`han-24` 排 11 + 11 + 8（实测的是行数，与 1 em 相容）。
//! 它换了一个 CJK 字体，换的是哪一个**未测**。所以 `--font` 盖不住某些 eastAsia 字符时，
//! 这里按回退链补字体：
//!
//! 1. `--no-fallback`：不装；
//! 2. 给了 `--fallback-font` 就用它们，按给出的顺序（任何平台——这是显式要的）；
//! 3. `--platform android`：用 `RSWORD_FALLBACK_FONT`，没设就用仓库里的
//!    `fixtures/fonts/DroidSansFallbackFull.ttf`（Apache-2.0，AOSP 的 CJK 回退字体；
//!    按编译时的仓库位置找，挪走了二进制就找不到，找不到只警告）；
//! 4. `--platform mac`（默认）：不装。Mac 回放（`tools/measure/sweep.py`）按采集时的字体原样绑定，
//!    不该被悄悄补字——量具方法 §6.2 的「不许无声替换」。
//!
//! 回退链也盖不住的 CJK 字符画 `.notdef`、占 1 em（这一条在库里，任何平台都有），
//! 轨迹顶层 `notdefGlyphs` 计数。
//!
//! **只在用得上时才装，且只装用得上的**：只数 eastAsia 槽里成字形的字符（控制字符、
//! 格式字符不算）；回退链上的每个 face 只在它盖得住还缺着的字符时才注册，盖不住的不装。
//! 装上之后，槽里的字体画不出的 eastAsia 字符**先查回退链、再查其他 `--font`**
//! （见 `FontRegistry` 的顺序说明；这个顺序是假定）。「缺着」按链的来路分两种（`Trigger`）：
//!
//! - 显式给的链（`--fallback-font`、`RSWORD_FALLBACK_FONT`）与选 face 同一个判据：
//!   槽里的字体画不出就算，哪怕别的 `--font` 画得出（`w:hint="eastAsia"` 下的 `“`、空格）。
//!   一个字符用哪个 face 只看它自己，与文档别处有没有缺字无关。
//! - 默认链（Droid）只数**哪个 `--font` 都画不出**的字符。于是 `--font` 已经盖住全文的文档
//!   （如 `--font calibri --font NotoSansCJK` 排 CJK 夹具），轨迹与不装回退字体时相同
//!   （字体指纹也不变），只多一个 `notdefGlyphs: 0`——按上一条的判据，Droid 会把汉字从 Noto
//!   手里接走。代价是**不局部**：文档里只要有一个谁都画不出的字，Droid 就装上，然后按上面的
//!   顺序接走全文槽里画不出、它又盖得住的字符，包括 `--font` 画得出的——同一段
//!   `hint="eastAsia"` 的 `“`，文档别处有没有一个缺字，排出来的字体不同。
//!
//! 装了什么、各落到哪（回退链、其他 `--font`、名义 1 em、跳过），写进 stderr 与轨迹的 `metrics` 栏。
//!
//! Droid 不是手机上的字体：手机进程映射里能看到的 CJK 字体是 `NotoSansCJK-Regular.ttc`、
//! Noto Serif CJK 与 MiSans VF（word_analyse 的 `reports/maps-word-13336.txt`，那份快照不全）。
//! 它们的汉字与常用全角标点都是 1 em，断行上分不出来；**纵向量不同**（hhea 升部 + 降部 + 行距：
//! Droid 1.309 em，Noto Sans CJK 1.448 em），分页因此不同（审查的合成长文档：名义 47、Droid 44、
//! Noto 39–40 行一页），这些都没有 Word 的数。要逐页对齐手机时用
//! `--fallback-font …/NotoSansCJK-Regular.ttc#2`（或设 `RSWORD_FALLBACK_FONT`）。
//! Droid 也不全：11172 个谚文音节只有 3 个，没有 `〈〉`，这些字落到名义 1 em。
//!
//! # 为什么 `--require` 不是可有可无的讲究
//!
//! 量具方法 §6.2：**度量兼容克隆的字体替换，在几何上完全不可见。**
//! Liberation Serif 是 Times New Roman 的度量兼容克隆（Sans↔Arial、Carlito↔Calibri），
//! 替换后 `glyphOrigin` 与 `advanceVector` 一字不差（2618 条记录 max |Δ| = 0.000000pt）。
//! **只有字体名能发现它。** 核不过就退出——不核就跑，跑出来的是废数据，
//! 而且废得看不出来。

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::PathBuf;

use rsword_layout_core::font::FontRegistry;
use rsword_layout_core::{
    LayoutOptions, LayoutRecord, Para, Platform, PreparedDocument, TraceMeta, VerticalGrid, View,
    WrapPolicy, to_trace_json,
};
use skrifa::MetadataProvider;

#[derive(Default)]
struct Args {
    input: Option<String>,
    output: Option<String>,
    fonts: Vec<PathBuf>,
    require: Vec<String>,
    metrics: Option<String>,
    grid: Option<String>,
    margin: Option<i32>,
    content_width: Option<i32>,
    page_width: Option<i32>,
    platform: Platform,
    view: View,
    /// `--fallback-font` 的原文，按给出的顺序。
    fallback_fonts: Vec<String>,
    no_fallback: bool,
}

/// 解析命令行（不含程序名）。单独拆出来是为了能测：默认值、显式选择、坏值被拒。
fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = argv.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} 缺参数"));
        match arg.as_str() {
            "--font" => args.fonts.push(PathBuf::from(value("--font")?)),
            "--require" => args.require.push(value("--require")?),
            "--metrics" => args.metrics = Some(value("--metrics")?),
            "--vertical-grid" => args.grid = Some(value("--vertical-grid")?),
            // 取值当场核：选项漏了值时会吞掉后面的输入路径，这样报的是取值不对，
            // 而不是一个莫名其妙的「找不到文件」。
            "--platform" => args.platform = parse_platform(&value("--platform")?)?,
            "--view" => args.view = parse_view(&value("--view")?)?,
            "--fallback-font" => args.fallback_fonts.push(value("--fallback-font")?),
            "--no-fallback" => args.no_fallback = true,
            "--margin" => {
                args.margin = Some(value("--margin")?.parse().map_err(|e| format!("--margin {e}"))?)
            }
            "--page-width" => {
                args.page_width = Some(
                    value("--page-width")?.parse().map_err(|e| format!("--page-width {e}"))?,
                )
            }
            "--content-width" => {
                args.content_width = Some(
                    value("--content-width")?
                        .parse()
                        .map_err(|e| format!("--content-width {e}"))?,
                )
            }
            other if other.starts_with("--") => return Err(format!("未知选项 {other}")),
            other if args.input.is_none() => args.input = Some(other.to_string()),
            other if args.output.is_none() => args.output = Some(other.to_string()),
            other => return Err(format!("多余的参数 {other}")),
        }
    }
    Ok(args)
}

fn parse_platform(value: &str) -> Result<Platform, String> {
    match value {
        "mac" => Ok(Platform::Desktop),
        "android" => Ok(Platform::Android),
        other => Err(format!("--platform 只接受 mac / android，收到 {other}")),
    }
}

fn parse_view(value: &str) -> Result<View, String> {
    match value {
        "print" => Ok(View::Print),
        "mobile" => Ok(View::Mobile),
        other => Err(format!("--view 只接受 print / mobile，收到 {other}")),
    }
}

/// 默认回退字体：仓库里已收录的 Droid Sans Fallback（见 `fixtures/fonts/README.md`）。
/// 按编译时的仓库位置找，运行时才核它在不在——不 `include_bytes!` 进二进制（4 MB）。
const DEFAULT_FALLBACK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/fonts/DroidSansFallbackFull.ttf"
);

/// 替换 `android` 下默认回退字体的环境变量。
const FALLBACK_ENV: &str = "RSWORD_FALLBACK_FONT";

/// 回退链上的一项：`path` 或 `path#index`。
#[derive(Debug, Clone, PartialEq)]
struct FallbackSpec {
    path: PathBuf,
    /// 只装 TTC 的这一个 face；`None` 是按序号装到装不进为止（与 `--font` 同样装法）。
    index: Option<u32>,
}

impl FallbackSpec {
    fn parse(spec: &str) -> Result<FallbackSpec, String> {
        match spec.rsplit_once('#') {
            Some((path, index))
                if !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) =>
            {
                let index = index.parse().map_err(|e| format!("回退字体 {spec}：{e}"))?;
                Ok(FallbackSpec {
                    path: PathBuf::from(path),
                    index: Some(index),
                })
            }
            _ => Ok(FallbackSpec {
                path: PathBuf::from(spec),
                index: None,
            }),
        }
    }
}

/// 这一次用的回退链，以及它从哪来（记进轨迹）。
#[derive(Debug, PartialEq)]
struct FallbackChain {
    specs: Vec<FallbackSpec>,
    from: &'static str,
    /// 按哪些字符决定装不装、装哪些 face。
    trigger: Trigger,
}

/// 回退链按哪些字符来装。
#[derive(Debug, Clone, Copy, PartialEq)]
enum Trigger {
    /// 显式给的链（`--fallback-font`、`RSWORD_FALLBACK_FONT`）：与选 face 同一个判据——
    /// eastAsia 槽的字体画不出的字符都算，**哪怕别的 `--font` 画得出**
    /// （`FontRegistry::fallback_candidates`）。装上的 face 因此一定接走这些字符里它盖得住的，
    /// 一个字符用哪个 face 只看它自己，与文档别处有没有缺字无关。
    SlotMissing,
    /// 默认链（`android` 的 Droid）：只数哪个 `--font` 都画不出的字符
    /// （`FontRegistry::uncovered_chars`）。不按上一条装，是因为 CJK 夹具 eastAsia 槽都写
    /// SimSun，`--font` 里已有 Noto 时汉字也全算「槽里画不出」，Droid 一装就把它们从 Noto
    /// 手里接走，改掉本来对得上的行。代价是**不局部**：链一旦因为某个缺字装上，
    /// 它就按选 face 的顺序接走全文 eastAsia 槽里画不出、它又盖得住的字符——包括
    /// 别的 `--font` 画得出的（`w:hint="eastAsia"` 下的 `“`、空格，Noto 已有的汉字）。
    NoFace,
}

/// 按 `--no-fallback` → `--fallback-font` → 平台（`android` 读环境变量，没设用默认 Droid；
/// `mac` 不装）的顺序定回退链。**路径当场核**：显式给的（选项或环境变量）读不到就报错，
/// 不等到文档里真有缺字才发现；默认那一份读不到只警告（`warnings`），退到名义 1 em。
/// 显式给的链按 [`Trigger::SlotMissing`] 装，默认链按 [`Trigger::NoFace`]。
fn fallback_chain(
    args: &Args,
    env: Option<&OsStr>,
    warnings: &mut Vec<String>,
) -> Result<FallbackChain, String> {
    let explicit = |from: &'static str, specs: Vec<String>| -> Result<FallbackChain, String> {
        let specs = specs
            .iter()
            .map(|s| FallbackSpec::parse(s))
            .collect::<Result<Vec<_>, _>>()?;
        for spec in &specs {
            if !spec.path.is_file() {
                return Err(format!(
                    "{from} 给的回退字体读不到：{}",
                    spec.path.display()
                ));
            }
        }
        Ok(FallbackChain {
            specs,
            from,
            trigger: Trigger::SlotMissing,
        })
    };
    if args.no_fallback {
        return Ok(FallbackChain {
            specs: Vec::new(),
            from: "--no-fallback",
            trigger: Trigger::NoFace,
        });
    }
    if !args.fallback_fonts.is_empty() {
        return explicit("--fallback-font", args.fallback_fonts.clone());
    }
    if args.platform == Platform::Desktop {
        return Ok(FallbackChain {
            specs: Vec::new(),
            from: "mac 不装",
            trigger: Trigger::NoFace,
        });
    }
    if let Some(value) = env.filter(|v| !v.is_empty()) {
        let specs = std::env::split_paths(value)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        return explicit(FALLBACK_ENV, specs);
    }
    let default = FallbackSpec {
        path: PathBuf::from(DEFAULT_FALLBACK),
        index: None,
    };
    if !default.path.is_file() {
        warnings.push(format!(
            "默认回退字体读不到 {}：缺字的 CJK 只能按名义 1 em 画 .notdef",
            default.path.display()
        ));
        return Ok(FallbackChain {
            specs: Vec::new(),
            from: "默认（读不到）",
            trigger: Trigger::NoFace,
        });
    }
    Ok(FallbackChain {
        specs: vec![default],
        from: "默认",
        trigger: Trigger::NoFace,
    })
}

/// 回退相关选项的组合里不起作用的那些。只警告，不退出。
fn fallback_warnings(args: &Args) -> Vec<String> {
    let mut out = Vec::new();
    if args.no_fallback && !args.fallback_fonts.is_empty() {
        out.push(
            "--no-fallback 与 --fallback-font 同时给了：以 --no-fallback 为准，不装回退字体".into(),
        );
    }
    if !args.fallback_fonts.is_empty() && args.fonts.is_empty() {
        out.push(
            "--fallback-font 没有 --font 时不起作用：没有正文字体就走桩度量，回退字体轮不到".into(),
        );
    }
    out
}

/// 所有段落里回退链要管的 eastAsia 字符（判据按 `trigger`）：（出现次数，去重后的字符）。
fn missing_chars(
    registry: &FontRegistry,
    paras: &[Para],
    trigger: Trigger,
) -> (usize, BTreeSet<char>) {
    let mut count = 0;
    let mut set = BTreeSet::new();
    for run in paras.iter().flat_map(|p| &p.runs).filter(|run| !run.hidden) {
        let chars: Vec<char> = match trigger {
            Trigger::SlotMissing => registry.fallback_candidates(&run.text, &run.font).collect(),
            Trigger::NoFace => registry.uncovered_chars(&run.text, &run.font).collect(),
        };
        count += chars.len();
        set.extend(chars);
    }
    (count, set)
}

/// 装完之后，会查回退链的字符（`FontRegistry::fallback_candidates`）各落到哪，按出现次数。
#[derive(Debug, Default, PartialEq)]
struct Landing {
    /// 回退链上的 face。
    chain: usize,
    /// 回退链盖不住、别的 `--font` 画的。
    primary: usize,
    /// 谁都画不出、按名义 1 em 画 `.notdef` 的 CJK。
    nominal: usize,
    /// 谁都画不出、也没有名义宽度的，跳过。
    skipped: usize,
}

fn landing(registry: &FontRegistry, paras: &[Para]) -> Landing {
    let mut out = Landing::default();
    for run in paras.iter().flat_map(|p| &p.runs).filter(|run| !run.hidden) {
        for ch in registry.fallback_candidates(&run.text, &run.font) {
            match registry.select_face_for(&run.font, ch) {
                Some(face) if registry.fallback_faces().contains(&face) => out.chain += 1,
                Some(_) => out.primary += 1,
                None if registry.face_for_char(&run.font, ch).is_some() => out.nominal += 1,
                None => out.skipped += 1,
            }
        }
    }
    out
}

/// 装回退链。只在 `--font` 装上了东西、且文档里确有回退链要管的 eastAsia 字符
/// （判据见 [`Trigger`]）时才装：没有正文字体时走的是桩度量，回退字体轮不到。
/// 链上每个 face 只在它的 cmap 盖得住**还缺着**的字符时才注册（与 fontenv 的 `covers`
/// 同一口径：字形 id 非零），盖不住的不装——装了会改字体指纹。「装了也不改结果」对段落标记
/// **不成立**：缺字扫描不看段落标记，带 eastAsia 提示的标记空格在全装时会落到回退字体
/// （`fixtures/table.docx`，见 `docs/SHARED-FONT-SESSION-2026-09-28.md`）。
/// 已是 `--font` 的 face 不进回退链（`FontRegistry::add_fallback` 不降级），它盖得住的字符
/// 仍算缺着，留给链上后面的 face；轨迹里记一句，不静默。
///
/// 返回写进轨迹 `metrics` 栏的一句；没有要管的字符时为空，轨迹的这一栏与原来相同。
fn load_fallback(
    registry: &mut FontRegistry,
    paras: &[Para],
    chain: &FallbackChain,
) -> Result<String, String> {
    if registry.is_empty() {
        return Ok(String::new());
    }
    let (missing, mut left) = missing_chars(registry, paras, chain.trigger);
    if missing == 0 {
        return Ok(String::new());
    }
    let distinct = left.len();
    let mut loaded = Vec::new();
    let mut already_primary = Vec::new();
    for spec in &chain.specs {
        if left.is_empty() {
            break;
        }
        let data = std::fs::read(&spec.path)
            .map_err(|e| format!("回退字体读不到 {}：{e}", spec.path.display()))?;
        let name = spec
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| spec.path.display().to_string());
        let mut index = spec.index.unwrap_or(0);
        loop {
            let face = match skrifa::FontRef::from_index(&data, index) {
                Ok(face) => face,
                // 没给 `#index` 时装到装不进为止；第一个就装不进是文件不对。
                Err(_) if spec.index.is_none() && index > 0 => break,
                Err(error) => {
                    return Err(format!("回退字体装不进去 {name}#{index}：{error}"));
                }
            };
            let charmap = face.charmap();
            let covered: Vec<char> = left
                .iter()
                .copied()
                .filter(|&ch| charmap.map(ch).is_some_and(|g| g.to_u32() != 0))
                .collect();
            if !covered.is_empty() {
                let key = registry
                    .add_fallback(data.clone(), index)
                    .map_err(|e| format!("回退字体装不进去 {name}#{index}：{e}"))?;
                let label = if spec.index.is_some() || index > 0 {
                    format!("{name}#{index}")
                } else {
                    name.clone()
                };
                if registry.fallback_faces().contains(&key) {
                    for ch in covered {
                        left.remove(&ch);
                    }
                    loaded.push(label);
                } else {
                    already_primary.push(label);
                }
            }
            if spec.index.is_some() || left.is_empty() {
                break;
            }
            index += 1;
        }
    }
    let landed = landing(registry, paras);
    let tried: Vec<String> = chain
        .specs
        .iter()
        .map(|s| {
            let name = s.path.file_name().map_or_else(
                || s.path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            match s.index {
                Some(i) => format!("{name}#{i}"),
                None => name,
            }
        })
        .collect();
    let rest = format!(
        "余下 CJK {} 个按名义 1 em 画 .notdef、其他 {} 个跳过",
        landed.nominal, landed.skipped
    );
    Ok(match chain.trigger {
        Trigger::NoFace => format!(
            "；--font 缺 eastAsia 字符 {missing} 个（{distinct} 种），回退链（{}）[{}] 装上 [{}] 补 {}，{rest}",
            chain.from,
            tried.join(", "),
            loaded.join(", "),
            missing - landed.nominal - landed.skipped,
        ),
        Trigger::SlotMissing => format!(
            "；eastAsia 槽的字体画不出的字符 {missing} 个（{distinct} 种），回退链（{}）[{}] 装上 [{}]{} \
             接走 {}、其他 --font 画 {}，{rest}",
            chain.from,
            tried.join(", "),
            loaded.join(", "),
            if already_primary.is_empty() {
                String::new()
            } else {
                format!(
                    "（[{}] 已是 --font，不进回退链）",
                    already_primary.join(", ")
                )
            },
            landed.chain,
            landed.primary,
        ),
    })
}

/// 轨迹 `metrics` 栏的文字。平台与视图跟度量一起记：同一份文档换平台、换视图，
/// 排出来的行可以不同，轨迹得自己说清是哪一种。取值按命令行的写法记，照着就能重跑。
/// 由平台决定的断行规则（行末标点挂不挂出）也写明，读轨迹的人不必去翻 `Platform` 的说明。
/// `fallback` 是 [`load_fallback`] 的那一句（没有回退链要管的字符时为空）。
fn metrics_note(
    use_stub: bool,
    grid: VerticalGrid,
    platform: Platform,
    view: View,
    fallback: &str,
) -> String {
    let metrics = if use_stub {
        format!("SimpleMetrics (近似桩：按字符类别给固定宽度，不读字体文件{fallback})")
    } else {
        format!(
            "RealMetrics (读字体 + rustybuzz 整形；纵向栅格 {}{fallback})",
            match grid {
                VerticalGrid::None => "无",
                VerticalGrid::MacWordThreeHundredthsInch => "Mac Word 1/300 英寸（含回测规则）",
            }
        )
    };
    let (platform, overflow) = match platform {
        Platform::Desktop => ("mac", "按 w:overflowPunct 挂出"),
        Platform::Android => ("android", "不挂出"),
    };
    let view = match view {
        View::Print => "print",
        View::Mobile => "mobile",
    };
    format!("{metrics}；平台 {platform}，视图 {view}；行末标点 {overflow}")
}

/// `--metrics` 的取值：`true` 是要近似桩。没给就是 real。
///
/// `simple` 不读字体，所以不许与 `--font` 同用：近似度量排出的行配上真字体整形的字形，
/// 行与字形两边对不上，却看着像一份有字形的轨迹。
fn stub_requested(args: &Args) -> Result<bool, String> {
    match args.metrics.as_deref() {
        None | Some("real") => Ok(false),
        Some("simple") if !args.fonts.is_empty() => Err(
            "--metrics simple 不读字体，不能与 --font 同用：近似度量的行配不上真字体整形的字形".into(),
        ),
        Some("simple") => Ok(true),
        Some(other) => Err(format!("--metrics 只接受 simple / real，收到 {other}")),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(std::env::args().skip(1))?;
    let stub = stub_requested(&args)?;
    // 回退链当场定、路径当场核：拼错的路径不该等到文档里真有缺字才报。
    let mut warnings = fallback_warnings(&args);
    let chain = fallback_chain(
        &args,
        std::env::var_os(FALLBACK_ENV).as_deref(),
        &mut warnings,
    )?;
    for warning in &warnings {
        eprintln!("{warning}");
    }
    let input = args.input.ok_or("用法：layout-trace [选项] <input.docx> [output.json]")?;
    let output = args.output.unwrap_or_else(|| "trace.json".to_string());

    let bytes = std::fs::read(&input)?;
    // 与 SVG CLI 同一个文档会话。不直接 `SessionTable::document()`：并排的 `w:rPr`
    // 解析器只留最后一个，`PreparedDocument::load` 先把它们并起来。
    let mut prepared = PreparedDocument::load(&bytes)?;
    let overrides = prepared.apply_page_overrides(rsword_layout_core::PageOverrides {
        margin: args.margin,
        page_width: args.page_width,
        content_width: args.content_width,
    });
    for diagnostic in prepared.diagnostics() {
        eprintln!("{diagnostic}");
    }
    overrides?;
    let document = prepared.document();
    // 表格单元格里的段落也要参与缺字扫描，否则回退字体只按正文挑。
    let paras: Vec<Para> = document.paras.iter()
        .chain(document.tables.iter().flat_map(|table| &table.rows)
            .flat_map(|row| &row.cells).flat_map(|cell| &cell.paras))
        .cloned()
        .collect();
    let paras = &paras;
    let skipped = document.skipped_blocks;
    if !prepared.has_layout_content() {
        return Err("没有可排版的段落".into());
    }

    // 装字体。没有整形器时 `paint_document` 不产字形序列，
    // 轨迹里就只有行、没有字形——那种轨迹过不了比较器的字形层，所以要说清楚。
    let mut registry = FontRegistry::new();
    for path in &args.fonts {
        let data = std::fs::read(path).map_err(|e| format!("字体读不到 {}：{e}", path.display()))?;
        let mut index = 0u32;
        loop {
            match registry.add(data.clone(), index) {
                Ok(_) => index += 1,
                Err(error) => {
                    if index == 0 {
                        eprintln!("装不进去 {}：{error}", path.display());
                    }
                    break;
                }
            }
        }
    }
    // 回退链排在 `--font` 之后：它只补 `--font` 盖不住的字。
    let fallback_note = load_fallback(&mut registry, paras, &chain)?;
    if !fallback_note.is_empty() {
        eprintln!("{}", fallback_note.trim_start_matches('；'));
    }

    // §6.2 的核查。放在排版之后、写出之前都行，但**必须在写出之前**。
    if !args.require.is_empty() {
        let missing: Vec<&String> = args
            .require
            .iter()
            .filter(|want| !registry.covers_family(want))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "要求的字体族没装上：{}。字体替换在几何上完全不可见（方法 §6.2），所以这里必须停。",
                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")
            )
            .into());
        }
    }

    let grid = match args.grid.as_deref() {
        Some("mac") => VerticalGrid::MacWordThreeHundredthsInch,
        Some("none") | None => VerticalGrid::None,
        Some(other) => return Err(format!("--vertical-grid 只接受 mac / none，收到 {other}").into()),
    };
    // 一个字体都没装上时也走桩——轨迹的 `metrics` 栏与 stdout 都会说。
    let use_stub = stub || registry.is_empty();

    // 度量的**性质**要随数走：差值的来源常常就在这一栏里。
    let metrics_note = metrics_note(use_stub, grid, args.platform, args.view, &fallback_note);

    let setup = document.sections[0].setup;
    eprintln!(
        "版心 {} twips（页 {}，左右边距 {} / {}）",
        setup.content_area().width,
        setup.size.width,
        setup.margins.left,
        setup.margins.right
    );

    // 排版与绘制都在会话里：真度量时量宽、整形与字形记录用同一个注册表。
    let options = LayoutOptions { platform: args.platform, view: args.view, wrap: WrapPolicy::None };
    let session = if use_stub {
        prepared.layout_approximate(&options)
    } else {
        prepared.layout_with_fonts(registry, grid, &options)
    }
    .map_err(|e| e.to_string())?;
    for diagnostic in session.layout_diagnostics() {
        eprintln!("{diagnostic}");
    }
    let record = LayoutRecord::from_paint(&session.paint());

    let meta = TraceMeta {
        engine: format!("rsword-layout-core {}", env!("CARGO_PKG_VERSION")),
        metrics: metrics_note,
        glyph_origin_method: if session.is_approximate() {
            "none: no shaper registered, glyph sequences are empty".into()
        } else {
            "shaped: positions come from the shaper's own output (rustybuzz)".into()
        },
        source: input.clone(),
        font_fingerprint: session.font_fingerprint().map(str::to_string),
    };

    let document = session.document();
    let mut trace: serde_json::Value = serde_json::from_str(&to_trace_json(&record, &meta))?;
    trace["layoutInput"] = document.trace_metadata();
    if !document.tables.is_empty() {
        trace["tableLayout"] = session.table_layout();
    }
    std::fs::write(&output, serde_json::to_string_pretty(&trace)?)?;

    let lines: usize = record.pages.iter().map(|p| p.lines.len()).sum();
    println!(
        "段落 {}（表格 {}）· 页 {} · 行 {lines} · 字形 {}",
        paras.len(),
        document.tables.len(),
        record.page_count(),
        record.glyph_count()
    );
    if session.is_approximate() {
        println!("**没装字体**：轨迹里只有行、没有字形，过不了比较器的字形层（用 --font 指定）");
    }
    if skipped > 0 {
        // 跳过的块不会出现在轨迹里；比较器会把它读成行数不符，所以这里要明说。
        println!("跳过非文本块 {skipped}（不支持的表格 / 绘图等，见 layout 诊断）——轨迹里没有它们");
    }
    println!("已写出 {output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsword_layout_core::{Engine, PageSetup, RealMetrics, paint_document};

    fn parse(argv: &[&str]) -> Result<Args, String> {
        parse_args(argv.iter().map(|s| s.to_string()))
    }

    #[test]
    fn metrics_choice_is_explicit() {
        assert_eq!(stub_requested(&parse(&["in.docx"]).unwrap()), Ok(false));
        assert_eq!(stub_requested(&parse(&["--metrics", "real", "in.docx"]).unwrap()), Ok(false));
        assert_eq!(stub_requested(&parse(&["--metrics", "simple", "in.docx"]).unwrap()), Ok(true));
        // 近似度量不读字体：给了字体就是要字形，两边对不上，拒绝而不是悄悄配一套 shaper。
        let mixed = stub_requested(&parse(&["--metrics", "simple", "--font", "a.ttf", "in.docx"]).unwrap());
        assert!(mixed.unwrap_err().contains("不能与 --font 同用"));
        // 拼错的取值原来悄悄当 real。
        let typo = stub_requested(&parse(&["--metrics", "rael", "in.docx"]).unwrap());
        assert!(typo.unwrap_err().contains("只接受 simple / real"));
    }

    #[test]
    fn defaults_are_mac_print() {
        // 默认必须是 Mac + 分页视图：`tools/measure/sweep.py` 不传这两个选项。
        let args = parse(&["in.docx"]).unwrap();
        assert_eq!(args.platform, Platform::Desktop);
        assert_eq!(args.view, View::Print);
        assert_eq!(args.input.as_deref(), Some("in.docx"));
        assert_eq!(args.output, None);
    }

    #[test]
    fn android_and_mobile_are_opt_in() {
        // 与 word_analyse 窄路径的调用同形：选项夹在别的选项与输入路径之间。
        let args = parse(&[
            "--font", "a.ttf", "--platform", "android", "--view", "mobile",
            "--content-width", "5329", "in.docx", "out.json",
        ])
        .unwrap();
        assert_eq!(args.platform, Platform::Android);
        assert_eq!(args.view, View::Mobile);
        assert_eq!(args.content_width, Some(5329));
        assert_eq!(args.input.as_deref(), Some("in.docx"));
        assert_eq!(args.output.as_deref(), Some("out.json"));

        // 两个选项各管各的：只给平台，视图仍是默认。
        let args = parse(&["--platform", "android", "in.docx"]).unwrap();
        assert_eq!((args.platform, args.view), (Platform::Android, View::Print));
        let args = parse(&["--view", "mobile", "in.docx"]).unwrap();
        assert_eq!((args.platform, args.view), (Platform::Desktop, View::Mobile));
    }

    #[test]
    fn explicit_defaults_are_accepted() {
        let args = parse(&["--platform", "mac", "--view", "print", "in.docx"]).unwrap();
        assert_eq!((args.platform, args.view), (Platform::Desktop, View::Print));
    }

    #[test]
    fn the_last_occurrence_wins() {
        let args = parse(&["--platform", "android", "--platform", "mac", "in.docx"]).unwrap();
        assert_eq!(args.platform, Platform::Desktop);
    }

    #[test]
    fn bad_values_are_rejected() {
        for bad in ["ios", "Android", "MAC", "windows", "desktop", ""] {
            let err = parse(&["--platform", bad, "in.docx"]).err();
            assert!(err.as_deref().is_some_and(|e| e.starts_with("--platform")), "{bad:?}: {err:?}");
        }
        for bad in ["web", "Mobile", "draft", ""] {
            let err = parse(&["--view", bad, "in.docx"]).err();
            assert!(err.as_deref().is_some_and(|e| e.starts_with("--view")), "{bad:?}: {err:?}");
        }
    }

    #[test]
    fn a_missing_value_is_rejected_instead_of_eating_the_input() {
        // `--view` 后面紧跟输入路径：要报取值不对，不能把路径当视图吞掉、再去找输出文件。
        let err = parse(&["--view", "in.docx"]).err().unwrap();
        assert!(err.contains("print / mobile"), "{err}");
        let err = parse(&["in.docx", "--platform"]).err().unwrap();
        assert!(err.contains("缺参数"), "{err}");
    }

    #[test]
    fn the_metrics_note_records_platform_and_view() {
        let note = metrics_note(true, VerticalGrid::None, Platform::Desktop, View::Print, "");
        assert!(note.starts_with("SimpleMetrics"), "{note}");
        assert!(note.contains("平台 mac，视图 print"), "{note}");
        let note = metrics_note(
            false,
            VerticalGrid::MacWordThreeHundredthsInch,
            Platform::Android,
            View::Mobile,
            "",
        );
        assert!(note.starts_with("RealMetrics"), "{note}");
        assert!(note.contains("1/300"), "{note}");
        assert!(note.contains("平台 android，视图 mobile"), "{note}");
        // 回退链那一句记在度量的括号里，平台与视图仍在后面。
        let note = metrics_note(
            false,
            VerticalGrid::None,
            Platform::Android,
            View::Print,
            "；回退",
        );
        assert!(note.contains("纵向栅格 无；回退)"), "{note}");
        assert!(note.ends_with("平台 android，视图 print；行末标点 不挂出"), "{note}");
    }

    const MISSING: &str = "/nonexistent/rsword-no-such-font.ttf";

    fn chain(argv: &[&str], env: Option<&str>) -> (Result<FallbackChain, String>, Vec<String>) {
        let args = parse(argv).unwrap();
        let mut warnings = fallback_warnings(&args);
        let chain = fallback_chain(&args, env.map(OsStr::new), &mut warnings);
        (chain, warnings)
    }

    fn spec(path: &str, index: Option<u32>) -> FallbackSpec {
        FallbackSpec {
            path: PathBuf::from(path),
            index,
        }
    }

    #[test]
    fn fallback_specs_take_an_optional_ttc_index() {
        assert_eq!(
            FallbackSpec::parse("a/Noto.ttc#2").unwrap(),
            spec("a/Noto.ttc", Some(2))
        );
        assert_eq!(
            FallbackSpec::parse("a/Droid.ttf").unwrap(),
            spec("a/Droid.ttf", None)
        );
        // `#` 后面不全是数字就是路径的一部分。
        assert_eq!(
            FallbackSpec::parse("a#b/x.ttf").unwrap(),
            spec("a#b/x.ttf", None)
        );
        assert_eq!(FallbackSpec::parse("x.ttc#").unwrap(), spec("x.ttc#", None));
        assert!(FallbackSpec::parse("x.ttc#99999999999").is_err());
    }

    #[test]
    fn fallback_options_parse_in_order() {
        let args = parse(&[
            "--fallback-font",
            "b.ttf",
            "--fallback-font",
            "a.ttc#2",
            "in.docx",
        ])
        .unwrap();
        assert_eq!(args.fallback_fonts, ["b.ttf", "a.ttc#2"]);
        assert!(!args.no_fallback);
        let args = parse(&["--no-fallback", "in.docx", "out.json"]).unwrap();
        assert!(args.no_fallback);
        assert_eq!(args.output.as_deref(), Some("out.json"));
        assert!(
            parse(&["in.docx", "--fallback-font"])
                .err()
                .unwrap()
                .contains("缺参数")
        );
    }

    #[test]
    fn the_default_chain_is_droid_on_android_and_nothing_on_mac() {
        // D7：默认只在 android 下开；mac（默认平台）不装，Mac 回放不被悄悄补字。
        let (mac, warnings) = chain(&["--font", "a.ttf", "in.docx"], None);
        assert_eq!(
            mac.unwrap(),
            FallbackChain {
                specs: Vec::new(),
                from: "mac 不装",
                trigger: Trigger::NoFace
            }
        );
        assert!(warnings.is_empty());
        // mac 下也不读环境变量：它只替换 android 的默认。
        let (mac, _) = chain(&["--font", "a.ttf", "in.docx"], Some(MISSING));
        assert!(mac.unwrap().specs.is_empty());

        let (android, warnings) = chain(
            &["--font", "a.ttf", "--platform", "android", "in.docx"],
            None,
        );
        assert_eq!(
            android.unwrap(),
            FallbackChain {
                specs: vec![spec(DEFAULT_FALLBACK, None)],
                from: "默认",
                trigger: Trigger::NoFace
            }
        );
        assert!(warnings.is_empty(), "仓库里有 Droid：{warnings:?}");
    }

    #[test]
    fn the_environment_replaces_the_android_default() {
        let argv = ["--font", "a.ttf", "--platform", "android", "in.docx"];
        let env = format!("{DEFAULT_FALLBACK}#0");
        let (got, _) = chain(&argv, Some(&env));
        assert_eq!(
            got.unwrap(),
            FallbackChain {
                specs: vec![spec(DEFAULT_FALLBACK, Some(0))],
                from: FALLBACK_ENV,
                trigger: Trigger::SlotMissing
            }
        );
        // 设了空值等于没设。
        assert_eq!(chain(&argv, Some("")).0.unwrap().from, "默认");
        // 环境变量指错了路径：当场报错，不退回默认。
        let err = chain(&argv, Some(MISSING)).0.unwrap_err();
        assert!(err.contains(FALLBACK_ENV) && err.contains(MISSING), "{err}");
    }

    #[test]
    fn explicit_fallback_fonts_are_checked_eagerly_on_every_platform() {
        // 路径当场核：文档全盖住、根本不用回退时也要报。
        for platform in ["mac", "android"] {
            let err = chain(
                &[
                    "--platform",
                    platform,
                    "--font",
                    "a.ttf",
                    "--fallback-font",
                    MISSING,
                    "in.docx",
                ],
                None,
            )
            .0
            .unwrap_err();
            assert!(
                err.contains("--fallback-font") && err.contains(MISSING),
                "{err}"
            );
            // 显式给了就用，mac 也一样；比环境变量优先。
            let (got, _) = chain(
                &[
                    "--platform",
                    platform,
                    "--font",
                    "a.ttf",
                    "--fallback-font",
                    DEFAULT_FALLBACK,
                    "in.docx",
                ],
                Some(MISSING),
            );
            assert_eq!(
                got.unwrap(),
                FallbackChain {
                    specs: vec![spec(DEFAULT_FALLBACK, None)],
                    from: "--fallback-font",
                    trigger: Trigger::SlotMissing
                }
            );
        }
    }

    #[test]
    fn no_fallback_wins_and_contradictory_options_warn() {
        let (got, warnings) = chain(
            &[
                "--platform",
                "android",
                "--font",
                "a.ttf",
                "--no-fallback",
                "--fallback-font",
                MISSING,
                "in.docx",
            ],
            None,
        );
        assert_eq!(
            got.unwrap(),
            FallbackChain {
                specs: Vec::new(),
                from: "--no-fallback",
                trigger: Trigger::NoFace
            }
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("--no-fallback 与 --fallback-font")),
            "{warnings:?}"
        );

        // 没有 --font 走桩度量，回退字体轮不到：警告。
        let (_, warnings) = chain(&["--fallback-font", DEFAULT_FALLBACK, "in.docx"], None);
        assert!(
            warnings.iter().any(|w| w.contains("没有 --font")),
            "{warnings:?}"
        );
        let (_, warnings) = chain(&["--platform", "android", "--no-fallback", "in.docx"], None);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    const FONT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/fonts/");

    /// 夹具的写法：西文槽是装了的字体，eastAsia 槽是装不上的 SimSun。
    fn simsun_para(text: &str) -> Para {
        let mut font = rsword_layout_core::FontSpec::new("Liberation Sans", 24);
        font.slots.h_ansi = Some("Liberation Sans".into());
        font.slots.east_asia = Some("SimSun".into());
        Para {
            runs: vec![rsword_layout_core::Run {
                text: text.into(),
                font,
                color: rsword_layout_core::Color::BLACK,
                placeholders: Vec::new(),
                rise: 0,
                rise_fine: None,
                hidden: false,
            }],
            ..Para::default()
        }
    }

    #[test]
    fn hidden_cjk_does_not_load_fallback_or_change_font_fingerprint() {
        let sans = std::fs::read(format!("{FONT_DIR}LiberationSans-Regular.ttf")).unwrap();
        let mut hidden = simsun_para("汉");
        hidden.runs[0].hidden = true;
        let paras = [simsun_para("Hello"), hidden];
        for trigger in [Trigger::NoFace, Trigger::SlotMissing] {
            let chain = FallbackChain {
                specs: vec![spec(DEFAULT_FALLBACK, None)],
                from: "test",
                trigger,
            };
            let mut registry = FontRegistry::new();
            registry.add(sans.clone(), 0).unwrap();
            let before = registry.fingerprint().map(str::to_string);
            assert_eq!(missing_chars(&registry, &paras, trigger), (0, BTreeSet::new()));
            assert_eq!(landing(&registry, &paras), Landing::default());
            assert_eq!(load_fallback(&mut registry, &paras, &chain).unwrap(), "");
            assert!(registry.fallback_faces().is_empty());
            assert_eq!(registry.fingerprint().map(str::to_string), before);
        }
    }

    #[test]
    fn fallback_faces_load_only_when_they_cover_something_still_missing() {
        let sans = std::fs::read(format!("{FONT_DIR}LiberationSans-Regular.ttf")).unwrap();
        let chain = FallbackChain {
            specs: vec![
                spec(&format!("{FONT_DIR}DejaVuSans.ttf"), None),
                spec(DEFAULT_FALLBACK, None),
            ],
            from: "--fallback-font",
            trigger: Trigger::SlotMissing,
        };
        let mut registry = FontRegistry::new();
        registry.add(sans, 0).unwrap();
        let before = registry.fingerprint().map(str::to_string);

        // 全盖住：什么都不装，字体指纹不变，`metrics` 栏不多一句。
        // U+3164（谚文填充符）是默认可忽略码位，不算缺字——Droid 虽然映射了它。
        let note = load_fallback(&mut registry, &[simsun_para("Hello \u{3164}")], &chain).unwrap();
        assert_eq!(note, "");
        assert!(registry.fallback_faces().is_empty());
        assert_eq!(registry.fingerprint().map(str::to_string), before);

        // 缺「汉」：DejaVu 盖不住它，不装（装了只会改指纹）；Droid 装上，补齐。
        let paras = [simsun_para("Hello 汉\u{3164}"), simsun_para("汉")];
        let note = load_fallback(&mut registry, &paras, &chain).unwrap();
        assert_eq!(registry.fallback_faces().len(), 1, "{note}");
        assert!(
            note.contains("eastAsia 槽的字体画不出的字符 2 个（1 种）"),
            "{note}"
        );
        assert!(
            note.contains("装上 [DroidSansFallbackFull.ttf] 接走 2、其他 --font 画 0"),
            "{note}"
        );
        assert!(note.contains("余下 CJK 0 个"), "{note}");
        assert_ne!(registry.fingerprint().map(str::to_string), before);
    }

    /// `w:hint="eastAsia"` 的 30 个 `“`（U+201C）：eastAsia 槽写 SimSun（装不上），
    /// 西文字体画得出。G4 审查的 `quote-hint.docx` 就是这么写的。
    fn hinted_quotes() -> Para {
        let mut para = simsun_para(&"\u{201C}".repeat(30));
        para.runs[0].font.slots.hint = rsword_layout_core::FontHint::EastAsia;
        para
    }

    /// 照 `main` 的装法排窄路径（`--platform android --view mobile --content-width 5329`），
    /// 返回每行的（行首，各字形的 face）。
    fn lay_out(registry: &FontRegistry, paras: &[Para]) -> Vec<(u32, Vec<String>)> {
        let mut setup = PageSetup::a4();
        let slack = setup.size.width - 5329;
        setup.margins.left = slack / 2;
        setup.margins.right = slack - setup.margins.left;
        let real = RealMetrics::new(registry);
        let pages = Engine::new(&real, setup)
            .with_platform(Platform::Android, View::Mobile)
            .layout(paras);
        let record = LayoutRecord::from_paint(&paint_document(
            &pages,
            Some(registry),
            &registry.face_ids(),
        ));
        record
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .map(|l| {
                (
                    l.source.map_or(0, |s| s.start),
                    l.glyphs.iter().map(|g| g.face.clone()).collect(),
                )
            })
            .collect()
    }

    /// 显式的回退链按选 face 的判据装（[`Trigger::SlotMissing`]）：`hint="eastAsia"` 的 `“`
    /// 在 eastAsia 槽，槽里的 SimSun 装不上，就归回退链——哪怕 `--font` 的 Liberation 画得出、
    /// 全文也没有一个谁都画不出的字。原来按「谁都画不出」装，这里什么都不装，
    /// `--fallback-font` 被静默忽略；再加一段不相干的「汉」，链才装上，`“` 跟着换字体。
    /// 顺序本身（链先于其他 `--font`）是**假定**，见 `FontRegistry`。
    #[test]
    fn an_explicit_chain_takes_east_asia_slot_chars_a_primary_font_also_draws() {
        let sans = std::fs::read(format!("{FONT_DIR}LiberationSans-Regular.ttf")).unwrap();
        let dejavu_path = format!("{FONT_DIR}DejaVuSans.ttf");
        let args = parse(&[
            "--font",
            "x.ttf",
            "--fallback-font",
            &dejavu_path,
            "in.docx",
        ])
        .unwrap();
        let chain = fallback_chain(&args, None, &mut Vec::new()).unwrap();
        assert_eq!(chain.trigger, Trigger::SlotMissing);
        let quote_font = hinted_quotes().runs[0].font.clone();

        let alone = [hinted_quotes()];
        let with_han = [hinted_quotes(), simsun_para("汉")];
        let mut layouts = Vec::new();
        for paras in [&alone[..], &with_han[..]] {
            let mut registry = FontRegistry::new();
            let liberation = registry.add(sans.clone(), 0).unwrap();
            // 判别力：`“` 没有一个是谁都画不出的，旧判据在这里不装链。
            assert_eq!(missing_chars(&registry, &alone, Trigger::NoFace).0, 0);
            assert_eq!(
                registry.select_face_for(&quote_font, '\u{201C}'),
                Some(liberation.clone())
            );

            let note = load_fallback(&mut registry, paras, &chain).unwrap();
            let [dejavu] = registry.fallback_faces() else {
                panic!("DejaVu 该装上：{note}");
            };
            let dejavu = dejavu.clone();
            assert_eq!(
                registry.select_face_for(&quote_font, '\u{201C}'),
                Some(dejavu.clone())
            );
            assert!(note.contains("回退链（--fallback-font）"), "{note}");
            assert!(
                note.contains("装上 [DejaVuSans.ttf] 接走 30、其他 --font 画 0"),
                "{note}"
            );
            // 排出来的第一段每个字形都是 DejaVu 的。
            let lines = lay_out(&registry, paras);
            let quote_lines = &lines[..lines.len() + 1 - paras.len()];
            assert!(
                quote_lines
                    .iter()
                    .flat_map(|(_, faces)| faces)
                    .all(|f| *f == dejavu),
                "{lines:?}"
            );
            // 30 个 `“` 加段落标记。
            assert_eq!(
                quote_lines
                    .iter()
                    .map(|(_, faces)| faces.len())
                    .sum::<usize>(),
                31
            );
            layouts.push(quote_lines.to_vec());
        }
        // 一个字符用哪个 face 只看它自己：加一段「汉」，第一段一字不变。
        assert_eq!(layouts[0], layouts[1]);
    }

    /// 默认链（Droid）仍按「谁都画不出」装（[`Trigger::NoFace`]），所以**不局部**——
    /// 有意保留：按槽的判据装，CJK 夹具的汉字会从 `--font` 的 Noto 手里被 Droid 接走。
    /// `hint="eastAsia"` 的空格（Droid 映射了 U+0020，没有 `“`）：只有这一段时不装，
    /// 空格用 Liberation；文档别处有一个「汉」，Droid 装上，同一段的空格也改用 Droid。
    #[test]
    fn the_default_chain_still_loads_only_for_a_char_no_font_draws() {
        let sans = std::fs::read(format!("{FONT_DIR}LiberationSans-Regular.ttf")).unwrap();
        let (chain, _) = chain(
            &["--platform", "android", "--font", "x.ttf", "in.docx"],
            None,
        );
        let chain = chain.unwrap();
        assert_eq!(chain.trigger, Trigger::NoFace);
        let mut spaced = hinted_quotes();
        spaced.runs[0].text = "\u{201C} \u{201C} \u{201C}".into();
        let font = spaced.runs[0].font.clone();

        let mut registry = FontRegistry::new();
        let liberation = registry.add(sans.clone(), 0).unwrap();
        let alone = [spaced.clone()];
        assert_eq!(load_fallback(&mut registry, &alone, &chain).unwrap(), "");
        assert!(registry.fallback_faces().is_empty());
        assert_eq!(
            registry.select_face_for(&font, ' '),
            Some(liberation.clone())
        );

        let mut registry = FontRegistry::new();
        registry.add(sans, 0).unwrap();
        let note = load_fallback(&mut registry, &[spaced, simsun_para("汉")], &chain).unwrap();
        assert!(
            note.contains("--font 缺 eastAsia 字符 1 个（1 种）"),
            "{note}"
        );
        let [droid] = registry.fallback_faces() else {
            panic!("Droid 该装上：{note}");
        };
        assert_eq!(registry.select_face_for(&font, ' ').as_ref(), Some(droid));
        // Droid 没有 `“`，它仍归 Liberation。
        assert_eq!(
            registry.select_face_for(&font, '\u{201C}'),
            Some(liberation)
        );
    }

    /// 链上的 face 已是 `--font`：`add_fallback` 不降级，它进不了链。轨迹里明说，
    /// 它盖得住的字符仍算缺着，留给链上后面的 face。
    #[test]
    fn a_chain_face_that_is_already_a_primary_font_is_reported_not_dropped_silently() {
        let sans = std::fs::read(format!("{FONT_DIR}LiberationSans-Regular.ttf")).unwrap();
        let dejavu = std::fs::read(format!("{FONT_DIR}DejaVuSans.ttf")).unwrap();
        let chain = FallbackChain {
            specs: vec![
                spec(&format!("{FONT_DIR}DejaVuSans.ttf"), None),
                spec(&format!("{FONT_DIR}LiberationSerif-Regular.ttf"), None),
            ],
            from: "--fallback-font",
            trigger: Trigger::SlotMissing,
        };
        let mut registry = FontRegistry::new();
        registry.add(sans, 0).unwrap();
        registry.add(dejavu, 0).unwrap();
        let note = load_fallback(&mut registry, &[hinted_quotes()], &chain).unwrap();
        assert!(
            note.contains(
                "装上 [LiberationSerif-Regular.ttf]（[DejaVuSans.ttf] 已是 --font，不进回退链） 接走 30、其他 --font 画 0"
            ),
            "{note}"
        );
        let [serif] = registry.fallback_faces() else {
            panic!("Liberation Serif 该装上：{note}");
        };
        let font = hinted_quotes().runs[0].font.clone();
        assert_eq!(
            registry.select_face_for(&font, '\u{201C}').as_ref(),
            Some(serif)
        );
    }

    #[test]
    fn without_a_chain_missing_cjk_is_reported_as_nominal() {
        let sans = std::fs::read(format!("{FONT_DIR}LiberationSans-Regular.ttf")).unwrap();
        let mut registry = FontRegistry::new();
        registry.add(sans, 0).unwrap();
        let before = registry.fingerprint().map(str::to_string);
        let off = FallbackChain {
            specs: Vec::new(),
            from: "mac 不装",
            trigger: Trigger::NoFace,
        };
        // U+3105（注音）在 eastAsia 槽却不在 `is_cjk` 里：没有名义宽度，跳过。
        let note = load_fallback(&mut registry, &[simsun_para("汉汉\u{3105}")], &off).unwrap();
        assert!(note.contains("回退链（mac 不装）[] 装上 [] 补 0"), "{note}");
        assert!(
            note.contains("余下 CJK 2 个按名义 1 em 画 .notdef、其他 1 个跳过"),
            "{note}"
        );
        assert_eq!(registry.fingerprint().map(str::to_string), before);
        // 没有正文字体：桩度量，回退链轮不到。
        let mut empty = FontRegistry::new();
        let chain = FallbackChain {
            specs: vec![spec(DEFAULT_FALLBACK, None)],
            from: "默认",
            trigger: Trigger::NoFace,
        };
        assert_eq!(
            load_fallback(&mut empty, &[simsun_para("汉")], &chain).unwrap(),
            ""
        );
        assert!(empty.is_empty());
    }

    #[test]
    fn the_metrics_note_records_the_overflow_rule_of_the_platform() {
        // 挂不挂出没有单独的开关，由平台定；两条轨迹只差这一点时，`metrics` 栏要看得出来。
        let note = metrics_note(true, VerticalGrid::None, Platform::Desktop, View::Print, "");
        assert!(note.ends_with("行末标点 按 w:overflowPunct 挂出"), "{note}");
        for view in [View::Print, View::Mobile] {
            let note = metrics_note(true, VerticalGrid::None, Platform::Android, view, "");
            assert!(note.ends_with("行末标点 不挂出"), "{note}");
        }
    }

    #[test]
    fn there_is_no_separate_overflow_switch() {
        // 原型的 `--overflow-punct` 不存在：挂出跟着 `--platform` 走，不能单独拨成与平台矛盾。
        let err = parse(&["--overflow-punct", "mac", "in.docx"]).err().unwrap();
        assert!(err.contains("未知选项"), "{err}");
    }
}
