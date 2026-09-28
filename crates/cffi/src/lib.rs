//! C ABI 绑定：把「docx 字节 → 布局结果」暴露给 C/C++/Python/任何能调 C 的语言。
//!
//! 与 `rsword-layout-wasm` 是同一层的两种绑定，职责相同、目标不同：那个给 JS，这个给 C。
//! 两者都**不做渲染**，只交出布局产物。
//!
//! # 所有权约定
//!
//! - 所有 `rsl_*_new` 返回的指针必须用对应的 `rsl_*_free` 释放，重复释放是未定义行为。
//! - 返回的字符串（错误信息）由本库分配，必须用 [`rsl_string_free`] 释放。
//! - 传入的缓冲区只在调用期间被读取，本库不持有它们。
//! - 所有函数对空指针都返回错误码而不是崩溃——跨语言边界上 panic 是未定义行为，
//!   所以这里一律转成错误码。
//!
//! # 线程
//!
//! `RslSession` 不是线程安全的：同一个指针不要在多个线程里并发使用。
//! 不同的 session 之间互不影响。
//!
//! # 排版
//!
//! 与 SVG CLI、`layout-trace` 同一个核心会话（`PreparedDocument` → `DocumentSession`）。
//! [`rsl_session_new`] 是近似模式（不读字体）；[`rsl_session_new_ex`] 可以带字体集
//! （`fontenv` 特性下的 [`rsl_fonts_new`]）与选项。诊断随会话交出（[`rsl_diagnostic_count`]），
//! 建会话失败时并进 [`rsl_last_error`] 的文字，一条一行。
//! [`rsl_layout_json`] 给规范化的布局结果，同输入与 Rust 侧逐字节相同。
//!
//! 帧（[`rsl_frame_copy`]）目前只有矩形类几何：本库还不给 C 侧字形图集，字形批次为空。

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use rsword_layout_core::{DocumentSession, LayoutOptions, Platform, PreparedDocument, View, WrapPolicy};
use rsword_layout_gpu::{Frame, Viewport, build_page};

/// 错误码。0 为成功，负值为失败。
pub const RSL_OK: c_int = 0;
/// 传入了空指针。
pub const RSL_ERR_NULL: c_int = -1;
/// 页号越界。
pub const RSL_ERR_RANGE: c_int = -2;
/// 解析或排版失败，详情用 [`rsl_last_error`] 取。
pub const RSL_ERR_FAILED: c_int = -3;
/// 缓冲区太小，所需大小已写回出参。
pub const RSL_ERR_BUFFER: c_int = -4;

thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> =
        const { std::cell::RefCell::new(None) };
}

fn set_error(msg: impl Into<Vec<u8>>) {
    // CString::new 只在内容含 NUL 时失败；那种情况退化成固定串，不让错误路径再出错。
    let c = CString::new(msg).unwrap_or_else(|_| c"错误信息含 NUL 字节".into());
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
}

/// 取最近一次错误的描述。返回的指针在**同一线程**下次调用本库前有效，不要释放它。
/// 没有错误时返回 NULL。
#[unsafe(no_mangle)]
pub extern "C" fn rsl_last_error() -> *const c_char {
    LAST_ERROR.with(|e| match &*e.borrow() {
        Some(c) => c.as_ptr(),
        None => std::ptr::null(),
    })
}

/// 释放本库分配的字符串。
///
/// # Safety
/// `s` 必须是本库返回且尚未释放的指针，或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// 一次布局会话。对 C 侧是不透明指针。
pub struct RslSession {
    session: DocumentSession,
    dpi: f32,
    /// 诊断的 C 字符串，与会话同寿：`rsl_diagnostic_*` 返回的指针指向这里。
    diagnostics: Vec<(CString, CString)>,
}

/// 平台：桌面 Word（默认）。
pub const RSL_PLATFORM_DESKTOP: c_int = 0;
/// 平台：Android Word。
pub const RSL_PLATFORM_ANDROID: c_int = 1;
/// 视图：分页视图（默认）。
pub const RSL_VIEW_PRINT: c_int = 0;
/// 视图：移动视图。
pub const RSL_VIEW_MOBILE: c_int = 1;
/// 不按锚定对象绕排（默认）。
pub const RSL_WRAP_NONE: c_int = 0;
/// 按锚定对象绕排（近似，见核心 `WrapPolicy::Anchors`）。
pub const RSL_WRAP_ANCHORS: c_int = 1;
/// 纵向不量化（默认）。
pub const RSL_GRID_NONE: c_int = 0;
/// 纵向量化到 Mac Word 的 1/300 英寸栅格（含回测规则）。只用于真字体。
pub const RSL_GRID_MAC: c_int = 1;

