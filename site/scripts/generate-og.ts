import { chromium } from "playwright"
import { fileURLToPath } from "node:url"

// Run against the local Vite site. Headless Chromium on Apple silicon has Metal WebGPU, so this
// captures the live hero just after its first automatic take is pasted.
const browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--enable-unsafe-webgpu"] })
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 })
  await page.goto(process.env.HEX_SITE_URL ?? "http://127.0.0.1:5173")
  await page.addStyleTag({
    content: `
    .hero { min-height: 630px; --ring: 400px; }
    .hero__inner { padding: 3.5rem 64px 0; grid-template-columns: 440px 1fr; }
    .header { padding: 2rem 64px; }
    .nav, .meta, .brew--compact, .instrument__note, .instrument__side, .hero__actions { display: none !important; }
    h1 { font-size: 64px; }
  `,
  })
  await page.evaluate(() => document.fonts.ready)
  await page.waitForFunction(() => document.querySelector(".readout__state")?.textContent?.includes("Pasted"), null, {
    timeout: 60000,
  })
  await page.waitForTimeout(700)
  await page.screenshot({ path: fileURLToPath(new URL("../public/og.png", import.meta.url)) })
} finally {
  await browser.close()
}
