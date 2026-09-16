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

use rsword::bind::native::SessionTable;
use rsword_layout_core::bridge::paras_from_document;
use rsword_layout_core::engine::{Engine, PageSetup};
use rsword_layout_core::fragment::LaidOutDocument;
use rsword_layout_core::gpu::{Frame, Viewport, build_page};
use rsword_layout_core::simple_metrics::SimpleMetrics;

/// 一次会话：持有排好版的文档，可反复取页。
pub struct LayoutSession {
    doc: LaidOutDocument,
    dpi: f32,
}

impl LayoutSession {
    /// 页数。
    pub fn page_count(&self) -> usize {
        self.doc.page_count()
    }

    /// 某页的像素宽高，返回 `[w, h]`。越界返回空数组。
    pub fn page_size(&self, index: usize) -> Vec<f32> {
        match self.doc.pages.get(index) {
            Some(p) => {
                let vp = Viewport::from_page(p.size, self.dpi);
                vec![vp.width_px, vp.height_px]
            }
            None => Vec::new(),
        }
    }

    /// 某页的片段数，用于自查布局是否产出了内容。
    pub fn fragment_count(&self, index: usize) -> usize {
        self.doc.pages.get(index).map_or(0, |p| p.fragments.len())
    }
}

impl LayoutSession {
    /// 与 wasm 无关的构造，便于在原生测试里复用。
    pub fn build(docx: &[u8], dpi: f32) -> Result<LayoutSession, String> {
        let mut sessions = SessionTable::default();
        let id = sessions.open(docx, None).map_err(|e| format!("解析失败：{e}"))?;
        let json = sessions
            .document(&id, None)
            .map_err(|e| format!("取模型失败：{e}"))?;
        sessions.close(&id);

        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("模型 JSON 无法解析：{e}"))?;
        let (paras, _skipped) = paras_from_document(&value);
        if paras.is_empty() {
            return Err("文档里没有可排版的段落".into());
        }

        let metrics = SimpleMetrics;
        let engine = Engine::new(&metrics, PageSetup::a4());
        Ok(LayoutSession {
            doc: engine.layout(&paras),
            dpi: if dpi > 0.0 { dpi } else { 96.0 },
        })
    }

    /// 取某页的 GPU 数据。渲染后端用这个。
    ///
    /// `glyphs` 为 `None` 时文字批次为空——没有字形图集就画不了字，
    /// 与其画错不如不画。
    pub fn frame(
        &self,
        index: usize,
        glyphs: Option<&dyn rsword_layout_core::gpu::GlyphSource>,
    ) -> Option<(Frame, Viewport)> {
        let page = self.doc.pages.get(index)?;
        Some((
            build_page(page, self.dpi, glyphs),
            Viewport::from_page(page.size, self.dpi),
        ))
    }

    pub fn dpi(&self) -> f32 {
        self.dpi
    }

    pub fn document(&self) -> &LaidOutDocument {
        &self.doc
    }
}
