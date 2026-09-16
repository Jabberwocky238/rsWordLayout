// rsword-layout WebGL demo
//
// 整条链路都在浏览器里：docx 字节 → wasm 解析与排版 → WebGL2 绘制。
// 文件不经过任何服务器，Vite 只负责把静态资源送过来。

import init, { LayoutSession, WebGlRenderer } from '@pkg/rsword_layout_webgl.js'
import wasmUrl from '@pkg/rsword_layout_webgl_bg.wasm?url'

const $ = (id) => document.getElementById(id)
const els = {
  file: $('file'), drop: $('drop'), stage: $('stage'), canvas: $('cv'),
  prev: $('prev'), next: $('next'), pageinfo: $('pageinfo'),
  dpi: $('dpi'), log: $('log'),
}

let renderer = null
let session = null
let page = 0
let lastFile = null   // 记住原始文件：改 DPI 要重排，不能只缩放画布
let ready = false     // wasm 是否已初始化

const log = (...parts) => { els.log.textContent = parts.join(' ') }

async function boot() {
  // Vite 把 .wasm 当资源处理，所以显式把 URL 交给 init，
  // 不依赖胶水代码里基于 import.meta.url 的默认推断。
  await init({ module_or_path: wasmUrl })
  ready = true
  log('引擎就绪，选择或拖入一个 .docx。')
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
  const size = session.page_size(page)
  if (size.length !== 2) return
  const [w, h] = size

  // canvas 的位图尺寸用物理像素，CSS 尺寸用 96 DPI 的逻辑像素，
  // 这样 HiDPI 下画面清晰而排版尺寸不变。
  const dpi = Number(els.dpi.value)
  els.canvas.width = Math.round(w)
  els.canvas.height = Math.round(h)
  els.canvas.style.width = Math.round(w * 96 / dpi) + 'px'

  renderer.clear(1, 1, 1)
  session.render_page(renderer, page)

  els.pageinfo.textContent = `${page + 1} / ${total}`
  els.prev.disabled = page === 0
  els.next.disabled = page === total - 1

  const frags = session.fragment_count(page)
  log(
    `第 ${page + 1}/${total} 页 · ${Math.round(w)}×${Math.round(h)}px · 片段 ${frags}`,
    '\n字形图集未接入，文字批次为空——当前只画得出矩形类片段（底纹、边框）。',
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
})

boot()
