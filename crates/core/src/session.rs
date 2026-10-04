//! 文档会话：一份 docx、一套字体与一组版面选项，排一次、画多次。
//!
//! 入口（SVG CLI、`layout-trace`，之后的 C ABI 与 WASM）都从这里建布局，不再各自拼
//! `load_document` → `Engine` → `paint_document`。会话保证三件事：
//!
//! - **同一字体环境**：量宽（`RealMetrics`）、整形与绘制（[`DocumentSession::paint`]）
//!   用会话持有的同一个 `FontRegistry`。字体交进来就不能再改，换字体只能建新会话、重排；
//!   `paint` 也不接受外面的 shaper——杜绝「布局后换一套不匹配的 shaper」。
//! - **明确的度量模式**：近似模式（`SimpleMetrics`）不带字体，画出的字形序列为空，
//!   并带 [`DiagnosticCode::MetricsApproximate`]，不能冒充精确结果进验收。
//! - **诊断不丢**：装载、投影、环绕与字体覆盖的遗漏都进 [`SessionDiagnostic`]，
//!   宿主（CLI 打到 stderr，C ABI / WASM 之后按码交出）不必各自去翻文档字段。
//!   锚定对象只在 [`WrapPolicy::Anchors`] 时扫；[`WrapPolicy::None`] 不扫、不报，与原来一样。
//!
//! # 字体怎么装不归这里
//!
//! 装哪些文件、TTC 装几个 face、回退链按什么判据装，是入口的策略：SVG CLI 按声明全装
//! （[`crate::font::FontRegistry::add_fallback`]），`layout-trace` 只装盖得住缺字的 face。
//! 两种装法在个别地方排出来不同（段落标记借回退字体的空格，见
//! `docs/SHARED-FONT-SESSION-2026-09-28.md`），所以会话只接**装好的**注册表，
//! 角色与次序原样留着（[`DocumentSession::faces`] 与注册表的 `fallback_faces`）。

use std::fmt;

use rsword::model::Document;
use rsword::package::Package;

use crate::anchor::AnchorScan;
use crate::document::{LayoutDocument, PageOverrides};
use crate::layout::{
    Engine, FaceId, Page, PageSetup, PaintList, PaintPage, Platform, TextShaper, View, WrapContext,
    paint_document, paint_page,
};
use crate::oracle::LayoutRecord;
use crate::oracle_json::{TraceMeta, to_trace_json};
#[cfg(feature = "fontenv")]
use crate::layout::Para;
use crate::font::{FontMetrics, SimpleMetrics};
use crate::load::load_document;

#[cfg(feature = "fontenv")]
use crate::font::{CharCoverage, FontRegistry, HorizontalGrid, RealMetrics, VerticalGrid};

/// 文字环绕从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapPolicy {
    /// 不扫锚定对象、不绕排。`layout-trace`、C ABI 与 WASM 一直如此。
    #[default]
    None,
    /// 重开 docx 包扫锚定对象，按 [`AnchorScan`] 绕排（SVG CLI）。
    ///
    /// 是近似：位置按第 0 节版心换算，同一块区域在每一页、每一节都排除（见 `anchor.rs`）。
    Anchors,
}

/// 版面选项。度量（近似或真字体）另给，见 [`PreparedDocument`] 的两个 `layout_*`。
///
/// 默认是 [`Platform::Desktop`] + [`View::Print`] + 不绕排，与 `Engine::new` 相同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutOptions {
    pub platform: Platform,
    pub view: View,
    pub wrap: WrapPolicy,
}

/// 诊断的种类。码是稳定的，文字不是。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// 并排的 `w:rPr` 已合并（原文件不合 schema）。
    RunPropsMerged,
    /// 并排 `w:rPr` 合并失败，按解析器原样的 JSON 排。
    RunPropsMergeFailed,
    /// 文档投影的诊断，`LayoutDocument::diagnostics` 的原文，一条一个。
    DocumentInput,
    /// 不支持的主文块没有排（表格、绘图等），不在布局结果里。
    BlocksSkipped,
    /// 解析器告警（`LayoutDocument::source_warnings`）。
    SourceWarnings,
    /// 近似度量：按字符类别估宽度，不读字体。
    MetricsApproximate,
    /// 环绕区是近似定位的。
    WrapApproximate,
    /// 锚定位置读不出，没有参与环绕。
    AnchorsSkipped,
    /// 没有字体盖得住的 CJK 字符，按名义 1 em 画 `.notdef`。
    GlyphNominal,
    /// 没有字体盖得住的其他字符，没画、不占宽度。
    GlyphDropped,
    /// 文档点名的字体族没装，按码位换了别的字体。
    FontFamilySubstituted,
    /// 点名的字体族装了，但没有要的粗体 / 斜体 face，用了最接近的（不合成）。
    FontStyleSubstituted,
}

