//! 引擎输出契约：喂给验收量具的「页 → 行 → 字形」。
//!
//! 对应量具方法 §9.7：
//!
//! > 引擎输出要给每页、每行、每字形的**原点与推进**、**所属源字符区间**、**行终止符类型**。
//! > 这是比较器的输入契约，越早定越省。
//!
//! 为什么不复用 [`crate::fragment::LaidOutDocument`]：那份产物是给后端画图用的，
//! [`crate::fragment::Page::push_line`] 会把行摊平成片段，**行这一层没了**；
//! 而比较器要做页 → 行 → 字形三层配对，任何一层数目对不上即结构失败（§9.6）。
//! 所以行必须留着。
//!
//! ## 两个口径，必须和 Word 侧对齐
//!
//! **坐标**：单位点（twips ÷ 20），页内、**页顶向下**、原点在页左上角。
//! Word 侧的 `glyphOrigin` 已翻成同一口径（相对 cropBox 左上角），可以直接相减。
//!
//! **偏移空间**：`source_start` / `source_end` 落在**与 Word `Range` 相同的字符偏移空间**里——
//! 各段文本依次拼接，**每段末尾算一个段落标记**。Word 的 `Range.Start/End` 就是这么数的，
//! 对不齐的话行区间就没法比。
//!
//! ## 一个已声明的近似
//!
//! 字形原点按**前缀推进量**算：第 i 个字符的原点 = 片段起点 + `measure(text[..i]).advance`。
//! 这对「前缀可加」的度量是准确的（[`crate::simple_metrics::SimpleMetrics`] 属于此类）。
//! 接入真正的 shaper 后**它不再准确**：连字与 kerning 会让前缀和不等于逐字形推进，
//! 那时应改由 shaper 直接给出每个字形的位置（`ShapedRun::x_offset` / `x_advance`）。
//! 这条近似记在输出的 `glyphOriginMethod` 字段里，让下游没法只抄数不抄限定。

use crate::measure::{FontMetrics, FontSpec};

/// 行终止符类型（§9.7 明确要求）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    /// 段落末行：源侧有一个段落标记。Word 为它画 **1 个空格**（量具方法 §4）。
    ParagraphMark,
    /// 自动换行：源侧**没有**对应字符，Word 也不为它画字形。
    Wrap,
}

impl Terminator {
    pub fn as_str(self) -> &'static str {
        match self {
            Terminator::ParagraphMark => "PARAGRAPH_MARK",
            Terminator::Wrap => "WRAP",
        }
    }
}

/// 一个已定位的字形。
#[derive(Debug, Clone)]
pub struct GlyphTrace {
    /// 原点 x，点。
    pub x: f64,
    /// 原点 y（基线），点，页顶向下。
    pub y: f64,
    /// 推进向量，点。横排时 `dy` 为 0。
    pub dx: f64,
    pub dy: f64,
    pub text: String,
    /// 在 Word 偏移空间里的字符下标。
    pub source_char: usize,
}

#[derive(Debug, Clone)]
pub struct LineTrace {
    pub index: usize,
    pub top: f64,
    pub height: f64,
    /// 基线相对行顶的偏移，点。
    pub baseline: f64,
    pub source_start: usize,
    pub source_end: usize,
    pub terminator: Terminator,
    pub glyphs: Vec<GlyphTrace>,
}

#[derive(Debug, Clone)]
pub struct PageTrace {
    pub index: usize,
    pub width: f64,
    pub height: f64,
    pub lines: Vec<LineTrace>,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentTrace {
    pub pages: Vec<PageTrace>,
}

/// twips → 点。
pub fn pt(twips: crate::geom::Twips) -> f64 {
    f64::from(twips) / 20.0
}

impl DocumentTrace {
    pub fn glyph_count(&self) -> usize {
        self.pages
            .iter()
            .flat_map(|p| p.lines.iter())
            .map(|l| l.glyphs.len())
            .sum()
    }