/// 版面选项。全零即默认：桌面、分页视图、不绕排、不量化。
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct RslOptions {
    pub platform: c_int,
    pub view: c_int,
    pub wrap: c_int,
    pub vertical_grid: c_int,
}

impl RslOptions {
    fn layout(&self) -> Result<LayoutOptions, String> {
        let platform = match self.platform {
            RSL_PLATFORM_DESKTOP => Platform::Desktop,
            RSL_PLATFORM_ANDROID => Platform::Android,
            other => return Err(format!("platform 取值无效：{other}")),
        };
        let view = match self.view {
            RSL_VIEW_PRINT => View::Print,
            RSL_VIEW_MOBILE => View::Mobile,
            other => return Err(format!("view 取值无效：{other}")),
        };
        let wrap = match self.wrap {
            RSL_WRAP_NONE => WrapPolicy::None,
            RSL_WRAP_ANCHORS => WrapPolicy::Anchors,
            other => return Err(format!("wrap 取值无效：{other}")),
        };
        if !matches!(self.vertical_grid, RSL_GRID_NONE | RSL_GRID_MAC) {
            return Err(format!("vertical_grid 取值无效：{}", self.vertical_grid));
        }
        Ok(LayoutOptions { platform, view, wrap })
    }
}

/// 字体集：按声明顺序记下字体与角色，每建一个会话取一份快照。对 C 侧是不透明指针。
///
/// 会话建好之后再往字体集里加字体，不影响已有的会话。
pub struct RslFonts {
    #[cfg(feature = "fontenv")]
    fonts: rsword_layout_core::FontSources,
}

/// 新建空字体集。用 [`rsl_fonts_free`] 释放。
#[cfg(feature = "fontenv")]
#[unsafe(no_mangle)]
pub extern "C" fn rsl_fonts_new() -> *mut RslFonts {
    Box::into_raw(Box::new(RslFonts { fonts: rsword_layout_core::FontSources::new() }))
}

/// 释放字体集。
///
/// # Safety
/// `f` 必须来自 [`rsl_fonts_new`] 且尚未释放，或为 NULL。
#[cfg(feature = "fontenv")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_fonts_free(f: *mut RslFonts) {
    if !f.is_null() {
        drop(unsafe { Box::from_raw(f) });
    }
}

/// 装一份字体（TTF/OTF/TTC 原始字节）。`index` 是 TTC 子字体序号；`fallback` 非零时接到
/// 回退链末尾（只给 eastAsia 槽里画不出的字符查），否则是正文字体。
/// 失败返回 [`RSL_ERR_FAILED`]，错误码（如 `FONT_INVALID`）用 [`rsl_last_error`] 取。
///
/// # Safety
/// `f` 必须是有效的字体集指针；`data` 必须指向至少 `len` 字节的可读内存。
#[cfg(feature = "fontenv")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_fonts_add(
    f: *mut RslFonts,
    data: *const u8,
    len: usize,
    index: u32,
    fallback: c_int,
) -> c_int {
    let Some(f) = (unsafe { f.as_mut() }) else {
        set_error("fonts 为空指针");
        return RSL_ERR_NULL;
    };
    if data.is_null() {
        set_error("data 为空指针");
        return RSL_ERR_NULL;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
    let result = if fallback != 0 { f.fonts.add_fallback(bytes, index) } else { f.fonts.add(bytes, index) };
    match result {
        Ok(_) => RSL_OK,
        Err(code) => {
            set_error(code);
            RSL_ERR_FAILED
        }
    }
}

/// 解析并近似排版一份 docx（不读字体，默认选项）。
///
/// 失败返回 NULL，原因（含随错误交出的诊断）用 [`rsl_last_error`] 取。
///
/// # Safety
/// `data` 必须指向至少 `len` 字节的可读内存。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_session_new(
    data: *const u8,
    len: usize,
    dpi: f32,
) -> *mut RslSession {
    unsafe { rsl_session_new_ex(data, len, dpi, std::ptr::null(), std::ptr::null()) }
}

