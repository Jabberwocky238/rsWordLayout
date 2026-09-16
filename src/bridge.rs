//! rsword 模型 JSON → 布局引擎输入。
//!
//! **当前是最小桥接，有意留了缺口**：`document()` 给的 `props` 是*声明值*，样式链的有效属性
//! 要走 `rsword::resolve`（Rust 内部 API，JSON 投影里没有）。这里按 `styleId` 做最小映射，
//! 只够把链路跑通；接 `Resolver` 后应当替换掉 [`style_defaults`]。
//!
//! 已知不覆盖：表格、绘图、页眉页脚、分节、编号、字段结果的复杂形态。

use serde_json::Value;

use crate::canvas::Color;
use crate::engine::{Align, LineRule, Para, Run};
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
                if let Some(t) = inlines.get("text").and_then(Value::as_str) {
                    if !t.is_empty() {
                        let props = inlines.get("props").cloned().unwrap_or(Value::Null);
                        out.push(Run {
                            text: t.to_string(),
                            font: run_font(&props, base_size, base_bold),
                            color: Color::BLACK,
                        });
                    }
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

        paras.push(Para {
            runs,
            align: Align::Left,
            space_before: before,
            space_after: after,
            line_rule: LineRule::Auto,
            line_value: 240,
            keep_next,
            source_node: block.get("node").and_then(Value::as_u64).map(|n| n as u32),
            ..Para::default()
        });
    }

    (paras, skipped)
}
