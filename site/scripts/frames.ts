// A take, for review: hero screenshots at moments of the first automatic take (mid-recording,
// just after a pause cut, transcribing, pasted, settled). Not a test.
import { chromium } from "playwright"
const base = process.env.HEX_SITE_URL ?? "http://127.0.0.1:5199"
const out = process.env.HEX_SHOTS ?? "./shots"
const width = Number(process.env.HEX_W ?? 1440)
const height = Number(process.env.HEX_H ?? 900)
const browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--enable-unsafe-webgpu"] })
const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 2 })
page.on("pageerror", (e) => console.log("pageerror", e))
page.on("console", (m) => m.type() === "error" && console.log("console", m.text()))
await page.goto(`${base}/${process.env.HEX_QUERY ?? ""}`)
if (width < 600) await page.evaluate(() => window.scrollTo(0, 380))
const state = () => page.locator(".readout__state").innerText()
const shot = (name: string) =>
  page.screenshot({ path: `${out}/take-${name}.png`, clip: width < 600 ? { x: 0, y: 380, width, height: height - 380 } : { x: width * 0.42, y: 40, width: width * 0.58, height: height - 40 } })
const until = async (ok: (s: string) => boolean, ms = 30000) => {
  const t0 = Date.now()
  while (Date.now() - t0 < ms) {
    if (ok(await state())) return
    await page.waitForTimeout(40)
  }
  throw new Error("timed out at " + (await state()))
}
await until((s) => s.includes("RECORDING") && /0:0[2-9]/.test(s))
await shot("1-recording")
await until((s) => /0:0[5-9]/.test(s))
await shot("2-after-cut")
await until((s) => s.includes("TRANSCRIBING") || s.includes("PASTED"))
await shot("3-transcribing")
await until((s) => s.includes("PASTED"))
await page.waitForTimeout(500)
await shot("4-pasted")
await page.waitForTimeout(2500)
await shot("5-settled")
await browser.close()
