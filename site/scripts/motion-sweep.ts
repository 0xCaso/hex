// @ts-nocheck: a Bun script; the site's tsconfig has no Bun types.
//
// A deterministic sweep of the instrument without a browser: the Director and the Instrument are pure, so this steps
// them at 1/120 s through the same scenario as motion-trace.ts (an automatic take, its paste, the next take, a hand
// release, a held take), writes their uniform slots as motion-lint channels, and runs the rules. The DOM field is
// motion-trace.ts's.
//
//   bun scripts/motion-sweep.ts [--seconds 26]
import { join, resolve } from "node:path"
import { Director } from "../src/hero/take"
import { Instrument } from "../src/hero/instrument"

const args = process.argv.slice(2)
const seconds = Number(args[args.indexOf("--seconds") + 1] || 26)
const motionLint = process.env.MOTION_LINT_DIR ?? resolve(import.meta.dir, "../../../motion-lint")
const { sweepClock } = await import(join(motionLint, "src/web/sweep.ts"))
const { lint, loadConfig } = await import(join(motionLint, "src/lint.ts"))

const d = new Director({ phase() {}, transcript() {}, pieces() {} }, 256, 160)
const inst = new Instrument(256)
const p = new Float32Array(128)
const dt = 1 / 120
let lastHand = -1e9
const input: [number, "down" | "up"][] = [
  [17.4, "down"],
  [17.55, "up"],
  [18.6, "down"],
  [21.3, "up"],
]
let holding = false
const trace = sweepClock({
  launch: "instrument",
  width: 1440,
  height: 900,
  dt,
  segments: [[0, seconds]],
  sample: (t) => {
    for (const [at, a] of input)
      if (t >= at && t - dt < at) {
        lastHand = t
        if (a === "down") (holding = true), d.press(false)
        else (holding = false), d.release()
      }
    if (!holding && d.idleFor > (d.phase === "pasted" ? 4.5 : 1.6) && t - lastHand > 9) d.press(true)
    d.step(dt)
    inst.step(dt, d, true)
    inst.pack(p)
    const ch = [
      ["arrive", { kind: "level", v: inst.arrive.x }],
      ["head-marker", { kind: "level", v: inst.rec.x }],
      ["head", { kind: "phase", v: inst.head.x, gate: "head-marker" }],
      ["gauge-glow", { kind: "level", v: inst.glow.x }],
      ["input-level", { kind: "level", v: inst.level.x }],
    ]
    for (let i = 0; i < 8; i++) ch.push([`cut-${i}`, { kind: "level", v: p[52 + i] < 0 ? 0 : p[92 + i] * p[84 + i] }])
    for (let i = 0; i < 6; i++) {
      const a = p[63 + i * 4] * p[100 + i * 4]
      ch.push([`bracket-${i}`, { kind: "level", v: a }])
      ch.push([`bracket-${i}-lock`, { kind: "position", v: p[62 + i * 4] * 100, gate: `bracket-${i}` }])
      ch.push([`digit-${i}`, { kind: "level", v: a * Math.min(1, Math.max(0, (p[62 + i * 4] - 0.5) * 2)) }])
    }
    ch.push(["bead", { kind: "level", v: p[124] >= 0 ? p[125] : 0 }])
    ch.push(["bead-travel", { kind: "position", v: Math.max(0, p[124]) * 100, gate: "bead" }])
    for (const b of [24, 72, 120, 168, 216]) ch.push([`band-${b}`, { kind: "level", v: inst.bins.x[b] }])
    return { elements: [], channels: ch }
  },
})
const text = trace.lines ? trace.lines.join("\n") + "\n" : String(trace)
const { candidates } = lint("hex/instrument", [text], loadConfig(join(import.meta.dir, "motion-lint.config.json")).config)
const byRule: Record<string, number> = {}
for (const c of candidates) byRule[c.rule] = (byRule[c.rule] ?? 0) + 1
console.log(`instrument: ${candidates.length} candidates ${JSON.stringify(byRule)}`)
for (const c of candidates.slice(0, 30))
  console.log(`  ${c.score.toFixed(2).padStart(5)}  ${c.rule.padEnd(13)} ${(c.t / 1000).toFixed(2).padStart(6)} s  ${String(c.label).slice(0, 24).padEnd(24)}  ${c.detail}`)
