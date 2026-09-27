//! rsword 模型 JSON → 布局引擎输入。
//!
//! [`crate::LoadedDocument::paragraphs`] 使用解析器 `Resolver` 的有效属性。
//! [`paras_from_document`] 保留声明值 JSON 的兼容入口及历史样式近似；正式 DOCX 入口应使用前者。
//!
//! 已知不覆盖：表格、绘图、页眉页脚、分节、编号、字段结果的复杂形态。
//!
//! 取 JSON 之前还有一步：同一 `w:r` 里并排多个 `w:rPr` 时，解析器只留**最后一个**，
//! 手机 Word 却用上了第一个。入口应走 [`crate::load_document`]，它先把并排的 `w:rPr`
//! 并成一个再取 JSON；直接 `SessionTable::document()` 拿到的 JSON 会丢属性。

use std::collections::BTreeMap;

use serde_json::Value;

use crate::layout::Color;
use crate::layout::{Align, LineRule, OBJECT_PLACEHOLDER, Para, PlaceholderKind, Run};
use crate::layout::{TabAlign, TabLeader, TabStop};
use crate::layout::Twips;
use crate::font::{Caps, FINE_PER_TWIP, FontHint, FontSlots, FontSpec};

/// 文档默认正文字体与字号（对应 fixture 的 `docDefaults`）。
const BODY_FAMILY: &str = "Times New Roman, SimSun, serif";
const BODY_SIZE_HALF_POINTS: u32 = 24;

/// 与 JSON 同一次解析所得的有效属性；节点号只在该主 part 中有效。
#[derive(Debug, Clone, Default)]
pub(crate) struct EffectiveProperties {
    pub runs: BTreeMap<u32, Value>,
    pub paras: BTreeMap<u32, Value>,
    pub marks: BTreeMap<u32, Value>,
    pub tabs: BTreeMap<u32, Vec<TabStop>>,
    pub recovered_blocks: BTreeMap<u32, Value>,
}

fn node_props<'a>(node: &Value, props: &'a BTreeMap<u32, Value>) -> Option<&'a Value> {
    let id = u32::try_from(node.get("node")?.as_u64()?).ok()?;
    props.get(&id)
}

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

/// `w:vertAlign` 与 `w:position` → (有效字号 0.01pt, 基线抬升 1/7200 英寸)。
///
/// 两者是**不同的机制**，不要合并：
///
/// - `w:vertAlign`（上下标）**同时**缩小字号并挪基线；
/// - `w:position`（半点，可负）**只挪基线，不改字号**——实测 `w:position=8`
///   的那段仍是 12pt，只是抬高了。
///
/// 本层不额外量化抬升量，保留布局层先量化行基线、再扣抬升的次序。
/// `w:position=8` 的名义值 4.00pt 与单份采集的 4.08pt 仍有差别。
fn vertical_run_shape(props: &Value, base_size: u32) -> (u64, i64) {
    let em_fine = i64::from(base_size) * 50;
    match props.get("vertAlign").and_then(Value::as_str) {
        Some("superscript") => (
            scaled_size(base_size),
            em_fine * SUPERSCRIPT_RISE_PERMILLE / 1000,
        ),
        Some("subscript") => (
            scaled_size(base_size),
            -(em_fine * SUBSCRIPT_DROP_PERMILLE / 1000),
        ),
        // 同时指定时保持上下标优先；此组合尚未实测。
        _ => {
            // `w:position` 的单位是**半点**，正值向上。
            let half_points = props.get("position").and_then(Value::as_i64).unwrap_or(0);
            (u64::from(base_size) * 50, half_points.saturating_mul(50))
        }
    }
}

/// 整数半点输入乘以 0.66 后，能用整数 0.01pt 精确表示。
fn scaled_size(base_size: u32) -> u64 {
    u64::from(base_size) * 50 * SUPERSUB_SIZE_NUM / SUPERSUB_SIZE_DEN
}

/// run 自己的 `w:sz`，没有就落回段落基准。
///
/// 单独拎出来，是因为**字号与抬升必须来自同一个数**。它们曾经不是：字号走
/// 这一条、抬升走段落基准，于是 run 上的 `w:sz` 一旦与段落不同，
/// 上下标就缩得对、挪得不对——10pt / 12pt / 18pt 三个字号抬升全是 4.05pt。
fn run_size(props: &Value, base_size: u32) -> u32 {
    props
        .get("size")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(base_size)
}

