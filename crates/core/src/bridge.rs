//! rsword 模型 JSON → 布局引擎输入。
//!
//! **当前是最小桥接，有意留了缺口**：`document()` 给的 `props` 是*声明值*，样式链的有效属性
//! 要走 `rsword::resolve`（Rust 内部 API，JSON 投影里没有）。这里按 `styleId` 做最小映射，
//! 只够把链路跑通；接 `Resolver` 后应当替换掉 [`style_defaults`]。
//!
//! 已知不覆盖：表格、绘图、页眉页脚、分节、编号、字段结果的复杂形态。

use serde_json::Value;

use crate::canvas::Color;
use crate::engine::{Align, BreakKind, LineRule, Para, Run};
use crate::geom::Twips;
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

/// 把一个 run 按 `segments` 拆开。
///
/// **必须按 segment 走，不能直接拿 `run.text`。** rsword 为 `w:br` 也给一个 segment，
/// 并在 `run.text` 里放一个 U+FFFC（`￼`）占位符。整串拿来排版会把这个占位符
/// **当成一个真字形画出去**——它在 Word 的 PDF 里根本不存在，
/// 于是每个分页符都凭空多出一个字形，行宽与后续所有字形的原点跟着偏。
fn split_run(run: &Value, font: FontSpec, out: &mut Vec<Run>) {
    let text = run.get("text").and_then(Value::as_str).unwrap_or("");
    let segments = match run.get("segments") {
        Some(Value::Array(items)) => items,
        // 没有 segments 的 run：整串当文本，与旧行为一致。
        _ => {
            if !text.is_empty() {
                out.push(Run { text: text.to_string(), font, color: Color::BLACK, break_after: None });
            }
            return;
        }
    };

    for segment in segments {
        let kind = segment.get("kind").and_then(|k| k.get("kind")).and_then(Value::as_str);
        let range = segment.get("text").and_then(Value::as_array);
        let slice = match range {
            Some(r) if r.len() == 2 => {
                let start = r[0].as_u64().unwrap_or(0) as usize;
                let end = r[1].as_u64().unwrap_or(0) as usize;
                text.get(start..end).unwrap_or("")
            }
            _ => "",
        };

        match kind {
            Some("br") => {
                // `breakKind` 缺省即 textWrapping（软回车）。
                let break_kind = match segment
                    .get("kind")
                    .and_then(|k| k.get("breakKind"))
                    .and_then(Value::as_str)
                {
                    Some("page") | Some("column") => BreakKind::Page,
                    _ => BreakKind::Line,
                };
                // 占位符不进文本：它不对应任何字形。
                out.push(Run {
                    text: String::new(),
                    font: font.clone(),
                    color: Color::BLACK,
                    break_after: Some(break_kind),
                });
            }
            _ => {
                if !slice.is_empty() {
                    out.push(Run {
                        text: slice.to_string(),
                        font: font.clone(),
                        color: Color::BLACK,
                        break_after: None,
                    });
                }
            }
        }
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
                let props = inlines.get("props").cloned().unwrap_or(Value::Null);
                split_run(inlines, run_font(&props, base_size, base_bold), out);
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

/// 哪些块下标是「另起一页」的分节起点。
///
/// `nextPage` / `evenPage` / `oddPage` 都要求新页；`continuous` 不要求；
/// `nextColumn` 是换栏不是换页，本版不实现分栏，所以**不当成换页**——
/// 当成换页会凭空多出页来，而多出来的页在比较器里只会报成结构失败，查起来更费事。
fn section_page_starts(doc: &Value) -> Vec<usize> {
    let mut out = Vec::new();
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
            out.push(start as usize);
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

    // 分节起始的块下标：该节的 `kind` 要求另起一页时记下来。
    //
    // 口径：`w:sectPr/w:type` 说的是**这一节自己怎么开始**，不是「上一节之后怎么断」。
    // 这是拿实测对过的——一份 5 节的夹具里，`nextPage` 的三节各自起新页、
    // `continuous` 的两节紧接上一节，五节的页归属逐条相符。
    let section_page_starts = section_page_starts(doc);

    for (block_index, block) in main.iter().enumerate() {
        let kind = block.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind != "text" {
            skipped += 1;
            continue;
        }
        let starts_section_page = section_page_starts.contains(&block_index);

        let style_id = block.get("styleId").and_then(Value::as_str);
        let level = block
            .get("textKind")
            .and_then(|t| t.get("level"))
            .and_then(Value::as_u64);
        let (size, bold, before, after, keep_next) = style_defaults(style_id, level);

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
                break_after: None,
            });
        }

        let props = block.get("props").cloned().unwrap_or(Value::Null);
        let (indent_left, indent_right, indent_first_line) = read_indent(&props);
        let (line_rule, line_value, space_before, space_after) =
            read_spacing(&props, before, after);

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
            page_break_before: starts_section_page
                || props.get("pageBreakBefore").map(as_bool).unwrap_or(false),
            source_node: block.get("node").and_then(Value::as_u64).map(|n| n as u32),
        });
    }

    (paras, skipped)
}
