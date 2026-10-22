// PROTOTYPE (site/voice-game): the "Play with your voice" overlay. Loaded only after the visitor
// opts in; it wires the microphone and the local recognizer into the Shout engine's game.
import { useCallback, useEffect, useRef, useState } from "react"
import "./voice-game.css"

export type VoiceSession = {
  engine: Promise<any>
  mic: Promise<MediaStream | null>
  micContext: AudioContext
  sfx: AudioContext | null
  muted: boolean
  reduced: boolean
  run?: Promise<void>
}

type Stats = { score: number; combo: number; best: number; words: number; broken: number; missed: number; left: number }
type Phase = "starting" | "loading" | "count" | "play" | "over"
const ZERO: Stats = { score: 0, combo: 0, best: 0, words: 0, broken: 0, missed: 0, left: 50 }

declare global {
  interface Window {
    __voice?: { log: any[]; ready?: any; over?: any; micAt?: number }
  }
}

export default function VoiceGame({ session, onClose }: { session: VoiceSession; onClose(): void }) {
  const [phase, setPhase] = useState<Phase>("starting")
  const [stats, setStats] = useState<Stats>(ZERO)
  const [final, setFinal] = useState<Stats | null>(null)
  const [heard, setHeard] = useState<{ text: string; final: boolean; key: number } | null>(null)
  const [progress, setProgress] = useState<{ loaded: number; total: number } | null>(null)
  const [typing, setTyping] = useState(false)
  const [note, setNote] = useState<string | null>(null)
  const game = useRef<any>(null)
  const listener = useRef<any>(null)
  const input = useRef<HTMLInputElement>(null)
  const closing = useRef(false)

  const startListening = useCallback(async (stream: MediaStream | null) => {
    if (!stream) {
      setTyping(true)
      setNote("No microphone, so type the words instead.")
      return false
    }
    const { recognizer, listen } = await import("./voice/listen.js")
    const asr = recognizer(new URLSearchParams(location.search).get("asr"))
    if (session.micContext.state !== "running") await session.micContext.resume().catch(() => {})
    listener.current = await listen({ context: session.micContext, stream, asr, onLevel: (v: number) => game.current?.setMic(v) })
    return true
  }, [session])

  useEffect(() => {
    // Runs once per session (StrictMode re-mounts in development).
    session.run ??= (async () => {
      window.__voice = { log: [] }
      const engine = await session.engine
      const g = await engine?.play?.({
        context: session.sfx,
        muted: session.muted,
        onEvent: (e: any) => {
          if (e.type === "state") {
            setStats({ score: e.score, combo: e.combo, best: e.best, words: e.words, broken: e.broken, missed: e.missed, left: e.left })
            if (e.phase === "count" || e.phase === "play") setPhase(e.phase)
          } else if (e.type === "heard") {
            setHeard({ text: e.text, final: e.final, key: performance.now() })
          } else if (e.type === "over") {
            window.__voice!.over = e
            console.info("[voice] round over", JSON.stringify({ ...e, latency: undefined }), JSON.stringify(e.latency))
            setFinal({ ...e, left: 0 })
            setPhase("over")
            listener.current?.stop()
            listener.current = null
          }
        },
      })
      if (!g) return onClose()
      game.current = g
      const { recognizer } = await import("./voice/listen.js")
      const asr = recognizer(new URLSearchParams(location.search).get("asr"))
      asr.listen((m: any) => {
        if (m.type === "progress") setProgress({ loaded: m.loaded, total: m.total })
        else if (m.type === "hyp") {
          const l = listener.current
          const speechEnd = l?.timeOf(m.speechEnd)
          const meta = { speechEnd, inferMs: m.inferMs, clipMs: m.clipMs, final: m.final }
          window.__voice!.log.push({ text: m.text, at: performance.now(), ...meta })
          game.current?.hear(m.text, { seg: m.seg, final: m.final, meta })
        }
      })
      const stream = await session.mic
      window.__voice!.micAt = performance.timeOrigin + performance.now()
      const ok = await startListening(stream)
      if (ok) {
        setPhase("loading")
        try {
          const ready = await asr.load()
          window.__voice!.ready = ready
          console.info("[voice] model ready", JSON.stringify(ready))
        } catch (err) {
          console.warn("[voice] speech model failed", err)
          setTyping(true)
          setNote("Speech recognition didn't load here, so type the words instead.")
        }
      }
      if (!closing.current) g.begin()
    })()
  }, [session, onClose, startListening])

  const quit = useCallback(() => {
    if (closing.current) return
    closing.current = true
    listener.current?.stop()
    listener.current = null
    void session.micContext.close().catch(() => {})
    game.current?.quit()
    onClose()
  }, [session, onClose])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") quit()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [quit])

  useEffect(() => {
    if (typing && (phase === "play" || phase === "count")) input.current?.focus()
  }, [typing, phase])

  const again = useCallback(async () => {
    setFinal(null)
    setHeard(null)
    const stream = typing ? null : await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true } }).catch(() => null)
    if (!typing) await startListening(stream)
    game.current?.restart()
  }, [typing, startListening])

  const download = useCallback(() => {
    const el = document.querySelector(".hero .download")
    quit()
    if (el instanceof HTMLAnchorElement) el.click()
    else (el as HTMLElement | null)?.focus()
  }, [quit])

  const submit = (text: string) => {
    if (text.trim()) game.current?.hear(text, { seg: `typed-${performance.now()}`, final: true })
  }

  const seconds = Math.ceil(stats.left)
  const mb = (n: number) => Math.max(1, Math.round(n / 1e6))

  return (
    <div className="voice" data-phase={phase}>
      {(phase === "play" || phase === "count") && (
        <div className="voice__hud" aria-live="off">
          <span className="voice__score">{stats.score.toLocaleString("en-US")}</span>
          <span className="voice__combo" data-on={stats.combo >= 2 || undefined}>
            ×{Math.max(stats.combo, 1)}
          </span>
          <span className="voice__time" style={{ "--left": stats.left / 50 } as React.CSSProperties}>
            <span className="voice__bar" />
            <span className="voice__secs">{seconds}s</span>
          </span>
        </div>
      )}

      {phase === "over" && final && (
        <div className="voice__end" role="dialog" aria-label="Round over">
          <p className="voice__big">
            You just dictated {final.words} {final.words === 1 ? "word" : "words"}.
          </p>
          <p className="voice__sub">
            {final.broken} {final.broken === 1 ? "note" : "notes"} broken · best combo ×{Math.max(final.best, 1)} · {final.score.toLocaleString("en-US")} points
          </p>
          <p className="voice__pitch">Hex does this in every app, on your own computer.</p>
          <div className="voice__actions">
            <button type="button" className="voice__primary" onClick={download}>
              Download Hex
            </button>
            <button type="button" className="voice__secondary" onClick={again}>
              Play again
            </button>
            <button type="button" className="voice__secondary" onClick={quit}>
              Done
            </button>
          </div>
        </div>
      )}

      {phase !== "over" && (
        <div className="voice__dock">
          {phase === "loading" && progress && progress.total > 0 && (
            <div className="voice__loading">
              <span>
                Getting the speech model · {mb(progress.loaded)} of {mb(progress.total)} MB
              </span>
              <span className="voice__progress" style={{ "--p": progress.loaded / progress.total } as React.CSSProperties} />
            </div>
          )}
          {phase === "starting" && <div className="voice__loading">Allow the microphone to play.</div>}
          {heard && !typing && (
            <p className="voice__heard" key={heard.key} data-final={heard.final || undefined}>
              {heard.text}
            </p>
          )}
          {note && <p className="voice__note">{note}</p>}
          <form
            className="voice__type"
            data-open={typing || undefined}
            onSubmit={(e) => {
              e.preventDefault()
              const el = input.current
              if (!el) return
              submit(el.value)
              el.value = ""
            }}
          >
            {typing ? (
              <input
                ref={input}
                type="text"
                placeholder="Type a word and press Enter"
                autoComplete="off"
                autoCapitalize="off"
                spellCheck={false}
                onKeyDown={(e) => {
                  if (e.key === " ") {
                    e.preventDefault()
                    submit(e.currentTarget.value)
                    e.currentTarget.value = ""
                  }
                }}
              />
            ) : (
              <button type="button" className="voice__link" onClick={() => setTyping(true)}>
                Type instead
              </button>
            )}
            <button type="button" className="voice__link" onClick={quit}>
              Quit <kbd>esc</kbd>
            </button>
          </form>
          <p className="voice__privacy">{typing ? "Typing only. Nothing leaves this page." : "Speech is transcribed on this device. Nothing is recorded or sent."}</p>
        </div>
      )}
    </div>
  )
}
