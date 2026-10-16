// The hero's scripted dictations. There's no microphone: each sample is a sentence with word
// timings and a synthetic speech envelope. The director plays a take the way Hex handles one:
// hold to record, a pause of 600 ms after at least 3 s of speech (or 300 ms after 8 s) cuts a
// piece that's transcribed in the background, release transcribes only the tail and pastes once,
// and a capture shorter than 300 ms is discarded. Timings of the real app live in
// docs/features/dictation.md.

export interface Word {
  text: string
  t0: number
  t1: number
}

export interface Sample {
  text: string
  words: Word[]
  duration: number
  level(t: number): number
}

/** Seconds of speech that fill the ring once. */
export const RING_SECONDS = 9
const MIN_CAPTURE = 0.3
const RELEASE_TAIL = 0.2
/** Release to pasted text in the demo, about what Hex takes for a short tail. */
export const PASTE_DELAY = 0.24
const SPEECH = 0.06

const SENTENCES = [
  "Can we move the design review to Thursday afternoon? I'd like Priya to see the new onboarding flow before we lock it.",
  "Pick up oat milk, coffee filters and something for dinner, and I'll be home around seven.",
  "The tests pass locally, but the Linux job still fails on CI. I'll look at it after lunch.",
]

function rng(seed: number) {
  let s = seed
  return () => {
    s = (s * 16807) % 2147483647
    return s / 2147483647
  }
}

function syllables(word: string) {
  const groups = word.toLowerCase().match(/[aeiouy]+/g)?.length ?? 1
  return Math.max(1, groups - (/[^aeiou]e$/i.test(word) && groups > 1 ? 1 : 0))
}