/// 从一个 run 的 `props` 读出影响度量的字段，叠在段落基准之上。
fn run_font(props: &Value, base_size: u32, base_bold: bool) -> FontSpec {
    let size = run_size(props, base_size);
    // 上下标要缩小字号——这一步必须在这里做，因为字号影响度量，
    // 而抬升不影响（抬升挂在 `Run::rise` 上）。
    let (size_centipoints, _) = vertical_run_shape(props, size);
    let bold = props.get("bold").map(as_bool).unwrap_or(base_bold);
    let italic = props.get("italic").map(as_bool).unwrap_or(false);
    let slots = read_slots(props.get("fonts"));
    // `family` 保留为 ascii 槽的值，供只认单一字体的调用方使用。
    let family = slots
        .ascii
        .clone()
        .or_else(|| slots.h_ansi.clone())
        .unwrap_or_else(|| BODY_FAMILY.to_string());

    // `w:kern` 是**启用字距调整的最小字号**（半点），不是开关。
    // 不写或写 0 即不调整——这是 OOXML 的语义，也与实测的 Word 行为一致。
    let kern_threshold = props.get("kern").and_then(Value::as_u64).unwrap_or(0);

    FontSpec {
        slots,
        family,
        size_half_points: size,
        size_centipoints: None,
        bold,
        italic,
        letter_spacing: run_letter_spacing(props),
        scale_pct: run_scale_pct(props),
        caps: read_caps(props),
        kerning: kern_threshold > 0 && size_centipoints >= kern_threshold.saturating_mul(50),
    }.with_size_centipoints(size_centipoints)
}

/// `w:spacing`（run 级，**不是**段落的 `w:spacing`）：字符间距，twips，可负。
///
/// 解析器的键名是 `spacing`，与段落属性里行距的 `spacing` 同名但不同物——
/// 这里读的是 run 的 `props`，段落那个在 [`read_spacing`]。
///
/// 每个字符（cluster）的推进量加上这个值。依据分三档（`latinspace`：手机 Word 窄路径，
/// Calibri 12pt，`w:val="20"`，24 行起点；见 `tests/letter_spacing.rs`）：
///
/// - **空格也加：实测。**「空格不加」不论行尾空格计不计宽，都在前几行就与 Word 错开；
/// - **行尾最后一个可见字符也加：假定。** 只有在「行尾空格不计宽」时 `latinspace` 才否掉
///   「末字不加」（第 4 行 130 对 124）。rsword 计行尾空格（`fit` 量到空格之后），这个口径下
///   「末字不加」（等价于只加 n−1 个字间）同样 24/24，余量还更宽（2.05pt 对 1.05pt）。
///   `latinspace` 每行都以空格结尾，没有一行能把两者分开。能分开的是行尾没有空格的夹具，
///   例如 `han22` 的文字加 `w:spacing="146"`：逐字加每行 13 个、只加字间每行 14 个（待测）；
/// - **量级 val/20 pt：** Android 只量过 val=20，恰好是整 1pt，零点几点的值取不取整没测。
///   Mac（hbox2 M2，15/30/60 twips）只佐证了 sp/20 这个量级，而 M2 本身判为不成立
///   （0/36：0.000384pt 的残差是真的，另有 −2.77 / −1.01pt 两处离群）。
///
/// 取不到数（缺省或原样保留的非法值）按 0。
fn run_letter_spacing(props: &Value) -> Twips {
    props
        .get("spacing")
        .and_then(Value::as_i64)
        .map(|v| v.clamp(i64::from(Twips::MIN), i64::from(Twips::MAX)) as Twips)
        .unwrap_or(0)
}

