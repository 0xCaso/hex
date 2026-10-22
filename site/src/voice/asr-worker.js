// PROTOTYPE (site/voice-game): local speech recognition for "Play with your voice".
// Moonshine runs in this worker through transformers.js (WebGPU when the browser has it, WASM
// otherwise). Audio arrives as 16 kHz mono chunks; an energy gate cuts utterances, and while one
// is open the growing clip is re-transcribed so words show up before the speaker stops.
import { env, pipeline } from "@huggingface/transformers"

env.allowLocalModels = false

const RATE = 16000
const FRAME = 320 // 20 ms
const PRE_ROLL = RATE * 0.2
const HANGOVER = 0.24 // seconds of quiet that end an utterance
const INTERIM_EVERY = 0.3
const QUICK = 0.09 // seconds of quiet before a speculative pass
const MAX_SEG = RATE * 6

let asr = null
let device = "wasm"
let busy = false

// Rolling input, VAD state, and the open utterance.
const ring = new Float32Array(RATE * 2)
let ringPos = 0
let total = 0 // samples received
const segBuf = new Float32Array(MAX_SEG + PRE_ROLL + FRAME * 4)
let frame = new Float32Array(FRAME)
let framePos = 0
let floor = -60
let loudFrames = 0
let quiet = 0
let seg = null // { id, len, start, lastVoice, lastInterim }
let segId = 0
let pendingFinal = []

const post = (m) => self.postMessage(m)

self.onmessage = async (e) => {
  const m = e.data
  if (m.type === "load") return load(m)
  if (m.type === "audio") return feed(m.samples)
  if (m.type === "reset") {
    seg = null
    pendingFinal = []
    loudFrames = 0
  }
}

async function load({ model, prefer, dtype: forced }) {
  const t0 = performance.now()
  const gpu = prefer !== "wasm" && "gpu" in navigator && (await navigator.gpu.requestAdapter().catch(() => null))
  // A software adapter (SwiftShader) is slower than WASM.
  device = gpu && !gpu.info?.isFallbackAdapter && !gpu.isFallbackAdapter ? "webgpu" : "wasm"
  const dtype = forced ?? (device === "webgpu" ? { encoder_model: "fp32", decoder_model_merged: "q4" } : { encoder_model: "q8", decoder_model_merged: "q8" })
  const files = new Map()
  try {
    asr = await pipeline("automatic-speech-recognition", model, {
      device,
      dtype,
      progress_callback: (p) => {
        if (p.status !== "progress" && p.status !== "done") return
        const f = files.get(p.file) ?? { loaded: 0, total: 0 }
        if (p.total) f.total = p.total
        f.loaded = p.status === "done" ? f.total || f.loaded : p.loaded
        files.set(p.file, f)
        let loaded = 0
        let all = 0
        for (const v of files.values()) {
          loaded += v.loaded
          all += v.total
        }
        post({ type: "progress", loaded, total: all })
      },
    })
    // Warm the kernels so the first real word isn't the slow one.
    await asr(new Float32Array(RATE * 0.6), { max_new_tokens: 4 })
    post({ type: "ready", device, dtype, ms: Math.round(performance.now() - t0) })
  } catch (err) {
    post({ type: "error", message: String(err?.message ?? err) })
  }
}

const db = (x) => 10 * Math.log10(x + 1e-10)

function feed(samples) {
  for (let i = 0; i < samples.length; i++) {
    const s = samples[i]
    ring[ringPos] = s
    ringPos = (ringPos + 1) % ring.length
    total++
    if (seg) segBuf[seg.len++] = s
    frame[framePos++] = s
    if (framePos === FRAME) {
      onFrame(frame)
      framePos = 0
    }
  }
  pump()
}

function onFrame(f) {
  let e = 0
  for (let i = 0; i < f.length; i++) e += f[i] * f[i]
  const level = db(e / f.length)
  // The floor follows quiet stretches quickly and loud ones slowly.
  floor = level < floor ? floor + (level - floor) * 0.2 : floor + Math.min(0.02, level - floor)
  floor = Math.max(-80, Math.min(floor, -28))
  const loud = level > Math.max(floor + 11, -52)
  if (loud) {
    loudFrames++
    quiet = 0
  } else {
    loudFrames = 0
    quiet += FRAME / RATE
  }
  const end = total // sample index just after this frame
  if (!seg && loudFrames >= 2) openSeg(end)
  if (seg) {
    if (loud) {
      seg.lastVoice = end
      seg.quick = false
    } else if (quiet >= HANGOVER) closeSeg()
    // A short pause is often the end of a word: transcribe now, ahead of the final pass.
    else if (quiet >= QUICK && !seg.quick) seg.quick = seg.force = true
    if (seg && seg.len >= MAX_SEG) closeSeg()
  }
}

function openSeg(end) {
  const pre = Math.min(PRE_ROLL + FRAME * 2, total, ring.length)
  for (let i = 0; i < pre; i++) segBuf[i] = ring[(ringPos - pre + i + ring.length * 2) % ring.length]
  seg = { id: ++segId, len: pre, start: end - pre, lastVoice: end, lastInterim: 0 }
  post({ type: "vad", speaking: true, seg: seg.id })
}

const clipOf = (s) => segBuf.slice(0, s.len)

function closeSeg() {
  const s = seg
  seg = null
  post({ type: "vad", speaking: false, seg: s.id })
  // Ignore clicks and pops: under 120 ms of voice is not a word.
  if (s.lastVoice - s.start - PRE_ROLL < RATE * 0.12) return
  pendingFinal.push({ id: s.id, clip: clipOf(s), speechEnd: s.lastVoice, closedAt: performance.now() })
}

async function pump() {
  if (busy || !asr) return
  let job = pendingFinal.shift()
  let final = true
  if (!job && seg && (seg.force || ((seg.len - seg.lastInterim) / RATE >= INTERIM_EVERY && seg.len > RATE * 0.35))) {
    seg.force = false
    seg.lastInterim = seg.len
    job = { id: seg.id, clip: clipOf(seg), speechEnd: seg.lastVoice, closedAt: 0 }
    final = false
  }
  if (!job) return
  busy = true
  const t0 = performance.now()
  try {
    // Moonshine emits roughly 6 tokens a second of speech; a cap stops runaway repeats.
    const out = await asr(job.clip, { max_new_tokens: Math.max(6, Math.ceil((job.clip.length / RATE) * 8)) })
    post({ type: "hyp", seg: job.id, final, text: out.text ?? "", inferMs: Math.round(performance.now() - t0), clipMs: Math.round((job.clip.length / RATE) * 1000), speechEnd: job.speechEnd, closedAt: job.closedAt, doneAt: performance.now() })
  } catch (err) {
    post({ type: "error", message: String(err?.message ?? err), during: "transcribe" })
  }
  busy = false
  if (pendingFinal.length || seg) pump()
}