impl DiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunPropsMerged => "RUN_PROPS_MERGED",
            Self::RunPropsMergeFailed => "RUN_PROPS_MERGE_FAILED",
            Self::DocumentInput => "DOCUMENT_INPUT",
            Self::BlocksSkipped => "BLOCKS_SKIPPED",
            Self::SourceWarnings => "SOURCE_WARNINGS",
            Self::MetricsApproximate => "METRICS_APPROXIMATE",
            Self::WrapApproximate => "WRAP_APPROXIMATE",
            Self::AnchorsSkipped => "ANCHORS_SKIPPED",
            Self::GlyphNominal => "GLYPH_NOMINAL",
            Self::GlyphDropped => "GLYPH_DROPPED",
            Self::FontFamilySubstituted => "FONT_FAMILY_SUBSTITUTED",
            Self::FontStyleSubstituted => "FONT_STYLE_SUBSTITUTED",
        }
    }
}

/// 一条会话诊断。`message` 是给人看的原文；机器按 `code` 分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDiagnostic {
    pub code: DiagnosticCode,
    pub message: String,
}

impl SessionDiagnostic {
    fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

/// 给终端的一行：文档投影的诊断保留 CLI 一直用的 `layout: ` 前缀。
impl fmt::Display for SessionDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            DiagnosticCode::DocumentInput => write!(f, "layout: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

/// 建会话失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionErrorKind {
    /// 没有可排版的段落或受支持的表格（[`LayoutDocument::has_layout_content`]）。
    NoLayoutContent,
    /// 真字体度量至少要一个正文字体。只有回退字体时，回退链只接 eastAsia 槽的字符，
    /// 其余字符没有 face、被跳过——排出来的不是一份连贯的版面。
    NoPrimaryFont,
    /// [`WrapPolicy::Anchors`] 要重开 docx 包，失败了。
    AnchorScan(String),
}

/// 建会话失败。已收集的诊断随错误交出，不因失败而丢。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionError {
    pub kind: SessionErrorKind,
    pub diagnostics: Vec<SessionDiagnostic>,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SessionErrorKind::NoLayoutContent => f.write_str("没有可排版的段落"),
            SessionErrorKind::NoPrimaryFont => {
                f.write_str("真字体度量至少要一个正文字体（回退字体不算）")
            }
            SessionErrorKind::AnchorScan(error) => write!(f, "环绕扫描打不开文档：{error}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl SessionError {
    /// 错误原因加上随错误交出的诊断，一条一行（`CODE: 原文`）。给没有单独诊断通道的宿主
    /// （C ABI 的 `rsl_last_error`、WASM 抛给 JS 的字符串）用，诊断不因失败而丢。
    pub fn report(&self) -> String {
        let mut out = self.to_string();
        for diagnostic in &self.diagnostics {
            out.push('\n');
            out.push_str(diagnostic.code.as_str());
            out.push_str(": ");
            out.push_str(&diagnostic.message);
        }
        out
    }
}

/// 已装载、已投影的文档，等着定度量。
///
/// 借着原始字节：[`WrapPolicy::Anchors`] 要重开包（锚定几何不在 JSON 投影里）。
/// 页面覆盖（[`PreparedDocument::apply_page_overrides`]）要在排版之前套，环绕区按覆盖后的版心算。
pub struct PreparedDocument<'a> {
    bytes: &'a [u8],
    merged_run_props: usize,
    merge_error: Option<String>,
    document: LayoutDocument,
}

impl<'a> PreparedDocument<'a> {
    /// 装载（[`load_document`]，并排 `w:rPr` 先合并）并投影成布局输入。
    /// 错误就是 `load_document` 的，原样交回。
    pub fn load(bytes: &'a [u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let loaded = load_document(bytes)?;
        let document = loaded.layout_document();
        Ok(Self {
            bytes,
            merged_run_props: loaded.merged_run_props,
            merge_error: loaded.merge_error,
            document,
        })
    }

    pub fn document(&self) -> &LayoutDocument {
        &self.document
    }

    /// 合并掉的并排 `w:rPr` 边界个数。
    pub fn merged_run_props(&self) -> usize {
        self.merged_run_props
    }

    /// 并排 `w:rPr` 合并失败的原因。
    pub fn merge_error(&self) -> Option<&str> {
        self.merge_error.as_deref()
    }

