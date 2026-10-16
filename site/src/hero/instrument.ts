// How the instrument moves: every light, mark and gauge the shader draws is a spring here, stepped once a frame
// from what the Director holds. Nothing appears or leaves in one frame: marks draw on and cool out, a retarget keeps
// its velocity, and a take's marks keep their identity (take and index) until they have cooled, even after the next
// take begins. Pure and deterministic: the same steps give the same values.
import { Spring, SpringArray, drawOn, type SpringSpec } from "./spring"
import { PASTE_DELAY, RELEASE_TAIL, RING_SECONDS, turnsOf, type Director } from "./take"
import { motion, timing } from "./tuning"

export const CUT_SLOTS = 8
export const MARK_SLOTS = 6
/** Where the instrument's slots start in the uniform block (floats). */
export const SLOTS_AT = 52
export const SLOTS_FLOATS = 128 - SLOTS_AT

/** How long the take stays lit after its paste, while the text is read. */
const HOLD = 0.9
const METER: SpringSpec = { duration: 0.12, bounce: 0 }

interface Cut {
  slot: number
  take: number
  index: number
  at: number
  born: number
  light: Spring
}
interface Mark {
  slot: number
  take: number
  index: number
  born: number
  from: Spring
  to: Spring
  lock: Spring
  alpha: Spring
}

export class Instrument {
  t = 0
  readonly arrive = new Spring(0)
  /** The write head's marker. */
  readonly rec = new Spring(0)
  /** The write head (turns): the clock's while recording, then coasting through the tail. */
  readonly head = new Spring(0)
  /** The gauge's light across the take. */
  readonly glow = new Spring(0)
  readonly level = new Spring(0)
  /** The band's envelope as drawn. */
  readonly bins: SpringArray
  private readonly rest: Float32Array
  private cuts: Cut[] = []
  private marks: Mark[] = []
  private take = 0
  private bead = { take: 0, at: -1, light: new Spring(0) }
  /** Reduced motion: no travel, and every change a short fade. */
  reduced = false

  constructor(bins: number) {
    this.bins = new SpringArray(bins)
    this.rest = new Float32Array(bins)
  }

  private spec(s: SpringSpec): SpringSpec {
    return this.reduced ? { duration: Math.min(s.duration, 0.2), bounce: 0 } : s
  }
  /** Toward `target`, on `on` when rising and `off` when falling. */
  private light(s: Spring, target: number, dt: number, on: SpringSpec, off: SpringSpec) {
    s.target = target
    s.step(dt, this.spec(target >= s.x ? on : off))
  }
  private drawn(born: number) {
    return this.reduced ? 1 : drawOn((this.t - born) / Math.max(0.01, timing.drawOn))
  }

