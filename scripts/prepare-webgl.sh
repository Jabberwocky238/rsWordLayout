#!/bin/sh
# 检查 WebGL 后端（feature `webgl`）的构建前提，并可选地装好 Rust 侧工具。
#
# 与 vulkan / opengl 不同：crates/webgl **有实际实现**，编译目标是 wasm32，
# 所以这里的依赖是真正的构建依赖，不是运行时依赖。
#
# 带 --install 时会执行 `rustup target add` 与 `cargo install wasm-bindgen-cli`：
# 两者都装进用户自己的 ~/.rustup 与 ~/.cargo，不需要 root，所以脚本可以代劳。

. "$(dirname "$0")/_common.sh"

DO_INSTALL=0
[ "${1:-}" = "--install" ] && DO_INSTALL=1

say "WebGL（wasm）后端环境检查"
[ "$DO_INSTALL" -eq 1 ] && say "${DIM}--install：会安装 wasm32 target 与 wasm-bindgen-cli${RST}"

head1 "Rust 工具链"
need_cmd cargo "装 Rust：https://rustup.rs"
need_cmd rustup "需要 rustup 来加 wasm32 target"

head1 "wasm32 target"
if rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  ok "wasm32-unknown-unknown"
elif [ "$DO_INSTALL" -eq 1 ]; then
  say "  安装 wasm32-unknown-unknown ..."
  rustup target add wasm32-unknown-unknown && ok "已安装"
else
  bad "wasm32-unknown-unknown 未安装"
  hint "rustup target add wasm32-unknown-unknown"
  hint "或重跑：$0 --install"
fi

head1 "wasm-bindgen-cli"
# 版本必须与 Cargo.lock 里的 wasm-bindgen 一致，否则生成的胶水代码对不上。
WANT=$(sed -n '/^name = "wasm-bindgen"$/,/^version/ s/^version = "\(.*\)"/\1/p' \
       "$(dirname "$0")/../Cargo.lock" 2>/dev/null | head -1)
if have wasm-bindgen; then
  GOT=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
  if [ -n "$WANT" ] && [ "$GOT" != "$WANT" ]; then
    bad "wasm-bindgen 版本不匹配：装的是 $GOT，Cargo.lock 要 $WANT"
    hint "cargo install -f wasm-bindgen-cli --version $WANT"
  else
    ok "wasm-bindgen ${GOT:-?}"
  fi
elif [ "$DO_INSTALL" -eq 1 ] && [ -n "$WANT" ]; then
  say "  安装 wasm-bindgen-cli $WANT（要编几分钟）..."
  cargo install -f wasm-bindgen-cli --version "$WANT" && ok "已安装"
else
  bad "wasm-bindgen 缺失"
  [ -n "$WANT" ] && hint "cargo install wasm-bindgen-cli --version $WANT" \
                 || hint "cargo install wasm-bindgen-cli"
  hint "版本必须与 Cargo.lock 对齐，否则胶水代码与 .wasm 不兼容"
fi

head1 "构建与打包"
hint "cargo build -p rsword-layout-webgl --target wasm32-unknown-unknown --release"
hint "wasm-bindgen target/wasm32-unknown-unknown/release/rsword_layout_webgl.wasm \\"
hint "  --out-dir pkg --target web"

head1 "本地预览（wasm 需要 HTTP，file:// 加载不了）"
if have python3; then ok "python3 -m http.server 8000"
else warn "没有 python3；用任意静态服务器起一个即可"
fi

finish
