// 发布前冒烟：从组装好的包目录加载 wasm，排一份真实 docx 并登记字体。
// WebGL 渲染要浏览器上下文，这里只验布局会话与字体集。
// 用法：node crates/webgl/smoke.mjs <包目录>（pack.mjs pack 会对 pkg/ 调用）
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const dir = resolve(process.argv[2])
const fixtures = new URL('../../fixtures/', import.meta.url)
const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'))
const js = await import(pathToFileURL(join(dir, manifest.main)).href)
js.initSync({ module: readFileSync(join(dir, 'rsword_layout_webgl_bg.wasm')) })

const session = new js.LayoutSession(readFileSync(new URL('plain.docx', fixtures)), 96)
assert.ok(session.page_count > 0, '没有排出页')
assert.ok(session.fragment_count(0) > 0, '第 0 页没有片段')
assert.equal(session.page_size(0, 96).length, 2)
session.free()

const fonts = new js.FontSet()
fonts.add_font(readFileSync(new URL('fonts/LiberationSans-Regular.ttf', fixtures)), 0)
assert.ok(!fonts.is_empty && fonts.fingerprint, '字体没有登记上')
fonts.free()
console.log(`smoke ok: ${manifest.name}@${manifest.version}`)
