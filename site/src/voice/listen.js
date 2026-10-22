// PROTOTYPE (site/voice-game): microphone → 16 kHz chunks for the recognizer, plus a live level
// for Bean's mouth. Nothing is stored; stop() releases the microphone.

const WORKLET = `
class Tap extends AudioWorkletProcessor {
  constructor(o) {
    super()
    this.ratio = sampleRate / 16000
    this.acc = 0
    this.sum = 0
    this.n = 0
    this.out = new Float32Array(640)
    this.len = 0
    this.peak = 0
  }
  process(inputs) {
    const ch = inputs[0] && inputs[0][0]
    if (!ch) return true
    for (let i = 0; i < ch.length; i++) {
      const s = ch[i]
      const a = s < 0 ? -s : s
      if (a > this.peak) this.peak = a
      // Box-filter decimation to 16 kHz; plenty for speech.
      this.sum += s
      this.n++
      this.acc += 1
      if (this.acc >= this.ratio) {
        this.acc -= this.ratio
        this.out[this.len++] = this.sum / this.n
        this.sum = 0
        this.n = 0
        if (this.len === this.out.length) {
          let e = 0
          for (let k = 0; k < this.len; k++) e += this.out[k] * this.out[k]
          this.port.postMessage({ samples: this.out.slice(0, this.len), rms: Math.sqrt(e / this.len), peak: this.peak })
          this.len = 0
          this.peak = 0
        }
      }
    }
    return true
  }
}
registerProcessor("hex-tap", Tap)
`

const MODELS = { tiny: "onnx-community/moonshine-tiny-ONNX", base: "onnx-community/moonshine-base-ONNX" }
export const MODEL = MODELS[new URLSearchParams(location.search).get("model")] ?? MODELS.tiny

export function canListen() {
  return !!(navigator.mediaDevices?.getUserMedia && window.AudioWorkletNode && window.Worker && typeof WebAssembly === "object" && isSecureContext)
}

// Ask for the microphone. Call from the click so the permission prompt belongs to it.
export function askMic() {
  return navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  })
}

// The worker is created once per page and keeps the model warm between rounds.
let shared = null
export function recognizer(prefer) {
  if (shared) return shared
  const worker = new Worker(new URL("./asr-worker.js", import.meta.url), { type: "module" })
  const listeners = new Set()
  worker.onmessage = (e) => {
    if (e.data.type === "error") console.warn("[voice] recognizer:", e.data.message)
    for (const fn of listeners) fn(e.data)
  }
  worker.onerror = (e) => console.error("[voice] worker failed:", e.message)
  let ready = null
  shared = {
    worker,
    listen(fn) {
      listeners.add(fn)
      return () => listeners.delete(fn)
    },
    load() {
      ready ??= new Promise((resolve, reject) => {
        const off = shared.listen((m) => {
          if (m.type === "ready") {
            off()
            resolve(m)
          } else if (m.type === "error" && !m.during) {
            off()
            ready = null
            reject(new Error(m.message))
          }
        })
        // WASM by default: on the Macs tested it beat WebGPU (≈50 ms vs ≈130 ms per word) and its
        // int8 weights are a third of the download. ?asr=webgpu opts in.
        worker.postMessage({ type: "load", model: MODEL, prefer: prefer ?? "wasm" })
      })
      return ready
    },
  }
  return shared
}

// Streams the mic into the recognizer. `onLevel(0..1)` runs per 40 ms chunk.
export async function listen({ context, stream, asr, onLevel }) {
  const url = URL.createObjectURL(new Blob([WORKLET], { type: "text/javascript" }))
  await context.audioWorklet.addModule(url)
  URL.revokeObjectURL(url)
  const src = context.createMediaStreamSource(stream)
  const tap = new AudioWorkletNode(context, "hex-tap", { numberOfInputs: 1, numberOfOutputs: 1, channelCount: 1 })
  const sink = context.createGain()
  sink.gain.value = 0
  src.connect(tap)
  tap.connect(sink)
  sink.connect(context.destination)
  asr.worker.postMessage({ type: "reset" })
  // Sample index → wall clock, so latency can be measured from the end of speech.
  let sent = 0
  const clock = []
  tap.port.onmessage = (e) => {
    const { samples, rms } = e.data
    sent += samples.length
    clock.push([sent, performance.now()])
    if (clock.length > 600) clock.splice(0, 100)
    asr.worker.postMessage({ type: "audio", samples }, [samples.buffer])
    // Speech sits around −30 to −12 dBFS; map that onto the mouth.
    const dbfs = 20 * Math.log10(rms + 1e-8)
    onLevel?.(Math.max(0, Math.min(1, (dbfs + 50) / 34)))
  }
  const timeOf = (sample) => {
    for (const [n, t] of clock) if (n >= sample) return t
    return performance.now()
  }
  return {
    timeOf,
    stop() {
      tap.port.onmessage = null
      try {
        src.disconnect()
        tap.disconnect()
        sink.disconnect()
      } catch {}
      for (const t of stream.getTracks()) t.stop()
    },
  }
}
