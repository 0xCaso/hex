import { useEffect, useRef, useState } from "react"

function webglAvailable() {
  try {
    const canvas = document.createElement("canvas")
    return Boolean(canvas.getContext("webgl2") ?? canvas.getContext("webgl"))
  } catch {
    return false
  }
}

export default function ChatteringTeeth() {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [poster, setPoster] = useState(false)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const params = new URLSearchParams(window.location.search)
    const mode = params.has("og") ? "og" : params.has("poster") ? "poster" : "live"
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    // Reduced motion and no-WebGL get the pre-rendered still; three.js then
    // never downloads.
    if (mode === "live" && (reduced || !webglAvailable())) {
      setPoster(true)
      return
    }

    let dispose: (() => void) | undefined
    let cancelled = false
    const start = () => {
      import("./teeth/scene")
        .then(({ mountTeeth }) => {
          if (cancelled) return
          dispose = mountTeeth(canvas, mode)
          canvas.dataset.ready = "true"
        })
        .catch(() => {
          if (!cancelled) setPoster(true)
        })
    }
    // The entrance is staged seconds after load, so the renderer can wait
    // until the page is idle instead of competing with first paint.
    const idle = mode === "live" && "requestIdleCallback" in window
      ? window.requestIdleCallback(start, { timeout: 1200 })
      : window.setTimeout(start, 0)

    return () => {
      cancelled = true
      if (mode === "live" && "cancelIdleCallback" in window) window.cancelIdleCallback(idle)
      else window.clearTimeout(idle)
      dispose?.()
    }
  }, [])

  return (
    <>
      <canvas ref={canvasRef} className="teeth-canvas" aria-hidden="true" />
      {poster && <img className="teeth-poster" src="/teeth-poster.webp" alt="" aria-hidden="true" decoding="async" />}
    </>
  )
}
