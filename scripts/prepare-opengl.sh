#!/bin/sh
# 检查 OpenGL 后端（feature `opengl`）的构建与运行前提。
#
# 同 Vulkan：crates/opengl 只提供顶点布局与 GLSL 源码，不链接 GL，
# `cargo build --features opengl` 无系统依赖。下面查的是调用方运行时需要的。

. "$(dirname "$0")/_common.sh"

say "OpenGL 后端环境检查"

head1 "Rust 侧"
need_cmd cargo "装 Rust：https://rustup.rs"

head1 "GL 运行时与头文件"
if have glxinfo; then
  ok "glxinfo"
  _ver=$(glxinfo 2>/dev/null | sed -n 's/^OpenGL core profile version string: *//p' | head -1)
  [ -z "$_ver" ] && _ver=$(glxinfo 2>/dev/null | sed -n 's/^OpenGL version string: *//p' | head -1)
  if [ -n "$_ver" ]; then
    hint "版本：$_ver"
    # 着色器要求 GLSL 330 core，即 GL 3.3+。
    _maj=$(printf '%s' "$_ver" | sed -n 's/^\([0-9]*\)\..*/\1/p')
    _min=$(printf '%s' "$_ver" | sed -n 's/^[0-9]*\.\([0-9]*\).*/\1/p')
    if [ -n "$_maj" ] && { [ "$_maj" -gt 3 ] || { [ "$_maj" -eq 3 ] && [ "${_min:-0}" -ge 3 ]; }; }; then
      ok "满足 GL 3.3（着色器用 GLSL 330 core）"
    else
      bad "GL 版本低于 3.3，crates/opengl 的着色器编不过"
    fi
  else
    warn "读不到 GL 版本（无显示环境时正常）"
  fi
else
  bad "glxinfo 缺失（mesa-utils）"
  hint "$(pkg_hint 'mesa-utils' 'glx-utils' 'mesa-utils')"
fi

if [ -f /usr/include/GL/gl.h ]; then
  ok "GL/gl.h"
else
  bad "GL 头文件缺失"
  hint "$(pkg_hint 'libgl1-mesa-dev' 'mesa-libGL-devel' 'mesa')"
fi

head1 "窗口系统（调用方建上下文时需要）"
if [ -n "${WAYLAND_DISPLAY:-}" ]; then ok "Wayland：$WAYLAND_DISPLAY"
elif [ -n "${DISPLAY:-}" ]; then ok "X11：$DISPLAY"
else warn "无 DISPLAY/WAYLAND_DISPLAY —— 无头环境需要 EGL 离屏或 Xvfb"
fi

head1 "验证构建"
hint "cargo build --features opengl"

finish