/// `w:w`：横向缩放百分比（`ST_TextScale`）。缺省 100。
///
/// 0 不是合法值，度量侧本来就把 0 当 100（不缩放），这里原样带过去，不在桥接层另立规则。
///
/// **比例只是近似**：手机 Word 的 `w:w=80` 有效比例不是 0.80——`latinscale`、`zero-scale`、
/// `m-scale-80` 三者的交集，按 rsword 的口径（行尾空格计宽）约 0.787–0.790，
/// 行尾空格不计宽时约 0.788–0.797；50 / 55 / 90（`zero-scale-*`）则与名义比例相容。
/// 原因未知，这里按名义比例，不拿一档数据凑常数，于是 `zero-scale` 每行 54 个（Word 55）、
/// `latinscale` 第 4 行起点 194（Word 197）。见 word_analyse `reports/rsword-diff/char-scale.md`。
/// Mac（vmisc3 R3：120 / 180 / 66）也只是「与比例相容」，差在 ±0.011pt 内，不是逐位相同。
fn run_scale_pct(props: &Value) -> u32 {
    props
        .get("scale")
        .and_then(Value::as_u64)
        .map(|v| v.min(u64::from(u32::MAX)) as u32)
        .unwrap_or(100)
}

/// `w:caps` / `w:smallCaps` → [`Caps`]。
///
/// 两者都是 `ST_OnOff`：解析器把 `w:val="0"` / `"false"` 读成 `false`，这里照读，
/// 所以直接格式上的 `caps=false` 关得掉。有效文档入口已按解析器的
/// `ToggleRule::WordDesktop` 合成样式层；那条规则是在桌面 Word 上量的，
/// 手机 Word 的 toggle 行为**未测**。裸 JSON 兼容入口仍只读声明值。
///
/// 两个同时为真时取全大写：**假定**。OOXML 说两者互斥、不该同时出现；
/// 解析器的兼容层（`compat_ts/decl.rs`）也是 `caps` 优先。那是解析器的代码，不是 Word 的行为，
/// 手机 Word 未测（两者都设的 120 个 `a` 在 10466 上：全大写 75、小型大写 95，一测就分得开）。
fn read_caps(props: &Value) -> Caps {
    let on = |k: &str| props.get(k).map(as_bool).unwrap_or(false);
    if on("caps") {
        Caps::All
    } else if on("smallCaps") {
        Caps::Small
    } else {
        Caps::None
    }
}

/// 读 `w:rFonts` 的四个槽与 `w:hint`。
///
/// **这是 Word 选字体的真实规则**：同一 run 里每个字符按所属区查对应的槽，
/// 而不是整个 run 用一个字体再靠 fallback 补。JSON 里的键名与 OOXML 一致。
///
/// 有效文档入口已把主题字体解析为实际槽位。裸 JSON 入口不解析主题，
/// 不把 `minorHAnsi` 这类引用当成字体名。
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
/// 这些常数是待替换的历史近似，不是已经验证的 Word 通用规则。
/// 2026-09-22 的独立同行实验覆盖三字体、七字号、42 个上下标条件：
/// 0.66 的绘制字号只匹配 6/42，0.34/0.08 的原点位移只匹配 7/42。
/// 字体 OS/2 未经量化的字号和偏移候选同样失败，不能直接换上。
/// 详见 docs/ENGINE-ITERATION-2026-09-22-ROUND4.md。
///
/// PDF 绘制字号、整 run 推进和内部逻辑字号是不同的量。
/// 替代算法需要独立检验其量化顺序及推进模型；当前保留近似行为，
/// 用 0.01pt 精确保留其结果，避免把表示误差混入模型误差。
const SUPERSUB_SIZE_NUM: u64 = 66;
const SUPERSUB_SIZE_DEN: u64 = 100;
/// 上标抬升，em 的千分比。
const SUPERSCRIPT_RISE_PERMILLE: i64 = 340;
/// 下标下沉，em 的千分比。
const SUBSCRIPT_DROP_PERMILLE: i64 = 80;

/// 一个段的种类若落成占位符，是哪一种。
fn segment_placeholder(kind: &Value) -> PlaceholderKind {
    match kind.get("kind").and_then(Value::as_str) {
        Some("br") => match kind.get("breakKind").and_then(Value::as_str) {
            Some("page") => PlaceholderKind::PageBreak,
            Some("column") => PlaceholderKind::ColumnBreak,
            // 缺省即 textWrapping（软回车）。
            _ => PlaceholderKind::LineBreak,
        },
        Some("cr") => PlaceholderKind::LineBreak,
        // 行内对象，以及其余落成 U+FFFC 的段（脚注引用、ruby、未知元素、`w:t` 里字面的
        // U+FFFC）：占位但不断开。
        _ => PlaceholderKind::Object,
    }
}

