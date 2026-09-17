//! rsword 模型 JSON → 布局引擎输入。
//!
//! **当前是最小桥接，有意留了缺口**：`document()` 给的 `props` 是*声明值*，样式链的有效属性
//! 要走 `rsword::resolve`（Rust 内部 API，JSON 投影里没有）。这里按 `styleId` 做最小映射，
//! 只够把链路跑通；接 `Resolver` 后应当替换掉 [`style_defaults`]。
//!
//! 已知不覆盖：表格、绘图、页眉页脚、分节、编号、字段结果的复杂形态。

use serde_json::Value;

use crate::layout::Color;
use crate::layout::{Align, LineRule, OBJECT_PLACEHOLDER, Para, PlaceholderKind, Run};
use crate::layout::{Twips, half_points_to_twips};
use crate::font::{FontHint, FontSlots, FontSpec};

/// 文档默认正文字体与字号（对应 fixture 的 `docDefaults`）。
const BODY_FAMILY: &str = "Times New Roman, SimSun, serif";
const BODY_SIZE_HALF_POINTS: u32 = 24;

/// 按样式 id 给的近似有效属性。接 `resolve` 后删除。
fn style_defaults(style_id: Option<&str>, level: Option<u64>) -> (u32, bool, Twips, Twips, bool) {
    // 返回 (字号半点, 粗体, space_before, space_after, keep_next)
    match (style_id, level) {
        (Some("Heading1"), _) | (_, Some(1)) => (32, true, 240, 120, true),
        (Some("Heading2"), _) | (_, Some(2)) => (28, true, 200, 100, true),
        _ => (BODY_SIZE_HALF_POINTS, false, 0, 120, false),
    }
}

fn as_bool(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}

/// `w:vertAlign` 与 `w:position` → (有效字号半点, 基线抬升 twips)。
///
/// 两者是**不同的机制**，不要合并：
///
/// - `w:vertAlign`（上下标）**同时**缩小字号并挪基线；
/// - `w:position`（半点，可负）**只挪基线，不改字号**——实测 `w:position=8`
///   的那段仍是 12pt，只是抬高了。
///
/// 抬升量落在实测的 1/300 英寸纵向栅格上（`w:position=8` 即 4.00pt，
/// 实测抬升 4.08pt = 17 × 0.24pt）。这里不做量化——量化属于度量层，
/// 本仓库尚未实现（见 docs 的 G-5）。
fn vertical_run_shape(props: &Value, base_size: u32) -> (u32, Twips) {
    let em = i64::from(half_points_to_twips(base_size));
    match props.get("vertAlign").and_then(Value::as_str) {
        Some("superscript") => (
            scaled_size(base_size),
            (em * SUPERSCRIPT_RISE_PERMILLE / 1000) as Twips,
        ),
        Some("subscript") => (
            scaled_size(base_size),
            -((em * SUBSCRIPT_DROP_PERMILLE / 1000) as Twips),
        ),
        // `w:position` 与上下标互斥时以上下标为准（Word 的行为，实测未覆盖，
        // 此处按 OOXML 的属性独立性取「没有 vertAlign 才看 position」）。
        _ => {
            // `w:position` 的单位是**半点**，正值向上。
            let half_points = props.get("position").and_then(Value::as_i64).unwrap_or(0);
            (base_size, half_points_to_twips_signed(half_points))
        }
    }
}

/// 上下标的字号，取**最近的**整数半点。
///
/// 截断会差得多：12pt 的 0.66 是 15.84 半点，截断给 15（7.5pt，差 −0.42pt），
/// 四舍五入给 16（8pt，差 +0.08pt）。
fn scaled_size(base_size: u32) -> u32 {
    // 四舍五入，不是向上取整：向上取整在 15.2 这类值上会给 16，偏得更远。
    ((base_size * SUPERSUB_SIZE_NUM + SUPERSUB_SIZE_DEN / 2) / SUPERSUB_SIZE_DEN).max(1)
}

/// 半点 → twips，带符号（`half_points_to_twips` 只收无符号）。
fn half_points_to_twips_signed(half_points: i64) -> Twips {
    (half_points * 10) as Twips
}