/// 解析并排版一份 docx。`fonts` 为 NULL 时近似排版；否则用它当下的快照排真字体。
/// `options` 为 NULL 时用默认选项。
///
/// 失败返回 NULL，原因（含随错误交出的诊断）用 [`rsl_last_error`] 取。
///
/// # Safety
/// `data` 必须指向至少 `len` 字节的可读内存；`fonts`、`options` 必须有效或为 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_session_new_ex(
    data: *const u8,
    len: usize,
    dpi: f32,
    fonts: *const RslFonts,
    options: *const RslOptions,
) -> *mut RslSession {
    if data.is_null() {
        set_error("data 为空指针");
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let options = unsafe { options.as_ref() }.copied().unwrap_or_default();
    match build_session(bytes, dpi, unsafe { fonts.as_ref() }, &options) {
        Ok(s) => Box::into_raw(Box::new(s)),
        Err(e) => {
            set_error(e);
            std::ptr::null_mut()
        }
    }
}

fn build_session(
    docx: &[u8],
    dpi: f32,
    fonts: Option<&RslFonts>,
    options: &RslOptions,
) -> Result<RslSession, String> {
    let layout = options.layout()?;
    // 与 layout-trace 同一个入口：并排的 `w:rPr` 解析器只留最后一个，`PreparedDocument::load`
    // 先把它们并起来；合并失败、投影诊断都进会话诊断。
    let prepared = PreparedDocument::load(docx).map_err(|e| format!("解析失败：{e}"))?;
    let session = match fonts {
        None if options.vertical_grid != RSL_GRID_NONE => {
            return Err("vertical_grid 只用于真字体：近似排版不量化".into());
        }
        None => prepared.layout_approximate(&layout),
        #[cfg(feature = "fontenv")]
        Some(f) => {
            let grid = match options.vertical_grid {
                RSL_GRID_MAC => rsword_layout_core::VerticalGrid::MacWordThreeHundredthsInch,
                _ => rsword_layout_core::VerticalGrid::None,
            };
            prepared.layout_with_fonts(f.fonts.build(), grid, &layout)
        }
        #[cfg(not(feature = "fontenv"))]
        // 不开 fontenv 时 C 侧造不出字体集；传进来的只能是坏指针，报错而不是 panic 过边界。
        Some(_) => return Err("本库没有 fontenv 特性，不支持字体集".into()),
    }
    .map_err(|e| e.report())?;
    let c = |s: &str| CString::new(s).unwrap_or_else(|_| c"诊断含 NUL 字节".into());
    let diagnostics = session
        .diagnostics()
        .iter()
        .map(|d| (c(d.code.as_str()), c(&d.message)))
        .collect();
    Ok(RslSession {
        session,
        dpi: if dpi > 0.0 { dpi } else { 96.0 },
        diagnostics,
    })
}

/// 诊断条数。空指针返回 0。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_diagnostic_count(s: *const RslSession) -> usize {
    unsafe { s.as_ref() }.map_or(0, |s| s.diagnostics.len())
}

/// 第 `index` 条诊断的码（如 `GLYPH_NOMINAL`，稳定）。越界或空指针返回 NULL。
/// 指针在会话释放前有效，不要释放它。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_diagnostic_code(s: *const RslSession, index: usize) -> *const c_char {
    unsafe { s.as_ref() }
        .and_then(|s| s.diagnostics.get(index))
        .map_or(std::ptr::null(), |(code, _)| code.as_ptr())
}

/// 第 `index` 条诊断的原文（UTF-8）。越界或空指针返回 NULL。指针在会话释放前有效。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_diagnostic_message(s: *const RslSession, index: usize) -> *const c_char {
    unsafe { s.as_ref() }
        .and_then(|s| s.diagnostics.get(index))
        .map_or(std::ptr::null(), |(_, message)| message.as_ptr())
}

/// 近似会话（不读字体、没有字形）返回 1，真字体返回 0，空指针返回 -1。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_session_is_approximate(s: *const RslSession) -> c_int {
    match unsafe { s.as_ref() } {
        Some(s) => c_int::from(s.session.is_approximate()),
        None => -1,
    }
}

