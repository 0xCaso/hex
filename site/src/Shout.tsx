import { useCallback, useEffect, useRef, useState, type RefObject } from "react"

// The hero show lives in ./shout (plain JS, WebGPU with a Canvas 2D fallback). The prerendered
// page shows a still of the idle head; the engine loads when the browser is idle or the visitor
// reaches for the key, takes over the same box, and only fetches audio once they press it.

type Engine = {
  renderer: string
  start(options: { context: AudioContext | null; muted: boolean }): Promise<void>
  setMuted(muted: boolean, context?: AudioContext | null): void
  destroy(): void
}
export type Phase = "idle" | "live" | "starting" | "started"

const MUTED_KEY = "hex-shout-muted"
const readMuted = () => {
  try {
    return localStorage.getItem(MUTED_KEY) === "1"
  } catch {
    return false
  }
}
const newContext = () => {
  try {
    return new AudioContext({ latencyHint: "interactive" })
  } catch {
    return null
  }
}

export function useShout(root: RefObject<HTMLElement | null>, canvas: RefObject<HTMLCanvasElement | null>, head: RefObject<HTMLElement | null>) {
  const [phase, setPhase] = useState<Phase>("idle")
  const [muted, setMutedState] = useState(false)
  const engine = useRef<Promise<Engine | null> | null>(null)
  const live = useRef<Engine | null>(null)
  const reduced = useRef(false)

  const load = useCallback(() => {
    if (!engine.current) {
      const params = new URLSearchParams(location.search)
      engine.current = import("./shout/main.js")
        .then(({ mountShout }) =>
          mountShout({
            root: root.current,
            canvas: canvas.current,
            head: head.current,
            reduced: reduced.current,
            forceCanvas2d: params.has("canvas2d"),
          }),
        )
        .then((e: Engine) => {
          live.current = e
          setPhase((p) => (p === "idle" ? "live" : p))
          return e
        })
        .catch(() => null)
    }
    return engine.current
  }, [root, canvas, head])

  useEffect(() => {
    setMutedState(readMuted())
    reduced.current = matchMedia("(prefers-reduced-motion: reduce)").matches
    // Reduced motion keeps the still until the visitor asks for the show.
    if (reduced.current) return
    const idle = "requestIdleCallback" in window ? requestIdleCallback(() => void load(), { timeout: 2500 }) : setTimeout(() => void load(), 1200)
    return () => {
      if ("cancelIdleCallback" in window) cancelIdleCallback(idle as number)
      else clearTimeout(idle as ReturnType<typeof setTimeout>)
    }
  }, [load])

  useEffect(() => () => live.current?.destroy(), [])

  const context = useRef<AudioContext | null>(null)
  // Must run inside the click or key press: that gesture is what lets the browser play audio.
  const start = useCallback(() => {
    if (phase === "starting" || phase === "started") return
    const quiet = readMuted()
    if (!quiet) context.current ??= newContext()
    void context.current?.resume()
    setPhase("starting")
    void load().then(async (e) => {
      if (!e) return setPhase("idle")
      await e.start({ context: context.current, muted: quiet })
      setPhase("started")
    })
  }, [phase, load])

  const toggleMuted = useCallback(() => {
    const next = !muted
    setMutedState(next)
    try {
      localStorage.setItem(MUTED_KEY, next ? "1" : "0")
    } catch {}
    if (!next) {
      context.current ??= newContext()
      void context.current?.resume()
    }
    live.current?.setMuted(next, context.current)
  }, [muted])

  // Some browsers don't count a lone ⌥ press as permission to play; the next click or key does.
  useEffect(() => {
    if (phase !== "started") return
    const wake = () => {
      if (!readMuted() && context.current?.state === "suspended" && !document.hidden) void context.current.resume()
    }
    addEventListener("pointerdown", wake)
    addEventListener("keydown", wake)
    return () => {
      removeEventListener("pointerdown", wake)
      removeEventListener("keydown", wake)
    }
  }, [phase])

  return { phase, muted, start, load, toggleMuted }
}

export function OptionKey({ phase, onStart, onIntent }: { phase: Phase; onStart(): void; onIntent(): void }) {
  const [pressed, setPressed] = useState(false)
  const gone = phase === "started"

  useEffect(() => {
    if (gone) return
    const down = (e: KeyboardEvent) => {
      if (e.key !== "Alt" || e.repeat) return
      setPressed(true)
      onStart()
    }
    const up = (e: KeyboardEvent) => {
      if (e.key === "Alt") setPressed(false)
    }
    const blur = () => setPressed(false)
    addEventListener("keydown", down)
    addEventListener("keyup", up)
    addEventListener("blur", blur)
    return () => {
      removeEventListener("keydown", down)
      removeEventListener("keyup", up)
      removeEventListener("blur", blur)
    }
  }, [gone, onStart, onIntent])

  return (
    <button
      type="button"
      className="keycap"
      data-pressed={pressed || phase === "starting" || undefined}
      data-gone={gone || undefined}
      aria-label="Play the demo with sound"
      tabIndex={gone ? -1 : undefined}
      aria-hidden={gone || undefined}
      onPointerEnter={onIntent}
      onFocus={onIntent}
      onPointerDown={onIntent}
      onClick={onStart}
    >
      <span className="keycap__glow" aria-hidden="true" />
      <span className="keycap__well" aria-hidden="true" />
      <span className="keycap__cap" aria-hidden="true">
        <span className="keycap__dish">
          <svg viewBox="0 0 24 18" className="keycap__glyph">
            <path d="M2.5 3.5h5.2l6.2 11h7.6M14.2 3.5h7.3" />
          </svg>
          <span className="keycap__label">option</span>
        </span>
      </span>
      <span className="keycap__hint" aria-hidden="true">
        <span className="keycap__hint-fine">Click or press ⌥</span>
        <span className="keycap__hint-touch">Tap to play</span>
      </span>
    </button>
  )
}

export function SoundToggle({ muted, onToggle }: { muted: boolean; onToggle(): void }) {
  return (
    <button type="button" className="sound" aria-pressed={!muted} aria-label={muted ? "Unmute" : "Mute"} onClick={onToggle}>
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="M3 8v4h3l4 3V5L6 8z" />
        {muted ? <path d="m13.5 8 4 4m0-4-4 4" fill="none" /> : <path d="M13 7.5a3.5 3.5 0 0 1 0 5M15.2 5.5a6.5 6.5 0 0 1 0 9" fill="none" />}
      </svg>
    </button>
  )
}
