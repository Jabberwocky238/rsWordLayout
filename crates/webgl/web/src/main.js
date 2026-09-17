// rsword-layout WebGL demo
//
// 整条链路都在浏览器里：docx 字节 → wasm 解析与排版 → WebGL2 绘制。
// 文件不经过任何服务器，Vite 只负责把静态资源送过来。

import init, { FontSet, LayoutSession, WebGlRenderer } from '@pkg/rsword_layout_webgl.js'
import wasmUrl from '@pkg/rsword_layout_webgl_bg.wasm?url'

const $ = (id) => document.getElementById(id)
const els = {
  file: $('file'), drop: $('drop'), stage: $('stage'), canvas: $('cv'),
  prev: $('prev'), next: $('next'), pageinfo: $('pageinfo'),
  dpi: $('dpi'), log: $('log'),
  zoomIn: $('zoomIn'), zoomOut: $('zoomOut'), zoomReset: $('zoomReset'),
  zoomLabel: $('zoomLabel'),
}

let renderer = null
let session = null
let fonts = null
let page = 0
let zoom = 1
let lastFile = null   // 记住原始文件：改 DPI 要重排，不能只缩放画布
let ready = false     // wasm 是否已初始化

const log = (...parts) => { els.log.textContent = parts.join(' ') }

async function boot() {
  // Vite 把 .wasm 当资源处理，所以显式把 URL 交给 init，
  // 不依赖胶水代码里基于 import.meta.url 的默认推断。
  await init({ module_or_path: wasmUrl })
  await loadFonts()
  ready = true
  log(`引擎就绪（${fontNote}），选择或拖入一个 .docx。`)
}

// 默认字体集：运行时 fetch，不编进 wasm——二进制因此保持在 ~3.9MB，
// 字体则可被浏览器缓存。覆盖不全时 fontenv 会报 FONT_MISSING 而非画错。
const DEFAULT_FONTS = [
  './fonts/DejaVuSans.ttf',           // 拉丁/希腊/西里尔
  './fonts/DroidSansFallbackFull.ttf' // CJK 及大量回退字形
]
let fontNote = '无字体'

async function loadFonts() {
  fonts = new FontSet()
  let ok = 0
  for (const url of DEFAULT_FONTS) {
    try {
      const res = await fetch(url)
      if (!res.ok) { console.warn('字体取不到', url, res.status); continue }
      fonts.add_font(new Uint8Array(await res.arrayBuffer()), 0)
      ok++
    } catch (e) {
      console.warn('字体加载失败', url, e)
    }
  }
  fontNote = ok ? `${ok} 份字体` : '无字体，文字将不显示'
}

/// 建 WebGL2 上下文。
///
/// **必须在 canvas 可见之后调用**：canvas 在 `hidden` 容器里时布局尺寸为 0，
/// 此时取 webgl2 上下文在部分浏览器上会失败或拿到不可用的上下文。
/// 所以渲染器是懒建的，不在 boot 里建。
function ensureRenderer() {
  if (renderer) return true
  try {
    renderer = new WebGlRenderer('cv')
    return true
  } catch (e) {
    log('WebGL2 初始化失败：', e?.message ?? e)
    return false
  }
}

function setPage(n) {
  if (!session) return
  const total = session.page_count
  page = Math.max(0, Math.min(n, total - 1))
  draw()
}

function draw() {
  if (!session || !renderer) return
  const total = session.page_count
  const dpi = Number(els.dpi.value) * zoom
  const size = session.page_size(page, dpi)
  if (size.length !== 2) return
  const [w, h] = size

  // 关键：缩放不是 CSS 拉伸，而是提高 DPI 后**重新栅格化**。
  // 这正是 Word / WPS（DirectWrite / FreeType）的做法：每个缩放级别重来一遍，
  // 所以放大不糊。CSS 拉伸只会把位图插值放大。
  els.canvas.width = Math.round(w)
  els.canvas.height = Math.round(h)
  els.canvas.style.width = Math.round(w / zoom) + 'px'

  renderer.clear(1, 1, 1)
  session.render_page(renderer, fonts, page, dpi)

  els.pageinfo.textContent = `${page + 1} / ${total}`
  els.prev.disabled = page === 0
  els.next.disabled = page === total - 1

  const frags = session.fragment_count(page)
  log(
    `第 ${page + 1}/${total} 页 · ${Math.round(w)}×${Math.round(h)}px · 缩放 ${zoom.toFixed(2)}× · DPI ${Math.round(dpi)}`,
    `\n片段 ${frags} · 字形缓存 ${fonts.glyph_count} 个 · ${fontNote}`,
  )
}

async function load(file) {
  if (!file) return
  if (!ready) { log('引擎还在加载，请稍候再试。'); return }
  try {
    log(`读取 ${file.name} …`)
    const bytes = new Uint8Array(await file.arrayBuffer())
    const next = new LayoutSession(bytes, Number(els.dpi.value))
    // 新会话建成功之后再替换旧的，失败时保留当前画面。
    session?.free()
    session = next
    lastFile = file
    page = 0

    // 先让 canvas 可见，再建上下文——见 ensureRenderer 的说明。
    els.drop.hidden = true
    els.stage.hidden = false
    if (!ensureRenderer()) return
    draw()
  } catch (e) {
    log('失败：', e?.message ?? e)
  }
}

els.file.addEventListener('change', (e) => load(e.target.files[0]))
els.prev.addEventListener('click', () => setPage(page - 1))
els.next.addEventListener('click', () => setPage(page + 1))
els.dpi.addEventListener('change', () => {
  // DPI 改变要重排：像素尺寸由 DPI 决定，不能只缩放画布。
  if (lastFile) load(lastFile)
})

// 拖放
for (const type of ['dragenter', 'dragover']) {
  document.addEventListener(type, (e) => {
    e.preventDefault()
    els.drop.classList.add('over')
  })
}
for (const type of ['dragleave', 'drop']) {
  document.addEventListener(type, (e) => {
    e.preventDefault()
    els.drop.classList.remove('over')
  })
}
document.addEventListener('drop', (e) => {
  const f = e.dataTransfer?.files?.[0]
  if (f?.name.endsWith('.docx')) load(f)
  else if (f) log('只支持 .docx')
})

document.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowLeft') setPage(page - 1)
  if (e.key === 'ArrowRight') setPage(page + 1)
  if (e.key === '+' || e.key === '=') setZoom(zoom * 1.5)
  if (e.key === '-') setZoom(zoom / 1.5)
  if (e.key === '0') setZoom(1)
})

function setZoom(z) {
  // 上限 16×：再高单页位图会超出常见的 canvas 尺寸限制。
  zoom = Math.max(0.25, Math.min(16, z))
  els.zoomLabel.textContent = zoom.toFixed(2) + '×'
  draw()
}

els.zoomIn.addEventListener('click', () => setZoom(zoom * 1.5))
els.zoomOut.addEventListener('click', () => setZoom(zoom / 1.5))
els.zoomReset.addEventListener('click', () => setZoom(1))

boot()
