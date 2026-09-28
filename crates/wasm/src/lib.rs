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
//! 排版走核心的文档会话，与 SVG CLI、`layout-trace` 同一条路；诊断随会话交出（`diagnostics`）。
//! 真字体模式要 `fontenv` 特性（`build_with_fonts`）；不开时只有近似模式。
//!
//! 构建见仓库 `scripts/prepare-webgl.sh`。

use rsword_layout_core::{
    DocumentSession, LayoutOptions, Page, PaintList, PaintPage, PreparedDocument, SessionDiagnostic,
};
#[cfg(feature = "fontenv")]
use rsword_layout_core::{FontSources, VerticalGrid};

/// 一次会话：持有排好版的文档会话，可反复取页。
///
/// 与 SVG CLI、`layout-trace` 同一个核心会话（`PreparedDocument` → `DocumentSession`）：
/// 真字体模式下量宽、整形与绘制用同一个注册表，[`LayoutSession::paint`] 不接受外面的 shaper。
pub struct LayoutSession {
    session: DocumentSession,
    dpi: f32,
}

impl LayoutSession {
    /// 页数。
    pub fn page_count(&self) -> usize {
        self.session.pages().len()
    }

    /// 某页的宽高，**单位 twips**，返回 `[w, h]`。越界返回空数组。
    ///
    /// 不返回像素：换算要知道目标 DPI，那是后端的事。调用方拿 twips
    /// 自己按 `twips / 1440 * dpi` 换，或交给 `rsword_layout_gpu::Viewport`。
    pub fn page_size_twips(&self, index: usize) -> Vec<i32> {
        match self.session.pages().get(index) {
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
        use rsword_layout_core::{LayoutRecord, LineTerminator};
        let Some(page) = self.session.paint_page(index) else {
            return Vec::new();
        };
        let rec = LayoutRecord::from_paint(&PaintList { pages: vec![page] });
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
        self.session.pages().get(index).map_or(0, |p| p.fragments.len())
    }
}

impl LayoutSession {
    /// 近似模式（`SimpleMetrics`，默认选项）：与 wasm 无关的构造，便于在原生测试里复用。
    ///
    /// 画出来没有字形序列；要字形就用 [`LayoutSession::build_with_fonts`]。
    pub fn build(docx: &[u8], dpi: f32) -> Result<LayoutSession, String> {
        Self::build_approximate(docx, dpi, &LayoutOptions::default())
    }

    /// 近似模式，自选平台、视图与环绕。
    pub fn build_approximate(
        docx: &[u8],
        dpi: f32,
        options: &LayoutOptions,
    ) -> Result<LayoutSession, String> {
        let session = prepare(docx)?.layout_approximate(options).map_err(|e| e.report())?;
        Ok(Self::wrap(session, dpi))
    }

    /// 真字体：用 `fonts` 当下的快照排版（[`FontSources::build`]），之后 `fonts` 再变也不影响本会话。
    #[cfg(feature = "fontenv")]
    pub fn build_with_fonts(
        docx: &[u8],
        dpi: f32,
        fonts: &FontSources,
        vertical_grid: VerticalGrid,
        options: &LayoutOptions,
    ) -> Result<LayoutSession, String> {
        let session = prepare(docx)?
            .layout_with_fonts(fonts.build(), vertical_grid, options)
            .map_err(|e| e.report())?;
        Ok(Self::wrap(session, dpi))
    }

    fn wrap(session: DocumentSession, dpi: f32) -> LayoutSession {
        LayoutSession { session, dpi: if dpi > 0.0 { dpi } else { 96.0 } }
    }

    /// 取某页的**矢量**绘制指令。坐标一律是 twips，不产生像素，也不需要 DPI。
    ///
    /// 整形用排版时的同一套字体；近似会话不整形，`DrawGlyphs` 的字形序列为空但保留原文。
    pub fn paint(&self, index: usize) -> Option<PaintPage> {
        self.session.paint_page(index)
    }

    /// 整篇文档的绘制指令。
    pub fn paint_all(&self) -> PaintList {
        self.session.paint()
    }

    /// 装载、投影、排版阶段的全部诊断。
    pub fn diagnostics(&self) -> &[SessionDiagnostic] {
        self.session.diagnostics()
    }

    /// 诊断的 JSON：`[{"code": "...", "message": "..."}]`，给 JS 用。
    pub fn diagnostics_json(&self) -> String {
        let items: Vec<_> = self
            .session
            .diagnostics()
            .iter()
            .map(|d| serde_json::json!({"code": d.code.as_str(), "message": d.message}))
            .collect();
        serde_json::Value::Array(items).to_string()
    }

    /// 规范化的布局结果（`DocumentSession::layout_json`），与 Rust 侧同输入逐字节相同。
    pub fn layout_json(&self) -> String {
        self.session.layout_json()
    }

    pub fn is_approximate(&self) -> bool {
        self.session.is_approximate()
    }

    pub fn session(&self) -> &DocumentSession {
        &self.session
    }

    pub fn dpi(&self) -> f32 {
        self.dpi
    }

    pub fn pages(&self) -> &[Page] {
        self.session.pages()
    }
}

/// 与 SVG CLI、layout-trace 同一个入口：并排的 `w:rPr` 解析器只留最后一个，`load_document`
/// 先把它们并起来；合并失败与投影诊断都进会话的诊断，不再在这里丢掉。
fn prepare(docx: &[u8]) -> Result<PreparedDocument<'_>, String> {
    PreparedDocument::load(docx).map_err(|e| format!("解析失败：{e}"))
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
        let spacing: Vec<_> = session.pages()[0]
            .fragments
            .iter()
            .filter_map(|f| match f {
                Fragment::Text(t) => Some((t.text.as_str(), t.source, t.font.letter_spacing)),
                _ => None,
            })
            .collect();
        assert_eq!(spacing, [("alpha", Some((0, 5)), 20), (" ", Some((5, 6)), 0)]);
    }
}
