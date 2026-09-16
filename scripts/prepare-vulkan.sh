#!/bin/sh
# 检查 Vulkan 后端（feature `vulkan`）的构建与运行前提。
#
# 注意 crates/vulkan 本身**不链接 Vulkan**，它只提供顶点输入描述与 NDC 约定，
# 所以 `cargo build --features vulkan` 不需要任何系统库。下面检查的是
# **调用方真正跑起来**时需要的东西：loader、驱动、以及着色器编译器。

. "$(dirname "$0")/_common.sh"

say "Vulkan 后端环境检查"

head1 "Rust 侧（构建 crates/vulkan 必需）"
need_cmd cargo "装 Rust：https://rustup.rs"

head1 "Vulkan loader 与驱动（运行时必需，构建不需要）"
if have vulkaninfo; then
  ok "vulkaninfo"
  _dev=$(vulkaninfo --summary 2>/dev/null | sed -n 's/.*deviceName *= *//p' | head -3)
  if [ -n "$_dev" ]; then
    printf '%s' "$_dev" | while IFS= read -r d; do hint "设备：$d"; done
  else
    warn "vulkaninfo 跑不出设备，可能没有可用驱动"
  fi
else
  bad "vulkaninfo 缺失（vulkan-tools）"
  hint "$(pkg_hint 'vulkan-tools libvulkan-dev' 'vulkan-tools vulkan-loader-devel' 'vulkan-tools vulkan-headers')"
fi

if [ -f /usr/include/vulkan/vulkan.h ]; then
  ok "vulkan.h"
else
  bad "vulkan 头文件缺失"
  hint "$(pkg_hint 'libvulkan-dev' 'vulkan-loader-devel' 'vulkan-headers')"
fi

# ICD 决定实际用哪个驱动；没有 ICD 时 loader 能装上但跑不出设备。
head1 "ICD（驱动清单）"
_icd=$(ls /usr/share/vulkan/icd.d/*.json 2>/dev/null | wc -l)
if [ "$_icd" -gt 0 ]; then
  ok "$_icd 个 ICD"
  ls /usr/share/vulkan/icd.d/*.json 2>/dev/null | while IFS= read -r f; do hint "$(basename "$f")"; done
else
  bad "没有 ICD，Vulkan 找不到任何设备"
  hint "Intel/AMD 开源驱动：$(pkg_hint 'mesa-vulkan-drivers' 'mesa-vulkan-drivers' 'vulkan-intel vulkan-radeon')"
  hint "NVIDIA：装官方驱动（含 nvidia_icd.json）"
  hint "无显卡时可用软件光栅：$(pkg_hint 'mesa-vulkan-drivers' 'mesa-vulkan-drivers' 'vulkan-swrast')，并设 VK_ICD_FILENAMES 指向 lvp_icd"
fi

head1 "着色器编译器（把 GLSL 编成 SPIR-V）"
if have glslc || have glslangValidator; then
  have glslc && ok "glslc" || ok "glslangValidator"
else
  bad "没有 SPIR-V 编译器"
  hint "$(pkg_hint 'glslc' 'glslc' 'shaderc')"
  hint "或 glslang：$(pkg_hint 'glslang-tools' 'glslang' 'glslang')"
fi

head1 "验证构建"
hint "cargo build --features vulkan"

finish
