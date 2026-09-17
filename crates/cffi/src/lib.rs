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

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use rsword_layout_core::fragment::LaidOutDocument;
use rsword_layout_core::paint::paint_page;
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
    doc: LaidOutDocument,
    dpi: f32,
}

/// 解析并排版一份 docx。
///
/// 失败返回 NULL，原因用 [`rsl_last_error`] 取。
///
/// # Safety
/// `data` 必须指向至少 `len` 字节的可读内存。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsl_session_new(
    data: *const u8,
    len: usize,
    dpi: f32,
) -> *mut RslSession {
    if data.is_null() {
        set_error("data 为空指针");
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    match build_session(bytes, dpi) {
        Ok(s) => Box::into_raw(Box::new(s)),
        Err(e) => {
            set_error(e);
            std::ptr::null_mut()
        }
    }
}

fn build_session(docx: &[u8], dpi: f32) -> Result<RslSession, String> {
    use rsword::bind::native::SessionTable;
    use rsword_layout_core::bridge::paras_from_document;
    use rsword_layout_core::engine::{Engine, PageSetup};
    use rsword_layout_core::simple_metrics::SimpleMetrics;

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
    Ok(RslSession {
        doc: engine.layout(&paras),
        dpi: if dpi > 0.0 { dpi } else { 96.0 },
    })
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
        Some(s) => s.doc.page_count(),
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
    let Some(p) = s.doc.pages.get(index) else {
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
        Some(s) => s.doc.pages.get(index).map_or(0, |p| p.fragments.len()),
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
    let Some(page) = s.doc.pages.get(index) else {
        set_error(format!("页号越界：{index}"));
        return RSL_ERR_RANGE;
    };
    // 先转成矢量绘制指令，再按 viewport 栅格化——分辨率只在后一步进入。
    // 没有 GlyphSource：文字批次为空，只产出矩形类几何。
    let vp = Viewport::from_page(page.size.width, page.size.height, s.dpi);
    let f = build_page(&paint_page(page, None, &[]), &vp, None);
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
    let Some(page) = s.doc.pages.get(index) else {
        set_error(format!("页号越界：{index}"));
        return RSL_ERR_RANGE;
    };
    let vp = Viewport::from_page(page.size.width, page.size.height, s.dpi);
    let f = build_page(&paint_page(page, None, &[]), &vp, None);

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
    let Some(p) = s.doc.pages.get(index) else {
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