/// 规范化的布局结果（JSON，schema `rsword-layout-result/1`），同输入与 Rust 侧逐字节相同。
/// 返回的字符串用 [`rsl_string_free`] 释放；空指针返回 NULL。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_layout_json(s: *const RslSession) -> *mut c_char {
    let Some(s) = (unsafe { s.as_ref() }) else {
        set_error("session 为空指针");
        return std::ptr::null_mut();
    };
    match CString::new(s.session.layout_json()) {
        Ok(json) => json.into_raw(),
        Err(_) => {
            set_error("布局结果含 NUL 字节");
            std::ptr::null_mut()
        }
    }
}

/// 释放会话。
///
/// # Safety
/// `s` 必须来自 [`rsl_session_new`] 且尚未释放，或为 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_session_free(s: *mut RslSession) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
}

/// 页数。空指针返回 0。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_page_count(s: *const RslSession) -> usize {
    match unsafe { s.as_ref() } {
        Some(s) => s.session.pages().len(),
        None => 0,
    }
}

/// 取某页的像素宽高，写入 `out_w` / `out_h`。
///
/// # Safety
/// 三个指针都必须有效可写。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_page_size(
    s: *const RslSession,
    index: usize,
    out_w: *mut f32,
    out_h: *mut f32,
) -> c_int {
    let Some(s) = (unsafe { s.as_ref() }) else {
        set_error("session 为空指针");
        return RSL_ERR_NULL;
    };
    if out_w.is_null() || out_h.is_null() {
        set_error("输出指针为空");
        return RSL_ERR_NULL;
    }
    let Some(p) = s.session.pages().get(index) else {
        set_error(format!("页号越界：{index}"));
        return RSL_ERR_RANGE;
    };
    let vp = Viewport::from_page(p.size.width, p.size.height, s.dpi);
    unsafe {
        *out_w = vp.width_px;
        *out_h = vp.height_px;
    }
    RSL_OK
}

/// 某页的片段数。
///
/// # Safety
/// `s` 必须是有效的会话指针或 NULL。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_fragment_count(s: *const RslSession, index: usize) -> usize {
    match unsafe { s.as_ref() } {
        Some(s) => s.session.pages().get(index).map_or(0, |p| p.fragments.len()),
        None => 0,
    }
}

/// 顶点与索引的字节数，供调用方按需分配缓冲。
///
/// # Safety
/// 所有指针必须有效。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_frame_sizes(
    s: *const RslSession,
    index: usize,
    out_vertex_bytes: *mut usize,
    out_index_bytes: *mut usize,
    out_batch_count: *mut usize,
) -> c_int {
    let Some(s) = (unsafe { s.as_ref() }) else {
        set_error("session 为空指针");
        return RSL_ERR_NULL;
    };
    if out_vertex_bytes.is_null() || out_index_bytes.is_null() || out_batch_count.is_null() {
        set_error("输出指针为空");
        return RSL_ERR_NULL;
    }
    let Some(page) = s.session.paint_page(index) else {
        set_error(format!("页号越界：{index}"));
        return RSL_ERR_RANGE;
    };
    // 先转成矢量绘制指令，再按 viewport 栅格化——分辨率只在后一步进入。
    // 没有 GlyphSource：文字批次为空，只产出矩形类几何。
    let vp = Viewport::from_page(page.width, page.height, s.dpi);
    let f = build_page(&page, &vp, None);
    unsafe {
        *out_vertex_bytes = f.vertex_bytes();
        *out_index_bytes = f.index_bytes();
        *out_batch_count = f.batches.len();
    }
    RSL_OK
}

/// 一个绘制批次的 C 视图。
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct RslBatch {
    /// 0 = Solid，1 = Glyph，2 = Image。
    pub kind: c_int,
    pub index_offset: u32,
    pub index_count: u32,
}