    /// 见 [`LayoutDocument::apply_page_overrides`]。
    pub fn apply_page_overrides(&mut self, overrides: PageOverrides) -> Result<(), String> {
        self.document.apply_page_overrides(overrides)
    }

    pub fn has_layout_content(&self) -> bool {
        self.document.has_layout_content()
    }

    /// 装载与投影阶段的诊断，按出现先后：合并、投影原文、跳过的块、解析器告警。
    pub fn diagnostics(&self) -> Vec<SessionDiagnostic> {
        let mut out = Vec::new();
        if self.merged_run_props > 0 {
            out.push(SessionDiagnostic::new(
                DiagnosticCode::RunPropsMerged,
                format!(
                    "合并了 {} 处并排的 w:rPr（原文件不合 schema；解析器原本只留最后一个）",
                    self.merged_run_props
                ),
            ));
        }
        if let Some(error) = &self.merge_error {
            out.push(SessionDiagnostic::new(
                DiagnosticCode::RunPropsMergeFailed,
                format!("并排 w:rPr 的合并失败，按解析器原样的 JSON 排：{error}"),
            ));
        }
        out.extend(
            self.document
                .diagnostics
                .iter()
                .map(|d| SessionDiagnostic::new(DiagnosticCode::DocumentInput, d.clone())),
        );
        if self.document.skipped_blocks > 0 {
            out.push(SessionDiagnostic::new(
                DiagnosticCode::BlocksSkipped,
                format!(
                    "{} 个主文块没有排（不支持的表格 / 绘图等），不在布局结果里",
                    self.document.skipped_blocks
                ),
            ));
        }
        if !self.document.source_warnings.is_empty() {
            out.push(SessionDiagnostic::new(
                DiagnosticCode::SourceWarnings,
                format!(
                    "解析器告警 {} 条（原文在 LayoutDocument::source_warnings）",
                    self.document.source_warnings.len()
                ),
            ));
        }
        out
    }

    /// 近似模式：`SimpleMetrics` 量宽，不带字体，画出来没有字形序列。
    pub fn layout_approximate(self, options: &LayoutOptions) -> Result<DocumentSession, SessionError> {
        let mut layout = self.begin(options)?;
        layout.diagnostics.push(SessionDiagnostic::new(
            DiagnosticCode::MetricsApproximate,
            "近似度量（SimpleMetrics）：按字符类别估宽度、不读字体，结果不进精确验收",
        ));
        let pages = layout.run(&SimpleMetrics, options);
        Ok(layout.finish(pages, *options, Vec::new()))
    }

    /// 真字体：`fonts` 同时用于量宽、整形与绘制，交进来之后不能再改。
    ///
    /// `vertical_grid` 见 [`VerticalGrid`] 的限定（Mac 栅格含回测规则）。
    #[cfg(feature = "fontenv")]
    pub fn layout_with_fonts(
        self,
        fonts: FontRegistry,
        vertical_grid: VerticalGrid,
        options: &LayoutOptions,
    ) -> Result<DocumentSession, SessionError> {
        let primary = fonts.face_ids().iter().any(|f| !fonts.fallback_faces().contains(f));
        if !primary {
            return Err(SessionError {
                kind: SessionErrorKind::NoPrimaryFont,
                diagnostics: self.diagnostics(),
            });
        }
        let mut layout = self.begin(options)?;
        layout.diagnostics.extend(coverage_diagnostics(&layout.document, &fonts));
        let metrics = RealMetrics::new(&fonts)
            .with_vertical_grid(vertical_grid)
            .with_horizontal_grid(horizontal_grid(options))
            .with_kerning_by_default(kerning_by_default(options))
            .with_east_asian_line_scale(options.platform == Platform::Android);
        let pages = layout.run(&metrics, options);
        let mut session = layout.finish(pages, *options, fonts.face_ids());
        session.fonts = Some((fonts, vertical_grid));
        session.horizontal = horizontal_grid(options);
        session.kerning_by_default = kerning_by_default(options);
        Ok(session)
    }

