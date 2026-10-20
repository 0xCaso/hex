// Fourth-wall screen glass: Z-axis words fired at the camera, spiderweb glass fractures with
// full-viewport refraction, Voronoi screen shatter into the void (including knocking off the
// Download button), and time-reverse crystalline rebuild.

const SANS = '"Inter Variable", ui-sans-serif, system-ui'
const MONO = 'ui-monospace, "SF Mono", Menlo, monospace'

const clamp = (x, a, b) => Math.max(a, Math.min(b, x))
const lerp = (a, b, t) => a + (b - a) * t
const smooth = (a, b, x) => {
  const t = clamp((x - a) / (b - a), 0, 1)
  return t * t * (3 - 2 * t)
}

function clipPolyHalfPlane(poly, mx, my, nx, ny) {
  const out = []
  const n = poly.length
  if (n === 0) return out
  for (let i = 0; i < n; i++) {
    const a = poly[i]
    const b = poly[(i + 1) % n]
    const da = (a[0] - mx) * nx + (a[1] - my) * ny
    const db = (b[0] - mx) * nx + (b[1] - my) * ny
    const ina = da <= 0
    const inb = db <= 0
    if (ina && inb) {
      out.push(b)
    } else if (ina && !inb) {
      const t = da / (da - db + 1e-9)
      out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    } else if (!ina && inb) {
      const t = da / (da - db + 1e-9)
      out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
      out.push(b)
    }
  }
  return out
}

export class ScreenBreak {
  constructor(root, stageCanvas, rnd = Math.random) {
    this.root = root
    this.stageCanvas = stageCanvas
    this.rnd = (a = 0, b = 1) => a + rnd() * (b - a)

    this.canvas = document.createElement("canvas")
    this.canvas.className = "screen-glass"
    this.canvas.setAttribute("aria-hidden", "true")
    root.appendChild(this.canvas)
    this.ctx = this.canvas.getContext("2d")

    this.pageCanvas = document.createElement("canvas")
    this.pageCtx = this.pageCanvas.getContext("2d")

    this.cleanCanvas = document.createElement("canvas")
    this.cleanCtx = this.cleanCanvas.getContext("2d")

    this.W = 0
    this.H = 0
    this.dpr = 1
    this.state = "idle" // "idle" | "cracking" | "shattered" | "rebuilding"
    this.zWords = [] // incoming words flying along +Z toward the screen
    this.cracks = [] // spiderweb glass fracture networks
    this.chips = [] // small glass chips flaking off impacts
    this.shards = [] // full-viewport Voronoi glass shards
    this.flash = 0
    this.rebuildAge = 0
    this.rebuildDur = 1.78
    this.downloadKnock = { x: 0, y: 0, rot: 0, vx: 0, vy: 0, vrot: 0, active: false }
  }

  resize(W, H, dpr = 1) {
    this.W = Math.max(1, W)
    this.H = Math.max(1, H)
    this.dpr = Math.min(dpr, W < 640 ? 1.25 : 1.5)
    const pw = Math.max(1, Math.round(this.W * this.dpr))
    const ph = Math.max(1, Math.round(this.H * this.dpr))
    if (this.canvas.width !== pw || this.canvas.height !== ph) {
      this.canvas.width = pw
      this.canvas.height = ph
      this.pageCanvas.width = pw
      this.pageCanvas.height = ph
      this.cleanCanvas.width = pw
      this.cleanCanvas.height = ph
    }
  }