/// 把某页的顶点、索引、批次拷进调用方的缓冲。
///
/// 缓冲不够时返回 [`RSL_ERR_BUFFER`]，所需大小可先用 [`rsl_frame_sizes`] 问。
///
/// # Safety
/// 三个缓冲指针必须分别指向至少 `*_cap` 字节/元素的可写内存。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_frame_copy(
    s: *const RslSession,
    index: usize,
    vertices: *mut c_void,
    vertices_cap: usize,
    indices: *mut c_void,
    indices_cap: usize,
    batches: *mut RslBatch,
    batches_cap: usize,
) -> c_int {
    let Some(s) = (unsafe { s.as_ref() }) else {
        set_error("session 为空指针");
        return RSL_ERR_NULL;
    };
    let Some(page) = s.session.paint_page(index) else {
        set_error(format!("页号越界：{index}"));
        return RSL_ERR_RANGE;
    };
    let vp = Viewport::from_page(page.width, page.height, s.dpi);
    let f = build_page(&page, &vp, None);

    if vertices_cap < f.vertex_bytes() || indices_cap < f.index_bytes()
        || batches_cap < f.batches.len()
    {
        set_error("缓冲区容量不足，请先用 rsl_frame_sizes 查询");
        return RSL_ERR_BUFFER;
    }
    if (vertices.is_null() && f.vertex_bytes() > 0)
        || (indices.is_null() && f.index_bytes() > 0)
        || (batches.is_null() && !f.batches.is_empty())
    {
        set_error("输出缓冲为空指针");
        return RSL_ERR_NULL;
    }

    copy_frame(&f, vertices, indices, batches);
    RSL_OK
}

/// 实际拷贝。`Vertex` 与 `u32` 都是紧密 POD，可整块 memcpy。
fn copy_frame(f: &Frame, vertices: *mut c_void, indices: *mut c_void, batches: *mut RslBatch) {
    use rsword_layout_gpu::batch::BatchKind;

    if !f.vertices.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                f.vertices.as_ptr().cast::<u8>(),
                vertices.cast::<u8>(),
                f.vertex_bytes(),
            );
        }
    }
    if !f.indices.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                f.indices.as_ptr().cast::<u8>(),
                indices.cast::<u8>(),
                f.index_bytes(),
            );
        }
    }
    for (i, b) in f.batches.iter().enumerate() {
        let kind = match b.kind {
            BatchKind::Solid => 0,
            BatchKind::Glyph => 1,
            BatchKind::Image => 2,
        };
        unsafe {
            *batches.add(i) = RslBatch {
                kind,
                index_offset: b.index_offset,
                index_count: b.index_count,
            };
        }
    }
}

/// 正交投影矩阵（列主序 16 个 f32），可直接作为 GPU uniform 上传。
///
/// # Safety
/// `out` 必须指向至少 16 个 f32 的可写内存。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_page_ortho(
    s: *const RslSession,
    index: usize,
    out: *mut f32,
) -> c_int {
    let Some(s) = (unsafe { s.as_ref() }) else {
        set_error("session 为空指针");
        return RSL_ERR_NULL;
    };
    if out.is_null() {
        set_error("输出指针为空");
        return RSL_ERR_NULL;
    }
    let Some(p) = s.session.pages().get(index) else {
        set_error(format!("页号越界：{index}"));
        return RSL_ERR_RANGE;
    };
    let m = Viewport::from_page(p.size.width, p.size.height, s.dpi).ortho();
    unsafe { std::ptr::copy_nonoverlapping(m.as_ptr(), out, 16) };
    RSL_OK
}

/// 版本串，静态生命周期，不要释放。
#[unsafe(no_mangle)]
pub extern "C" fn rsl_version() -> *const c_char {
    c"0.1.0".as_ptr()
}

/// 供 C 侧自查的常量：一个顶点多少字节。
#[unsafe(no_mangle)]
pub extern "C" fn rsl_vertex_stride() -> usize {
    rsword_layout_gpu::vertex::Vertex::STRIDE
}

/// 让 `CStr` 参数不至于未使用；保留给将来按名字取配置。
#[allow(dead_code)]
fn _unused(_: &CStr) {}

#[cfg(test)]
mod tests {
    use super::build_session;
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
    fn session_goes_through_load_document() {
        // 同一份 docx 换个入口不该排得不一样：这里的字符间距要与 layout-trace 一样到位。
        let session = build_session(&double_rpr_docx(), 96.0, None, &Default::default()).unwrap();
        let spacing: Vec<_> = session.session.pages()[0]
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
