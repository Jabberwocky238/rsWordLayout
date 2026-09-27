//! 布局会话：把「docx 字节 → 布局结果」封装成可反复取页的对象。
//!
//! 本 crate 是**纯 Rust 逻辑层，不带 `#[wasm_bindgen]` 属性**。
//! 面向 JS 的那一层在 `rsword-layout-webgl`：两个 wasm 模块各有独立线性内存，
//! JS 无法跨模块传对象，所以会话与渲染器必须由同一个 crate 导出给 JS，
//! 否则 wasm-bindgen 会报同名导出冲突。
//!
//! 本 crate **不做渲染**。它只负责解析、排版，并把布局产物按页交出去；
//! 怎么画由 `rsword-layout-webgl` 一类的后端决定。这样分开的理由是布局与渲染的
//! 生命周期不同——同一份布局可以画很多次（翻页、缩放、重绘），不该每次都重排。
//!
//! 布局整个跑在 wasm 里（而不是原生侧算好喂二进制），代价是产物带上了整个 rsword
//! 解析器，换来的是浏览器端自足：拿到 docx 就能渲染，不需要服务端配合。
//!
//! 构建见仓库 `scripts/prepare-webgl.sh`。

use rsword_layout_core::load_document;
use rsword_layout_core::{Engine, PageSetup};
use rsword_layout_core::Page;
use rsword_layout_core::{PaintList, PaintPage, TextShaper, paint_document, paint_page};
use rsword_layout_core::SimpleMetrics;

/// 一次会话：持有排好版的文档，可反复取页。
pub struct LayoutSession {
    doc: Vec<Page>,
    dpi: f32,
}

impl LayoutSession {
    /// 页数。
    pub fn page_count(&self) -> usize {
        self.doc.len()
    }

    /// 某页的宽高，**单位 twips**，返回 `[w, h]`。越界返回空数组。
    ///
    /// 不返回像素：换算要知道目标 DPI，那是后端的事。调用方拿 twips
    /// 自己按 `twips / 1440 * dpi` 换，或交给 `rsword_layout_gpu::Viewport`。
    pub fn page_size_twips(&self, index: usize) -> Vec<i32> {
        match self.doc.get(index) {
            Some(p) => vec![p.size.width, p.size.height],
            None => Vec::new(),
        }
    }

    /// 比较器记录的摘要，供前端自查引擎是否真的产出了可比对的量。
    ///
    /// 返回 `[行数, 带源区间的行数, 段落标记数, 终止符应产出字形合计]`。
    /// 按量具方法，这些数必须自洽：带源区间的行数应等于行数（源区间与是否
    /// 栅格化无关），终止符合计应等于段落标记数（每个画 1 个空格）。
    pub fn oracle_summary(&self, index: usize) -> Vec<u32> {
        use rsword_layout_core::{LayoutRecord, LineTerminator, paint_document};
        let Some(page) = self.doc.get(index) else {
            return Vec::new();
        };
        let rec = LayoutRecord::from_paint(&paint_document(
            std::slice::from_ref(page),
            None,
            &[],
        ));
        let lines = rec.pages.first().map(|p| &p.lines[..]).unwrap_or(&[]);
        let with_source = lines.iter().filter(|l| l.source.is_some()).count();
        let marks = lines
            .iter()
            .filter(|l| l.terminator == LineTerminator::ParagraphMark)
            .count();
        let expected: usize = lines.iter().map(|l| l.terminator.expected_glyphs()).sum();
        vec![lines.len() as u32, with_source as u32, marks as u32, expected as u32]
    }

    /// 某页的片段数，用于自查布局是否产出了内容。
    pub fn fragment_count(&self, index: usize) -> usize {
        self.doc.get(index).map_or(0, |p| p.fragments.len())
    }
}

impl LayoutSession {
    /// 与 wasm 无关的构造，便于在原生测试里复用。
    pub fn build(docx: &[u8], dpi: f32) -> Result<LayoutSession, String> {
        // 与 layout-trace 同一个入口：并排的 `w:rPr` 解析器只留最后一个，`load_document`
        // 先把它们并起来。合并失败时它交回原样的 JSON（`merge_error`），这里照原样排。
        let loaded = load_document(docx).map_err(|e| format!("解析失败：{e}"))?;
        let document = loaded.layout_document();
        if document.paras.is_empty() {
            return Err("文档里没有可排版的段落".into());
        }

        let metrics = SimpleMetrics;
        let engine = Engine::new(&metrics, PageSetup::a4());
        Ok(LayoutSession {
            doc: engine.layout_document(&document),
            dpi: if dpi > 0.0 { dpi } else { 96.0 },
        })
    }

    /// 取某页的**矢量**绘制指令。
    ///
    /// 与旧的 `frame(index, glyphs)` 的关键差别：这里不产生像素，也不需要 DPI。
    /// 坐标一律是 twips，要栅格化的后端自己带 `Viewport` 去 `rsword_layout_gpu`，
    /// 能直接输出矢量的后端（SVG / PDF）则根本不必栅格化。
    ///
    /// `shaper` 为 `None` 时 `PaintCmd::Glyphs` 的字形序列为空但保留原文，
    /// 能直接排文字的后端照样能画。
    pub fn paint(
        &self,
        index: usize,
        shaper: Option<&dyn TextShaper>,
        faces: &[String],
    ) -> Option<PaintPage> {
        let page = self.doc.get(index)?;
        Some(paint_page(page, shaper, faces))
    }

    /// 整篇文档的绘制指令。
    pub fn paint_all(&self, shaper: Option<&dyn TextShaper>, faces: &[String]) -> PaintList {
        paint_document(&self.doc, shaper, faces)
    }

    pub fn dpi(&self) -> f32 {
        self.dpi
    }

    pub fn pages(&self) -> &[Page] {
        &self.doc
    }
}

#[cfg(test)]
mod tests {
    use super::LayoutSession;
    use rsword::bind::native::SessionTable;
    use rsword_layout_core::Fragment;

    /// 与 word_analyse `latinspace.docx` 同形：同一 `w:r` 里并排两个 `w:rPr`，
    /// 字符间距在第一个里。解析器只留最后一个，不走 `load_document` 就丢了它。
    fn double_rpr_docx() -> Vec<u8> {
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
            r#"<w:p><w:r><w:rPr><w:spacing w:val="20"/></w:rPr><w:rPr><w:sz w:val="24"/></w:rPr>"#,
            r#"<w:t>alpha</w:t></w:r></w:p>"#,
            r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#,
        );
        let blank = rsword::save::blank_docx(None).expect("空白 docx");
        let mut sessions = SessionTable::default();
        let id = sessions.open(&blank, None).unwrap();
        let doc: serde_json::Value =
            serde_json::from_str(&sessions.document(&id, None).unwrap()).unwrap();
        let op = serde_json::json!({"op": "replacePartXml", "part": doc["mainPart"], "xml": xml});
        sessions.apply(&id, &op.to_string(), None).unwrap();
        let bytes = sessions.save(&id, None).unwrap();
        sessions.close(&id);
        bytes
    }

    #[test]
    fn build_goes_through_load_document() {
        // 同一份 docx 换个入口不该排得不一样：这里的字符间距要与 layout-trace 一样到位。
        let session = LayoutSession::build(&double_rpr_docx(), 96.0).unwrap();
        let spacing: Vec<i32> = session.pages()[0]
            .fragments
            .iter()
            .filter_map(|f| match f {
                Fragment::Text(t) => Some(t.font.letter_spacing),
                _ => None,
            })
            .collect();
        assert_eq!(spacing, [20]);
    }
}