/// 软回车段：`w:br`（缺省或 `textWrapping`）与 `w:cr`。
fn is_soft_break(kind: &Value) -> bool {
    match kind.get("kind").and_then(Value::as_str) {
        Some("br") => !matches!(
            kind.get("breakKind").and_then(Value::as_str),
            Some("page") | Some("column")
        ),
        Some("cr") => true,
        _ => false,
    }
}

/// 一个 run 的排版文本，与其中每个 U+FFFC 的 [`PlaceholderKind`]（按文档顺序）。
///
/// 占位符本身**不区分种类**——分页符、行内图在 run 文本里都是同一个 U+FFFC。
/// 种类只在 `segments[].kind` 里，所以必须在这里读出来带给排版层，
/// 否则排版分不出「这里要翻页」和「这里有张图」。
///
/// **软回车在解析器 JSON 里不是 U+FFFC，是 `'\n'`**（rsWordParser 399e36a `segment()`：
/// `textWrapping` 的 `w:br` 与 `w:cr` 推 `'\n'`，分页、分栏才推 U+FFFC）。这里把这种段的
/// `'\n'` 换成 U+FFFC 并记成 [`PlaceholderKind::LineBreak`]，排版层才会在此处收行。
/// 两者都是 1 个 UTF-16 码元，源偏移不变。实测：Android 窄路径 `br-soft`（`textWrapping`）、
/// `br-bare`（不写 `w:type`）、`br-cr`（`w:cr`）的起点都是 0、5，软回车码元收在上一行
/// （word_analyse `reports/rsword-diff/br-soft.md`）。
///
/// **按段的种类换，不按字符换**：`xml:space="preserve"` 的 `w:t` 里字面的 LF 也是 `'\n'`，
/// Word 不在那里断——word_analyse `breakme.docx` 在 UTF-16 576 处有一个，Android 窄路径
/// 的那一行是 561–593，没有在 576 断开。
///
/// 占位符按**段的字节范围**配种类：每个 U+FFFC 取包含它的那个段的种类。`segments[].text`
/// 是段在 run 文本里的字节区间。以前按「非文本段的个数」对位，而 `w:tab`（`'\t'`）、
/// `w:lastRenderedPageBreak`、`w:fldChar` 这些不落成 U+FFFC 的段也算了进去，个数一对不上
/// 就整体退回 `Object`——软回车改成 U+FFFC 之后，同一 run 里只要还有一个 `w:tab`，
/// 软回车就又丢了。
///
/// `soft_breaks` 为假时软回车段的 `'\n'` 原样留作文字、不断行：给 `w:vanish` 的 run 用。
/// `Run::hidden` 保留其源长度，但布局不为整个 run 生成片段或执行其中的控制字符。
/// 只看 run 自己声明的 `vanish`，样式链上继承来的看不到（与粗体、斜体同一个缺口）。
/// 修订删除的 run（`w:del` / `w:moveFrom`）照常转换：删除文字本来就照样排，与删除的分页符
/// 一向照常翻页同一口径；Word 在哪种标记视图下隐藏删除内容，没测。
///
/// 段不带字节区间时（手写的 JSON、别的生产者）退回按种类次序对位，见
/// [`placeholders_by_order`]；这时软回车无从定位，不改文本。区间越界／不在字符边界／重叠时，
/// 文本不改，每个 U+FFFC 都按 `Object` 算：保守方向——宁可少一次分页，也不凭空造出一次。
/// 凭空多出来的页在比较器里只会报结构失败，查起来更费事。
fn run_text_and_placeholders(
    run: &Value,
    text: &str,
    soft_breaks: bool,
) -> (String, Vec<PlaceholderKind>) {
    let conservative = || {
        (text.to_string(), vec![PlaceholderKind::Object; text.matches(OBJECT_PLACEHOLDER).count()])
    };
    let Some(Value::Array(segments)) = run.get("segments") else {
        return conservative();
    };
    if segments.iter().any(|s| s.get("kind").is_some() && s.get("text").is_none()) {
        return (text.to_string(), placeholders_by_order(segments, text));
    }

    let mut ranges: Vec<(usize, usize, &Value)> = Vec::with_capacity(segments.len());
    for segment in segments {
        let (Some(kind), Some(Value::Array(span))) = (segment.get("kind"), segment.get("text"))
        else {
            continue;
        };
        let (Some(start), Some(end)) = (
            span.first().and_then(Value::as_u64).map(|v| v as usize),
            span.get(1).and_then(Value::as_u64).map(|v| v as usize),
        ) else {
            return conservative();
        };
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return conservative();
        }
        ranges.push((start, end, kind));
    }
    ranges.sort_by_key(|&(start, end, _)| (start, end));
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return conservative();
    }

    /// 原样抄一截，其中每个 U+FFFC 记成 `kind`。
    fn copy(slice: &str, kind: PlaceholderKind, out: &mut String, kinds: &mut Vec<PlaceholderKind>) {
        out.push_str(slice);
        kinds.extend(std::iter::repeat_n(kind, slice.matches(OBJECT_PLACEHOLDER).count()));
    }

    let mut out = String::with_capacity(text.len() + 2 * ranges.len());
    let mut kinds = Vec::new();
    let mut cursor = 0;
    for (start, end, kind) in ranges {
        // 段外的 U+FFFC 没有种类可查，按 `Object`。
        copy(&text[cursor..start], PlaceholderKind::Object, &mut out, &mut kinds);
        let slice = &text[start..end];
        if soft_breaks && is_soft_break(kind) && slice == "\n" {
            out.push(OBJECT_PLACEHOLDER);
            kinds.push(PlaceholderKind::LineBreak);
        } else {
            copy(slice, segment_placeholder(kind), &mut out, &mut kinds);
        }
        cursor = end;
    }
    copy(&text[cursor..], PlaceholderKind::Object, &mut out, &mut kinds);
    (out, kinds)
}

