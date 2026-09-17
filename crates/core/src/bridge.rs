//! rsword 模型 JSON → 布局引擎输入。
//!
//! **当前是最小桥接，有意留了缺口**：`document()` 给的 `props` 是*声明值*，样式链的有效属性
//! 要走 `rsword::resolve`（Rust 内部 API，JSON 投影里没有）。这里按 `styleId` 做最小映射，
//! 只够把链路跑通；接 `Resolver` 后应当替换掉 [`style_defaults`]。
//!
//! 已知不覆盖：表格、绘图、页眉页脚、分节、编号、字段结果的复杂形态。

use serde_json::Value;

use crate::layout::Color;
use crate::layout::{Align, LineRule, Para, Run};
use crate::layout::Twips;
use crate::measure::FontSpec;

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

/// 从一个 run 的 `props` 读出影响度量的字段，叠在段落基准之上。
fn run_font(props: &Value, base_size: u32, base_bold: bool) -> FontSpec {
    let size = props
        .get("size")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(base_size);
    let bold = props.get("bold").map(as_bool).unwrap_or(base_bold);
    let italic = props.get("italic").map(as_bool).unwrap_or(false);
    let family = props
        .get("fonts")
        .and_then(|f| f.get("ascii"))
        .and_then(Value::as_str)
        .unwrap_or(BODY_FAMILY)
        .to_string();

    FontSpec {
        family,
        size_half_points: size,
        bold,
        italic,
        letter_spacing: 0,
        scale_pct: 100,
    }
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

    for block in main {
        let kind = block.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind != "text" {
            skipped += 1;
            continue;
        }

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
            page_break_before: props.get("pageBreakBefore").map(as_bool).unwrap_or(false),
            source_node: block.get("node").and_then(Value::as_u64).map(|n| n as u32),
            terminator,
        });
    }

    (paras, skipped)
}
