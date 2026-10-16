// Springs by perceptual duration and bounce, the knobs solver (knobs src/core/spring.ts): fixed 480 Hz substeps so
// motion is the same at 60 and 120 Hz, and velocity carries across retargets so every change is interruptible.
// ω = 2π / duration, ζ = 1 − bounce.

export interface SpringSpec {
  readonly duration: number
  readonly bounce: number
}

const H = 1 / 480

export function omegaOf(spec: SpringSpec) {
  return (2 * Math.PI) / Math.max(0.01, spec.duration)
}
export function zetaOf(spec: SpringSpec) {
  return spec.bounce >= 0 ? 1 - spec.bounce : 1 / (1 + spec.bounce)
}

export class Spring {
  v = 0
  target: number
  constructor(
    public x = 0,
    target = x,
  ) {
    this.target = target
  }
  /** Moves toward `target` for `dt` seconds on `spec`, keeping velocity. */
  step(dt: number, spec: SpringSpec) {
    if (this.x === this.target && this.v === 0) return this.x
    const w = omegaOf(spec)
    const z = zetaOf(spec)
    const d = Math.min(dt, 0.1)
    for (let t = 0; t < d; t += H) {
      const k = Math.min(H, d - t)
      this.v += (-w * w * (this.x - this.target) - 2 * z * w * this.v) * k
      this.x += this.v * k
    }
    if (Math.abs(this.x - this.target) < 1e-4 && Math.abs(this.v) < 4e-4) {
      this.x = this.target
      this.v = 0
    }
    return this.x
  }
  /** Follows a value the clock drives (1:1), remembering its speed for the spring that takes over. */
  follow(x: number, v: number) {
    this.x = this.target = x
    this.v = v
  }
  snap(x: number) {
    this.x = this.target = x
    this.v = 0
  }
}

/** Many springs at once (the band's envelope bins), on one spec per direction. */
export class SpringArray {
  readonly x: Float32Array
  readonly v: Float32Array
  constructor(n: number) {
    this.x = new Float32Array(n)
    this.v = new Float32Array(n)
  }
  step(dt: number, target: Float32Array, rise: SpringSpec, fall: SpringSpec) {
    const wr = omegaOf(rise)
    const zr = zetaOf(rise)
    const wf = omegaOf(fall)
    const zf = zetaOf(fall)
    const d = Math.min(dt, 0.1)
    const { x, v } = this
    for (let i = 0; i < x.length; i++) {
      const goal = target[i]!
      let xi = x[i]!
      let vi = v[i]!
      if (xi === goal && vi === 0) continue
      const up = goal > xi
      const w = up ? wr : wf
      const z = up ? zr : zf
      for (let t = 0; t < d; t += H) {
        const k = Math.min(H, d - t)
        vi += (-w * w * (xi - goal) - 2 * z * w * vi) * k
        xi += vi * k
      }
      if (Math.abs(xi - goal) < 1e-4 && Math.abs(vi) < 4e-4) {
        xi = goal
        vi = 0
      }
      x[i] = xi
      v[i] = vi
    }
  }
}

/** The draw-on curve, cubic-bezier(.45, 0, .2, 1), solved for x. */
export function drawOn(t: number) {
  if (t <= 0) return 0
  if (t >= 1) return 1
  const x1 = 0.45
  const x2 = 0.2
  const bx = (s: number) => 3 * x1 * s * (1 - s) ** 2 + 3 * x2 * s * s * (1 - s) + s ** 3
  const by = (s: number) => 3 * s * s * (1 - s) + s ** 3
  let lo = 0
  let hi = 1
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2
    if (bx(mid) < t) lo = mid
    else hi = mid
  }
  return by((lo + hi) / 2)
}
