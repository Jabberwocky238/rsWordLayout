# `rsword-layout-webgl` · WebGL2 后端 / npm `@jabberwocky238/rs-word-layout`

rsWordLayout 的 wasm 入口，也是 GitHub Packages 上 npm 包 `@jabberwocky238/rs-word-layout`
的全部来源：wasm + wasm-bindgen 胶水（`--target web`）+ 类型声明，不含原生后端。
布局会话与 WebGL2 渲染器在同一个 wasm 模块里（两个模块的线性内存互不相通，JS 无法跨模块传对象）。

```js
import init, { FontSet, LayoutSession, WebGlRenderer } from '@jabberwocky238/rs-word-layout'

await init() // 浏览器 / 打包器：按 import.meta.url 取同目录的 rsword_layout_webgl_bg.wasm
// Node：import { initSync } …; initSync({ module: fs.readFileSync(wasm 路径) })

const session = new LayoutSession(docxBytes, 96) // 96 = CSS 像素，192 = 2x HiDPI
const fonts = new FontSet()
fonts.add_font(fontBytes, 0) // TTC 用序号选字体；字体由宿主提供，包里不带
const renderer = new WebGlRenderer('canvas-id')
session.render_page(renderer, fonts, 0, 96)
```

## 面

- `LayoutSession`：`new(docx, dpi)` 解析并排版；`page_count`（getter）、`page_size(index, dpi)`（像素
  `[w, h]`）、`fragment_count(index)`、`oracle_summary(index)`、`render_page(renderer, fonts, index, dpi)`。
- `FontSet`：`add_font(bytes, index)`；getter `fingerprint`、`is_empty`、`glyph_count`。
- `WebGlRenderer`：`new(canvasId)`、`clear(r, g, b)`，其余由 `render_page` 驱动。

## 构建

```sh
cargo build -p rsword-layout-webgl --target wasm32-unknown-unknown --release --locked
wasm-bindgen target/wasm32-unknown-unknown/release/rsword_layout_webgl.wasm --out-dir crates/webgl/pkg --target web
```

`wasm-bindgen-cli` 必须与 Cargo.lock 里的 `wasm-bindgen` 同版本（`scripts/prepare-webgl.sh` 会检查）。

## 安装与发布

包在 GitHub Packages（`npm.pkg.github.com`），安装需要带 `read:packages` 的 GitHub token：

```ini
# .npmrc
@jabberwocky238:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${GITHUB_TOKEN}
```

```sh
export GITHUB_TOKEN=$(gh auth token)   # 先 gh auth refresh -h github.com -s read:packages
npm install @jabberwocky238/rs-word-layout
```


包的元数据在本目录 `package.json`（不写 `version`）；**版本号就是本 crate `Cargo.toml` 的
`version`**。`.github/workflows/npm.yml` 在 `main` 每次推送时检查 `@jabberwocky238/rs-word-layout@<版本>`：
注册表上已有则跳过，没有则构建、组装、冒烟并发布。改版本号即发版；带 `-` 的预发布版本发到
dist-tag `next`。用 workflow 自带的 `GITHUB_TOKEN` 发布，不需要额外 secret。

本地组装与冒烟（不发布）：

```sh
node crates/webgl/pack.mjs pack   # pkg/ 成为完整的包目录，并跑 smoke.mjs
```

`smoke.mjs` 从包目录加载 wasm，排 `fixtures/plain.docx` 并登记一个字体；WebGL 渲染要浏览器，不在冒烟范围内。

License: MIT OR Apache-2.0.
