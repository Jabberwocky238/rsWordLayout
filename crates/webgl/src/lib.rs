//! WebGL2 后端（wasm 目标）。
//!
//! 与 `vulkan` / `opengl` 两个 crate 不同，本 crate **有实际实现**：WebGL 的上下文就在
//! 浏览器里，没有「交给调用方去接」的余地，所以这里直接编译着色器、建缓冲、发绘制命令。
//!
//! 编译：
//!
//! ```sh
//! cargo build -p rsword-layout-webgl --target wasm32-unknown-unknown --release
//! wasm-bindgen target/wasm32-unknown-unknown/release/rsword_layout_webgl.wasm --out-dir pkg --target web
//! ```
//!
//! 顶点数据来自 [`rsword_layout_core::gpu::Frame`]，与另外两个后端同源——
//! 同一份布局产物在三个 API 上画出的几何完全一致。
//!
//! # 为什么这里再导出一次 `LayoutSession`
//!
//! 布局会话定义在 `rsword-layout-wasm`，但**两个 wasm 模块各有独立的线性内存**，
//! JS 没法把 A 模块造的对象传进 B 模块的方法。所以给 JS 用的那一个 `.wasm`
//! 必须同时含有会话与渲染器——本 crate 负责把两者装进同一个模块，
//! 打包时只对 `rsword_layout_webgl.wasm` 跑 wasm-bindgen。




/// 顶点步长（字节），与 `rsword_layout_gpu::vertex::Vertex` 一致。
/// 供宿主设置 `glVertexAttribPointer` 时使用。
pub const VERTEX_STRIDE: usize = rsword_layout_gpu::vertex::Vertex::STRIDE;

/// 顶点着色器（GLSL ES 3.0）。原生构建下也导出，供宿主自行接 GL 时参考。
pub const VS: &str = r#"#version 300 es
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;
uniform mat4 u_proj;
out vec2 v_uv;
out vec4 v_color;
void main() {
    v_uv = a_uv;
    v_color = a_color;
    gl_Position = u_proj * vec4(a_pos, 0.0, 1.0);
}
"#;

/// 片段着色器。字形图集单通道；纯色批次采样到不透明白纹素，共用同一路径。
pub const FS: &str = r#"#version 300 es
precision mediump float;
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_atlas;
out vec4 frag;
void main() {
    float a = texture(u_atlas, v_uv).r;
    frag = vec4(v_color.rgb, v_color.a * a);
}
"#;

