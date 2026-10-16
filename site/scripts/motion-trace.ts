// @ts-nocheck: a Bun script; the site's tsconfig has no Bun types.
//
// A deterministic motion trace of the hero, for motion-lint. Playwright's fake clock drives every timer, rAF and
// performance.now, and Element.prototype.animate is removed so Motion animates on that clock too (no WAAPI), so
// the page steps at exactly 1/60 s per frame however slowly headless WebGPU renders. Per frame it records the
// field's DOM (boxes and drawn opacity) and the instrument's lights as channels read from the uniform block, then
// runs motion-lint's rules, writes the trace, and (with --clip) a 30 fps clip of the instrument.
//
//   bun scripts/motion-trace.ts [--url http://127.0.0.1:5199] [--out dir] [--seconds 26] [--clip] [--label after]
//
// The scenario: an automatic take, its paste, the next automatic take replacing it, then a short tap (discarded),
// and a held take released early. Needs motion-lint beside hex (~/code/open-source/motion-lint) or MOTION_LINT_DIR.
import { chromium } from "playwright"
import { mkdirSync, rmSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { spawnSync } from "node:child_process"

const args = process.argv.slice(2)
const flag = (name: string) => {
  const i = args.indexOf(`--${name}`)
  return i >= 0 ? args[i + 1] : undefined
}
const has = (name: string) => args.includes(`--${name}`)
const url = flag("url") ?? "http://127.0.0.1:5199"
const label = flag("label") ?? "trace"
const out = resolve(flag("out") ?? `./shots/motion/${label}`)
const seconds = Number(flag("seconds") ?? 26)
const width = Number(flag("width") ?? 1440)
const height = Number(flag("height") ?? 900)
const motionLint = process.env.MOTION_LINT_DIR ?? resolve(import.meta.dir, "../../../motion-lint")
const { TraceWriter } = await import(join(motionLint, "src/web/trace.ts"))
const { lint, loadConfig, mergeConfig } = await import(join(motionLint, "src/lint.ts"))

rmSync(out, { recursive: true, force: true })
mkdirSync(join(out, "frames"), { recursive: true })

const browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--enable-unsafe-webgpu"] })
const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 })
page.on("pageerror", (e) => console.log("pageerror", String(e)))
await page.addInitScript(() => {
  delete (Element.prototype as any).animate
})
await page.clock.install({ time: 0 })
await page.clock.pauseAt(10)
await page.goto(url)
// Let WebGPU come up in real time while the page clock stands still.
for (let i = 0; i < 300 && !(await page.evaluate(() => !!(window as any).__hexHero?.params)); i++) await new Promise((r) => setTimeout(r, 100))
await page.evaluate(() => document.fonts.ready)

const writer = new TraceWriter({ launch: "hero", width, height, hz: 60, platform: "web", units: "px" })
const FRAME = 1000 / 60
const steps = Math.round(seconds * 60)
// Input, by step: [step, action].
const script = new Map<number, string>([
  [Math.round(17.4 * 60), "down"],
  [Math.round(17.55 * 60), "up"],
  [Math.round(18.6 * 60), "down"],
  [Math.round(21.3 * 60), "up"],
])
const clip = has("clip")
const box = await page.evaluate(() => {
  const r = document.querySelector(".instrument")!.getBoundingClientRect()
  return { x: Math.max(0, r.left - 40), y: Math.max(0, r.top - 30), width: Math.min(innerWidth - Math.max(0, r.left - 40), r.width + 80), height: r.height + 60 }
})
const even = (v: number) => Math.floor(v / 2) * 2
const crop = { x: Math.round(box.x), y: Math.round(box.y), width: even(box.width), height: even(box.height) }

