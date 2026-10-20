// Web Audio mixer for the hero. Nothing is fetched until the show starts; then the clips the
// opening needs are decoded first and the rest of the bank streams in behind them. In capture
// builds every play() is logged (time, file, gain, rate, pan, bus) so a recording can be mixed
// offline without playing anything aloud.

const URLS = import.meta.glob("./audio/*.mp3", { query: "?url", import: "default", eager: true })
const urlOf = (file) => URLS[`./audio/${file}`]

export class AudioEngine {
  constructor({ clock }) {
    this.clock = clock
    this.log = import.meta.env.VITE_SHOUT_CAPTURE === "1" ? [] : null
    this.ctx = null
    this.manifest = null
    this.buffers = new Map()
    this.muted = false
    this.hidden = false
    this.duck = 0
  }

  // The context is created by the caller inside the start gesture (or later, on unmute).
  attach(context, { muted }) {
    this.muted = muted
    if (context) this.build(context)
  }

  // Resolves once the first replies and the interface sounds are decoded.
  async load(manifest) {
    this.manifest = manifest
    const first = new Set(Object.values(manifest.sfx))
    const rest = new Set()
    for (const line of Object.values(manifest.voice)) for (const t of line.takes) (line.stage === "1" ? first : rest).add(t.file)
    const queue = [...first, ...rest]
    const firstDone = Promise.all([...first].map((f) => this.fetchOne(f)))
    queue.splice(0, first.size)
    const worker = async () => {
      while (queue.length) await this.fetchOne(queue.shift())
    }
    void firstDone.then(() => Promise.all([worker(), worker(), worker()]))
    // Never hold the show hostage to a slow network.
    await Promise.race([firstDone, new Promise((r) => setTimeout(r, 2500))])
  }

  async fetchOne(file) {
    if (this.buffers.has(file)) return
    try {
      const bytes = await (await fetch(urlOf(file))).arrayBuffer()
      this.buffers.set(file, bytes)
      if (this.ctx) await this.decode(file)
    } catch {
      this.buffers.delete(file)
    }
  }

  async decode(file) {
    const v = this.buffers.get(file)
    if (!(v instanceof ArrayBuffer)) return
    this.buffers.set(file, null)
    try {
      this.buffers.set(file, await this.ctx.decodeAudioData(v))
    } catch {
      this.buffers.delete(file)
    }
  }

  get on() {
    return !this.muted && !this.hidden && this.ctx?.state === "running"
  }

  setMuted(muted, context) {
    this.muted = muted
    if (!muted && !this.ctx && context) this.build(context)
    this.sync()
  }

  setHidden(hidden) {
    this.hidden = hidden
    this.sync()
  }

  sync() {
    if (!this.ctx) return
    if (this.muted || this.hidden) this.ctx.suspend().catch(() => {})
    else this.ctx.resume().catch(() => {})
  }

  close() {
    this.ctx?.close().catch(() => {})
    this.ctx = null
  }

  build(ctx) {
    this.ctx = ctx
    const limiter = ctx.createDynamicsCompressor()
    limiter.threshold.value = -3
    limiter.knee.value = 0
    limiter.ratio.value = 20
    limiter.attack.value = 0.002
    limiter.release.value = 0.12
    const comp = ctx.createDynamicsCompressor()
    comp.threshold.value = -18
    comp.ratio.value = 3
    comp.attack.value = 0.01
    comp.release.value = 0.2
    this.master = ctx.createGain()
    this.master.gain.value = 0.8
    this.master.connect(comp).connect(limiter).connect(ctx.destination)
    this.buses = {}
    for (const [name, g] of Object.entries(BUS)) {
      const bus = ctx.createGain()
      bus.gain.value = g
      this.buses[name] = bus
    }
    this.buses.ui.connect(this.master)
    this.buses.voice.connect(this.master)
    // Impacts get brighter as intensity rises.
    this.tone = ctx.createBiquadFilter()
    this.tone.type = "lowpass"
    this.tone.frequency.value = 6000
    this.buses.impacts.connect(this.tone).connect(this.master)
    for (const f of this.buffers.keys()) void this.decode(f)
    this.sync()
  }

  setIntensity(i) {
    this.i = i
    if (!this.ctx) return
    const t = this.ctx.currentTime
    this.tone.frequency.setTargetAtTime(3500 + 16000 * i * i, t, 0.3)
    this.buses.ui.gain.setTargetAtTime(BUS.ui * (1 - 0.35 * i) * (1 - 0.6 * this.duck), t, 0.05)
  }

  sfx(name, opts = {}) {
    const file = this.manifest?.sfx[name]
    if (file) this.play(file, { bus: "ui", ...opts })
  }

  voice(file, opts = {}) {
    this.play(file, { bus: "voice", ...opts })
    if (opts.duck) {
      this.duck = 1
      clearTimeout(this.duckTimer)
      this.duckTimer = setTimeout(() => (this.duck = 0), (opts.dur ?? 1) * 1000)
    }
  }

  play(file, { bus = "ui", gain = 1, rate = 1, pan = 0 } = {}) {
    this.log?.push({ t: +this.clock().toFixed(4), file, bus, gain: +gain.toFixed(3), rate: +rate.toFixed(3), pan: +pan.toFixed(3), i: +(this.i ?? 0).toFixed(3), duck: this.duck })
    if (!this.on) return
    const buf = this.buffers.get(file)
    if (!(buf instanceof AudioBuffer)) return
    const ctx = this.ctx
    const src = ctx.createBufferSource()
    src.buffer = buf
    src.playbackRate.value = rate
    const g = ctx.createGain()
    g.gain.value = gain
    const p = ctx.createStereoPanner()
    p.pan.value = Math.max(-1, Math.min(1, pan))
    src.connect(g).connect(p).connect(this.buses[bus])
    src.start()
  }
}

const BUS = { ui: 0.42, voice: 1, impacts: 0.7 }