/// 浏览器实现。只在 wasm32 下编译——`web_sys` 在原生目标上不存在，
/// 而本 crate 在原生构建时仍需可用（`Vertex` 常量、着色器源码供宿主参考）。
#[cfg(target_arch = "wasm32")]
pub mod fonts;

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{FS, VS};
    use rsword_layout_gpu::batch::BatchKind;
    use rsword_layout_gpu::vertex::Vertex;
    use rsword_layout_gpu::{Frame, Viewport};
    use wasm_bindgen::prelude::*;
    use web_sys::{
        WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture,
        WebGlUniformLocation,
    };

    fn compile(gl: &Gl, kind: u32, src: &str) -> Result<WebGlShader, String> {
        let sh = gl.create_shader(kind).ok_or("无法创建 shader")?;
        gl.shader_source(&sh, src);
        gl.compile_shader(&sh);
        if gl
            .get_shader_parameter(&sh, Gl::COMPILE_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            Ok(sh)
        } else {
            Err(gl.get_shader_info_log(&sh).unwrap_or_else(|| "shader 编译失败".into()))
        }
    }

    fn link(gl: &Gl, vs: &WebGlShader, fs: &WebGlShader) -> Result<WebGlProgram, String> {
        let p = gl.create_program().ok_or("无法创建 program")?;
        gl.attach_shader(&p, vs);
        gl.attach_shader(&p, fs);
        gl.link_program(&p);
        if gl
            .get_program_parameter(&p, Gl::LINK_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            Ok(p)
        } else {
            Err(gl.get_program_info_log(&p).unwrap_or_else(|| "program 链接失败".into()))
        }
    }

    /// WebGL2 渲染器：持有 program 与缓冲，可反复提交不同帧。
    #[wasm_bindgen]
    pub struct WebGlRenderer {
        gl: Gl,
        program: WebGlProgram,
        vbo: WebGlBuffer,
        ibo: WebGlBuffer,
        u_proj: Option<WebGlUniformLocation>,
        /// 字形图集。左上角第一个纹素必须是不透明白，纯色批次靠它复用同一套着色器。
        atlas: WebGlTexture,
        /// 已分配的图集尺寸。与来料不符时要重新分配而不是子区域更新。
        atlas_size: std::cell::Cell<(u32, u32)>,
    }

    #[wasm_bindgen]
    impl WebGlRenderer {
        /// 从一个 canvas 元素 id 建渲染器。
        #[wasm_bindgen(constructor)]
        pub fn new(canvas_id: &str) -> Result<WebGlRenderer, JsValue> {
            let win = web_sys::window().ok_or("无 window")?;
            let doc = win.document().ok_or("无 document")?;
            let el = doc
                .get_element_by_id(canvas_id)
                .ok_or_else(|| JsValue::from_str(&format!("找不到 canvas #{canvas_id}")))?;
            let canvas: web_sys::HtmlCanvasElement = el.dyn_into()?;
            let gl: Gl = canvas
                .get_context("webgl2")?
                .ok_or("浏览器不支持 WebGL2")?
                .dyn_into()?;

            let vs = compile(&gl, Gl::VERTEX_SHADER, VS).map_err(|e| JsValue::from_str(&e))?;
            let fs = compile(&gl, Gl::FRAGMENT_SHADER, FS).map_err(|e| JsValue::from_str(&e))?;
            let program = link(&gl, &vs, &fs).map_err(|e| JsValue::from_str(&e))?;

            let vbo = gl.create_buffer().ok_or("无法创建 VBO")?;
            let ibo = gl.create_buffer().ok_or("无法创建 IBO")?;
            let u_proj = gl.get_uniform_location(&program, "u_proj");

            // 文字是预乘 alpha 的图集，用标准 alpha 混合。
            gl.enable(Gl::BLEND);
            gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

            // 图集纹理。先建成 1×1 的不透明白：纯色批次采样左上角那个纹素，
            // 在字形图集上传之前也能正确画出矩形类片段。
            let atlas = gl.create_texture().ok_or("无法创建图集纹理")?;
            gl.bind_texture(Gl::TEXTURE_2D, Some(&atlas));
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
            // R8 单通道：着色器取 .r 当覆盖率。
            gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                Gl::R8 as i32,
                1,
                1,
                0,
                Gl::RED,
                Gl::UNSIGNED_BYTE,
                Some(&[255u8]),
            )?;

            Ok(WebGlRenderer {
                gl,
                program,
                vbo,
                ibo,
                u_proj,
                atlas,
                atlas_size: std::cell::Cell::new((1, 1)),
            })
        }

        /// 清屏。
        pub fn clear(&self, r: f32, g: f32, b: f32) {
            self.gl.clear_color(r, g, b, 1.0);
            self.gl.clear(Gl::COLOR_BUFFER_BIT);
        }
    }

    /// 图集像素上传。**不导出给 JS**：参数含元组，wasm-bindgen 传不了；
    /// 而它本就只该由同模块内的绘制逻辑调用，JS 不必直接碰图集像素。
    impl WebGlRenderer {
        /// 上传字形图集像素。
        ///
        /// `dirty` 为 `None` 表示无改动，直接跳过；尺寸变化或发生过淘汰时要整幅重传
        /// （淘汰会让存活字形上移，旧 UV 全部失效）。
        pub fn upload_atlas(
            &self,
            pixels: &[u8],
            width: u32,
            height: u32,
            dirty: Option<(u32, u32, u32, u32)>,
        ) -> Result<(), String> {
            let Some((dx, dy, dw, dh)) = dirty else {
                return Ok(());
            };
            if dw == 0 || dh == 0 {
                return Ok(());
            }
            let expect = (width as usize) * (height as usize);
            if pixels.len() < expect {
                return Err(format!(
                    "图集像素不足：给了 {} 字节，{}×{} 需要 {}",
                    pixels.len(),
                    width,
                    height,
                    expect
                ));
            }

            let gl = &self.gl;
            gl.active_texture(Gl::TEXTURE0);
            gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas));
            gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 1);

            let full = self.atlas_size.get() != (width, height)
                || (dx == 0 && dy == 0 && dw == width && dh == height);

            if full {
                gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                    Gl::TEXTURE_2D,
                    0,
                    Gl::R8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    Gl::RED,
                    Gl::UNSIGNED_BYTE,
                    Some(pixels),
                )
                .map_err(|e| format!("图集整幅上传失败：{e:?}"))?;
                self.atlas_size.set((width, height));
                return Ok(());
            }

            // 子区域更新：把脏矩形的行逐条抠出来，避免传整幅。
            let mut sub = Vec::with_capacity((dw as usize) * (dh as usize));
            for row in 0..dh {
                let start = ((dy + row) as usize) * (width as usize) + (dx as usize);
                sub.extend_from_slice(&pixels[start..start + dw as usize]);
            }
            gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                dx as i32,
                dy as i32,
                dw as i32,
                dh as i32,
                Gl::RED,
                Gl::UNSIGNED_BYTE,
                Some(&sub),
            )
            .map_err(|e| format!("图集子区域上传失败：{e:?}"))?;
            Ok(())
        }
    }

    impl WebGlRenderer {
        /// 提交一帧。`Frame` 来自 `rsword_layout_core::gpu::build_page`。
        ///
        /// 顶点以 `f32` 视图上传：[`Vertex`] 是 `#[repr(C)]` 的紧密 POD，
        /// 整块重解释为 `f32` 切片是安全的（布局由 core 的 `repr_c` 测试钉死）。
        pub fn draw(&self, frame: &Frame, vp: &Viewport) -> Result<(), String> {
            if frame.is_empty() {
                return Ok(());
            }
            let gl = &self.gl;
            gl.viewport(0, 0, vp.width_px as i32, vp.height_px as i32);
            gl.use_program(Some(&self.program));

            let proj = vp.ortho();
            gl.uniform_matrix4fv_with_f32_array(self.u_proj.as_ref(), false, &proj);

            gl.active_texture(Gl::TEXTURE0);
            gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas));

            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vbo));
            gl.buffer_data_with_u8_array(
                Gl::ARRAY_BUFFER,
                &vertices_to_bytes(&frame.vertices),
                Gl::DYNAMIC_DRAW,
            );

            let stride = Vertex::STRIDE as i32;
            for (loc, offset, size) in [
                (0, Vertex::OFFSET_POS, 2),
                (1, Vertex::OFFSET_UV, 2),
                (2, Vertex::OFFSET_COLOR, 4),
            ] {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_with_i32(loc, size, Gl::FLOAT, false, stride, offset as i32);
            }

            gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
            gl.buffer_data_with_u8_array(
                Gl::ELEMENT_ARRAY_BUFFER,
                &indices_to_bytes(&frame.indices),
                Gl::DYNAMIC_DRAW,
            );

            for batch in &frame.batches {
                // 纹理绑定由调用方在 batch 之间处理；Image 批次需要各自的纹理，
                // 这里跳过而不是画错——缺纹理时画出来的会是整块纯色。
                if batch.kind == BatchKind::Image {
                    continue;
                }
                gl.draw_elements_with_i32(
                    Gl::TRIANGLES,
                    batch.index_count as i32,
                    Gl::UNSIGNED_INT,
                    (batch.index_offset as i32) * 4,
                );
            }
            Ok(())
        }
    }

    /// 顶点切片 → 字节。
    ///
    /// 本 crate `unsafe_code = "forbid"`，不能用 `transmute` / `align_to` 做零拷贝重解释，
    /// 所以显式序列化。每页几千个顶点，拷贝开销远小于一次 GPU 上传，换来整个 crate 无 `unsafe`。
    fn vertices_to_bytes(v: &[Vertex]) -> Vec<u8> {
        let mut out = Vec::with_capacity(std::mem::size_of_val(v));
        for x in v {
            for f in [x.x, x.y, x.u, x.v, x.r, x.g, x.b, x.a] {
                out.extend_from_slice(&f.to_ne_bytes());
            }
        }
        out
    }

    fn indices_to_bytes(v: &[u32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(std::mem::size_of_val(v));
        for i in v {
            out.extend_from_slice(&i.to_ne_bytes());
        }
        out
    }


}