    /// 序列化成量具吃的 JSON（schema `rsword-layout-trace/1`）。
    ///
    /// 手写而不引 serde：这个契约只有一个消费者（`tools/measure`），
    /// 为它给整个 core 加一条 derive 依赖不划算。
    pub fn to_json(&self, meta: &TraceMeta) -> String {
        let mut out = String::new();
        out.push_str("{\n  \"schema\": \"rsword-layout-trace/1\",\n");
        out.push_str("  \"unit\": \"pt\",\n");
        out.push_str(
            "  \"originFrame\": \"page-top-down, origin at page top-left; y is the baseline\",\n",
        );
        out.push_str(
            "  \"offsetSpace\": \"Word Range offsets: paragraphs concatenated, \
             one paragraph mark counted per paragraph\",\n",
        );
        out.push_str(
            "  \"glyphOriginMethod\": \"prefix advance: origin(i) = piece_x + measure(text[..i]).advance; \
             exact for prefix-additive metrics, an approximation once a shaper does ligatures/kerning\",\n",
        );
        push_str_field(&mut out, "engine", &meta.engine, 2);
        push_str_field(&mut out, "metrics", &meta.metrics, 2);
        push_str_field(&mut out, "source", &meta.source, 2);
        push_str_field(&mut out, "sourceSha256", &meta.source_sha256, 2);
        out.push_str("  \"pages\": [\n");
        for (pi, page) in self.pages.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"index\": {},\n", page.index));
            out.push_str(&format!("      \"width\": {},\n", num(page.width)));
            out.push_str(&format!("      \"height\": {},\n", num(page.height)));
            out.push_str("      \"lines\": [\n");
            for (li, line) in page.lines.iter().enumerate() {
                out.push_str("        {\n");
                out.push_str(&format!("          \"index\": {},\n", line.index));
                out.push_str(&format!("          \"top\": {},\n", num(line.top)));
                out.push_str(&format!("          \"height\": {},\n", num(line.height)));
                out.push_str(&format!("          \"baseline\": {},\n", num(line.baseline)));
                out.push_str(&format!("          \"sourceStart\": {},\n", line.source_start));
                out.push_str(&format!("          \"sourceEnd\": {},\n", line.source_end));
                out.push_str(&format!(
                    "          \"terminator\": \"{}\",\n",
                    line.terminator.as_str()
                ));
                out.push_str("          \"glyphs\": [\n");
                for (gi, glyph) in line.glyphs.iter().enumerate() {
                    out.push_str(&format!(
                        "            {{\"origin\": [{}, {}], \"advance\": [{}, {}], \
                         \"text\": {}, \"sourceChar\": {}}}",
                        num(glyph.x),
                        num(glyph.y),
                        num(glyph.dx),
                        num(glyph.dy),
                        json_string(&glyph.text),
                        glyph.source_char
                    ));
                    out.push_str(if gi + 1 == line.glyphs.len() { "\n" } else { ",\n" });
                }
                out.push_str("          ]\n");
                out.push_str(if li + 1 == page.lines.len() { "        }\n" } else { "        },\n" });
            }
            out.push_str("      ]\n");
            out.push_str(if pi + 1 == self.pages.len() { "    }\n" } else { "    },\n" });
        }
        out.push_str("  ]\n}\n");
        out
    }
}

/// 输出里的溯源信息。量具会把它连同环境指纹一起记进比较记录。
#[derive(Debug, Clone, Default)]
pub struct TraceMeta {
    pub engine: String,
    pub metrics: String,
    pub source: String,
    pub source_sha256: String,
}

fn num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v:.6}")
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn push_str_field(out: &mut String, key: &str, value: &str, indent: usize) {
    out.push_str(&" ".repeat(indent));
    out.push_str(&format!("\"{}\": {},\n", key, json_string(value)));
}

/// 把一段文字按**前缀推进量**展开成逐字形的原点。
///
/// `start_x` 是片段左端，`baseline_y` 是基线；`source_base` 是该片段首字符在
/// Word 偏移空间里的下标。返回的推进量是相邻原点之差，最后一个字符用总宽补齐。
pub(crate) fn expand_glyphs<M: FontMetrics>(
    metrics: &M,
    text: &str,
    font: &FontSpec,
    start_x: f64,
    baseline_y: f64,
    source_base: usize,
) -> Vec<GlyphTrace> {
    let mut out = Vec::new();
    let mut offsets: Vec<(usize, char, f64)> = Vec::new();
    for (byte_i, c) in text.char_indices() {
        let advance = pt(metrics.measure(&text[..byte_i], font).advance);
        offsets.push((byte_i, c, start_x + advance));
    }
    let total = start_x + pt(metrics.measure(text, font).advance);
    for (i, &(_, c, x)) in offsets.iter().enumerate() {
        let next = offsets.get(i + 1).map(|&(_, _, nx)| nx).unwrap_or(total);
        out.push(GlyphTrace {
            x,
            y: baseline_y,
            dx: next - x,
            dy: 0.0,
            text: c.to_string(),
            source_char: source_base + i,
        });
    }
    out
}
