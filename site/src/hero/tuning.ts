// The swarm's tunable values. The hero reads this plain object every frame; the knobs panel,
// loaded only when someone presses ` (or opens the page with ?tune), writes into it.

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
  const [{ defineControls, knob, color, colorHex }, panelModule] = await Promise.all([
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