#[cfg(target_arch = "wasm32")]
pub use browser::WebGlRenderer;

// ---------------------------------------------------------------------------
// 给 JS 的统一入口：会话与渲染器必须在同一个 wasm 模块里（见文件头说明）
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod js_api {
    use rsword_layout_gpu::{GlyphAtlas, Viewport};
    use rsword_layout_wasm::LayoutSession as CoreSession;
    use wasm_bindgen::prelude::*;

    use crate::browser::WebGlRenderer;
    use crate::fonts::RegistrySource;
    use rsword_layout_core::font::FontRegistry;

    /// 字形图集边长。2048² 的单通道图集是 4MB 纹素，能放约四千个 32px 字形——
    /// 一页文档通常几百个不同字形，足够，满了会按货架淘汰最久未用的。
    const ATLAS_SIDE: u32 = 2048;

    /// 字体集。与 [`LayoutSession`] 分开，因为字体的生命周期更长：
    /// 换一份 docx 不该重新下载与解析字体。
    #[wasm_bindgen]
    pub struct FontSet {
        registry: FontRegistry,
        atlas: GlyphAtlas,
    }

    #[wasm_bindgen]
    impl FontSet {
        #[wasm_bindgen(constructor)]
        pub fn new() -> FontSet {
            FontSet {
                registry: FontRegistry::new(),
                atlas: GlyphAtlas::new(ATLAS_SIDE, ATLAS_SIDE),
            }
        }

        /// 注册一份字体（TTF/OTF/TTC 的原始字节）。返回它的内容哈希。
        ///
        /// `index` 是 TTC 里的子字体序号，普通 TTF/OTF 传 0。
        pub fn add_font(&mut self, bytes: &[u8], index: u32) -> Result<String, JsValue> {
            self.registry
                .add(bytes.to_vec(), index)
                .map_err(|e| JsValue::from_str(e))
        }

        /// 字体环境指纹：字体集变了它就变。
        #[wasm_bindgen(getter)]
        pub fn fingerprint(&self) -> Option<String> {
            self.registry.fingerprint().map(str::to_owned)
        }

        #[wasm_bindgen(getter)]
        pub fn is_empty(&self) -> bool {
            self.registry.is_empty()
        }

        /// 当前图集里缓存了多少字形。
        #[wasm_bindgen(getter)]
        pub fn glyph_count(&self) -> usize {
            self.atlas.len()
        }
    }

    impl Default for FontSet {
        fn default() -> Self {
            Self::new()
        }
    }

    /// 一次布局会话：docx 进来，排好版，按页交给渲染器画。
    #[wasm_bindgen]
    pub struct LayoutSession {
        inner: CoreSession,
    }

    #[wasm_bindgen]
    impl LayoutSession {
        /// 解析并排版。`dpi`：96 = CSS 像素，192 = 2x HiDPI；传 0 或负数按 96 处理。
        #[wasm_bindgen(constructor)]
        pub fn new(docx: &[u8], dpi: f32) -> Result<LayoutSession, JsValue> {
            CoreSession::build(docx, dpi)
                .map(|inner| LayoutSession { inner })
                .map_err(|e| JsValue::from_str(&e))
        }

        #[wasm_bindgen(getter)]
        pub fn page_count(&self) -> usize {
            self.inner.page_count()
        }

        /// 某页在指定 DPI 下的像素宽高 `[w, h]`；越界返回空数组。
        ///
        /// 会话本身只存 twips（与分辨率无关），像素在这里才算出来——
        /// 所以同一份布局可以用不同 DPI 反复取尺寸，不必重排。
        pub fn page_size(&self, index: usize, dpi: f32) -> Vec<f32> {
            let tw = self.inner.page_size_twips(index);
            if tw.len() != 2 {
                return Vec::new();
            }
            let vp = Viewport::from_page(tw[0], tw[1], if dpi > 0.0 { dpi } else { 96.0 });
            vec![vp.width_px, vp.height_px]
        }

        /// 某页的片段数，用于自查布局是否产出了内容。
        pub fn fragment_count(&self, index: usize) -> usize {
            self.inner.fragment_count(index)
        }

        /// 比较器记录摘要 `[行数, 带源区间行数, 段落标记数, 终止符应产出字形]`。
        pub fn oracle_summary(&self, index: usize) -> Vec<u32> {
            self.inner.oracle_summary(index)
        }

        /// 把某页画到渲染器上。
        ///
        /// 分三步，顺序不能换：
        ///   1. core 产出**矢量**绘制指令（twips，整形已做，无像素）；
        ///   2. gpu 按 `Viewport` 栅格化成顶点，过程中按需往图集填字形；
        ///   3. 图集有脏区域就先传纹理，再绘制。
        ///
        /// DPI 只在第 2 步进入——这正是重构后 core 与设备解耦的体现。
        pub fn render_page(
            &self,
            r: &WebGlRenderer,
            fonts: &mut FontSet,
            index: usize,
            dpi: f32,
        ) -> Result<(), JsValue> {
            let dpi = if dpi > 0.0 { dpi } else { 96.0 };

            // 1. 矢量指令。整形要借 registry，先取出 face 列表供 paint 层回查。
            let faces = fonts.registry.face_ids();
            let page = self
                .inner
                .paint(index, Some(&fonts.registry), &faces)
                .ok_or_else(|| JsValue::from_str(&format!("页号越界：{index}")))?;

            // 2. 栅格化。建帧过程会按需填图集，所以必须先建帧再上传纹理。
            let vp = Viewport::from_page(page.width, page.height, dpi);
            let FontSet { registry, atlas } = fonts;
            let source = RegistrySource {
                registry: std::cell::RefCell::new(registry),
                atlas: std::cell::RefCell::new(atlas),
            };
            let frame = rsword_layout_gpu::build_page(&page, &vp, Some(&source));
            drop(source);

            let dirty = fonts.atlas.dirty().map(|d| (d.x, d.y, d.width, d.height));
            r.upload_atlas(
                fonts.atlas.pixels(),
                fonts.atlas.width(),
                fonts.atlas.height(),
                dirty,
            )
            .map_err(|e| JsValue::from_str(&e))?;
            fonts.atlas.clear_dirty();

            r.draw(&frame, &vp).map_err(|e| JsValue::from_str(&e))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use js_api::LayoutSession;