    /// 两种度量共用的前半：核内容、扫环绕。
    fn begin(self, options: &LayoutOptions) -> Result<Layout, SessionError> {
        let mut diagnostics = self.diagnostics();
        let input_diagnostics = diagnostics.len();
        if !self.has_layout_content() {
            return Err(SessionError { kind: SessionErrorKind::NoLayoutContent, diagnostics });
        }
        let setup = self.document.sections.first().map_or(PageSetup::a4(), |s| s.setup);
        let (wrap, anchors) = match options.wrap {
            WrapPolicy::None => (WrapContext::new(), None),
            WrapPolicy::Anchors => {
                let scan = match scan_anchors(self.bytes, setup) {
                    Ok(scan) => scan,
                    Err(error) => {
                        return Err(SessionError {
                            kind: SessionErrorKind::AnchorScan(error),
                            diagnostics,
                        });
                    }
                };
                let report = AnchorReport {
                    regions: scan.wrap.len(),
                    skipped: scan.skipped,
                    not_wrapping: scan.not_wrapping,
                };
                if report.regions > 0 {
                    diagnostics.push(SessionDiagnostic::new(
                        DiagnosticCode::WrapApproximate,
                        format!(
                            "环绕区 {} 个按第 0 节版心定位，每一页都排除同一块区域（近似）",
                            report.regions
                        ),
                    ));
                }
                if report.skipped > 0 {
                    diagnostics.push(SessionDiagnostic::new(
                        DiagnosticCode::AnchorsSkipped,
                        format!("{} 个锚定对象的位置读不出，没有参与环绕", report.skipped),
                    ));
                }
                (scan.wrap, Some(report))
            }
        };
        Ok(Layout {
            document: self.document,
            setup,
            wrap,
            anchors,
            diagnostics,
            input_diagnostics,
        })
    }
}

/// Android 移动视图按设备像素量字宽（[`HorizontalGrid`]）；其余平台与视图不量化。
///
/// 每英寸 778 像素是 word_analyse 唯一一台设备（b0e3d198）上读到的换算常数（Q15 的
/// `w3 = (1440 × rect + 389) / 778`，P0-1b 的 run 度量）；别的设备未测。
#[cfg(feature = "fontenv")]
fn horizontal_grid(options: &LayoutOptions) -> HorizontalGrid {
    match (options.platform, options.view) {
        (Platform::Android, View::Mobile) => HorizontalGrid::DevicePixels { per_inch: ANDROID_MOBILE_PIXELS_PER_INCH },
        _ => HorizontalGrid::None,
    }
}

/// Android Word 不写 `w:kern` 也做字距调整，写了照阈值（见 `font::real` 的 `kerning_font`）；
/// 桌面只照 `w:kern` 的阈值。
#[cfg(feature = "fontenv")]
fn kerning_by_default(options: &LayoutOptions) -> bool {
    options.platform == Platform::Android
}

/// 见 [`horizontal_grid`]。
#[cfg(feature = "fontenv")]
pub const ANDROID_MOBILE_PIXELS_PER_INCH: u32 = 778;

/// 扫描锚定对象。锚定几何只在 rsword 的 Rust 模型里，得重开一次包。
fn scan_anchors(bytes: &[u8], setup: PageSetup) -> Result<AnchorScan, String> {
    let mut package = Package::open(bytes).map_err(|e| e.to_string())?;
    let model = Document::rebuild(&mut package).map_err(|e| e.to_string())?;
    let scan = match package.dom(model.main_part).map_err(|e| e.to_string())? {
        Some(dom) => AnchorScan::from_document(&model, dom, setup.content_area()),
        None => AnchorScan::default(),
    };
    Ok(scan)
}

/// 定了度量之后、排版之前的中间态。
struct Layout {
    document: LayoutDocument,
    setup: PageSetup,
    wrap: WrapContext,
    anchors: Option<AnchorReport>,
    diagnostics: Vec<SessionDiagnostic>,
    input_diagnostics: usize,
}

impl Layout {
    fn run<M: FontMetrics>(&self, metrics: &M, options: &LayoutOptions) -> Vec<Page> {
        Engine::with_wrap(metrics, self.setup, self.wrap.clone())
            .with_platform(options.platform, options.view)
            .layout_document(&self.document)
    }

    /// 字体（真字体模式）由调用方随后放进去。
    fn finish(self, pages: Vec<Page>, options: LayoutOptions, faces: Vec<FaceId>) -> DocumentSession {
        DocumentSession {
            document: self.document,
            pages,
            options,
            faces,
            #[cfg(feature = "fontenv")]
            fonts: None,
            #[cfg(feature = "fontenv")]
            horizontal: HorizontalGrid::None,
            #[cfg(feature = "fontenv")]
            kerning_by_default: false,
            anchors: self.anchors,
            diagnostics: self.diagnostics,
            input_diagnostics: self.input_diagnostics,
        }
    }
}

/// [`WrapPolicy::Anchors`] 扫到的锚定对象。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorReport {
    /// 参与环绕的区域数。
    pub regions: usize,
    /// 锚定方式暂不支持而跳过的对象数。
    pub skipped: usize,
    /// 不参与绕排的对象数（`wrapNone` 或随文）。
    pub not_wrapping: usize,
}

