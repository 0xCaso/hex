// Screenshots of the running site for review: desktop and mobile, the live WebGPU hero (headless
// Chromium on Apple silicon has Metal WebGPU) and the ?nogpu still. Not a test.
import { chromium } from "playwright"
import { mkdirSync } from "node:fs"

const base = process.env.HEX_SITE_URL ?? "http://127.0.0.1:5199"
const out = process.env.HEX_SHOTS ?? "./shots"
const at = Number(process.env.HEX_SHOT_AT ?? 5200)
mkdirSync(out, { recursive: true })

const browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--enable-unsafe-webgpu"] })
try {
  for (const [name, width, height, scale, mobile] of [
    ["desktop", 1440, 900, 2, false],
    ["mobile", 390, 844, 3, true],
  ] as const) {
    for (const query of ["", "?nogpu"]) {
      const page = await browser.newPage({
        viewport: { width, height },
        deviceScaleFactor: scale,
        isMobile: mobile,
        hasTouch: mobile,
      })
      const logs: string[] = []
      page.on("console", (m) => (m.type() === "error" || m.type() === "warning") && logs.push(m.text()))
      page.on("pageerror", (e) => logs.push(String(e)))
      await page.goto(`${base}/${query}`)
      await page.evaluate(() => document.fonts.ready)
      await page.waitForTimeout(query ? 800 : at)
      const tag = `${name}${query ? "-still" : ""}`
      await page.screenshot({ path: `${out}/${tag}-hero.png` })
      if (!query) {
        const t = await page.evaluate(() => (window as unknown as { __hexHero?: { timings(): unknown; agents: number } }).__hexHero)
        const timings = await page.evaluate(() =>
          (window as unknown as { __hexHero?: { timings(): unknown } }).__hexHero?.timings(),
        )
        console.log(tag, "agents", t?.agents, "gpu ms", JSON.stringify(timings))
        await page.screenshot({ path: `${out}/${tag}-full.png`, fullPage: true })
      }
      if (logs.length) console.log(tag, "console:", logs.join("\n"))
      await page.close()
    }
  }
} finally {
  await browser.close()
}