/// 从一个 run 的 `props` 读出影响度量的字段，叠在段落基准之上。
fn run_font(props: &Value, base_size: u32, base_bold: bool) -> FontSpec {
    let size = props
        .get("size")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(base_size);
    // 上下标要缩小字号——这一步必须在这里做，因为字号影响度量，
    // 而抬升不影响（抬升挂在 `Run::rise` 上）。
    let (size, _) = vertical_run_shape(props, size);
    let bold = props.get("bold").map(as_bool).unwrap_or(base_bold);
    let italic = props.get("italic").map(as_bool).unwrap_or(false);
    let slots = read_slots(props.get("fonts"));
    // `family` 保留为 ascii 槽的值，供只认单一字体的调用方使用。
    let family = slots
        .ascii
        .clone()
        .or_else(|| slots.h_ansi.clone())
        .unwrap_or_else(|| BODY_FAMILY.to_string());

    FontSpec {
        slots,
        family,
        size_half_points: size,
        bold,
        italic,
        letter_spacing: 0,
        scale_pct: 100,
    }
}

/// 读 `w:rFonts` 的四个槽与 `w:hint`。
///
/// **这是 Word 选字体的真实规则**：同一 run 里每个字符按所属区查对应的槽，
/// 而不是整个 run 用一个字体再靠 fallback 补。JSON 里的键名与 OOXML 一致。
///
/// 主题字体（`asciiTheme` 等，值形如 `minorHAnsi`）需要查 theme part 解析，
/// 尚未实现——此时该槽留空，由继承或 `family` 兜底，而不是把 `minorHAnsi`
/// 当成字体名去找。
fn read_slots(fonts: Option<&Value>) -> FontSlots {
    let Some(f) = fonts else {
        return FontSlots::default();
    };
    let get = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_owned);
    FontSlots {
        ascii: get("ascii"),
        h_ansi: get("hAnsi"),
        east_asia: get("eastAsia"),
        cs: get("cs"),
        hint: match f.get("hint").and_then(Value::as_str) {
            Some("eastAsia") => FontHint::EastAsia,
            Some("cs") => FontHint::Cs,
            _ => FontHint::Default,
        },
    }
}

/// 上下标的字号比例与基线偏移。
///
/// **这些是 Word 自己的常数，不是字体给的。** 实测把两者分开了
/// （Liberation Serif 12pt，Word for Mac）：
///
/// | | 字体 OS/2 说 | Word 实际用 |
/// | --- | ---: | ---: |
/// | 字号 | 0.6499 em（7.7988pt） | **0.66 em（7.92pt）** |
/// | 上标偏移 | 0.4531 em（5.4375pt） | **0.34 em（4.08pt）** |
/// | 下标偏移 | 0.1431 em（1.7168pt） | **0.08 em（0.96pt）** |
///
/// 所以**不要改成读 `ySuperscriptYSize` / `ySubscriptYOffset`**——那看着更「正确」，
/// 但与 Word 对不上。
///
/// **证据强度：n=1**（一种字体、一个字号）。三个比例都很圆整，支持「Word 常数」
/// 这一读法；但要证实或推翻，需要一份**专门变字号与字体**的夹具。
/// 按量具方法 §7.5 标「回测」，**不当独立检验**。
///
/// **还有一条单位上的天花板**：12pt 的 0.66 是 **7.92pt = 15.84 半点**，
/// 而 `FontSpec::size_half_points` 是整数半点，**表达不了**。这里取最近的整数半点
/// （16 半点 = 8pt，差 +0.08pt）。要真正对上，字号得能表示到半点以下。
/// 与 `Twips` 对不上纵向栅格是同一类问题（见 docs 的 G-8）。
const SUPERSUB_SIZE_NUM: u32 = 66;
const SUPERSUB_SIZE_DEN: u32 = 100;
/// 上标抬升，em 的千分比。
const SUPERSCRIPT_RISE_PERMILLE: i64 = 340;
/// 下标下沉，em 的千分比。
const SUBSCRIPT_DROP_PERMILLE: i64 = 80;

