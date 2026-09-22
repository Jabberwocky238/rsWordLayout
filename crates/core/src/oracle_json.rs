//! 把 [`crate::oracle::LayoutRecord`] 写成量具吃的 JSON。
//!
//! [`crate::oracle`] 定义了引擎该吐什么；本模块只负责**把它搬出进程**，
//! 不改那边的任何判断。分开写是有意的：契约的形状归 `oracle`，
//! 序列化格式归这里，改一边不必动另一边。
//!
//! 产出 schema `rsword-layout-trace/1`，消费者是 `tools/measure` 的比较器
//! （`wm compare <采集包> <trace.json>`）。
//!
//! # 两个口径，必须和 Word 侧对齐，对不齐就没法比
//!
//! **坐标**：单位**点**（`oracle` 里是 twips，这里除以 20）；页内、**页顶向下**、
//! 原点在页左上角。Word 侧的 `glyphOrigin` 已翻成同一口径（相对 cropBox 左上角），
//! 两边可以直接相减。
//!
//! **偏移空间**：`sourceStart` / `sourceEnd` 用 **UTF-16 单位**，与 rsword 的坐标流
//! 以及 Word 的 `Range.Start/End` 一致。段落标记各占 1 个单位。
//!
//! # 手写而不引 serde
//!
//! 这个契约只有一个消费者，为它给整个 core 加一条 derive 依赖不划算。
//! 输出是 ASCII-safe 的紧凑 JSON，字段顺序固定，便于逐字节 diff。

use crate::layout::Twips;
use crate::oracle::{LayoutRecord, LineTerminator, PageBreakPosition};

/// 轨迹里的溯源信息。量具会把它连同环境指纹一起记进比较记录。
///
/// `metrics` 与 `glyph_origin_method` **必须如实填**：差值的来源常常就在这两栏里。
/// 拿桩度量跑出来的数与拿真字体跑出来的数不是一回事，混着读会把「桩没读字体」
/// 误判成「布局算错了」。
#[derive(Debug, Clone, Default)]
pub struct TraceMeta {
    pub engine: String,
    /// 度量实现的**性质**，不只是名字。
    pub metrics: String,
    /// 字形原点是怎么定出来的（整形器直接给 / 前缀推进量近似）。
    pub glyph_origin_method: String,
    pub source: String,
    pub font_fingerprint: Option<String>,
}

/// twips → 点。
fn pt(twips: Twips) -> f64 {
    f64::from(twips) / 20.0
}

/// 1/7200 英寸 → 点。
///
/// 纵坐标走这一条而不是 [`pt`]：Word 的基线在 0.24pt = 4.8 twips 的栅格上，
/// 先落到整 twips 再换算，残差会沿页累加，与 Word 就逐位对不上了。
fn pt_fine(fine: i64) -> f64 {
    fine as f64 / (20.0 * crate::font::FINE_PER_TWIP as f64)
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
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                // 非 ASCII 一律转义：轨迹要能逐字节 diff，不受终端编码影响。
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 终止符的字符串名。与量具 §4 约定表的键对齐。
fn terminator_name(t: LineTerminator) -> &'static str {
    match t {
        LineTerminator::ParagraphMark => "PARAGRAPH_MARK",
        LineTerminator::LineBreak => "SOFT_RETURN",
        LineTerminator::PageBreak(PageBreakPosition::OwnLine) => "PAGE_BREAK_OWN_LINE",
        LineTerminator::PageBreak(PageBreakPosition::BeforeMark) => "PAGE_BREAK_BEFORE_MARK",
        LineTerminator::PageBreak(PageBreakPosition::MidParagraph) => "PAGE_BREAK_MID_PARAGRAPH",
        LineTerminator::SectionBreak => "SECTION_BREAK",
        LineTerminator::Wrapped => "WRAP",
    }
}