  step(dt: number, d: Director, ready: boolean) {
    this.t += dt
    const take = d.take
    const id = take?.id ?? 0
    const recording = d.phase === "recording" && !!take
    if (id !== this.take) {
      if (take) this.head.snap(0)
      this.take = id
    }
    const lit = recording || d.phase === "transcribing" || (d.phase === "pasted" && d.phaseTime < HOLD)

    this.light(this.arrive, ready ? 1 : 0, dt, motion.arrive, motion.arrive)
    this.light(this.rec, recording ? 1 : 0, dt, motion.lightsOn, motion.lightsOut)
    this.light(this.glow, lit && take ? 1 : 0, dt, motion.lightsOn, motion.lightsCool)
    this.light(this.level, d.level, dt, METER, METER)
    if (take && recording) this.head.follow(turnsOf(take.elapsed), 1 / RING_SECONDS)
    else if (take && take.releasedAt !== null) {
      this.head.target = turnsOf(take.releasedAt + RELEASE_TAIL)
      this.head.step(dt, this.spec(motion.headStop))
    }
    this.bins.step(dt, lit && take ? d.bins : this.rest, this.spec(motion.bandRise), this.spec(motion.bandSettle))

    // Cut marks.
    if (take)
      take.cuts.forEach((c, index) => {
        if (!this.cuts.some((k) => k.take === id && k.index === index))
          this.cuts.push({ slot: freeSlot(this.cuts, CUT_SLOTS), take: id, index, at: turnsOf(c), born: this.t, light: new Spring(0) })
      })
    for (const c of this.cuts) this.light(c.light, c.take === id && lit ? 1 : 0, dt, motion.lightsOn, motion.lightsCool)
    this.cuts = this.cuts.filter((c) => c.take === id || c.light.x > 0.002)

    // Brackets on the pieces. The live piece's end follows the head; at its cut or the release it settles there.
    const pieces = take ? d.pieces() : []
    pieces.forEach((p, index) => {
      let m = this.marks.find((k) => k.take === id && k.index === index)
      if (!m) {
        m = { slot: freeSlot(this.marks, MARK_SLOTS), take: id, index, born: this.t, from: new Spring(p.from), to: new Spring(p.to), lock: new Spring(0), alpha: new Spring(0) }
        this.marks.push(m)
      }
      const live = p.lockedAt === null && recording
      if (live) m.to.follow(p.to, 1 / RING_SECONDS)
      else {
        m.to.target = p.to
        m.to.step(dt, this.spec(motion.lock))
      }
      m.from.target = p.from
      m.from.step(dt, this.spec(motion.lock))
      m.lock.target = live ? 0 : 1
      m.lock.step(dt, this.spec(motion.lock))
    })
    for (const m of this.marks) this.light(m.alpha, m.take === id && lit ? 1 : 0, dt, motion.lightsOn, motion.lightsCool)
    this.marks = this.marks.filter((m) => m.take === id || m.alpha.x > 0.002)

    // The paste's bead: from the ring's port to the field, landing as the words arrive, then handing its light on.
    if (d.phase === "transcribing" && take && this.bead.take !== id) {
      this.bead.take = id
      this.bead.at = this.t
    }
    if (this.bead.at >= 0) {
      const travel = (this.t - this.bead.at) / PASTE_DELAY
      this.light(this.bead.light, travel < 1 ? 1 : 0, dt, motion.lightsOut, motion.lightsOut)
      if (travel >= 1 && this.bead.light.x <= 0.002) {
        this.bead.at = -1
        this.bead.light.snap(0)
      }
    }
  }

  /** Writes cuts, brackets and the bead into the uniform block from SLOTS_AT. */
  pack(p: Float32Array) {
    const o = SLOTS_AT
    // Each cut and bracket keeps the slot it was given until it has cooled, so nothing changes places.
    for (let i = 0; i < CUT_SLOTS; i++) {
      const c = this.cuts.find((k) => k.slot === i)
      p[o + i] = c ? c.at : -1
      p[o + 32 + i] = c ? this.drawn(c.born) : 0
      p[o + 40 + i] = c ? c.light.x : 0
    }
    for (let i = 0; i < MARK_SLOTS; i++) {
      const m = this.marks.find((k) => k.slot === i)
      const at = o + 8 + i * 4
      const info = o + 48 + i * 4
      if (!m) {
        p.fill(0, at, at + 4)
        p.fill(0, info, info + 4)
        continue
      }
      const from = m.from.x
      const to = Math.max(from, m.to.x)
      p[at] = (from + to) / 2
      p[at + 1] = (to - from) / 2
      p[at + 2] = Math.min(1, Math.max(0, m.lock.x))
      p[at + 3] = m.alpha.x
      p[info] = this.drawn(m.born)
      p[info + 1] = m.index + 1
      p[info + 2] = 0
      p[info + 3] = 0
    }
    const b = this.bead
    p[o + 72] = b.at < 0 ? -1 : this.reduced ? 1 : smootherstep((this.t - b.at) / PASTE_DELAY)
    p[o + 73] = b.light.x
    p[o + 74] = 0
    p[o + 75] = 0
  }

  /** Still moving: whether another frame would change anything. */
  get moving() {
    return (
      this.rec.v !== 0 ||
      this.glow.v !== 0 ||
      this.head.v !== 0 ||
      this.arrive.v !== 0 ||
      this.cuts.some((c) => c.light.v !== 0) ||
      this.marks.some((m) => m.alpha.v !== 0 || m.lock.v !== 0 || m.to.v !== 0) ||
      this.bead.at >= 0
    )
  }
}

/** Starts and lands at rest: the bead's 14-frame run leaves no last-frame step. */
function smootherstep(t: number) {
  const x = Math.min(1, Math.max(0, t))
  return x * x * x * (x * (6 * x - 15) + 10)
}

/** The lowest slot nobody holds, or -1 (not drawn) when all are taken. */
function freeSlot(held: readonly { slot: number }[], n: number) {
  for (let i = 0; i < n; i++) if (!held.some((h) => h.slot === i)) return i
  return -1
}