let shot = 0
for (let step = 0; step < steps; step++) {
  const action = script.get(step)
  if (action === "down") await page.keyboard.down("Space")
  if (action === "up") await page.keyboard.up("Space")
  await page.clock.runFor(FRAME)
  const sample = await page.evaluate(() => {
    const w = window as any
    const ids: WeakMap<Element, string> = (w.__mlIds ??= new WeakMap())
    w.__mlNext ??= 0
    const key = (el: Element) => {
      let k = ids.get(el)
      if (!k) ids.set(el, (k = `${el.className || el.tagName}#${++w.__mlNext}`))
      return k
    }
    const alpha = (el: Element | null) => {
      let a = 1
      for (let e = el; e && e !== document.body; e = e.parentElement) a *= Number(getComputedStyle(e).opacity)
      return a
    }
    const els: [string, any][] = []
    const sel = ".instrument__field, .instrument__field p, .instrument__field p > span, .field__catch, .readout__state, .readout__state span, .readout__rows dd, .readout__rows dd span"
    for (const el of document.querySelectorAll(sel)) {
      const r = el.getBoundingClientRect()
      if (!r.width && !r.height) continue
      const cls = String(el.className || el.tagName)
      els.push([key(el), { x: r.left, y: r.top, w: r.width, h: r.height, a: alpha(el), meta: { cls, nm: cls.split(" ")[0] } }])
    }
    const p: Float32Array = w.__hexHero.params
    const v: Float32Array = w.__hexHero.voice
    const sm = (x: number) => x * x * (3 - 2 * x)
    const ch: [string, any][] = []
    const rec = p[31]
    const glow = p[35]
    ch.push(["arrive", { kind: "level", v: p[23] }])
    ch.push(["head-marker", { kind: "level", v: rec }])
    ch.push(["head", { kind: "phase", v: p[32], gate: "head-marker" }])
    ch.push(["gauge-glow", { kind: "level", v: glow }])
    ch.push(["input-level", { kind: "level", v: p[34] }])
    // Two uniform layouts: the original (84 floats, one glow for every mark) and the instrument's (128).
    const v2 = p.length >= 128
    for (let i = 0; i < 8; i++) {
      const cut = p[52 + i]
      ch.push([`cut-${i}`, { kind: "level", v: cut < 0 ? 0 : v2 ? p[92 + i] * p[84 + i] : glow }])
    }
    for (let i = 0; i < 6; i++) {
      const lock = p[62 + i * 4]
      const a = p[63 + i * 4] * (v2 ? p[100 + i * 4] : 1)
      ch.push([`bracket-${i}`, { kind: "level", v: a }])
      ch.push([`bracket-${i}-lock`, { kind: "position", v: lock * 100, gate: `bracket-${i}` }])
      const l = v2 ? lock : sm(Math.min(1, Math.max(0, lock)))
      ch.push([`digit-${i}`, { kind: "level", v: a * Math.min(1, Math.max(0, (l - 0.5) * 2)) }])
    }
    const prog = v2 ? p[124] : p[30]
    ch.push(["bead", { kind: "level", v: v2 ? (prog >= 0 ? p[125] : 0) : prog >= 0 && prog < 1.45 ? 1 : 0 }])
    ch.push(["bead-travel", { kind: "position", v: Math.max(0, prog) * 100, gate: "bead" }])
    for (const b of [24, 72, 120, 168, 216]) ch.push([`tick-${b}`, { kind: "level", v: v[b] }])
    return { els, ch }
  })
  writer.frame({ t: step * FRAME, n: 1, elements: sample.els, channels: sample.ch })
  if (clip && step % 2 === 0) await page.screenshot({ path: join(out, "frames", `${String(shot++).padStart(5, "0")}.png`), clip: crop })
}
await browser.close()

const text = writer.lines.join("\n") + "\n"
mkdirSync(join(out, "runs", "hero", "trace"), { recursive: true })
writeFileSync(join(out, "runs", "hero", "trace", "hero.jsonl"), text)
const base = loadConfig(join(import.meta.dir, "motion-lint.config.json")).config
const { candidates } = lint(`hex/${label}`, [text], base)
const byRule: Record<string, number> = {}
for (const c of candidates) byRule[c.rule] = (byRule[c.rule] ?? 0) + 1
console.log(`${label}: ${steps} frames · ${candidates.length} candidates ${JSON.stringify(byRule)}`)
for (const c of candidates.slice(0, Number(flag("top") ?? 40)))
  console.log(`  ${c.score.toFixed(2).padStart(5)}  ${c.rule.padEnd(13)} ${(c.t / 1000).toFixed(2).padStart(6)} s  ${String(c.label).slice(0, 40).padEnd(40)}  ${c.detail}`)
writeFileSync(join(out, "candidates.json"), JSON.stringify(candidates.map(({ rule, score, t, label, detail }) => ({ rule, score, t, label, detail })), null, 1))
if (clip) {
  const mp4 = join(out, `${label}.mp4`)
  spawnSync("ffmpeg", ["-y", "-loglevel", "error", "-framerate", "30", "-i", join(out, "frames", "%05d.png"), "-r", "30", "-c:v", "libx264", "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an", mp4])
  console.log("clip:", mp4)
}
