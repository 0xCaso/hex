// The hero's tunable values. The frame loop and the field read this plain object; the knobs panel,
// loaded only when someone presses ` (or opens the page with ?tune), writes into it.
import type { SpringSpec } from "./spring"

const s = (duration: number, bounce = 0): SpringSpec => ({ duration, bounce })

/** Every spring in the hero, by duration and bounce (Kit's palette: default .3/0, calm .4/0, slow .85/0, snappy .25/0). */
export const motion = {
  /** Words arriving in the field: fade and 2.5 px blur clearing in place. default */
  wordsIn: s(0.3),
  /** The previous take leaving the field, in place. snappy, 17% shorter than words in */
  textOut: s(0.25),
  /** The field's height when text needs more room. calm */
  field: s(0.4),
  /** The caret's fade, and readout labels swapping in place. default */
  readout: s(0.3),
  /** Lights striking: brackets, cut marks, the gauge, the write head. default */
  lightsOn: s(0.3),
  /** Lights cooling after the paste. slow */
  lightsCool: s(0.85),
  /** Lights leaving at once: the write head at release, the bead as it lands. snappy */
  lightsOut: s(0.25),
  /** A bracket closing on its piece, and a piece's end settling on its cut. calm */
  lock: s(0.4),
  /** The band swelling where the voice wrote. default */
  bandRise: s(0.3),
  /** The band settling back to rest after release. slow */
  bandSettle: s(0.85),
  /** The write head coasting through the 200 ms tail after release. calm */
  headStop: s(0.4),
  /** The instrument fading in once WebGPU is ready. slow */
  arrive: s(0.85),
}

export const timing = {
  /** Brackets, cut marks and the field's edge light draw on over this long, cubic-bezier(.45, 0, .2, 1). */
  drawOn: 0.4,
  /** Between successive words' entrances. */
  stagger: 0.018,
}

let version = 0
const listeners = new Set<() => void>()
/** For React: re-render when a spring changes in the panel. */
export function subscribeTuning(fn: () => void) {
  listeners.add(fn)
  return () => listeners.delete(fn)
}
export const tuningVersion = () => version

export const tuning = {
  cohesion: 0.16,
  alignment: 0.45,
  separation: 0.6,
  speed: 0.34,
  pull: 1,
  drift: 0.05,
  ring: 0.6,
  rest: 0.05,
  voice: 0.3,
  agitation: 0.012,
  trail: 0.09,
  bloom: 0.18,
  exposure: 1,
  accent: "#d0a467",
  bone: "#d8d1c4",
}

export type Tuning = typeof tuning

let open: Promise<{ toggle(): void }> | null = null

export function toggleTuning(onChange: () => void) {
  open ??= load(onChange)
  open.then((p) => p.toggle())
}

async function load(onChange: () => void) {
  const [{ defineControls, knob, color, colorHex, spring }, panelModule] = await Promise.all([
    import("knobs"),
    import("knobs/panel"),
  ])
  const pct = (v: number, label: string, group: string) =>
    knob(Math.round(v * 100), { min: 0, max: 100, step: 1, unit: "%", label, group })
  const store = defineControls(
    "hex-hero",
    {
      cohesion: pct(tuning.cohesion, "Cohesion", "Flock"),
      alignment: pct(tuning.alignment, "Alignment", "Flock"),
      separation: pct(tuning.separation, "Separation", "Flock"),
      speed: pct(tuning.speed, "Speed", "Flock"),
      pull: knob(tuning.pull, { min: 0, max: 2, step: 0.05, label: "Band pull", group: "Ring" }),
      drift: knob(tuning.drift, { min: 0, max: 0.3, step: 0.005, label: "Drift", group: "Ring" }),
      ring: knob(tuning.ring, { min: 0.35, max: 0.75, step: 0.01, label: "Radius", group: "Ring" }),
      rest: knob(tuning.rest, { min: 0, max: 0.15, step: 0.005, label: "Rest width", group: "Ring" }),
      voice: knob(tuning.voice, { min: 0, max: 0.45, step: 0.01, label: "Voice width", group: "Ring" }),
      agitation: knob(tuning.agitation, { min: 0, max: 0.1, step: 0.002, label: "Agitation", group: "Ring" }),
      trail: knob(tuning.trail, { min: 0.01, max: 0.4, step: 0.01, unit: "s", label: "Phosphor", group: "Look" }),
      bloom: knob(tuning.bloom, { min: 0, max: 0.6, step: 0.01, label: "Bloom", group: "Look" }),
      exposure: knob(tuning.exposure, { min: 0.4, max: 2, step: 0.05, label: "Exposure", group: "Look" }),
      accent: color(tuning.accent, { label: "Accent", group: "Look" }),
      bone: color(tuning.bone, { label: "Lines", group: "Look" }),
      ...Object.fromEntries(
        (Object.keys(motion) as (keyof typeof motion)[]).map((k) => [
          `m_${k}`,
          spring({ ...motion[k] }, { label: `${LABELS[k]} · ${fmt(motion[k])}`, group: "Motion" }),
        ]),
      ),
      drawOn: knob(timing.drawOn, { min: 0.1, max: 1.2, step: 0.01, unit: "s", label: "Draw on · .4 s", group: "Motion" }),
      stagger: knob(timing.stagger, { min: 0, max: 0.06, step: 0.001, unit: "s", label: "Word stagger · .018 s", group: "Motion" }),
    },
    { enabled: true, storage: null, ship: () => import("knobs/panel") },
  )
  const apply = () => {
    const v = store.get()
    tuning.cohesion = v.cohesion / 100
    tuning.alignment = v.alignment / 100
    tuning.separation = v.separation / 100
    tuning.speed = v.speed / 100
    tuning.pull = v.pull
    tuning.drift = v.drift
    tuning.ring = v.ring
    tuning.rest = v.rest
    tuning.voice = v.voice
    tuning.agitation = v.agitation
    tuning.trail = v.trail
    tuning.bloom = v.bloom
    tuning.exposure = v.exposure
    tuning.accent = colorHex(v.accent)
    tuning.bone = colorHex(v.bone)
    const values = v as unknown as Record<string, SpringSpec>
    for (const k of Object.keys(motion) as (keyof typeof motion)[]) motion[k] = values[`m_${k}`] ?? motion[k]
    timing.drawOn = v.drawOn
    timing.stagger = v.stagger
    version++
    for (const fn of listeners) fn()
    onChange()
  }
  store.subscribe(apply)
  const slot = document.createElement("div")
  slot.className = "tuning"
  slot.hidden = true
  document.body.append(slot)
  panelModule.mountPanel(store, slot, { title: "Swarm", theme: "dark", session: false, variants: false })
  return {
    toggle() {
      slot.hidden = !slot.hidden
    },
  }
}

const LABELS: Record<keyof typeof motion, string> = {
  wordsIn: "Words in",
  textOut: "Text out",
  field: "Field height",
  readout: "Caret, readouts",
  lightsOn: "Lights on",
  lightsCool: "Lights cool",
  lightsOut: "Lights out",
  lock: "Bracket lock",
  bandRise: "Band rise",
  bandSettle: "Band settle",
  headStop: "Head coast",
  arrive: "Arrive",
}
const fmt = (v: SpringSpec) => `${String(v.duration).replace(/^0/, "")}/${v.bounce}`
