// Without WebGPU (or with reduced motion before the GPU is ready): the instrument, still, in a 2D
// canvas. The band holds the first sample's envelope, as the ring looks just after a take.
import { RING_SECONDS, SAMPLES, Segmenter } from "./take"

export interface StillLayout {
  cx: number
  cy: number
  R: number
  fieldX: number
  fieldY: number
}

export function drawStill(canvas: HTMLCanvasElement, lay: StillLayout, colors: { bone: string; accent: string }) {
  const dpr = Math.min(2, devicePixelRatio || 1)
  const w = canvas.clientWidth
  const h = canvas.clientHeight
  canvas.width = Math.round(w * dpr)
  canvas.height = Math.round(h * dpr)
  const g = canvas.getContext("2d")
  if (!g) return
  g.setTransform(dpr, 0, 0, dpr, 0, 0)
  g.clearRect(0, 0, w, h)
  const { cx, cy, R } = lay
  const sample = SAMPLES[0]!
  const env = (u: number) => {
    const t = u * RING_SECONDS
    if (t > sample.duration) return 0
    let m = 0
    for (let k = -2; k <= 2; k++) m = Math.max(m, sample.level(t + k * 0.012))
    return m
  }
  const extent = sample.duration / RING_SECONDS
  const seg = new Segmenter()
  const cuts: number[] = []
  for (let t = 0; t < sample.duration; t += 1 / 60) {
    const c = seg.step(t, sample.level(t))
    if (c !== null) cuts.push(c / RING_SECONDS)
  }
  const at = (u: number, r: number) => [cx + Math.sin(u * Math.PI * 2) * r, cy - Math.cos(u * Math.PI * 2) * r] as const

  // The disc's edge and the gauge.
  g.strokeStyle = colors.bone
  g.globalAlpha = 0.08
  g.lineWidth = 1
  g.beginPath()
  g.arc(cx, cy, R, 0, Math.PI * 2)
  g.stroke()
  for (let i = 0; i < 144; i++) {
    const u = (i + 0.5) / 144
    const e = env(u)
    const lit = u < extent
    g.strokeStyle = lit ? colors.accent : colors.bone
    g.globalAlpha = lit ? 0.35 + e * 0.6 : 0.14
    const [x0, y0] = at(u, R * 1.045)
    const [x1, y1] = at(u, R * 1.045 + 3 + e * R * 0.09)
    g.beginPath()
    g.moveTo(x0, y0)
    g.lineTo(x1, y1)
    g.stroke()
  }

  // Cut marks, where a pause let Hex transcribe the piece before it.
  g.strokeStyle = colors.bone
  g.globalAlpha = 0.7
  for (const u of cuts) {
    const [x0, y0] = at(u, R * 0.99)
    const [x1, y1] = at(u, R * 1.17)
    g.beginPath()
    g.moveTo(x0, y0)
    g.lineTo(x1, y1)
    g.stroke()
  }

  // The band: dots in their lanes.
  let seed = 11
  const rnd = () => {
    seed = (seed * 16807) % 2147483647
    return seed / 2147483647
  }
  const ring = 0.6
  for (let i = 0; i < 9000; i++) {
    const u = rnd()
    const l = rnd() * 2 - 1
    const lane = Math.sign(l) * Math.abs(l) ** 1.25
    const r = (ring + lane * (0.03 + env(u) * 0.3)) * R
    const [x, y] = at(u, r)
    const hot = u < extent && rnd() < env(u) * 0.8
    g.fillStyle = hot ? colors.accent : colors.bone
    g.globalAlpha = hot ? 0.7 : 0.38
    g.fillRect(x - 0.5, y - 0.5, 1, 1)
  }

  // The trace to the field.
  if (lay.fieldY > 0) {
    g.strokeStyle = colors.bone
    g.globalAlpha = 0.18
    g.beginPath()
    g.moveTo(cx, cy + R * 1.24)
    g.lineTo(cx, lay.fieldY)
    g.lineTo(lay.fieldX, lay.fieldY)
    g.stroke()
  }
  g.globalAlpha = 1
}