/// 一个 run 的 `text` 里每个 [`PlaceholderKind`]，按文档顺序。
///
/// 占位符本身**不区分种类**——分页符、软回车、行内图在 run 文本里都是同一个 U+FFFC。
/// 种类只在 `segments[].kind` 里，所以必须在这里读出来带给排版层，
/// 否则排版分不出「这里要翻页」和「这里有张图」。
///
/// **个数对不上就整体退回 `Object`**：那是保守方向——宁可少一次分页，
/// 也不凭空造出一次。凭空多出来的页在比较器里只会报结构失败，查起来更费事。
fn run_placeholders(run: &Value, text: &str) -> Vec<PlaceholderKind> {
    let want = text.matches(OBJECT_PLACEHOLDER).count();
    if want == 0 {
        return Vec::new();
    }
    let Some(Value::Array(segments)) = run.get("segments") else {
        return vec![PlaceholderKind::Object; want];
    };

    let mut out = Vec::new();
    for segment in segments {
        let Some(kind) = segment.get("kind") else { continue };
        match kind.get("kind").and_then(Value::as_str) {
            Some("br") => out.push(match kind.get("breakKind").and_then(Value::as_str) {
                Some("page") => PlaceholderKind::PageBreak,
                Some("column") => PlaceholderKind::ColumnBreak,
                // 缺省即 textWrapping（软回车）。
                _ => PlaceholderKind::LineBreak,
            }),
            // 行内对象：占位但不断开。
            Some("drawing") | Some("object") | Some("pict") => out.push(PlaceholderKind::Object),
            // 文本段不占位；其余未知种类保守当对象。
            Some("text") => {}
            _ => out.push(PlaceholderKind::Object),
        }
    }

    if out.len() == want { out } else { vec![PlaceholderKind::Object; want] }
}

/// 递归收集一个块里的所有 run 文本。
fn collect_runs(inlines: &Value, base_size: u32, base_bold: bool, out: &mut Vec<Run>) {
    match inlines {
        Value::Array(items) => {
            for it in items {
                collect_runs(it, base_size, base_bold, out);
            }
        }
        Value::Object(_) => {
            let kind = inlines.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind == "run" {
                if let Some(t) = inlines.get("text").and_then(Value::as_str)
                    && !t.is_empty()
                {
                    let props = inlines.get("props").cloned().unwrap_or(Value::Null);
                    out.push(Run {
                        text: t.to_string(),
                        font: run_font(&props, base_size, base_bold),
                        color: Color::BLACK,
                        placeholders: run_placeholders(inlines, t),
                        rise: vertical_run_shape(&props, base_size).1,
                    });
                }
            } else if kind == "field" {
                if let Some(r) = inlines.get("result") {
                    collect_runs(r, base_size, base_bold, out);
                }
            } else if let Some(inner) = inlines.get("inlines") {
                collect_runs(inner, base_size, base_bold, out);
            }
        }
        _ => {}
    }
}


/// `w:jc` → 对齐。`both` / `distribute` 都按两端对齐处理。
fn read_align(props: &Value) -> Align {
    match props.get("jc").and_then(Value::as_str) {
        Some("center") => Align::Center,
        Some("right") | Some("end") => Align::Right,
        Some("both") | Some("distribute") => Align::Justify,
        _ => Align::Left,
    }
}

/// `w:ind` → (左, 右, 首行)。`hanging` 是负的首行缩进，与 `firstLine` 互斥。
fn read_indent(props: &Value) -> (Twips, Twips, Twips) {
    let ind = match props.get("indent") {
        Some(v) => v,
        None => return (0, 0, 0),
    };
    let num = |k: &str| ind.get(k).and_then(Value::as_i64).unwrap_or(0) as Twips;
    // start/end 是 Strict 的写法，left/right 是 Transitional 的。
    let left = if ind.get("start").is_some() { num("start") } else { num("left") };
    let right = if ind.get("end").is_some() { num("end") } else { num("right") };
    let first = if ind.get("hanging").is_some() { -num("hanging") } else { num("firstLine") };
    (left, right, first)
}

