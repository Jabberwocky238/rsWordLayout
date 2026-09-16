//! OpenGL 后端描述（feature `opengl`）。
//!
//! 与 `vulkan` 模块同理：不链接 GL、不引入 `glow` / `gl` 依赖，只给接上去所需的描述。
//! 提供可直接编译的 GLSL 着色器源码——文字与色块共用一套，靠图集的纯色纹素统一。

use rsword_layout_core::gpu::vertex::Vertex;

pub const STRIDE: i32 = Vertex::STRIDE as i32;
pub const OFFSET_POS: usize = Vertex::OFFSET_POS;
pub const OFFSET_UV: usize = Vertex::OFFSET_UV;
pub const OFFSET_COLOR: usize = Vertex::OFFSET_COLOR;

/// 顶点着色器（GLSL 3.3 core）。
pub const VERTEX_SHADER: &str = r#"#version 330 core
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

/// 片段着色器。字形图集是单通道 alpha，纯色批次因为采样到不透明白纹素，
/// 走同一条路径也得到正确结果——所以不需要 uniform 分支。
pub const FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_atlas;
out vec4 frag;
void main() {
    float a = texture(u_atlas, v_uv).r;
    frag = vec4(v_color.rgb, v_color.a * a);
}
"#;
