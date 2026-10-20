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
          // The canvas has drawn the head; the still can go.
          root.current?.setAttribute("data-live", "")
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
    // Reduced motion and phones keep the still until the visitor asks for the show: building
    // the renderer is a long task on a phone CPU.
    if (reduced.current || matchMedia("(pointer: coarse), (max-width: 40rem)").matches) return
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
    if (!muted && context.current?.state === "suspended" && !document.hidden) {
      void context.current.resume()
      return
    }
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

  // Browsers don't count a modifier key as permission to play, so starting with ⌥ leaves the
  // audio suspended. Say so, and let the next click or ordinary key turn the sound on.
  const [blocked, setBlocked] = useState(false)
  useEffect(() => {
    const ctx = context.current
    if (phase !== "started" || !ctx) return
    const check = () => setBlocked(ctx.state === "suspended" && !readMuted() && !document.hidden)
    const wake = (e: Event) => {
      if (e.target instanceof Element && e.target.closest(".sound")) return
      if (!readMuted() && ctx.state === "suspended" && !document.hidden) void ctx.resume()
    }
    check()
    ctx.addEventListener("statechange", check)
    addEventListener("pointerdown", wake)
    addEventListener("keydown", wake)
    return () => {
      ctx.removeEventListener("statechange", check)
      removeEventListener("pointerdown", wake)
      removeEventListener("keydown", wake)
    }
  }, [phase, muted])

  return { phase, muted, blocked, start, load, toggleMuted }
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

export function SoundToggle({ muted, blocked, onToggle }: { muted: boolean; blocked: boolean; onToggle(): void }) {
  const off = muted || blocked
  return (
    <button
      type="button"
      className="sound"
      data-blocked={blocked || undefined}
      aria-pressed={!off}
      aria-label={blocked ? "Turn on sound" : muted ? "Unmute" : "Mute"}
      onClick={onToggle}
    >
      {blocked && <span className="sound__label">Click for sound</span>}
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="M3 8v4h3l4 3V5L6 8z" />
        {off ? <path d="m13.5 8 4 4m0-4-4 4" fill="none" /> : <path d="M13 7.5a3.5 3.5 0 0 1 0 5M15.2 5.5a6.5 6.5 0 0 1 0 9" fill="none" />}
      </svg>
    </button>
  )
}