/// 把记录写成 `rsword-layout-trace/1`。
pub fn to_trace_json(record: &LayoutRecord, meta: &TraceMeta) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema\": \"rsword-layout-trace/1\",\n");
    out.push_str("  \"unit\": \"pt\",\n");
    out.push_str(
        "  \"originFrame\": \"page-top-down, origin at page top-left; y is the baseline\",\n",
    );
    out.push_str(
        "  \"offsetSpace\": \"UTF-16 code units, same stream as rsword and Word Range.Start/End; \
         one unit per paragraph mark\",\n",
    );
    out.push_str(&format!("  \"engine\": {},\n", json_string(&meta.engine)));
    out.push_str(&format!("  \"metrics\": {},\n", json_string(&meta.metrics)));
    out.push_str(&format!(
        "  \"glyphOriginMethod\": {},\n",
        json_string(&meta.glyph_origin_method)
    ));
    out.push_str(&format!("  \"source\": {},\n", json_string(&meta.source)));
    match &meta.font_fingerprint {
        Some(f) => out.push_str(&format!("  \"fontFingerprint\": {},\n", json_string(f))),
        None => out.push_str("  \"fontFingerprint\": null,\n"),
    }
    // 未归行的字形数原样带出：8.5% 的字形不进行划分是实测现象，
    // 量具按验收定义把它们排除，而不是给归属——这个分母不能丢。
    out.push_str(&format!(
        "  \"unassignedGlyphs\": {},\n",
        record.unassigned_glyphs
    ));

    out.push_str("  \"pages\": [\n");
    for (pi, page) in record.pages.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"index\": {},\n", page.index));
        out.push_str(&format!("      \"width\": {},\n", num(pt(page.width))));
        out.push_str(&format!("      \"height\": {},\n", num(pt(page.height))));
        out.push_str("      \"lines\": [\n");
        for (li, line) in page.lines.iter().enumerate() {
            out.push_str("        {\n");
            out.push_str(&format!("          \"index\": {li},\n"));
            // 行盒**仅供诊断，不参与验收**——行盒与行基线在现有通道上不可测。
            out.push_str(&format!(
                "          \"boxTopDiagnostic\": {},\n",
                num(pt(line.box_top))
            ));
            out.push_str(&format!(
                "          \"boxHeightDiagnostic\": {},\n",
                num(pt(line.box_height))
            ));
            match line.source {
                Some(s) => {
                    out.push_str(&format!("          \"sourceStart\": {},\n", s.start));
                    out.push_str(&format!("          \"sourceEnd\": {},\n", s.end));
                }
                // 缺失保持 null，让比较器报「判不了」，而不是把缺失读成 0。
                None => {
                    out.push_str("          \"sourceStart\": null,\n");
                    out.push_str("          \"sourceEnd\": null,\n");
                }
            }
            out.push_str(&format!(
                "          \"terminator\": \"{}\",\n",
                terminator_name(line.terminator)
            ));
            out.push_str(&format!(
                "          \"terminatorExpectedGlyphs\": {},\n",
                line.terminator.expected_glyphs()
            ));
            out.push_str("          \"glyphs\": [");
            for (gi, glyph) in line.glyphs.iter().enumerate() {
                out.push_str(if gi == 0 { "\n" } else { ",\n" });
                let source = match glyph.source {
                    Some(s) => format!("{}", s.start),
                    None => "null".to_string(),
                };
                out.push_str(&format!(
                    "            {{\"origin\": [{}, {}], \"advance\": [{}, {}], \
                     \"glyphId\": {}, \"face\": {}, \"sizeHalfPoints\": {}, \
                     \"sizeCentipoints\": {}, \"sourceChar\": {}}}",
                    num(glyph.origin_x_pt),
                    num(pt_fine(glyph.origin_y_fine)),
                    num(glyph.advance_x_pt),
                    num(pt(glyph.advance_y)),
                    glyph.glyph_id,
                    json_string(&glyph.face),
                    glyph.size_half_points,
                    glyph.size_centipoints,
                    source
                ));
            }
            out.push_str(if line.glyphs.is_empty() { "]\n" } else { "\n          ]\n" });
            out.push_str(if li + 1 == page.lines.len() { "        }\n" } else { "        },\n" });
        }
        out.push_str("      ]\n");
        out.push_str(if pi + 1 == record.pages.len() { "    }\n" } else { "    },\n" });
    }
    out.push_str("  ]\n}\n");
    out
}