/// `w:spacing` → (行距规则, 值, 段前, 段后)。段前后取不到时用样式默认。
fn read_spacing(props: &Value, def_before: Twips, def_after: Twips)
    -> (LineRule, Twips, Twips, Twips)
{
    let sp = match props.get("spacing") {
        Some(v) => v,
        None => return (LineRule::Auto, 240, def_before, def_after),
    };
    let num = |k: &str| sp.get(k).and_then(Value::as_i64).map(|v| v as Twips);
    let rule = match sp.get("lineRule").and_then(Value::as_str) {
        Some("exact") => LineRule::Exact,
        Some("atLeast") => LineRule::AtLeast,
        _ => LineRule::Auto,
    };
    let line = num("line").unwrap_or(240);
    (rule, line, num("before").unwrap_or(def_before), num("after").unwrap_or(def_after))
}

/// 本段最后一个 `w:br` 的类型（`textWrapping` / `page` / `column`）。
///
/// JSON 投影里的实际形状是 `inlines[].segments[].kind` 为一个**对象**：
/// `{"kind": "br", "breakKind": "page"}`。两个字段都在那个对象里，
/// 而不是 `kind` 为字符串、`breakKind` 在 segment 上——按后者去取会恒为 `None`。
fn last_break(block: &Value) -> Option<String> {
    let mut found = None;
    // 按文档顺序遍历（不能用栈后进先出，否则取到的是第一个而非最后一个 br）。
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(block.get("inlines")?);
    while let Some(v) = queue.pop_front() {
        match v {
            Value::Array(items) => queue.extend(items.iter()),
            Value::Object(_) => {
                if let Some(k) = v.get("kind")
                    && k.get("kind").and_then(Value::as_str) == Some("br")
                    && let Some(b) = k.get("breakKind").and_then(Value::as_str)
                {
                    found = Some(b.to_string());
                }
                for key in ["segments", "inlines", "result"] {
                    if let Some(inner) = v.get(key) {
                        queue.push_back(inner);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// 按计数约定定本段的终止符。
///
/// 优先级与各自画几个字形的依据见量具方法 §4：
/// 段落标记画 1 个空格、软回车画 1 个、分节符画 0 个、
/// 手动分页符按在行里的位置画 0 或 1 个。
///
/// **分栏符（`column`）未测**，按软回车同级处理并留待实测。
fn pick_terminator(
    has_sect_pr: bool,
    brk: Option<String>,
    runs: &[Run],
) -> crate::oracle::LineTerminator {
    use crate::oracle::{LineTerminator as T, PageBreakPosition as P};

    // 段内 sectPr 优先：它画 0 个字形。
    if has_sect_pr {
        return T::SectionBreak;
    }
    match brk.as_deref() {
        Some("page") => {
            // 位置决定画几个：本层只能区分「段中有文字」与「独占」。
            // 「紧跟段落标记」需要知道 br 是否是段落最后一个 segment，
            // 而这里已经把 inline 摊平成 runs，判不出——故只在两种可判的
            // 情形间选，不猜第三种。
            let has_text = runs.iter().any(|r| !r.text.trim().is_empty());
            T::PageBreak(if has_text { P::MidParagraph } else { P::OwnLine })
        }
        // 软回车与分栏符都画 1 个字形；分栏符未测，同级处理。
        Some("textWrapping") | Some("column") => T::LineBreak,
        _ => T::ParagraphMark,
    }
}

/// 哪些块下标是「另起一页」的分节起点。
///
/// 口径是**实测对过的**，不是照规范推的：`w:sectPr/w:type` 说的是
/// **这一节自己怎么开始**，不是「上一节之后怎么断」。
/// MR1 夹具 5 个节的页归属逐条相符（5/5）：
///
/// | 节 | `blockRange` | `kind` | Word 的页归属 |
/// | --- | --- | --- | --- |
/// | s0 | `[0,9)` | `nextPage` | 起于文档开头，不额外起页 |
/// | s1 | `[9,10)` | `continuous` | 紧接上一节 |
/// | s2 | `[10,11)` | `nextPage` | **起新页** |
/// | s3 | `[11,12)` | `continuous` | 紧接上一节 |
/// | s4 | `[12,16)` | `nextPage` | **起新页** |
///
/// **`evenPage` / `oddPage` 只当成「起新页」**：它们还要求落在偶／奇页上，
/// 必要时补一张空页——那一层**未实现也未测**，这里不猜。
///
/// `nextColumn` 是换栏不是换页，本版不实现分栏，故**不当成换页**——
/// 当成换页会凭空多出页来，而凭空多出的页在比较器里只会报结构失败。
fn section_page_starts(doc: &Value) -> std::collections::BTreeSet<usize> {
    let mut out = std::collections::BTreeSet::new();
    let Some(Value::Array(sections)) = doc.get("sections") else {
        return out;
    };
    for section in sections {
        let kind = section
            .get("props")
            .and_then(|p| p.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("nextPage");
        if !matches!(kind, "nextPage" | "evenPage" | "oddPage") {
            continue;
        }
        if let Some(Value::Array(range)) = section.get("blockRange")
            && let Some(start) = range.first().and_then(Value::as_u64)
        {
            out.insert(start as usize);
        }
    }
    out
}

/// 把 `document()` 的 JSON 转成段落序列。
///
/// 只处理 `main` 里 `kind == "text"` 的块；表格与绘图块被跳过（会在返回的第二项里计数，
/// 调用方应当把它报告出来，而不是假装文档已经排完）。
pub fn paras_from_document(doc: &Value) -> (Vec<Para>, usize) {
    let mut paras = Vec::new();
    let mut skipped = 0usize;

    let main = match doc.get("main") {
        Some(Value::Array(items)) => items,
        _ => return (paras, skipped),
    };

    // 分节起点按**块下标**给出，而非文本块会被跳过，所以要按原下标查，
    // 不能用段落序号。
    let section_starts = section_page_starts(doc);

    for (block_index, block) in main.iter().enumerate() {
        let kind = block.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind != "text" {
            skipped += 1;
            continue;
        }
        let starts_section_page = section_starts.contains(&block_index);

        let style_id = block.get("styleId").and_then(Value::as_str);
        let level = block
            .get("textKind")
            .and_then(|t| t.get("level"))
            .and_then(Value::as_u64);
        let (size, bold, before, after, keep_next) = style_defaults(style_id, level);
        let has_sect_pr = block
            .get("facts")
            .and_then(|f| f.get("hasSectPr"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let brk = last_break(block);

        let mut runs = Vec::new();
        if let Some(inlines) = block.get("inlines") {
            collect_runs(inlines, size, bold, &mut runs);
        }

        // 空段落也要占一行高度。
        if runs.is_empty() {
            runs.push(Run {
                text: String::new(),
                font: FontSpec::new(BODY_FAMILY, size),
                color: Color::BLACK,
                placeholders: Vec::new(),
                rise: 0,
            });
        }

        let props = block.get("props").cloned().unwrap_or(Value::Null);
        let (indent_left, indent_right, indent_first_line) = read_indent(&props);
        let (line_rule, line_value, space_before, space_after) =
            read_spacing(&props, before, after);

        // 必须在 runs 被 move 进 Para 之前算好。
        let terminator = pick_terminator(has_sect_pr, brk, &runs);

        paras.push(Para {
            runs,
            align: read_align(&props),
            indent_left,
            indent_right,
            indent_first_line,
            space_before,
            space_after,
            line_rule,
            line_value,
            keep_next: keep_next || props.get("keepNext").map(as_bool).unwrap_or(false),
            keep_lines: props.get("keepLines").map(as_bool).unwrap_or(false),
            // 两个来源：段落属性 `w:pageBreakBefore`，以及本段是「另起一页」的分节起点。
            // 引擎侧对首页为空的情形已有保护，所以文档开头的那个节不会多出一张空页。
            page_break_before: starts_section_page
                || props.get("pageBreakBefore").map(as_bool).unwrap_or(false),
            source_node: block.get("node").and_then(Value::as_u64).map(|n| n as u32),
            terminator,
        });
    }

    (paras, skipped)
}