  reset() {
    this.state = "idle"
    this.zWords.length = 0
    this.cracks.length = 0
    this.chips.length = 0
    this.shards.length = 0
    this.flash = 0
    this.rebuildAge = 0
    this.downloadKnock = { x: 0, y: 0, rot: 0, vx: 0, vy: 0, vrot: 0, active: false }
    this.root.removeAttribute("data-screen-shattered")
    const dl = this.root.querySelector(".hero .download")
    if (dl) dl.style.transform = ""
    this.ctx.setTransform(1, 0, 0, 1, 0, 0)
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height)
  }

  destroy() {
    this.reset()
    this.canvas.remove()
  }

  // Draws the live WebGPU/2D stage plus crisp DOM elements into targetCtx so glass facets
  // refract and shatter the entire page (header, title remains, subtitle, download button, cards).
  captureViewport(targetCtx, opts = {}) {
    const { W, H, dpr } = this
    if (!W || !H) return
    const cleanMode = !!opts.clean
    const rootRect = this.root.getBoundingClientRect()

    targetCtx.save()
    targetCtx.setTransform(dpr, 0, 0, dpr, 0, 0)
    targetCtx.fillStyle = "#070707"
    targetCtx.fillRect(0, 0, W, H)

    // 1. Copy the current engine stage (cards, head, particles)
    if (this.stageCanvas.width > 0 && this.stageCanvas.height > 0) {
      targetCtx.drawImage(this.stageCanvas, 0, 0, W, H)
    }

    // 2. Render topbar (brand + version + GitHub)
    const brand = this.root.querySelector(".brand")
    if (brand) {
      const br = brand.getBoundingClientRect()
      const bx = br.left - rootRect.left
      const by = br.top - rootRect.top
      targetCtx.fillStyle = "#f4f4f6"
      targetCtx.font = `700 14px ${SANS}`
      targetCtx.textBaseline = "middle"
      targetCtx.textAlign = "left"
      targetCtx.fillText("HEX", bx + 24, by + br.height / 2)
      targetCtx.strokeStyle = "#f4f4f6"
      targetCtx.lineWidth = 1.6
      targetCtx.strokeRect(bx + 2, by + br.height / 2 - 6, 12, 12)
    }

    // 3. Render title words ("Speak.", "It", "appears.") either at current fallen position or clean rest
    const h1 = this.root.querySelector(".hero__copy h1")
    const h1Style = h1 ? getComputedStyle(h1) : null
    const fontSize = h1Style ? parseFloat(h1Style.fontSize) || 56 : 56
    targetCtx.font = `680 ${fontSize}px ${SANS}`
    targetCtx.textBaseline = "middle"
    targetCtx.textAlign = "center"
    targetCtx.fillStyle = "#f4f4f6"
    for (const wEl of this.root.querySelectorAll("[data-title-word]")) {
      const prevTransform = wEl.style.transform
      if (cleanMode) wEl.style.transform = ""
      const wr = wEl.getBoundingClientRect()
      if (cleanMode) wEl.style.transform = prevTransform
      const cx = wr.left - rootRect.left + wr.width / 2
      const cy = wr.top - rootRect.top + wr.height / 2
      targetCtx.fillText((wEl.textContent || "").trim(), cx, cy)
    }

    // 4. Render hero subtitle paragraph
    const sub = this.root.querySelector(".hero__copy p")
    if (sub) {
      const sr = sub.getBoundingClientRect()
      const subStyle = getComputedStyle(sub)
      const subSize = parseFloat(subStyle.fontSize) || 17
      targetCtx.font = `450 ${subSize}px ${SANS}`
      targetCtx.fillStyle = "rgba(212, 214, 222, 0.88)"
      targetCtx.textAlign = "center"
      targetCtx.textBaseline = "top"
      const text = (sub.textContent || "").replace(/\s+/g, " ").trim()
      const maxW = Math.max(220, sr.width)
      const words = text.split(" ")
      let line = ""
      let ly = sr.top - rootRect.top
      const lh = subSize * 1.48
      for (const word of words) {
        const test = line ? `${line} ${word}` : word
        if (line && targetCtx.measureText(test).width > maxW) {
          targetCtx.fillText(line, sr.left - rootRect.left + sr.width / 2, ly)
          line = word
          ly += lh
        } else {
          line = test
        }
      }
      if (line) targetCtx.fillText(line, sr.left - rootRect.left + sr.width / 2, ly)
    }

    // 5. Render Download button pill & requirements unless omitted
    if (!opts.skipDownload) {
      const dl = this.root.querySelector(".hero .download")
      if (dl) {
        const prev = dl.style.transform
        if (cleanMode) dl.style.transform = ""
        const dr = dl.getBoundingClientRect()
        if (cleanMode) dl.style.transform = prev
        this.drawDownloadPill(
          targetCtx,
          dr.left - rootRect.left,
          dr.top - rootRect.top,
          dr.width,
          dr.height,
          (dl.textContent || "Download for Mac").trim(),
        )
      }
    }

    const req = this.root.querySelector(".hero .requirements")
    if (req) {
      const rr = req.getBoundingClientRect()
      targetCtx.font = `450 12.5px ${MONO}`
      targetCtx.fillStyle = "rgba(160, 164, 176, 0.78)"
      targetCtx.textAlign = "center"
      targetCtx.textBaseline = "middle"
      targetCtx.fillText((req.textContent || "").trim(), rr.left - rootRect.left + rr.width / 2, rr.top - rootRect.top + rr.height / 2)
    }

    targetCtx.restore()
  }

  drawDownloadPill(ctx, x, y, w, h, label) {
    ctx.save()
    ctx.shadowColor = "rgba(0, 0, 0, 0.55)"
    ctx.shadowBlur = 18
    ctx.shadowOffsetY = 6
    ctx.beginPath()
    ctx.roundRect(x, y, w, h, h / 2)
    ctx.fillStyle = "#f5f5f7"
    ctx.fill()
    ctx.restore()

    ctx.save()
    ctx.fillStyle = "#090a0d"
    ctx.font = `620 15px ${SANS}`
    ctx.textAlign = "center"
    ctx.textBaseline = "middle"
    ctx.fillText(label, x + w / 2 + 4, y + h / 2 + 0.5)
    // Small Apple mark dot/leaf on the left
    ctx.beginPath()
    ctx.arc(x + 25, y + h / 2 + 1, 5.5, 0, Math.PI * 2)
    ctx.fill()
    ctx.restore()
  }

  // Fires a word straight along the +Z axis from the head's mouth to (targetX, targetY) on the screen glass.
  fireWordAtScreen({ text, fromX, fromY, targetX, targetY, dur = 0.17, final = false, hitsDownload = false, onImpact }) {
    if (this.state === "idle") this.state = "cracking"
    this.zWords.push({
      text,
      fromX,
      fromY,
      targetX,
      targetY,
      age: 0,
      dur,
      final,
      hitsDownload,
      rot: this.rnd(-0.14, 0.14),
      vrot: this.rnd(-1.6, 1.6),
      onImpact,
    })
  }

  // Adds a realistic radial + concentric spiderweb fracture at (x, y) and refracts the page behind it.
  addCrack(x, y, power = 1, hitsDownload = false) {
    if (this.state === "idle") this.state = "cracking"
    this.captureViewport(this.pageCtx)
    this.flash = Math.max(this.flash, 0.55 * power)
    this.flashX = x
    this.flashY = y

    const rayCount = Math.round(lerp(11, 16, clamp(power, 0.6, 1.4)))
    const maxR = Math.min(this.W, this.H) * lerp(0.22, 0.36, clamp(power, 0.6, 1.4))
    const rays = []

    for (let i = 0; i < rayCount; i++) {
      const baseAng = (i / rayCount) * Math.PI * 2 + this.rnd(-0.11, 0.11)
      // 2 long stress runners per impact, rest localized around impact crater
      const isRunner = i % 5 === 0
      const len = maxR * (isRunner ? this.rnd(1.05, 1.45) : this.rnd(0.45, 0.95))
      const steps = 6
      const pts = [[x, y]]
      let curAng = baseAng
      for (let s = 1; s <= steps; s++) {
        const f = Math.pow(s / steps, 1.15)
        curAng += this.rnd(-0.095, 0.095)
        const r = len * f
        pts.push([x + Math.cos(curAng) * r, y + Math.sin(curAng) * r])
      }
      const forkStep = Math.floor(this.rnd(2, 5))
      const forkOrigin = pts[forkStep]
      const forkAng = curAng + (this.rnd() < 0.5 ? -1 : 1) * this.rnd(0.38, 0.68)
      const forkLen = len * this.rnd(0.22, 0.42)
      const forkPts = [
        forkOrigin,
        [forkOrigin[0] + Math.cos(forkAng) * forkLen * 0.5, forkOrigin[1] + Math.sin(forkAng) * forkLen * 0.5],
        [forkOrigin[0] + Math.cos(forkAng + this.rnd(-0.12, 0.12)) * forkLen, forkOrigin[1] + Math.sin(forkAng + this.rnd(-0.12, 0.12)) * forkLen],
      ]
      rays.push({ baseAng, len, pts, forkPts, spec: this.rnd(0.7, 1.0) })
    }

    // Concentric spiderweb rings + refracting glass wedges between adjacent rays
    const webs = []
    const wedges = []
    for (let i = 0; i < rayCount; i++) {
      const rA = rays[i]
      const rB = rays[(i + 1) % rayCount]
      const midAng = (rA.baseAng + rB.baseAng) * 0.5
      const disp = this.rnd(4.5, 11.5) * power
      const sign = i % 2 === 0 ? 1 : -1
      const pA = rA.pts[3]
      const pB = rB.pts[3]
      wedges.push({
        poly: [[x, y], rA.pts[1], pA, pB, rB.pts[1]],
        ox: Math.cos(midAng + sign * 1.1) * disp,
        oy: Math.sin(midAng + sign * 1.1) * disp,
      })

      for (let stepIdx = 1; stepIdx <= 4; stepIdx++) {
        if (this.rnd() < 0.18) continue
        const aPt = rA.pts[stepIdx]
        const bPt = rB.pts[stepIdx]
        const jitter = this.rnd(0.9, 1.06)
        webs.push({
          x0: x + (aPt[0] - x) * jitter,
          y0: y + (aPt[1] - y) * jitter,
          x1: x + (bPt[0] - x) * jitter,
          y1: y + (bPt[1] - y) * jitter,
          spec: this.rnd(0.6, 0.96),
        })
      }
    }

    this.cracks.push({ x, y, power, rays, webs, wedges, age: 0 })

    // Spawn small crystalline glass chips flaking off the impact point
    const chipCount = Math.round(20 * power)
    for (let i = 0; i < chipCount; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const spd = this.rnd(90, 440) * power
      const sz = this.rnd(3.5, 11)
      this.chips.push({
        x: x + this.rnd(-8, 8),
        y: y + this.rnd(-8, 8),
        vx: Math.cos(ang) * spd,
        vy: Math.sin(ang) * spd - this.rnd(60, 180),
        rot: this.rnd(0, Math.PI * 2),
        vrot: this.rnd(-14, 14),
        sz,
        age: 0,
        life: this.rnd(0.48, 0.95),
      })
    }

    // If this shot hit the Download button, knock the visual button askew!
    if (hitsDownload) {
      const dl = this.root.querySelector(".hero .download")
      if (dl) {
        dl.style.transition = "transform 90ms cubic-bezier(0.2, 0.9, 0.3, 1)"
        dl.style.transform = "translate3d(12px, 16px, 0) rotate(8.5deg)"
      }
    }
  }

  // Shatters the entire cracked viewport into falling 3D Voronoi glass shards + knocks off the Download button.
  shatter(epicX = this.W * 0.5, epicY = this.H * 0.48) {
    const { W, H, dpr } = this
    if (!W || !H) return

    // 1. Capture the clean restored page first (for the rebuild cross-fade)
    this.captureViewport(this.cleanCtx, { clean: true })

    // 2. Capture the current viewport (without the Download button so we can make the Download button its own falling piece)
    // and bake all accumulated spiderweb cracks onto pageCanvas so shards carry the cracks!
    this.captureViewport(this.pageCtx, { skipDownload: false })
    this.pageCtx.save()
    this.pageCtx.setTransform(dpr, 0, 0, dpr, 0, 0)
    this.drawCracksOnto(this.pageCtx, 1)
    this.pageCtx.restore()

    // 3. Hide visual DOM layers while keeping the real Download anchor clickable at opacity: 0
    this.root.setAttribute("data-screen-shattered", "")
    const dl = this.root.querySelector(".hero .download")
    const rootRect = this.root.getBoundingClientRect()
    let dlRect = null
    if (dl) {
      const r = dl.getBoundingClientRect()
      dlRect = {
        cx: r.left - rootRect.left + r.width / 2,
        cy: r.top - rootRect.top + r.height / 2,
        w: r.width,
        h: r.height,
        label: (dl.textContent || "Download for Mac").trim(),
      }
      dl.style.transform = ""
    }

    this.state = "shattered"
    this.flash = 0.75
    this.flashX = epicX
    this.flashY = epicY
    this.shards.length = 0

    // 4. Generate convex Voronoi tessellation across the whole viewport
    const cols = W < 640 ? 7 : 11
    const rows = W < 640 ? 6 : 7
    const seeds = []
    for (let iy = 0; iy < rows; iy++) {
      for (let ix = 0; ix < cols; ix++) {
        const u = (ix + 0.5) / cols
        const v = (iy + 0.5) / rows
        let sx = u * W + this.rnd(-W / cols, W / cols) * 0.36
        let sy = v * H + this.rnd(-H / rows, H / rows) * 0.36
        // Pull inner seeds slightly toward the central impact so center shards are finer
        const pull = (ix + iy) % 2 === 0 ? 0.18 : 0.04
        sx = lerp(sx, epicX, pull)
        sy = lerp(sy, epicY, pull)
        seeds.push([clamp(sx, 18, W - 18), clamp(sy, 18, H - 18)])
      }
    }

    const screenPoly = [
      [0, 0],
      [W, 0],
      [W, H],
      [0, H],
    ]
    const maxDist = Math.hypot(W * 0.6, H * 0.6)

    for (let i = 0; i < seeds.length; i++) {
      const si = seeds[i]
      let poly = screenPoly
      for (let j = 0; j < seeds.length; j++) {
        if (i === j) continue
        const sj = seeds[j]
        const nx = sj[0] - si[0]
        const ny = sj[1] - si[1]
        if (nx * nx + ny * ny < 1) continue
        const mx = (si[0] + sj[0]) * 0.5
        const my = (si[1] + sj[1]) * 0.5
        poly = clipPolyHalfPlane(poly, mx, my, nx, ny)
        if (poly.length < 3) break
      }
      if (poly.length < 3) continue

      let cx = 0
      let cy = 0
      for (const p of poly) {
        cx += p[0]
        cy += p[1]
      }
      cx /= poly.length
      cy /= poly.length
      const localPoly = poly.map((p) => [p[0] - cx, p[1] - cy])

      const dx = cx - epicX
      const dy = cy - epicY
      const dist = Math.hypot(dx, dy)
      const normD = clamp(dist / maxDist, 0, 1)
      const dirX = dist > 1 ? dx / dist : this.rnd(-1, 1)
      const dirY = dist > 1 ? dy / dist : this.rnd(-1, 1)
      const impulse = lerp(520, 190, normD) * this.rnd(0.82, 1.24)

      this.shards.push({
        isDownload: false,
        homeX: cx,
        homeY: cy,
        x: cx,
        y: cy,
        vx: dirX * impulse + this.rnd(-65, 65),
        vy: dirY * impulse * 0.65 - this.rnd(120, 310),
        ang: 0,
        spin: (dirX * 1.8 + this.rnd(-1.6, 1.6)) * lerp(2.2, 0.9, normD),
        tumble: this.rnd(0, 0.35),
        vTumble: this.rnd(3.2, 7.8) * (this.rnd() < 0.5 ? 1 : -1),
        poly: localPoly,
        normD,
        delay: normD * 0.08, // radial crack-wave propagation delay
        age: 0,
        fromX: cx,
        fromY: cy,
        fromAng: 0,
        fromTumble: 0,
        glint: 0,
      })
    }

    // 5. Add a dedicated hero piece for the knocked-off Download button so it visibly tumbles off!
    if (dlRect) {
      this.shards.push({
        isDownload: true,
        homeX: dlRect.cx,
        homeY: dlRect.cy,
        w: dlRect.w,
        h: dlRect.h,
        label: dlRect.label,
        x: dlRect.cx + 12,
        y: dlRect.cy + 16,
        vx: 85,
        vy: -440,
        ang: 0.15,
        spin: 2.3,
        tumble: 0.08,
        vTumble: 2.6,
        normD: 0.15,
        delay: 0,
        age: 0,
        fromX: dlRect.cx,
        fromY: dlRect.cy,
        fromAng: 0,
        fromTumble: 0,
        glint: 0,
      })
    }

    // 6. Burst of crystalline glass chips from the shatter epicenter
    for (let i = 0; i < 48; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const spd = this.rnd(160, 680)
      this.chips.push({
        x: epicX + this.rnd(-24, 24),
        y: epicY + this.rnd(-24, 24),
        vx: Math.cos(ang) * spd,
        vy: Math.sin(ang) * spd - this.rnd(80, 240),
        rot: this.rnd(0, Math.PI * 2),
        vrot: this.rnd(-16, 16),
        sz: this.rnd(4, 13),
        age: 0,
        life: this.rnd(0.7, 1.35),
      })
    }
  }

  // Starts the time-reverse crystalline rebuild where all glass shards and the Download button fly back up and heal.
  startRebuild(dur = 1.78) {
    if (this.state !== "shattered") return
    this.state = "rebuilding"
    this.rebuildAge = 0
    this.rebuildDur = dur
    this.needsCleanCapture = true
    for (const s of this.shards) {
      s.fromX = clamp(s.x, -160, this.W + 160)
      s.fromY = Math.min(s.y, this.H + 220)
      // Wrap starting angle to nearest turn so it unwinds cleanly to 0
      s.fromAng = ((s.ang + Math.PI) % (Math.PI * 2)) - Math.PI
      s.fromTumble = s.tumble % (Math.PI * 2)
      s.glint = 0
    }
  }

  drawCracksOnto(ctx, alpha = 1) {
    const { W, H } = this
    for (const c of this.cracks) {
      // 1. Refracted glass wedge facets displacing the underlying page
      for (const w of c.wedges) {
        ctx.save()
        ctx.beginPath()
        ctx.moveTo(w.poly[0][0], w.poly[0][1])
        for (let i = 1; i < w.poly.length; i++) ctx.lineTo(w.poly[i][0], w.poly[i][1])
        ctx.closePath()
        ctx.clip()
        ctx.globalAlpha = 0.96 * alpha
        ctx.drawImage(this.pageCanvas, 0, 0, this.pageCanvas.width, this.pageCanvas.height, w.ox, w.oy, W, H)
        ctx.restore()
      }

      // 2. Pulverized white glass spall zone at impact center
      ctx.save()
      const spall = ctx.createRadialGradient(c.x, c.y, 1, c.x, c.y, 26 * c.power)
      spall.addColorStop(0, `rgba(245, 250, 255, ${0.78 * alpha})`)
      spall.addColorStop(0.45, `rgba(185, 222, 255, ${0.32 * alpha})`)
      spall.addColorStop(1, "rgba(140, 190, 255, 0)")
      ctx.fillStyle = spall
      ctx.beginPath()
      ctx.arc(c.x, c.y, 26 * c.power, 0, Math.PI * 2)
      ctx.fill()
      ctx.restore()

      // 3. Concentric spiderweb bridge segments
      ctx.save()
      ctx.lineCap = "round"
      for (const web of c.webs) {
        ctx.strokeStyle = `rgba(0, 6, 16, ${0.68 * alpha})`
        ctx.lineWidth = 1.6
        ctx.beginPath()
        ctx.moveTo(web.x0 + 1.2, web.y0 + 1.2)
        ctx.lineTo(web.x1 + 1.2, web.y1 + 1.2)
        ctx.stroke()

        ctx.strokeStyle = `rgba(225, 244, 255, ${0.78 * web.spec * alpha})`
        ctx.lineWidth = 0.95
        ctx.beginPath()
        ctx.moveTo(web.x0, web.y0)
        ctx.lineTo(web.x1, web.y1)
        ctx.stroke()
      }

      // 4. Primary radial fracture rays + forks (dark bevel + cyan stress glow + razor specular edge)
      for (const ray of c.rays) {
        const pts = ray.pts
        for (let s = 0; s < pts.length - 1; s++) {
          const p0 = pts[s]
          const p1 = pts[s + 1]
          const taper = 1 - s / pts.length
          // Shadow bevel
          ctx.strokeStyle = `rgba(0, 4, 12, ${(0.82 * taper + 0.12) * alpha})`
          ctx.lineWidth = 2.5 * taper + 0.6
          ctx.beginPath()
          ctx.moveTo(p0[0] + 1.5, p0[1] + 1.5)
          ctx.lineTo(p1[0] + 1.5, p1[1] + 1.5)
          ctx.stroke()
          // Frosted cyan stress halo
          ctx.strokeStyle = `rgba(145, 212, 255, ${0.34 * taper * alpha})`
          ctx.lineWidth = 3.4 * taper + 0.8
          ctx.beginPath()
          ctx.moveTo(p0[0], p0[1])
          ctx.lineTo(p1[0], p1[1])
          ctx.stroke()
          // Bright specular glass edge
          ctx.strokeStyle = `rgba(242, 250, 255, ${(0.95 * ray.spec * taper + 0.18) * alpha})`
          ctx.lineWidth = 1.35 * taper + 0.45
          ctx.beginPath()
          ctx.moveTo(p0[0], p0[1])
          ctx.lineTo(p1[0], p1[1])
          ctx.stroke()
        }
        // Secondary fork
        const fp = ray.forkPts
        ctx.strokeStyle = `rgba(230, 245, 255, ${0.72 * alpha})`
        ctx.lineWidth = 0.9
        ctx.beginPath()
        ctx.moveTo(fp[0][0], fp[0][1])
        ctx.lineTo(fp[1][0], fp[1][1])
        ctx.lineTo(fp[2][0], fp[2][1])
        ctx.stroke()
      }
      ctx.restore()
    }
  }

  stepAndDraw(dt) {
    if (this.state === "idle" && this.zWords.length === 0) return
    const { ctx, W, H, dpr } = this
    if (!W || !H) return
    if (this.needsCleanCapture) {
      this.needsCleanCapture = false
      this.captureViewport(this.cleanCtx, { clean: true })
    }

    ctx.save()
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, W, H)

    this.flash = Math.max(0, this.flash - dt * 6.5)
    for (const c of this.cracks) c.age += dt

    // 1. If in "cracking" state, draw the spiderweb glass fractures & page refraction
    if (this.state === "cracking" && this.cracks.length > 0) {
      this.drawCracksOnto(ctx, 1)
    }

    // 2. If in "shattered" or "rebuilding" state, simulate & draw the Voronoi glass shards + Download button
    if (this.state === "shattered" || this.state === "rebuilding") {
      let allDone = false
      let healFade = 1
      if (this.state === "rebuilding") {
        this.rebuildAge += dt
        const overallT = clamp(this.rebuildAge / this.rebuildDur, 0, 1)
        // Fade out the healed glass overlay over the final 14% so handoff to live DOM is seamless
        healFade = 1 - smooth(0.86, 1.0, overallT)
        if (overallT >= 1) allDone = true
      }

      for (const s of this.shards) {
        let healMix = 0
        let seamGlint = 0
        if (this.state === "shattered") {
          s.age += dt
          if (s.age > s.delay) {
            const effDt = dt
            s.vx *= Math.exp(-0.35 * effDt)
            s.vy += (s.isDownload ? 620 : 1260) * effDt
            s.x += s.vx * effDt
            s.y += s.vy * effDt
            s.ang += s.spin * effDt
            s.tumble += s.vTumble * effDt
          }
        } else {
          // Time-reverse rebuild: outer shards lift first, center + Download button snap in last
          const startT = (1 - s.normD) * 0.24
          const span = 0.62
          const loc = clamp((this.rebuildAge / this.rebuildDur - startT) / span, 0, 1)
          // Smooth magnetic rewind curve with crisp snap at the end
          const ease = loc < 0.5 ? 4 * loc * loc * loc : 1 - Math.pow(-2 * loc + 2, 3) / 2
          const arcLift = Math.sin(loc * Math.PI) * -42 * (0.4 + s.normD)
          s.x = lerp(s.fromX, s.homeX, ease)
          s.y = lerp(s.fromY, s.homeY, ease) + arcLift
          s.ang = lerp(s.fromAng, 0, ease)
          s.tumble = lerp(s.fromTumble, 0, ease)
          healMix = smooth(0.48, 0.92, loc)
          // Flash the shard seam right as it snaps into place
          seamGlint = Math.exp(-Math.pow((loc - 0.92) / 0.055, 2))
        }

        if (s.y - 160 > H && this.state === "shattered") continue

        const tiltX = this.state === "rebuilding" ? lerp(0.35 + 0.65 * Math.cos(s.tumble), 1, healMix) : 0.28 + 0.72 * Math.cos(s.tumble)
        const specCatch = Math.pow(Math.max(0, Math.sin(s.tumble * 2 + s.ang)), 8) * (1 - healMix)

        ctx.save()
        ctx.globalAlpha = healFade
        ctx.translate(s.x, s.y)
        ctx.rotate(s.ang)
        ctx.scale(tiltX, 1)

        if (s.isDownload) {
          this.drawDownloadPill(ctx, -s.w / 2, -s.h / 2, s.w, s.h, s.label)
          if (specCatch > 0.05 || seamGlint > 0.05) {
            ctx.strokeStyle = `rgba(210, 245, 255, ${Math.min(1, specCatch + seamGlint)})`
            ctx.lineWidth = 2.2
            ctx.beginPath()
            ctx.roundRect(-s.w / 2, -s.h / 2, s.w, s.h, s.h / 2)
            ctx.stroke()
          }
        } else {
          const poly = s.poly
          ctx.beginPath()
          ctx.moveTo(poly[0][0], poly[0][1])
          for (let k = 1; k < poly.length; k++) ctx.lineTo(poly[k][0], poly[k][1])
          ctx.closePath()

          ctx.save()
          ctx.clip()
          if (healMix < 0.99) {
            ctx.drawImage(this.pageCanvas, 0, 0, this.pageCanvas.width, this.pageCanvas.height, -s.homeX, -s.homeY, W, H)
          }
          if (healMix > 0.01) {
            ctx.globalAlpha = healMix * healFade
            ctx.drawImage(this.cleanCanvas, 0, 0, this.cleanCanvas.width, this.cleanCanvas.height, -s.homeX, -s.homeY, W, H)
          }
          if (specCatch > 0.02) {
            ctx.fillStyle = `rgba(215, 242, 255, ${specCatch * 0.52})`
            ctx.fill()
          }
          ctx.restore()

          // Thick tempered-glass side bevel + healing seam glint (proportional to displacement so locked shards dissolve seamlessly)
          const disp = clamp(Math.hypot(s.x - s.homeX, s.y - s.homeY) / 48 + Math.abs(s.ang) * 1.4, 0, 1)
          const edgeAlpha = disp * (1 - healMix * 0.85) * 0.85 + seamGlint * 0.55
          if (edgeAlpha > 0.03) {
            ctx.strokeStyle =
              seamGlint > 0.15
                ? `rgba(215, 248, 255, ${Math.min(1, edgeAlpha)})`
                : `rgba(185, 232, 255, ${Math.min(1, edgeAlpha)})`
            ctx.lineWidth = disp * 1.5 + seamGlint * 1.6 + 0.3
            ctx.stroke()
          }
        }
        ctx.restore()
      }

      if (allDone) {
        this.reset()
        ctx.restore()
        return
      }
    }

    // 3. Simulate & draw small glass chips flaking off impacts
    for (let i = this.chips.length - 1; i >= 0; i--) {
      const ch = this.chips[i]
      ch.age += dt
      if (ch.age >= ch.life) {
        this.chips.splice(i, 1)
        continue
      }
      ch.vy += 1180 * dt
      ch.x += ch.vx * dt
      ch.y += ch.vy * dt
      ch.rot += ch.vrot * dt
      const t = 1 - ch.age / ch.life
      ctx.save()
      ctx.translate(ch.x, ch.y)
      ctx.rotate(ch.rot)
      ctx.fillStyle = `rgba(220, 244, 255, ${t * 0.85})`
      ctx.strokeStyle = `rgba(255, 255, 255, ${t * 0.95})`
      ctx.lineWidth = 0.9
      ctx.beginPath()
      ctx.moveTo(0, -ch.sz)
      ctx.lineTo(ch.sz * 0.85, ch.sz * 0.7)
      ctx.lineTo(-ch.sz * 0.75, ch.sz * 0.5)
      ctx.closePath()
      ctx.fill()
      ctx.stroke()
      ctx.restore()
    }

    // 4. Simulate & draw Z-axis words rocketing straight at the camera lens with zoom motion blur
    for (let i = this.zWords.length - 1; i >= 0; i--) {
      const zw = this.zWords[i]
      zw.age += dt
      const p = clamp(zw.age / zw.dur, 0, 1)
      // Perspective acceleration along +Z: stays compact leaving the mouth, then explodes huge into the camera
      const zCurve = Math.pow(p, 2.35)
      const x = lerp(zw.fromX, zw.targetX, zCurve)
      const y = lerp(zw.fromY, zw.targetY, zCurve)
      const scale = lerp(0.36, zw.final ? 8.2 : 4.8, Math.pow(p, 2.65))
      const rot = zw.rot + zw.vrot * zw.age

      // Draw 4 radial zoom motion-blur echoes behind the word
      for (let b = 3; b >= 0; b--) {
        const bp = clamp(p - b * 0.045, 0, 1)
        const bz = Math.pow(bp, 2.35)
        const bx = lerp(zw.fromX, zw.targetX, bz)
        const by = lerp(zw.fromY, zw.targetY, bz)
        const bScale = lerp(0.36, zw.final ? 8.2 : 4.8, Math.pow(bp, 2.65))
        const alpha = b === 0 ? 1 : (0.34 / b) * p

        ctx.save()
        ctx.translate(bx, by)
        ctx.rotate(rot)
        ctx.scale(bScale, bScale)
        ctx.font = `900 28px ${SANS}`
        ctx.textAlign = "center"
        ctx.textBaseline = "middle"
        if (b === 0) {
          ctx.shadowColor = zw.final ? "rgba(255, 195, 55, 0.95)" : "rgba(255, 135, 35, 0.9)"
          ctx.shadowBlur = 18
        }
        ctx.fillStyle = b === 0 ? "#fffdf6" : `rgba(255, 196, 92, ${alpha.toFixed(3)})`
        ctx.strokeStyle = `rgba(20, 12, 6, ${(alpha * 0.65).toFixed(3)})`
        ctx.lineWidth = 2.2
        ctx.strokeText(zw.text, 0, 0)
        ctx.fillText(zw.text, 0, 0)
        ctx.restore()
      }

      if (p >= 1) {
        this.zWords.splice(i, 1)
        if (zw.final) {
          this.shatter(zw.targetX, zw.targetY)
        } else {
          this.addCrack(zw.targetX, zw.targetY, zw.hitsDownload ? 1.25 : 1.0, zw.hitsDownload)
        }
        zw.onImpact?.(zw)
      }
    }

    // 5. Localized radial impact flash so the viewport background stays deep black
    if (this.flash > 0.01) {
      const fx = this.flashX ?? W * 0.5
      const fy = this.flashY ?? H * 0.5
      const fr = this.state === "shattered" ? Math.hypot(W, H) * 0.55 : 220
      const fg = ctx.createRadialGradient(fx, fy, 0, fx, fy, fr)
      fg.addColorStop(0, `rgba(235, 248, 255, ${Math.min(0.65, this.flash)})`)
      fg.addColorStop(0.35, `rgba(175, 225, 255, ${Math.min(0.28, this.flash * 0.45)})`)
      fg.addColorStop(1, "rgba(120, 190, 255, 0)")
      ctx.fillStyle = fg
      ctx.beginPath()
      ctx.arc(fx, fy, fr, 0, Math.PI * 2)
      ctx.fill()
    }

    ctx.restore()
  }
}