/// 没有字节区间时的旧对位法：非文本段按次序一一对上 U+FFFC。
///
/// **个数对不上就整体退回 `Object`**（理由同上）。不落成 U+FFFC 的段（`w:tab` 等）也会算进
/// 个数，所以这条路只留给不带区间的 JSON；解析器的输出都带区间，走字节对位。
fn placeholders_by_order(segments: &[Value], text: &str) -> Vec<PlaceholderKind> {
    let want = text.matches(OBJECT_PLACEHOLDER).count();
    if want == 0 {
        return Vec::new();
    }
    let out: Vec<PlaceholderKind> = segments
        .iter()
        .filter_map(|s| s.get("kind"))
        .filter(|k| k.get("kind").and_then(Value::as_str) != Some("text"))
        .map(segment_placeholder)
        .collect();
    if out.len() == want { out } else { vec![PlaceholderKind::Object; want] }
}

/// 递归收集一个块里的所有 run 文本。
fn collect_runs(
    inlines: &Value,
    base_size: u32,
    base_bold: bool,
    effective: Option<&EffectiveProperties>,
    out: &mut Vec<Run>,
) {
    match inlines {
        Value::Array(items) => {
            for it in items {
                collect_runs(it, base_size, base_bold, effective, out);
            }
        }
        Value::Object(_) => {
            let kind = inlines.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind == "run" {
                if let Some(t) = inlines.get("text").and_then(Value::as_str)
                    && !t.is_empty()
                {
                    let props = effective
                        .and_then(|e| node_props(inlines, &e.runs))
                        .or_else(|| inlines.get("props"))
                        .unwrap_or(&Value::Null);
                    // 与 `run_font` 取同一个字号——见 `run_size` 的说明。
                    let rise_fine = vertical_run_shape(props, run_size(props, base_size)).1;
                    let hidden = props.get("vanish").map(as_bool).unwrap_or(false);
                    let (text, placeholders) = run_text_and_placeholders(inlines, t, !hidden);
                    out.push(Run {
                        text,
                        hidden,
                        font: run_font(props, base_size, base_bold),
                        color: Color::BLACK,
                        placeholders,
                        rise: (rise_fine / FINE_PER_TWIP) as Twips,
                        rise_fine: Some(rise_fine),
                    });
                }
            } else if kind == "field" {
                if let Some(r) = inlines.get("result") {
                    collect_runs(r, base_size, base_bold, effective, out);
                }
            } else if let Some(inner) = inlines.get("inlines") {
                collect_runs(inner, base_size, base_bold, effective, out);
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

/// `w:defaultTabStop`（`settings.defaultTabStop`），twips。没写、或不是正数时给 `None`。
///
/// **缺省值不在这里定**：没写时 Word 用多少因平台而异（桌面照规范 720，Android 的夹具
/// 量出来窄得多），而桥接层不知道在模拟哪个平台——由排版引擎按 [`crate::Platform`] 补，
/// 见 `layout.rs` 的 `ANDROID_MISSING_DEFAULT_TAB_STOP`。
///
/// 写了就照用，两个平台都一样。**Android 认不认这一项未实测**：实测夹具都没有 settings.xml。
/// 写成 0 或负数按没写处理（规范没说，**假设**）；有 settings.xml 却没写这一项，
/// 与没有 settings.xml 同一支——实测只覆盖了后者。
fn default_tab_stop(doc: &Value) -> Option<Twips> {
    doc.get("settings")
        .and_then(|s| s.get("defaultTabStop"))
        .and_then(Value::as_i64)
        .filter(|&v| v > 0)
        .map(|v| v as Twips)
}

/// 一组 `w:tabs/w:tab` 叠到已有的制表位上。
///
/// 同位置的后来者覆盖先来者；`clear` 删掉继承来的同位置制表位（它本身不是制表位）。
fn apply_tabs(stops: &mut Vec<TabStop>, tabs: Option<&Value>) {
    let Some(Value::Array(list)) = tabs.and_then(|t| t.get("tab")) else {
        return;
    };
    for tab in list {
        let Some(pos) = tab.get("pos").and_then(Value::as_i64) else { continue };
        let pos = pos as Twips;
        stops.retain(|s| s.pos != pos);
        let align = match tab.get("val").and_then(Value::as_str) {
            Some("clear") => continue,
            Some("center") => TabAlign::Center,
            Some("right") | Some("end") => TabAlign::Right,
            Some("decimal") => TabAlign::Decimal,
            Some("bar") => TabAlign::Bar,
            // left、start、num（旧式列表制表位）与未知值都按左对齐。
            _ => TabAlign::Left,
        };
        let leader = match tab.get("leader").and_then(Value::as_str) {
            Some("dot") => TabLeader::Dot,
            Some("hyphen") => TabLeader::Hyphen,
            Some("underscore") => TabLeader::Underscore,
            Some("heavy") => TabLeader::Heavy,
            Some("middleDot") => TabLeader::MiddleDot,
            _ => TabLeader::None,
        };
        stops.push(TabStop { pos, align, leader });
    }
}

/// 解析器的 Tabs 合并覆盖整个数组，布局按位置合并并执行 clear。
pub(crate) fn merge_layout_tabs(layers: impl IntoIterator<Item = Value>) -> Vec<TabStop> {
    let mut stops = Vec::new();
    for tabs in layers {
        apply_tabs(&mut stops, Some(&tabs));
    }
    stops.sort_by_key(|s| s.pos);
    stops
}

/// 段落的有效制表位：段落样式链（从根到叶）再叠直接格式。
///
/// 这是 [`style_defaults`] 之外**唯一**读样式表的地方：制表位常写在样式里
/// （目录、页眉页脚样式），只看直接格式会丢。没写 `styleId` 时用默认段落样式。
///
/// 已知没合并的来源（都未实现，也都未测）：
/// - `w:pPrDefault` 里的制表位（文档默认段落属性）；
/// - 编号级别的制表位（`w:lvl/w:pPr/w:tabs`），列表项的编号与正文之间靠它；
/// - `styleId` 指向一个不存在的样式时不退回默认段落样式，整条样式链为空。
///
/// `w:ptab`（绝对位置制表符）在 run 文本里与普通制表符同是 `'\t'`，解析器的 `pTab`
/// 段带着它的对齐方式与参照（页边距 / 缩进），这里**没读**：它按普通制表符去找制表位，
/// 右对齐的 ptab 于是可能停到一个居中制表位上，而不是右页边距。
fn read_tabs(
    doc: &Value,
    style_id: Option<&str>,
    props: &Value,
) -> Vec<TabStop> {
    let styles: &[Value] = doc
        .get("styles")
        .and_then(|s| s.get("styles"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice);
    let paragraph = |s: &&Value| s.get("kind").and_then(Value::as_str) == Some("paragraph");
    let mut chain: Vec<&Value> = Vec::new();
    let mut next = match style_id {
        Some(id) => styles
            .iter()
            .filter(paragraph)
            .find(|s| s.get("styleId").and_then(Value::as_str) == Some(id)),
        None => styles
            .iter()
            .filter(paragraph)
            .find(|s| s.get("isDefault").and_then(Value::as_bool) == Some(true)),
    };
    // 深度上限防 basedOn 成环。
    while let Some(style) = next
        && chain.len() < 16
    {
        chain.push(style);
        next = style.get("basedOn").and_then(Value::as_str).and_then(|base| {
            styles
                .iter()
                .filter(paragraph)
                .find(|s| s.get("styleId").and_then(Value::as_str) == Some(base))
        });
    }
    let mut stops = Vec::new();
    for style in chain.iter().rev() {
        apply_tabs(&mut stops, style.get("ppr").and_then(|p| p.get("tabs")));
    }
    apply_tabs(&mut stops, props.get("tabs"));
    stops.sort_by_key(|s| s.pos);
    stops
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

/// 按计数约定定本段的终止符。
///
/// 优先级与各自画几个字形的依据见量具方法 §4：
/// 段落标记画 1 个空格、软回车画 1 个、分节符画 0 个、
/// 手动分页符按在行里的位置画 0 或 1 个。
///
/// **分栏符（`column`）未测**，按软回车同级处理并留待实测。
fn pick_terminator(has_sect_pr: bool) -> crate::oracle::LineTerminator {
    // Inline breaks terminate the line where they occur, not the paragraph.
    if has_sect_pr {
        crate::oracle::LineTerminator::SectionBreak
    } else {
        crate::oracle::LineTerminator::ParagraphMark
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
    project_paragraphs(doc, None)
}

pub(crate) fn project_paragraphs(
    doc: &Value,
    effective: Option<&EffectiveProperties>,
) -> (Vec<Para>, usize) {
    let mut paras = Vec::new();
    let mut skipped = 0usize;

    let main = match doc.get("main") {
        Some(Value::Array(items)) => items,
        _ => return (paras, skipped),
    };

    // 分节起点按**块下标**给出，而非文本块会被跳过，所以要按原下标查，
    // 不能用段落序号。
    let section_starts = section_page_starts(doc);
    let default_tab_stop = default_tab_stop(doc);

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
        let (size, bold, before, after, keep_next) = if effective.is_some() {
            (BODY_SIZE_HALF_POINTS, false, 0, 0, false)
        } else {
            style_defaults(style_id, level)
        };
        let has_sect_pr = block
            .get("facts")
            .and_then(|f| f.get("hasSectPr"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let mut runs = Vec::new();
        if let Some(inlines) = block.get("inlines") {
            collect_runs(inlines, size, bold, effective, &mut runs);
        }

        // 空段落也要占一行高度。
        if runs.is_empty() {
            let mark_props = effective
                .and_then(|e| node_props(block, &e.marks))
                .unwrap_or(&Value::Null);
            let rise_fine = vertical_run_shape(mark_props, run_size(mark_props, size)).1;
            runs.push(Run {
                text: String::new(),
                hidden: false,
                font: if effective.is_some() { run_font(mark_props, size, bold) } else { FontSpec::new(BODY_FAMILY, size) },
                color: Color::BLACK,
                placeholders: Vec::new(),
                rise: (rise_fine / FINE_PER_TWIP) as Twips,
                rise_fine: effective.map(|_| rise_fine),
            });
        }

        let direct_props = block.get("props").unwrap_or(&Value::Null);
        let props = effective
            .and_then(|e| node_props(block, &e.paras))
            .unwrap_or(direct_props);
        let (indent_left, indent_right, indent_first_line) = read_indent(props);
        let (line_rule, line_value, space_before, space_after) =
            read_spacing(props, before, after);

        // 必须在 runs 被 move 进 Para 之前算好。
        let terminator = pick_terminator(has_sect_pr);

        paras.push(Para {
            runs,
            align: read_align(props),
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
            overflow_punct: props.get("overflowPunct").map(as_bool).unwrap_or(true),
            tabs: effective
                .and_then(|e| block.get("node").and_then(Value::as_u64)
                    .and_then(|node| u32::try_from(node).ok())
                    .and_then(|node| e.tabs.get(&node)))
                .cloned()
                .unwrap_or_else(|| read_tabs(doc, style_id, direct_props)),
            default_tab_stop,
            source_node: block.get("node").and_then(Value::as_u64).map(|n| n as u32),
            terminator,
        });
    }

    (paras, skipped)
}