/// 排好的文档：页、字体快照、选项与诊断。不可变。
pub struct DocumentSession {
    document: LayoutDocument,
    pages: Vec<Page>,
    options: LayoutOptions,
    /// 绘制时交给 `paint_document` 的 face 表，顺序与整形器的下标一致。
    faces: Vec<FaceId>,
    #[cfg(feature = "fontenv")]
    fonts: Option<(FontRegistry, VerticalGrid)>,
    /// 断行量字宽用的横向栅格；绘制按同一栅格整形（见 [`Self::paint`]）。
    #[cfg(feature = "fontenv")]
    horizontal: HorizontalGrid,
    /// 不论 `w:kern` 总做字距调整（Android）；断行与绘制同一口径。
    #[cfg(feature = "fontenv")]
    kerning_by_default: bool,
    anchors: Option<AnchorReport>,
    diagnostics: Vec<SessionDiagnostic>,
    input_diagnostics: usize,
}

impl DocumentSession {
    /// 排版用的文档（已套页面覆盖）。
    pub fn document(&self) -> &LayoutDocument {
        &self.document
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn options(&self) -> &LayoutOptions {
        &self.options
    }

    /// 近似模式（`SimpleMetrics`，没有字体）。
    pub fn is_approximate(&self) -> bool {
        #[cfg(feature = "fontenv")]
        {
            self.fonts.is_none()
        }
        #[cfg(not(feature = "fontenv"))]
        {
            true
        }
    }

    /// 全部诊断：装载与投影阶段在前（[`PreparedDocument::diagnostics`]），排版阶段在后。
    pub fn diagnostics(&self) -> &[SessionDiagnostic] {
        &self.diagnostics
    }

    /// 只要排版阶段新增的（度量模式、环绕、字体覆盖）。已经报过装载诊断的入口用这个。
    pub fn layout_diagnostics(&self) -> &[SessionDiagnostic] {
        &self.diagnostics[self.input_diagnostics..]
    }

    /// [`WrapPolicy::Anchors`] 时的扫描结果。
    pub fn anchors(&self) -> Option<&AnchorReport> {
        self.anchors.as_ref()
    }

    /// 字体的 face 标识，注册顺序，即 `ShapedRun::face_index` 的下标。近似模式为空。
    pub fn faces(&self) -> &[FaceId] {
        &self.faces
    }

    /// 字体环境指纹（`FontRegistry::fingerprint`）。近似模式为 `None`。
    ///
    /// 它不含角色与次序：同一组 face 换成回退字体，指纹不变而排出来可以不同，
    /// 要核角色看 [`DocumentSession::faces`] 与注册表的 `fallback_faces`。
    pub fn font_fingerprint(&self) -> Option<&str> {
        #[cfg(feature = "fontenv")]
        {
            self.fonts.as_ref().and_then(|(fonts, _)| fonts.fingerprint())
        }
        #[cfg(not(feature = "fontenv"))]
        {
            None
        }
    }

    /// 排版与绘制用的注册表。近似模式为 `None`。只读：会话排完就不许换字体。
    #[cfg(feature = "fontenv")]
    pub fn fonts(&self) -> Option<&FontRegistry> {
        self.fonts.as_ref().map(|(fonts, _)| fonts)
    }

    /// 真字体模式的纵向栅格。
    #[cfg(feature = "fontenv")]
    pub fn vertical_grid(&self) -> Option<VerticalGrid> {
        self.fonts.as_ref().map(|(_, grid)| *grid)
    }

    /// 整篇的绘制指令。整形用排版时的同一个注册表；近似模式不整形，字形序列为空但保留原文。
    pub fn paint(&self) -> PaintList {
        self.with_paint_shaper(|shaper| paint_document(&self.pages, shaper, &self.faces))
    }

    /// 一页的绘制指令，与 [`DocumentSession::paint`] 的那一页相同。越界返回 `None`。
    pub fn paint_page(&self, index: usize) -> Option<PaintPage> {
        let page = self.pages.get(index)?;
        Some(self.with_paint_shaper(|shaper| paint_page(page, shaper, &self.faces)))
    }

    /// 这个会话是否正是用 `fonts` 这套字体排的：face 集合与注册顺序、回退链的成员与次序都相同。
    ///
    /// 字体指纹不含角色与次序，只比指纹会放过「同一组 face 换了角色」。宿主拿一个
    /// 比会话活得长的字体集去栅格化时（WebGL 的图集），先用这个核：字体集在排版之后变了，
    /// 就得重排，不能拿旧的布局配新的字体。近似会话没有字体，恒为 `false`。
    #[cfg(feature = "fontenv")]
    pub fn same_fonts(&self, fonts: &FontRegistry) -> bool {
        self.fonts.as_ref().is_some_and(|(own, _)| {
            own.face_ids() == fonts.face_ids()
                && own.fallback_faces() == fonts.fallback_faces()
                && own.fingerprint() == fonts.fingerprint()
        })
    }

    /// 表格行盒的引擎记账，twips。行号是页内行号，与轨迹 `pages[].lines[].index` 相同。
    /// 这是引擎的诊断，不是 Word 实测的行盒。
    pub fn table_layout(&self) -> serde_json::Value {
        use serde_json::json;
        json!({
            "unit": "twips",
            "status": "engine diagnostic, not measured Word geometry",
            "pages": self.pages.iter().enumerate().map(|(index, page)| json!({
                "page": index,
                "rows": page.table_rows.iter().map(|row| json!({
                    "table": row.table,
                    "row": row.row,
                    "column": row.column,
                    "rect": [row.rect.x, row.rect.y, row.rect.width, row.rect.height],
                    "topFine": row.top_fine,
                    "heightFine": row.height_fine,
                    "cells": row.cells.iter().map(|cell| json!({
                        "source": [cell.source.0, cell.source.1],
                        "lines": [cell.lines.start, cell.lines.end],
                        "overflowFine": cell.overflow_fine,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    /// 规范化的布局结果，给跨入口比较用（schema `rsword-layout-result/1`）。
    ///
    /// 同一份 DOCX、同一套字体与选项，从 Rust、C ABI 或 WASM 建会话，这份 JSON 逐字节相同。
    /// 含：模式与选项；字体快照（指纹、face 注册顺序、回退链）；轨迹的全部页、行、源区间与字形
    /// （`rsword-layout-trace/1` 的 `pages`）；布局输入（`layoutInput`）；有表格时的行盒；
    /// 全部诊断。不含任何宿主相关的东西（路径、DPI）。
    pub fn layout_json(&self) -> String {
        use serde_json::json;
        let meta = TraceMeta {
            engine: format!("rsword-layout-core {}", env!("CARGO_PKG_VERSION")),
            metrics: String::new(),
            glyph_origin_method: String::new(),
            source: String::new(),
            font_fingerprint: self.font_fingerprint().map(str::to_string),
        };
        let trace: serde_json::Value = serde_json::from_str(&to_trace_json(
            &LayoutRecord::from_paint(&self.paint()),
            &meta,
        ))
        .expect("trace JSON is valid");
        let platform = match self.options.platform {
            Platform::Desktop => "desktop",
            Platform::Android => "android",
        };
        let view = match self.options.view {
            View::Print => "print",
            View::Mobile => "mobile",
        };
        let wrap = match self.options.wrap {
            WrapPolicy::None => "none",
            WrapPolicy::Anchors => "anchors",
        };
        let fallback_faces: Vec<FaceId> = self.fallback_faces();
        let mut out = json!({
            "schema": "rsword-layout-result/1",
            "engine": trace["engine"],
            "metrics": if self.is_approximate() { "approximate" } else { "real" },
            "verticalGrid": self.vertical_grid_name(),
            "platform": platform,
            "view": view,
            "wrap": wrap,
            "fontFingerprint": trace["fontFingerprint"],
            "faces": self.faces,
            "fallbackFaces": fallback_faces,
            "notdefGlyphs": trace["notdefGlyphs"],
            "unassignedGlyphs": trace["unassignedGlyphs"],
            "pages": trace["pages"],
            "layoutInput": self.document.trace_metadata(),
            "diagnostics": self.diagnostics.iter().map(|d| json!({
                "code": d.code.as_str(),
                "message": d.message,
            })).collect::<Vec<_>>(),
        });
        #[cfg(feature = "fontenv")]
        if let HorizontalGrid::DevicePixels { per_inch } = self.horizontal {
            out["horizontalGrid"] = json!({ "devicePixelsPerInch": per_inch });
        }
        #[cfg(feature = "fontenv")]
        if self.kerning_by_default {
            out["kerning"] = json!("always");
        }
        if !self.document.tables.is_empty() {
            out["tableLayout"] = self.table_layout();
        }
        serde_json::to_string_pretty(&out).expect("layout JSON is valid")
    }

    fn fallback_faces(&self) -> Vec<FaceId> {
        #[cfg(feature = "fontenv")]
        if let Some((fonts, _)) = &self.fonts {
            return fonts.fallback_faces().to_vec();
        }
        Vec::new()
    }

    fn vertical_grid_name(&self) -> Option<&'static str> {
        #[cfg(feature = "fontenv")]
        if let Some((_, grid)) = &self.fonts {
            return Some(match grid {
                VerticalGrid::None => "none",
                VerticalGrid::MacWordThreeHundredthsInch => "mac",
            });
        }
        None
    }

    /// 绘制用的整形器：开了设备像素栅格时包一层，让字形推进量与断行量的宽同一口径。
    fn with_paint_shaper<R>(&self, paint: impl FnOnce(Option<&dyn TextShaper>) -> R) -> R {
        #[cfg(feature = "fontenv")]
        if let Some(inner) = self.shaper() {
            let pixels = match self.horizontal {
                HorizontalGrid::DevicePixels { per_inch } => Some(per_inch),
                HorizontalGrid::None => None,
            };
            if pixels.is_some() || self.kerning_by_default {
                return paint(Some(&crate::font::PaintShaper { inner, pixels, kerning_by_default: self.kerning_by_default }));
            }
        }
        paint(self.shaper())
    }

    fn shaper(&self) -> Option<&dyn TextShaper> {
        #[cfg(feature = "fontenv")]
        if let Some((fonts, _)) = &self.fonts {
            return Some(fonts);
        }
        None
    }
}

/// 宿主持有的字体集：按声明顺序记下每份字体与它的角色，能重放出一份完全相同的注册表。
///
/// `FontRegistry` 不能克隆，而会话要把注册表按值收走。比文档活得长的宿主字体集
/// （WebGL 的 `FontSet`、C ABI 的 `RslFonts`）就存在这里：每建一个会话 [`FontSources::build`]
/// 一份新的注册表交进去——注册是确定的，同样的字节、序号与角色按同样的顺序装，
/// face 表、回退链与指纹都相同（[`DocumentSession::same_fonts`] 核的就是这三样）。
///
/// 自己也留一份注册表，装的时候就核字体（坏字节当场报错），并给图集栅格化用
/// （[`FontSources::rasterizer`]）。代价是字节多存一份。
#[cfg(feature = "fontenv")]
pub struct FontSources {
    entries: Vec<(Vec<u8>, u32, bool)>,
    registry: FontRegistry,
}

#[cfg(feature = "fontenv")]
impl Default for FontSources {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "fontenv")]
impl FontSources {
    pub fn new() -> Self {
        Self { entries: Vec::new(), registry: FontRegistry::new() }
    }

    /// 装一份正文字体。错误码与 [`FontRegistry::add`] 相同（如 `FONT_INVALID`）。
    pub fn add(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let face = self.registry.add(bytes.clone(), index)?;
        self.entries.push((bytes, index, false));
        Ok(face)
    }

    /// 把一份字体接到回退链末尾，见 [`FontRegistry::add_fallback`]。
    pub fn add_fallback(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let face = self.registry.add_fallback(bytes.clone(), index)?;
        self.entries.push((bytes, index, true));
        Ok(face)
    }

    /// 当前的注册表，只读。
    pub fn registry(&self) -> &FontRegistry {
        &self.registry
    }

    /// 重放出一份新的注册表，交给 [`PreparedDocument::layout_with_fonts`]。
    pub fn build(&self) -> FontRegistry {
        let mut out = FontRegistry::new();
        for (bytes, index, fallback) in &self.entries {
            let result = if *fallback {
                out.add_fallback(bytes.clone(), *index)
            } else {
                out.add(bytes.clone(), *index)
            };
            result.expect("font bytes were accepted when first added");
        }
        out
    }

    /// 给图集栅格化用。只交出 [`crate::font::Rasterizer`]：能画字形，不能增删字体。
    pub fn rasterizer(&mut self) -> &mut dyn crate::font::Rasterizer {
        self.registry.rasterizer_mut()
    }
}

/// 主文段落与表格单元格里的段落。
#[cfg(feature = "fontenv")]
fn story_paragraphs(document: &LayoutDocument) -> impl Iterator<Item = &Para> {
    document.paras.iter().chain(
        document
            .tables
            .iter()
            .flat_map(|table| &table.rows)
            .flat_map(|row| &row.cells)
            .flat_map(|cell| &cell.paras),
    )
}

/// 缺字与换字体的诊断：按源字符，扫正文与单元格里不隐藏的 run。
///
/// 口径与整形相同（[`FontRegistry::char_coverage`]），数的是**源字符**，不是画出的字形；
/// `w:caps` 下显示字符换了 face 的情形不在内，段落标记不在内。
///
/// 字体族只核文档**点名**的：字符所在的槽，没写就是 ascii / hAnsi 槽（`FontSpec::family`
/// 的来路）。哪个槽都没写时 `family` 是桥接层的宿主缺省（`Times New Roman, SimSun, serif`），
/// 不是文档要的，不报。族装了、选中的也是这个族的 face，但不是要的粗体 / 斜体时
/// （选 face 取最接近的字重与斜度，引擎不合成，宽度随之不同），单独报；
/// 族装了却盖不住这个字、换到别的族的，不算这一类。
#[cfg(feature = "fontenv")]
fn coverage_diagnostics(document: &LayoutDocument, fonts: &FontRegistry) -> Vec<SessionDiagnostic> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut nominal = (0usize, BTreeSet::new());
    let mut dropped = (0usize, BTreeSet::new());
    let mut installed: BTreeMap<&str, bool> = BTreeMap::new();
    let mut substituted = BTreeSet::new();
    let mut restyled = BTreeSet::new();
    for run in story_paragraphs(document).flat_map(|p| &p.runs).filter(|run| !run.hidden) {
        let slots = &run.font.slots;
        for ch in run.text.chars() {
            let coverage = fonts.char_coverage(&run.font, ch);
            match &coverage {
                CharCoverage::Invisible => continue,
                CharCoverage::Face(_) => {}
                CharCoverage::Nominal => {
                    nominal.0 += 1;
                    nominal.1.insert(ch);
                }
                CharCoverage::Dropped => {
                    dropped.0 += 1;
                    dropped.1.insert(ch);
                }
            }
            let declared = slots.get(slots.slot_for(ch)).or(slots.ascii.as_deref()).or(slots.h_ansi.as_deref());
            let Some(family) = declared.filter(|f| !f.is_empty()) else {
                continue;
            };
            if !*installed.entry(family).or_insert_with(|| fonts.covers_family(family)) {
                substituted.insert(family.to_owned());
                continue;
            }
            if let CharCoverage::Face(face) = &coverage
                && let Some((weight, italic)) = fonts.family_face_style(face, family)
                && ((weight >= 600) != run.font.bold || italic != run.font.italic)
            {
                restyled.insert((family.to_owned(), run.font.bold, run.font.italic));
            }
        }
    }
    let mut out = Vec::new();
    if nominal.0 > 0 {
        out.push(SessionDiagnostic::new(
            DiagnosticCode::GlyphNominal,
            format!(
                "{} 个 CJK 字符（{} 种）没有字体盖得住，按名义 1 em 画 .notdef：{}",
                nominal.0,
                nominal.1.len(),
                sample(&nominal.1)
            ),
        ));
    }
    if dropped.0 > 0 {
        out.push(SessionDiagnostic::new(
            DiagnosticCode::GlyphDropped,
            format!(
                "{} 个字符（{} 种）没有字体盖得住，没画、不占宽度：{}",
                dropped.0,
                dropped.1.len(),
                sample(&dropped.1)
            ),
        ));
    }
    if !substituted.is_empty() {
        out.push(SessionDiagnostic::new(
            DiagnosticCode::FontFamilySubstituted,
            format!(
                "文档点名的字体族没装，按码位换了别的字体（度量兼容的替换在几何上看不出来）：{}",
                substituted.iter().map(|f| format!("「{f}」")).collect::<Vec<_>>().join("、")
            ),
        ));
    }
    if !restyled.is_empty() {
        let style = |bold: bool, italic: bool| match (bold, italic) {
            (true, true) => "粗斜体",
            (true, false) => "粗体",
            (false, true) => "斜体",
            (false, false) => "常规",
        };
        out.push(SessionDiagnostic::new(
            DiagnosticCode::FontStyleSubstituted,
            format!(
                "文档要的字形没有对应的 face，用了字重与斜度最接近的（不合成粗体 / 斜体，宽度随之不同）：{}",
                restyled
                    .iter()
                    .map(|(f, bold, italic)| format!("「{f}」{}", style(*bold, *italic)))
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        ));
    }
    out
}

/// 前几个字符的码位，给诊断举例。
#[cfg(feature = "fontenv")]
fn sample(chars: &std::collections::BTreeSet<char>) -> String {
    const SHOWN: usize = 8;
    let mut out: Vec<String> = chars.iter().take(SHOWN).map(|c| format!("U+{:04X}", *c as u32)).collect();
    if chars.len() > SHOWN {
        out.push("…".into());
    }
    out.join(" ")
}
