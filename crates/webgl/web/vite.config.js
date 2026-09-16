import { defineConfig } from 'vite'
import { fileURLToPath } from 'node:url'

// pkg/ 是 wasm-bindgen 的产物，在 web/ 的上一级；用别名引，避免相对路径散落在源码里。
const pkg = fileURLToPath(new URL('../pkg', import.meta.url))

export default defineConfig({
  resolve: {
    alias: { '@pkg': pkg },
  },
  server: {
    port: 5173,
    open: true,
    fs: {
      // 默认只允许访问项目根；pkg/ 在根之外，必须显式放行。
      allow: ['..'],
    },
  },
  // .wasm 走 Vite 的 asset 流程，dev 与 build 都能正确解析 URL。
  assetsInclude: ['**/*.wasm'],
  optimizeDeps: {
    // wasm-bindgen 的胶水用了顶层 await 与 import.meta.url，预打包会破坏它。
    exclude: ['@pkg'],
  },
})
