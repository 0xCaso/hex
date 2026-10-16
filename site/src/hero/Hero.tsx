import { useEffect, useRef, useState, type ReactNode } from "react"
import { AnimatePresence } from "motion/react"
import * as m from "motion/react-m"
import { BINS, LEVELS_N, UNIFORM_FLOATS, createEngine, type Engine } from "./engine"
import { drawStill } from "./still"
import { Director, RING_SECONDS, SAMPLES, type Phase } from "./take"
import { toggleTuning, tuning } from "./tuning"

const PHASE_LABEL: Record<Phase, string> = {
  ready: "Ready",
  recording: "Recording",
  transcribing: "Transcribing",
  pasted: "Pasted",
}

const toLinear = (x: number) => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4)
const hexLinear = (hex: string) => {
  const h = /^#?([0-9a-f]{6})$/i.exec(hex)?.[1] ?? "000000"
  return [0, 2, 4].map((i) => toLinear(parseInt(h.slice(i, i + 2), 16) / 255))
}
const PAGE = "#0e0e0d"

interface Transcript {
  id: number
  words: string[]
}

export default function Hero({ children }: { children: ReactNode }) {
  const stageRef = useRef<HTMLDivElement>(null)
  const ringRef = useRef<HTMLDivElement>(null)
  const fieldRef = useRef<HTMLDivElement>(null)
  const scopeRef = useRef<HTMLDivElement>(null)
  const clockRef = useRef<HTMLSpanElement>(null)
  const [phase, setPhase] = useState<Phase>("ready")
  const [transcript, setTranscript] = useState<Transcript | null>(null)
  const [pieces, setPieces] = useState(0)
  const [mode, setMode] = useState<"gpu" | "still" | "pending">("pending")

  useEffect(() => {
    const stage = stageRef.current!
    const ring = ringRef.current!
    const field = fieldRef.current!
    const scope = scopeRef.current!
    const reduced = matchMedia("(prefers-reduced-motion: reduce)")
    const narrow = matchMedia("(max-width: 900px)")
    let phaseAt = performance.now()
    const director = new Director(
      {
        phase: (p) => {
          phaseAt = performance.now()
          setPhase(p)
          if (p === "recording") setTranscript(null)
        },
        transcript: (id, words) => setTranscript({ id, words }),
        pieces: setPieces,
      },
      BINS,
      LEVELS_N,
    )

    // ── Layout, measured from the DOM ──
    const lay = { cx: 0, cy: 0, R: 1, fieldX: 0, fieldY: 0, scope: [0, 0, 0, 0] }
    const fit = () => {
      const s = stage.getBoundingClientRect()
      const r = ring.getBoundingClientRect()
      const f = field.getBoundingClientRect()
      const sc = scope.getBoundingClientRect()
      lay.cx = r.left - s.left + r.width / 2
      lay.cy = r.top - s.top + r.height / 2
      lay.R = r.width / 2 / 1.3
      lay.fieldX = f.left - s.left + f.width / 2
      lay.fieldY = f.top - s.top
      lay.scope = sc.width > 0 ? [sc.left - s.left, sc.top - s.top, sc.width, sc.height] : [0, 0, 0, 0]
    }

    // ── Input: the pointer lures, a press or Space dictates ──
    let lastHand = -1e9
    const lure = { x: 0, y: 0, on: 0, want: 0 }
    const onMove = (e: PointerEvent) => {
      if (e.pointerType === "touch") return
      const s = stage.getBoundingClientRect()
      lure.x = (e.clientX - s.left - lay.cx) / lay.R
      lure.y = (e.clientY - s.top - lay.cy) / lay.R
      lure.want = Math.hypot(lure.x, lure.y) < 1.15 ? 1 : 0
    }
    const onLeave = () => {
      lure.want = 0
    }
    let holding = false
    const press = () => {
      lastHand = performance.now()
      holding = true
      director.press(false)
      wake()
    }
    const releaseHand = () => {
      if (!holding) return
      holding = false
      lastHand = performance.now()
      director.release()
    }
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0) return
      e.preventDefault()
      ring.setPointerCapture(e.pointerId)
      press()
    }
    const onUp = (e: PointerEvent) => {
      if (ring.hasPointerCapture(e.pointerId)) ring.releasePointerCapture(e.pointerId)
      releaseHand()
    }
    const interactive = (t: EventTarget | null) =>
      t instanceof HTMLElement && !!t.closest("a, button, input, textarea, select, [contenteditable], .tuning")
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "`" && !interactive(e.target)) {
        toggleTuning(() => wake())
        return
      }
      if (e.code !== "Space" || interactive(e.target) || !onScreen) return
      e.preventDefault()
      if (!e.repeat) press()
    }
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.code === "Space") releaseHand()
    }
    stage.parentElement!.addEventListener("pointermove", onMove)
    stage.parentElement!.addEventListener("pointerleave", onLeave)
    ring.addEventListener("pointerdown", onDown)
    ring.addEventListener("pointerup", onUp)
    ring.addEventListener("pointercancel", onUp)
    window.addEventListener("keydown", onKeyDown)
    window.addEventListener("keyup", onKeyUp)
    window.addEventListener("blur", releaseHand)
    if (new URLSearchParams(location.search).has("tune")) toggleTuning(() => wake())

    // ── Running: only on screen, in a visible tab, and with reduced motion only while a take plays ──
    let onScreen = true
    let running = false
    let raf = 0
    let engine: Engine | null = null
    let disposed = false
    const io = new IntersectionObserver((es) => {
      onScreen = es.some((e) => e.isIntersecting)
      wake()
    })
    io.observe(stage)
    const wanted = () => {
      if (!engine || !onScreen || document.visibilityState !== "visible") return false
      if (!reduced.matches) return true
      return director.phase === "recording" || director.phase === "transcribing" || performance.now() - phaseAt < 4000 || lure.on > 0.01
    }
    const wake = () => {
      const go = wanted()
      if (go && !running) {
        running = true
        last = performance.now()
        raf = requestAnimationFrame(frame)
      } else if (!go && running) {
        running = false
        cancelAnimationFrame(raf)
      }
    }
    document.addEventListener("visibilitychange", wake)
    let still: HTMLCanvasElement | null = null
    const paintStill = () => {
      if (!still) return
      drawStill(still, lay, { bone: tuning.bone, accent: tuning.accent })
    }
    const ro = new ResizeObserver(() => {
      fit()
      paintStill()
    })
    ro.observe(stage)
    ro.observe(ring)

    // ── The frame ──
    const params = new Float32Array(UNIFORM_FLOATS)
    const voice = new Float32Array(BINS + LEVELS_N)
    const page = hexLinear(PAGE)
    let simTime = 0
    let frameNo = 0
    let arrive = 0
    let last = performance.now()
    let clockShown = ""
    const marks = new Float32Array(24)
    const frame = (now: number) => {
      if (!running || !engine) return
      const dt = Math.min(1 / 30, Math.max(0, (now - last) / 1000))
      last = now
      if (
        !reduced.matches &&
        !holding &&
        director.idleFor > (director.phase === "pasted" ? 4.5 : 1.6) &&
        now - lastHand > 9000
      )
        director.press(true)
      director.step(dt)
      simTime += dt
      frameNo++
      arrive = Math.min(1, arrive + dt / 0.6)
      lure.on += (lure.want - lure.on) * (1 - Math.exp(-dt / 0.25))

      const take = director.take
      const clock =
        director.phase === "recording" && take ? `${Math.floor(take.elapsed / 60)}:${(take.elapsed % 60).toFixed(1).padStart(4, "0")}` : ""
      if (clock !== clockShown && clockRef.current) {
        clockRef.current.textContent = clock
        clockShown = clock
      }

      const p = params
      const set = (i: number, a: number, b: number, c: number, d: number) => {
        p[i * 4] = a
        p[i * 4 + 1] = b
        p[i * 4 + 2] = c
        p[i * 4 + 3] = d
      }
      const cell = (2.2 / 192) ** 2
      const band = Math.PI * 2 * tuning.ring * (2 * tuning.rest + 0.12)
      const meanNear = engine.agents / (band / cell)
      const meanFar = engine.agents / ((Math.PI * 0.85 * 0.85) / cell)
      const recording = director.phase === "recording" ? 1 : 0
      set(0, tuning.cohesion, tuning.alignment, tuning.separation, tuning.speed)
      set(1, tuning.pull, tuning.drift, dt, simTime)
      set(2, lure.x, lure.y, 0.35, lure.on)
      set(3, engine.agents, frameNo, meanNear, meanFar)
      set(4, 0, 0, 1, 0.5 ** (dt / tuning.trail))
      set(5, lay.cx, lay.cy, lay.R, arrive)
      set(6, lay.scope[0]!, lay.scope[1]!, lay.scope[2]!, lay.scope[3]!)
      set(7, lay.fieldX, lay.fieldY, director.paste, recording)
      set(8, director.head(), director.head(), director.level, director.glow)
      set(9, tuning.ring, tuning.rest, tuning.voice, tuning.agitation)
      const accent = hexLinear(tuning.accent)
      const bone = hexLinear(tuning.bone)
      set(10, accent[0]!, accent[1]!, accent[2]!, 1)
      set(11, bone[0]!, bone[1]!, bone[2]!, 0)
      set(12, page[0]!, page[1]!, page[2]!, 0)
      const cuts = take?.cuts ?? []
      for (let i = 0; i < 8; i++) p[52 + i] = i < cuts.length ? Math.min(0.999, cuts[i]! / RING_SECONDS) : -1
      marks.fill(0)
      const clockNow = take?.clock ?? 0
      director.pieces().slice(0, 6).forEach((pc, i) => {
        const lock = pc.lockedAt === null ? 0 : Math.min(1, (clockNow - pc.lockedAt) / 0.4)
        marks[i * 4] = (pc.from + pc.to) / 2
        marks[i * 4 + 1] = (pc.to - pc.from) / 2
        marks[i * 4 + 2] = Math.max(0, lock)
        marks[i * 4 + 3] = director.glow
      })
      p.set(marks, 60)
      voice.set(director.bins, 0)
      voice.set(director.levels, BINS)
      engine.frame(p, voice, { bloom: tuning.bloom, exposure: tuning.exposure })
      if (!wanted()) {
        running = false
        return
      }
      raf = requestAnimationFrame(frame)
    }

    fit()
    const fallback = () => {
      if (disposed) return
      still = document.createElement("canvas")
      still.className = "hero-canvas"
      still.setAttribute("aria-hidden", "true")
      stage.append(still)
      paintStill()
      setMode("still")
      // The still shows a take just finished: the first sample, pasted.
      const sample = SAMPLES[0]!
      setTranscript({ id: 0, words: sample.words.map((w) => w.text) })
      setPhase("pasted")
      setPieces(1)
    }
    const noGpu = new URLSearchParams(location.search).has("nogpu") || !("gpu" in navigator)
    if (noGpu) fallback()
    else
      createEngine(stage, { agents: narrow.matches ? 8192 : 16384, maxDpr: narrow.matches ? 1.5 : 2, page })
        .then((e) => {
          if (disposed) return e?.destroy()
          if (!e) return fallback()
          engine = e
          setMode("gpu")
          Object.assign(window, { __hexHero: { timings: () => e.timings(), agents: e.agents } })
          wake()
        })
        .catch((err) => {
          console.warn("hex hero: no WebGPU", err)
          fallback()
        })

    return () => {
      disposed = true
      running = false
      cancelAnimationFrame(raf)
      io.disconnect()
      ro.disconnect()
      document.removeEventListener("visibilitychange", wake)
      stage.parentElement?.removeEventListener("pointermove", onMove)
      stage.parentElement?.removeEventListener("pointerleave", onLeave)
      ring.removeEventListener("pointerdown", onDown)
      ring.removeEventListener("pointerup", onUp)
      ring.removeEventListener("pointercancel", onUp)
      window.removeEventListener("keydown", onKeyDown)
      window.removeEventListener("keyup", onKeyUp)
      window.removeEventListener("blur", releaseHand)
      engine?.destroy()
      still?.remove()
    }
  }, [])

  const live = mode === "gpu"
  return (
    <section className="hero" data-mode={mode}>
      <div className="hero__stage" ref={stageRef} />
      <div className="hero__inner">
        <div className="hero__copy">{children}</div>
        <div className="instrument" aria-label="Sample dictation">
          <div className="instrument__main">
            <div className="instrument__ring" ref={ringRef} data-live={live} />
            <div className="instrument__field" ref={fieldRef}>
              <AnimatePresence mode="popLayout" initial={false}>
                {transcript ? (
                  <m.p
                    key={transcript.id}
                    className="field__text"
                    exit={{ opacity: 0, filter: "blur(3px)", scale: 0.98, transition: { duration: 0.11, ease: [0.4, 0, 1, 1] } }}
                  >
                    {transcript.words.map((w, i) => (
                      <m.span
                        key={i}
                        initial={transcript.id === 0 ? false : { opacity: 0, filter: "blur(3px)", y: 2 }}
                        animate={{ opacity: 1, filter: "blur(0px)", y: 0 }}
                        transition={{ duration: 0.16, delay: i * 0.012, ease: [0.2, 0.8, 0.2, 1] }}
                      >
                        {w}{" "}
                      </m.span>
                    ))}
                    <span className="field__caret" aria-hidden="true" />
                  </m.p>
                ) : (
                  <m.p
                    key="empty"
                    className="field__text field__text--empty"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0, transition: { duration: 0.11 } }}
                  >
                    <span className="field__caret" aria-hidden="true" />
                    {mode === "still" && <span className="field__hint">Your words appear where you're typing</span>}
                  </m.p>
                )}
              </AnimatePresence>
            </div>
            {mode !== "still" ? (
              <p className="instrument__note">
                <span className="instrument__try">
                  <span className="only-fine">Hold Space or press the ring</span>
                  <span className="only-coarse">Press and hold the ring</span> to dictate a sample.
                </span>
                <span>Simulated, without your microphone.</span>
              </p>
            ) : (
              <p className="instrument__note">A still of the live display, which needs WebGPU.</p>
            )}
          </div>
          <div className="instrument__side">
            <div className="readout__state">
              <span className={`readout__dot readout__dot--${phase}`} aria-hidden="true" />
              <span>{PHASE_LABEL[phase]}</span>
              <span className="readout__clock" ref={clockRef} />
            </div>
            <div className="instrument__scope" ref={scopeRef} aria-hidden="true" />
            <dl className="readout__rows">
              <dt>Model</dt>
              <dd>Parakeet Unified English</dd>
              <dt>Runs on</dt>
              <dd>This Mac, on Metal</dd>
              <dt>Uploaded</dt>
              <dd>0 bytes</dd>
              <dt>Pieces</dt>
              <dd>{pieces ? `${pieces} during pauses` : "None yet"}</dd>
            </dl>
          </div>
        </div>
      </div>
    </section>
  )
}