export function makeSample(text: string, seed: number): Sample {
  const rnd = rng(seed * 7919 + 17)
  const words: Word[] = []
  const bumps: { t0: number; t1: number; a: number }[] = []
  let t = 0.32
  const all = text.split(/\s+/)
  all.forEach((raw, index) => {
    const n = syllables(raw.replace(/[^a-z']/gi, ""))
    const t0 = t
    for (let i = 0; i < n; i++) {
      const d = 0.13 + rnd() * 0.08
      bumps.push({ t0: t, t1: t + d, a: 0.5 + rnd() * 0.5 })
      t += d
    }
    words.push({ text: raw, t0, t1: t })
    if (index === all.length - 1) return
    t += 0.05 + rnd() * 0.05
    if (/[,;]$/.test(raw)) t += 0.58
    if (/[.?!]$/.test(raw)) t += 0.68
  })
  const duration = t + 0.2
  const level = (time: number) => {
    let v = 0
    for (const b of bumps) {
      if (time < b.t0 || time > b.t1) continue
      const x = (time - b.t0) / (b.t1 - b.t0)
      const shape = Math.sin(Math.PI * x) ** 0.7 * (1 - 0.35 * x)
      v = Math.max(v, b.a * shape * (0.86 + 0.14 * Math.sin(time * 70 + b.t0 * 13)))
    }
    return v
  }
  return { text, words, duration, level }
}

export const SAMPLES = SENTENCES.map((s, i) => makeSample(s, i + 1))

export type Phase = "ready" | "recording" | "transcribing" | "pasted"

export interface Piece {
  /** Turns round the ring. */
  from: number
  to: number
  /** When it locked (s, take time), or null while it's still being spoken. */
  lockedAt: number | null
}

/** The pause rule: cut mid-pause, in take seconds. */
export class Segmenter {
  private pieceStart = 0
  private silenceStart: number | null = null
  step(t: number, level: number): number | null {
    if (level > SPEECH) {
      this.silenceStart = null
      return null
    }
    if (this.silenceStart === null) this.silenceStart = t
    const silent = t - this.silenceStart
    const piece = this.silenceStart - this.pieceStart
    if ((silent >= 0.6 && piece >= 3) || (silent >= 0.3 && piece >= 8)) {
      const cut = this.silenceStart + silent / 2
      this.pieceStart = cut
      this.silenceStart = null
      return cut
    }
    return null
  }
}

export interface Take {
  id: number
  sample: Sample
  auto: boolean
  /** Seconds of capture. */
  elapsed: number
  /** Seconds since the press, still running after release. */
  clock: number
  releasedAt: number | null
  cuts: number[]
  segmenter: Segmenter
}

export interface DirectorEvents {
  phase(p: Phase): void
  transcript(id: number, words: string[]): void
  pieces(n: number): void
}

/** Plays takes and keeps what the instrument draws: the envelope by angle and the level scope. */
export class Director {
  phase: Phase = "ready"
  take: Take | null = null
  /** The envelope round the ring, 0–1 per bin. */
  readonly bins: Float32Array
  /** Recent input level, oldest first. */
  readonly levels: Float32Array
  /** Gauge and bracket light, 0–1. */
  glow = 0
  level = 0
  /** The paste's travel, 0–1, or -1. */
  paste = -1
  private next = 0
  private ids = 0
  private sincePhase = 0
  private levelClock = 0
  private written = 0

  constructor(
    private readonly events: DirectorEvents,
    binCount: number,
    levelCount: number,
  ) {
    this.bins = new Float32Array(binCount)
    this.levels = new Float32Array(levelCount)
  }

  get idleFor() {
    return this.phase === "ready" || this.phase === "pasted" ? this.sincePhase : 0
  }

  private setPhase(p: Phase) {
    if (p === this.phase) return
    this.phase = p
    this.sincePhase = 0
    this.events.phase(p)
  }

  press(auto: boolean) {
    if (this.phase === "recording") {
      // The hand takes over an automatic take.
      if (this.take && this.take.auto && !auto) this.take.auto = false
      return
    }
    const sample = SAMPLES[this.next % SAMPLES.length]!
    this.next++
    this.take = {
      id: ++this.ids,
      sample,
      auto,
      elapsed: 0,
      clock: 0,
      releasedAt: null,
      cuts: [],
      segmenter: new Segmenter(),
    }
    this.written = 0
    this.paste = -1
    this.events.pieces(0)
    this.setPhase("recording")
  }

  release() {
    const take = this.take
    if (!take || this.phase !== "recording") return
    if (take.elapsed < MIN_CAPTURE) {
      this.take = null
      this.setPhase("ready")
      return
    }
    take.releasedAt = take.elapsed
    this.paste = 0
    this.setPhase("transcribing")
  }

  /** The pieces to bracket, in turns. */
  pieces(): Piece[] {
    const take = this.take
    if (!take) return []
    const out: Piece[] = []
    let from = 0
    for (const c of take.cuts) {
      out.push({ from: turnsOf(from), to: turnsOf(c), lockedAt: c })
      from = c
    }
    const end = take.releasedAt ?? take.elapsed
    if (end - from > 0.15) out.push({ from: turnsOf(from), to: turnsOf(end), lockedAt: take.releasedAt })
    return out
  }

  head() {
    return turnsOf(this.take?.elapsed ?? 0)
  }

  step(dt: number) {
    this.sincePhase += dt
    const take = this.take
    const recording = this.phase === "recording" && take
    if (take) take.clock += dt
    if (recording) {
      take.elapsed += dt
      if (take.auto && take.elapsed >= take.sample.duration + 0.15) this.release()
    }
    const t = take?.elapsed ?? 0
    const lv = recording ? take.sample.level(t) : 0
    this.level += (lv - this.level) * (1 - Math.exp(-dt / 0.03))

    if (recording) {
      const cut = take.segmenter.step(t, lv)
      if (cut !== null) {
        take.cuts.push(cut)
        this.events.pieces(take.cuts.length)
      }
      // Write the bins the head passed this frame.
      const n = this.bins.length
      const upto = Math.min(n, Math.floor((t / RING_SECONDS) * n))
      for (let b = this.written; b < upto; b++) {
        const bt = ((b + 0.5) / n) * RING_SECONDS
        let m = 0
        for (let k = -2; k <= 2; k++) m = Math.max(m, take.sample.level(bt + k * 0.012))
        this.bins[b] = m
      }
      this.written = Math.max(this.written, upto)
    }
    // Unwritten bins (an earlier take's) fade quickly; after release the whole band settles.
    const settle = this.phase === "transcribing" || this.phase === "pasted" || this.phase === "ready"
    const kFast = Math.exp(-dt / 0.35)
    const kSettle = Math.exp(-dt / 1.1)
    for (let b = 0; b < this.bins.length; b++) {
      if (settle) this.bins[b]! *= kSettle
      else if (b >= this.written) this.bins[b]! *= kFast
    }

    if (this.phase === "transcribing" && take) {
      this.paste = Math.min(1, this.sincePhase / PASTE_DELAY)
      if (this.sincePhase >= PASTE_DELAY) {
        const end = take.releasedAt! + RELEASE_TAIL
        const words = take.sample.words.filter((w) => w.t1 <= end).map((w) => w.text)
        if (words.length) {
          this.events.transcript(take.id, words)
          this.setPhase("pasted")
        } else {
          this.setPhase("ready")
        }
      }
    } else if (this.paste >= 0) {
      // The last beads run on into the field, then the trace rests.
      this.paste = this.sincePhase < 0.3 ? 1 + this.sincePhase * 1.5 : -1
    }

    const lit = this.phase === "recording" || this.phase === "transcribing" ? 1 : 0
    const tau = lit ? 0.12 : 1.4
    this.glow += (lit - this.glow) * (1 - Math.exp(-dt / tau))

    // The scope: a sample every 25 ms.
    this.levelClock += dt
    while (this.levelClock >= 0.025) {
      this.levelClock -= 0.025
      this.levels.copyWithin(0, 1)
      this.levels[this.levels.length - 1] = this.level
    }
  }
}

function turnsOf(seconds: number) {
  return Math.min(0.999, seconds / RING_SECONDS)
}
