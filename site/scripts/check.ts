// One-off review captures: reduced motion (does the loop stop?), the hidden tuning panel, and
// the still. Not a test.
import { chromium } from "playwright"
const base = process.env.HEX_SITE_URL ?? "http://127.0.0.1:5199"
const out = process.env.HEX_SHOTS ?? "./shots"
const browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--enable-unsafe-webgpu"] })
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, reducedMotion: "reduce" })
const page = await ctx.newPage()
const logs: string[] = []
page.on("console", (m) => (m.type() === "error" || m.type() === "warning") && logs.push(m.text()))
page.on("pageerror", (e) => logs.push(String(e)))
await page.goto(base)
await page.waitForTimeout(7000)
const count = () => page.evaluate(() => new Promise<number>((r) => { let n = 0; const t0 = performance.now(); const f = () => { n++; if (performance.now() - t0 < 1000) requestAnimationFrame(f); else r(n) }; requestAnimationFrame(f) }))
const a = await page.screenshot({ path: `${out}/reduced-a.png` })
await page.waitForTimeout(1500)
const b = await page.screenshot({ path: `${out}/reduced-b.png` })
console.log("reduced: still frames identical:", a.equals(b), "state:", await page.locator(".readout__state").innerText())
await page.keyboard.down("Space")
await page.waitForTimeout(2500)
console.log("reduced, holding space:", await page.locator(".readout__state").innerText())
await page.keyboard.up("Space")
await page.waitForTimeout(1500)
console.log("after release:", await page.locator(".readout__state").innerText(), "|", await page.locator(".instrument__field").innerText())
await page.keyboard.press("Backquote")
await page.waitForTimeout(1500)
await page.screenshot({ path: `${out}/tuning.png` })
console.log("tuning panel mounted:", await page.locator(".tuning .knobs-panel").count(), "visible:", await page.locator(".tuning").isVisible())
console.log("rafs/s (page-level, sanity):", await count())
if (logs.length) console.log("console:", logs.join("\n"))
await browser.close()
