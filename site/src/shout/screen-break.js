// Fourth-wall photorealistic screen glass: Z-axis words fired at the camera, stress-field
// fracture propagation (pulverized conchoidal impact craters, branching radial cracks with
// tapering width, densely packed Hertzian ring cracks, green-tinted tempered glass edges,
// RGB chromatic dispersion, and head-lit golden Fresnel glints), 3D extruded Voronoi glass
// slabs suspended in slow-motion bullet-time with depth-of-field and sparkling glass dust,
// time-reverse crystalline rebuild, and bold word-synced finale captions.

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

    this.basePageCanvas = document.createElement("canvas")
    this.basePageCtx = this.basePageCanvas.getContext("2d")

    this.pageCanvas = document.createElement("canvas")
    this.pageCtx = this.pageCanvas.getContext("2d")

    this.cleanCanvas = document.createElement("canvas")
    this.cleanCtx = this.cleanCanvas.getContext("2d")

    this.W = 0
    this.H = 0
    this.dpr = 1
    this.headX = 0
    this.headY = 0
    this.headR = 90
    this.state = "idle" // "idle" | "cracking" | "shattered" | "rebuilding"
    this.zWords = [] // incoming words flying along +Z toward the screen
    this.cracks = [] // branching radial + Hertzian ring glass fracture networks
    this.dust = [] // sparkling crystalline micro-splinters and golden glass powder
    this.shards = [] // 3D extruded tempered glass slabs
    this.flash = 0
    this.flashX = 0
    this.flashY = 0
    this.rebuildAge = 0
    this.rebuildDur = 1.8
    this.caption = null // { words: [{ text, t }], startRtime, endRtime }
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
      this.basePageCanvas.width = pw
      this.basePageCanvas.height = ph
      this.pageCanvas.width = pw
      this.pageCanvas.height = ph
      this.cleanCanvas.width = pw
      this.cleanCanvas.height = ph
    }
  }

  setHeadLight(x, y, R) {
    this.headX = x
    this.headY = y
    this.headR = R
  }

  setCaption(caption) {
    this.caption = caption
  }

  reset() {
    this.state = "idle"
    this.zWords.length = 0
    this.cracks.length = 0
    this.dust.length = 0
    this.shards.length = 0
    this.flash = 0
    this.rebuildAge = 0
    this.caption = null
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

    // 2. Render header (brand + GitHub)
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
    targetCtx.save()
    targetCtx.font = `680 ${fontSize}px ${SANS}`
    if ("letterSpacing" in targetCtx && h1Style?.letterSpacing) {
      targetCtx.letterSpacing = h1Style.letterSpacing
    }
    targetCtx.textBaseline = "middle"
    targetCtx.textAlign = "center"
    targetCtx.fillStyle = "#f4f4f6"
    if (cleanMode && h1) {
      const hr = h1.getBoundingClientRect()
      targetCtx.fillText("Speak. It appears.", hr.left - rootRect.left + hr.width / 2, hr.top - rootRect.top + hr.height / 2)
    } else {
      for (const wEl of this.root.querySelectorAll("[data-title-word]")) {
        const wr = wEl.getBoundingClientRect()
        const cx = wr.left - rootRect.left + wr.width / 2
        const cy = wr.top - rootRect.top + wr.height / 2
        targetCtx.fillText((wEl.textContent || "").trim(), cx, cy)
      }
    }
    targetCtx.restore()

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
    ctx.shadowColor = "rgba(0, 0, 0, 0.65)"
    ctx.shadowBlur = 20
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
    ctx.beginPath()
    ctx.arc(x + 25, y + h / 2 + 1, 5.5, 0, Math.PI * 2)
    ctx.fill()
    ctx.restore()
  }

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
      rot: this.rnd(-0.12, 0.12),
      vrot: this.rnd(-1.4, 1.4),
      onImpact,
    })
  }

  // Recursive stress-field crack propagation with fine jagged micro-steps and secondary/tertiary Y-bifurcations
  buildCrackBranch(x0, y0, startAng, totalLen, uStart, uSpan, depth, outSegments, outSpecks) {
    const steps = depth === 0 ? 22 : depth === 1 ? 11 : 6
    let px = x0
    let py = y0
    let ang = startAng
    const stepLen = totalLen / steps

    for (let s = 1; s <= steps; s++) {
      const frac = s / steps
      const u = clamp(uStart + frac * uSpan, 0, 1)
      const kink = this.rnd() < 0.2 ? (this.rnd() < 0.5 ? -1 : 1) * this.rnd(0.11, 0.24) : 0
      ang += this.rnd(-0.055, 0.055) + kink
      const rStep = stepLen * this.rnd(0.78, 1.24)
      const nxPos = px + Math.cos(ang) * rStep
      const nyPos = py + Math.sin(ang) * rStep

      const dx = nxPos - px
      const dy = nyPos - py
      const dl = Math.hypot(dx, dy) || 1
      const nx = -dy / dl
      const ny = dx / dl

      outSegments.push({
        x0: px,
        y0: py,
        x1: nxPos,
        y1: nyPos,
        mx: (px + nxPos) * 0.5,
        my: (py + nyPos) * 0.5,
        nx,
        ny,
        u0: clamp(uStart + ((s - 1) / steps) * uSpan, 0, 1),
        u1: u,
        depth,
        spec: this.rnd(0.7, 1.0),
      })

      // Embedded crystalline micro-chips and golden silica glints along the crack
      if (this.rnd() < (depth === 0 ? 0.78 : 0.45)) {
        const off = (this.rnd() < 0.5 ? -1 : 1) * this.rnd(0.4, 5.2) * (1 - u * 0.6)
        outSpecks.push({
          x: (px + nxPos) * 0.5 + nx * off,
          y: (py + nyPos) * 0.5 + ny * off,
          r: this.rnd(0.85, 2.8) * (1 - u * 0.4),
          ang: ang + this.rnd(-0.5, 0.5),
          stretch: this.rnd(1.3, 3.4),
          gold: this.rnd() < 0.68,
          star: this.rnd() < 0.22,
          alpha: this.rnd(0.65, 1.0) * (1 - u * 0.3),
        })
      }

      if (depth === 0 && (s === 6 || s === 13) && this.rnd() < 0.76) {
        const side = this.rnd() < 0.5 ? -1 : 1
        const forkAng = ang + side * this.rnd(0.26, 0.54)
        const forkLen = (totalLen - s * stepLen) * this.rnd(0.48, 0.78)
        this.buildCrackBranch(nxPos, nyPos, forkAng, forkLen, u, (1 - u) * 0.75, 1, outSegments, outSpecks)
      } else if (depth === 1 && s === 5 && this.rnd() < 0.52) {
        const side = this.rnd() < 0.5 ? -1 : 1
        const forkAng = ang + side * this.rnd(0.3, 0.6)
        const forkLen = totalLen * this.rnd(0.38, 0.58)
        this.buildCrackBranch(nxPos, nyPos, forkAng, forkLen, u, (1 - u) * 0.52, 2, outSegments, outSpecks)
      }

      px = nxPos
      py = nyPos
    }
  }

  // Adds a photorealistic ballistic impact fracture at (x, y) and refracts the page behind it.
  addCrack(x, y, power = 1, hitsDownload = false) {
    if (this.state === "idle") this.state = "cracking"
    if (hitsDownload) {
      const dl = this.root.querySelector(".hero .download")
      if (dl) {
        dl.style.transition = "transform 90ms cubic-bezier(0.2, 0.9, 0.3, 1)"
        dl.style.transform = "translate3d(12px, 16px, 0) rotate(8.5deg)"
      }
    }
    // Capture clean underlying viewport (DOM + stageCanvas, never this.canvas)
    this.captureViewport(this.basePageCtx)

    this.flash = Math.max(this.flash, 0.45 * power)
    this.flashX = x
    this.flashY = y

    const segments = []
    const specks = []

    // 1. Primary, medium & dense crater star-rays (38-46 rays per impact, matching target-cracks.png)
    const rayCount = Math.round(lerp(36, 46, clamp(power, 0.7, 1.4)))
    const maxRunnerLen = Math.min(this.W, this.H) * lerp(0.38, 0.64, clamp(power, 0.7, 1.4))
    const primaryRays = []

    for (let i = 0; i < rayCount; i++) {
      const baseAng = (i / rayCount) * Math.PI * 2 + this.rnd(-0.07, 0.07)
      const isLongRunner = i % 3 === 0
      const isMedium = i % 3 === 1
      const len = isLongRunner
        ? maxRunnerLen * this.rnd(0.7, 1.18)
        : isMedium
          ? maxRunnerLen * this.rnd(0.32, 0.64)
          : maxRunnerLen * this.rnd(0.11, 0.28)

      const startIdx = segments.length
      this.buildCrackBranch(x, y, baseAng, len, 0, 1, isLongRunner ? 0 : 1, segments, specks)
      const spine = [[x, y]]
      for (let k = startIdx; k < segments.length; k++) {
        if (segments[k].depth === (isLongRunner ? 0 : 1)) spine.push([segments[k].x1, segments[k].y1])
      }
      primaryRays.push({ baseAng, len, spine })
    }

    // 2. Densely packed concentric Hertzian stress rings (14 rings: tight spiderweb rosette near core, wider outward)
    const ringDist = [6, 9, 13, 18, 25, 33, 43, 56, 72, 92, 118, 150, 190, 238].map((d) => d * power)
    const rings = []
    const wedges = []

    for (let i = 0; i < primaryRays.length; i++) {
      const rA = primaryRays[i]
      const rB = primaryRays[(i + 1) % primaryRays.length]
      const midAng = (rA.baseAng + rB.baseAng) * 0.5

      // Optical refraction wedge every 3 rays out to ~150px
      if (i % 3 === 0) {
        const rNext = primaryRays[(i + 3) % primaryRays.length]
        const wR = Math.min(rA.len, rNext.len, 150 * power)
        if (wR > 26) {
          const disp = this.rnd(4.5, 10.5) * power
          const sign = (i / 3) % 2 === 0 ? 1 : -1
          wedges.push({
            poly: [
              [x, y],
              [x + Math.cos(rA.baseAng) * wR * 0.45, y + Math.sin(rA.baseAng) * wR * 0.45],
              [x + Math.cos(rA.baseAng) * wR, y + Math.sin(rA.baseAng) * wR],
              [x + Math.cos(rNext.baseAng) * wR, y + Math.sin(rNext.baseAng) * wR],
              [x + Math.cos(rNext.baseAng) * wR * 0.45, y + Math.sin(rNext.baseAng) * wR * 0.45],
            ],
            ox: Math.cos(midAng + sign * 1.15) * disp,
            oy: Math.sin(midAng + sign * 1.15) * disp,
            chromaX: Math.cos(midAng) * 2.0 * sign,
            chromaY: Math.sin(midAng) * 2.0 * sign,
          })
        }
      }

      for (let ri = 0; ri < ringDist.length; ri++) {
        const baseD = ringDist[ri]
        if (baseD > rA.len * 0.94 || baseD > rB.len * 0.94) continue
        const prob = ri < 6 ? 0.94 : ri < 10 ? 0.76 : 0.48
        if (this.rnd() > prob) continue

        const dA = baseD * this.rnd(0.92, 1.08)
        const dB = baseD * this.rnd(0.92, 1.08)
        const ptA = this.pointAlongSpine(rA.spine, dA)
        const ptB = this.pointAlongSpine(rB.spine, dB)
        if (!ptA || !ptB) continue

        const dx = ptB[0] - ptA[0]
        const dy = ptB[1] - ptA[1]
        const dl = Math.hypot(dx, dy) || 1
        const nx = -dy / dl
        const ny = dx / dl
        const u = clamp(baseD / (238 * power), 0, 1)
        const arch = this.rnd(-2.0, 1.5)
        const cx = (ptA[0] + ptB[0]) * 0.5 + nx * arch
        const cy = (ptA[1] + ptB[1]) * 0.5 + ny * arch

        rings.push({
          x0: ptA[0],
          y0: ptA[1],
          cx,
          cy,
          x1: ptB[0],
          y1: ptB[1],
          nx,
          ny,
          u,
          spec: this.rnd(0.68, 1.0),
        })
      }
    }

    // 3. Pulverized conchoidal impact crater (dense white/gold crushed silica rosette)
    const craterFacets = []
    const facetCount = Math.round(68 * power)
    for (let i = 0; i < facetCount; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const dist = Math.pow(this.rnd(), 1.45) * 30 * power
      const fx = x + Math.cos(ang) * dist
      const fy = y + Math.sin(ang) * dist
      const r = this.rnd(1.5, 5.8) * (1 - dist / (36 * power))
      const a0 = this.rnd(0, Math.PI * 2)
      craterFacets.push({
        pts: [
          [fx + Math.cos(a0) * r, fy + Math.sin(a0) * r],
          [fx + Math.cos(a0 + 2.1) * r * 0.85, fy + Math.sin(a0 + 2.1) * r * 0.85],
          [fx + Math.cos(a0 + 4.2) * r * 1.1, fy + Math.sin(a0 + 4.2) * r * 1.1],
        ],
        dark: dist < 7 * power ? this.rnd() < 0.55 : this.rnd() < 0.22,
        gold: this.rnd() < 0.42,
        alpha: this.rnd(0.65, 0.98),
      })
    }

    // 135 sparkling crushed-glass powder specks radiating around the impact crater
    for (let i = 0; i < 135; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const dist = Math.pow(this.rnd(), 1.55) * 88 * power
      specks.push({
        x: x + Math.cos(ang) * dist,
        y: y + Math.sin(ang) * dist,
        r: this.rnd(0.75, 2.6),
        ang: ang + this.rnd(-0.4, 0.4),
        stretch: this.rnd(1.1, 2.8),
        gold: this.rnd() < 0.62,
        star: this.rnd() < 0.18,
        alpha: this.rnd(0.58, 0.98),
      })
    }

    this.cracks.push({ x, y, power, segments, rings, wedges, craterFacets, specks, age: 0 })

    // 4. Spawn sparkling crystalline micro-splinters and golden glass dust
    const dustCount = Math.round(64 * power)
    for (let i = 0; i < dustCount; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const spd = this.rnd(40, 320) * power
      const isNeedle = this.rnd() < 0.45
      this.dust.push({
        x: x + this.rnd(-12, 12),
        y: y + this.rnd(-12, 12),
        vx: Math.cos(ang) * spd,
        vy: Math.sin(ang) * spd - this.rnd(30, 130),
        rot: ang + this.rnd(-0.5, 0.5),
        vrot: this.rnd(-14, 14),
        r: isNeedle ? this.rnd(2.0, 4.8) : this.rnd(0.9, 2.5),
        aspect: isNeedle ? this.rnd(0.22, 0.42) : this.rnd(0.65, 1.0),
        gold: this.rnd() < 0.72,
        green: this.rnd() < 0.25,
        phase: this.rnd(0, Math.PI * 2),
        age: 0,
        life: this.rnd(0.75, 1.6),
      })
    }
  }

  pointAlongSpine(spine, targetDist) {
    let acc = 0
    for (let i = 0; i < spine.length - 1; i++) {
      const a = spine[i]
      const b = spine[i + 1]
      const seg = Math.hypot(b[0] - a[0], b[1] - a[1])
      if (acc + seg >= targetDist) {
        const t = (targetDist - acc) / (seg || 1)
        return [lerp(a[0], b[0], t), lerp(a[1], b[1], t)]
      }
      acc += seg
    }
    return spine[spine.length - 1]
  }

  // Shatters the entire cracked viewport into 3D extruded tempered glass slabs suspended in
  // slow-motion bullet-time around the glowing Super Saiyan head (matching target-shatter.png!).
  shatter(epicX = this.W * 0.5, epicY = this.H * 0.46) {
    const { W, H, dpr } = this
    if (!W || !H) return

    this.captureViewport(this.cleanCtx, { clean: true, skipDownload: true })
    this.captureViewport(this.pageCtx, { skipDownload: true })
    this.pageCtx.save()
    this.pageCtx.setTransform(dpr, 0, 0, dpr, 0, 0)
    this.drawCracksOnto(this.pageCtx, 0.9)
    this.pageCtx.restore()

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
    this.flash = 0.55
    this.flashX = epicX
    this.flashY = epicY
    this.shards.length = 0

    // 1. Multi-scale Voronoi tessellation across the viewport
    const cols = W < 640 ? 7 : 10
    const rows = W < 640 ? 6 : 7
    const seeds = []
    for (let iy = 0; iy < rows; iy++) {
      for (let ix = 0; ix < cols; ix++) {
        const u = (ix + 0.5) / cols
        const v = (iy + 0.5) / rows
        let sx = u * W + this.rnd(-W / cols, W / cols) * 0.38
        let sy = v * H + this.rnd(-H / rows, H / rows) * 0.38
        const pull = (ix + iy) % 2 === 0 ? 0.14 : 0.03
        sx = lerp(sx, epicX, pull)
        sy = lerp(sy, epicY, pull)
        seeds.push([clamp(sx, 16, W - 16), clamp(sy, 16, H - 16)])
      }
    }

    const screenPoly = [
      [0, 0],
      [W, 0],
      [W, H],
      [0, H],
    ]
    const maxDist = Math.hypot(W * 0.55, H * 0.55)
    const headX = this.headX || W * 0.5
    const headY = this.headY || H * 0.38
    const clearR = (this.headR || 95) * 1.95

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
      let area = 0
      for (let k = 0; k < poly.length; k++) {
        const p0 = poly[k]
        const p1 = poly[(k + 1) % poly.length]
        cx += p0[0]
        cy += p0[1]
        area += p0[0] * p1[1] - p1[0] * p0[1]
      }
      cx /= poly.length
      cy /= poly.length
      area = Math.abs(area) * 0.5
      const localPoly = poly.map((p) => [p[0] - cx, p[1] - cy])

      // Generate internal star-fracture glints inside each 3D slab (like target-shatter.png!)
      const innerCracks = []
      const gx = localPoly[0][0] * 0.25
      const gy = localPoly[0][1] * 0.25
      for (let k = 0; k < localPoly.length; k++) {
        const vp = localPoly[k]
        innerCracks.push([gx, gy, (gx + vp[0]) * 0.5 + this.rnd(-5, 5), (gy + vp[1]) * 0.5 + this.rnd(-5, 5), vp[0] * 0.92, vp[1] * 0.92])
      }

      // Compute radial burst vector from the head so the center opens cleanly around the head
      // while the slabs decelerate into a slow-motion 3D ring framing the viewport!
      const dx = cx - headX
      const dy = cy - headY
      const dist = Math.hypot(dx, dy)
      const normD = clamp(dist / maxDist, 0, 1)
      const outAng = Math.atan2(dy, dx) + this.rnd(-0.14, 0.14)
      const dirX = Math.cos(outAng)
      const dirY = Math.sin(outAng)

      // Shards covering the head burst outward far enough to clear `clearR`, then suspend in bullet-time
      const needClear = dist < clearR ? (clearR - dist) * 3.4 + 280 : lerp(260, 95, normD)
      const impulse = needClear * this.rnd(0.86, 1.18)
      const zVel = normD > 0.52 ? this.rnd(0.08, 0.22) : this.rnd(-0.04, 0.12)

      this.shards.push({
        isDownload: false,
        isSplinter: false,
        homeX: cx,
        homeY: cy,
        x: cx,
        y: cy,
        z: 1,
        vz: zVel,
        vx: dirX * impulse + this.rnd(-28, 28),
        vy: dirY * impulse * 0.86 - this.rnd(35, 95),
        driftX: dirX * this.rnd(12, 34),
        driftY: dirY * this.rnd(8, 24) + this.rnd(14, 36),
        ang: this.rnd(-0.08, 0.08),
        spin: (dirX >= 0 ? 1 : -1) * this.rnd(0.35, 1.15),
        pitch: this.rnd(0.22, 0.85) * (this.rnd() < 0.5 ? 1 : -1),
        vPitch: this.rnd(0.65, 1.65) * (this.rnd() < 0.5 ? 1 : -1),
        yaw: this.rnd(0.25, 0.92) * (this.rnd() < 0.5 ? 1 : -1),
        vYaw: this.rnd(0.65, 1.65) * (this.rnd() < 0.5 ? 1 : -1),
        thick: lerp(8.0, 15.5, clamp(area / 15000, 0, 1)),
        refractShift: this.rnd(6, 14),
        specSeed: this.rnd(0, Math.PI * 2),
        poly: localPoly,
        innerCracks,
        normD,
        delay: normD * 0.04,
        age: 0,
        fromX: cx,
        fromY: cy,
        fromZ: 1,
        fromAng: 0,
        fromPitch: 0,
        fromYaw: 0,
      })
    }

    // 2. Secondary 3D crystalline glass splinters (sharp angular shards floating in bullet-time around the head)
    const splinterCount = W < 640 ? 38 : 68
    for (let i = 0; i < splinterCount; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const rDist = this.rnd(clearR * 0.72, maxDist * 0.92)
      const sx = clamp(headX + Math.cos(ang) * rDist, 24, W - 24)
      const sy = clamp(headY + Math.sin(ang) * rDist, 24, H - 24)
      const normD = clamp(rDist / maxDist, 0, 1)
      const len = this.rnd(10, 28)
      const wid = len * this.rnd(0.28, 0.58)
      const localPoly = [
        [-len, -wid * 0.2],
        [-len * 0.1, -wid],
        [len, wid * 0.15],
        [len * 0.15, wid * 0.9],
      ]
      const spd = this.rnd(90, 260)
      this.shards.push({
        isDownload: false,
        isSplinter: true,
        homeX: sx,
        homeY: sy,
        x: sx,
        y: sy,
        z: this.rnd(0.88, 1.28),
        vz: this.rnd(-0.04, 0.14),
        vx: Math.cos(ang) * spd,
        vy: Math.sin(ang) * spd * 0.8 - this.rnd(20, 60),
        driftX: Math.cos(ang) * this.rnd(8, 24),
        driftY: this.rnd(12, 32),
        ang: ang + this.rnd(-0.8, 0.8),
        spin: this.rnd(-1.8, 1.8),
        pitch: this.rnd(0.3, 1.2),
        vPitch: this.rnd(-2.2, 2.2),
        yaw: this.rnd(0.3, 1.2),
        vYaw: this.rnd(-2.2, 2.2),
        thick: this.rnd(4.5, 8.0),
        refractShift: 6,
        specSeed: this.rnd(0, Math.PI * 2),
        poly: localPoly,
        innerCracks: [[-len * 0.8, 0, 0, 0, len * 0.8, 0]],
        normD,
        delay: normD * 0.03,
        age: 0,
        fromX: sx,
        fromY: sy,
        fromZ: 1,
        fromAng: 0,
        fromPitch: 0,
        fromYaw: 0,
      })
    }

    // 3. Dedicated hero piece for the knocked-off Download button: pops up and tumbles in slow-motion
    // right below the glowing head (matching target-shatter.png!) so the viewer watches it float away!
    if (dlRect) {
      this.shards.push({
        isDownload: true,
        isSplinter: false,
        homeX: dlRect.cx,
        homeY: dlRect.cy,
        w: dlRect.w,
        h: dlRect.h,
        label: dlRect.label,
        x: dlRect.cx + 10,
        y: dlRect.cy - 28,
        z: 1.06,
        vz: 0.02,
        vx: 38,
        vy: -240,
        driftX: 12,
        driftY: -6,
        ang: 0.14,
        spin: 0.22,
        pitch: 0.12,
        vPitch: 0.28,
        yaw: 0.14,
        vYaw: 0.24,
        thick: 9,
        normD: 0.15,
        delay: 0,
        age: 0,
        fromX: dlRect.cx,
        fromY: dlRect.cy,
        fromZ: 1,
        fromAng: 0,
        fromPitch: 0,
        fromYaw: 0,
      })
    }

    // 4. Cloud of 320 sparkling golden & white-diamond glass powder particles suspended in the void
    const glitterCount = W < 640 ? 160 : 320
    for (let i = 0; i < glitterCount; i++) {
      const ang = this.rnd(0, Math.PI * 2)
      const dist = Math.pow(this.rnd(), 0.72) * maxDist * 0.85
      const gx = headX + Math.cos(ang) * dist * 0.65 + this.rnd(-25, 25)
      const gy = headY + Math.sin(ang) * dist * 0.65 + this.rnd(-25, 25)
      const spd = this.rnd(25, 180)
      const isSliver = this.rnd() < 0.38
      this.dust.push({
        x: gx,
        y: gy,
        vx: Math.cos(ang) * spd + this.rnd(-22, 22),
        vy: Math.sin(ang) * spd * 0.75 - this.rnd(15, 65),
        BulletTime: true,
        rot: this.rnd(0, Math.PI * 2),
        vrot: this.rnd(-8, 8),
        r: isSliver ? this.rnd(1.8, 4.4) : this.rnd(0.85, 2.5),
        aspect: isSliver ? this.rnd(0.24, 0.46) : this.rnd(0.7, 1.0),
        gold: this.rnd() < 0.8,
        green: this.rnd() < 0.2,
        phase: this.rnd(0, Math.PI * 2),
        age: 0,
        life: this.rnd(4.5, 6.2),
      })
    }
  }

  startRebuild(dur = 1.8) {
    if (this.state !== "shattered") return
    this.state = "rebuilding"
    this.rebuildAge = 0
    this.rebuildDur = dur
    this.needsCleanCapture = true
    for (const s of this.shards) {
      s.fromX = clamp(s.x, -180, this.W + 180)
      s.fromY = Math.min(s.y, this.H + 240)
      s.fromZ = s.z ?? 1
      s.fromAng = ((s.ang + Math.PI) % (Math.PI * 2)) - Math.PI
      s.fromPitch = ((s.pitch + Math.PI) % (Math.PI * 2)) - Math.PI
      s.fromYaw = ((s.yaw + Math.PI) % (Math.PI * 2)) - Math.PI
    }
  }

  drawCracksOnto(ctx, alpha = 1) {
    const { W, H } = this
    const hx = this.headX || W * 0.5
    const hy = this.headY || H * 0.38
    const lightRange = Math.min(W, H) * 0.82

    for (const c of this.cracks) {
      // 1. Optical refraction across angular glass facets + subtle RGB chromatic dispersion
      for (const w of c.wedges) {
        ctx.save()
        ctx.beginPath()
        ctx.moveTo(w.poly[0][0], w.poly[0][1])
        for (let i = 1; i < w.poly.length; i++) ctx.lineTo(w.poly[i][0], w.poly[i][1])
        ctx.closePath()
        ctx.clip()
        ctx.globalAlpha = 0.96 * alpha
        ctx.drawImage(this.basePageCanvas, 0, 0, this.basePageCanvas.width, this.basePageCanvas.height, w.ox, w.oy, W, H)
        ctx.globalCompositeOperation = "lighter"
        ctx.globalAlpha = 0.14 * alpha
        ctx.drawImage(
          this.basePageCanvas,
          0,
          0,
          this.basePageCanvas.width,
          this.basePageCanvas.height,
          w.ox + w.chromaX,
          w.oy + w.chromaY,
          W,
          H,
        )
        ctx.restore()
      }

      // 2. Densely packed concentric Hertzian stress rings (with head-lit golden & green-edge highlights)
      ctx.save()
      ctx.lineCap = "round"
      for (const ring of c.rings) {
        const taper = Math.pow(1 - ring.u, 0.52)
        const lx = hx - ring.cx
        const ly = hy - ring.cy
        const ld = Math.hypot(lx, ly) || 1
        const ndotl = Math.abs((ring.nx * lx + ring.ny * ly) / ld)
        const headProx = clamp(1.32 - ld / lightRange, 0.25, 1.32)
        const goldAmt = Math.pow(ndotl, 1.35) * headProx

        ctx.strokeStyle = `rgba(0, 4, 10, ${(0.82 * taper + 0.14) * alpha})`
        ctx.lineWidth = 1.5 * taper + 0.35
        ctx.beginPath()
        ctx.moveTo(ring.x0 + 1.0, ring.y0 + 1.0)
        ctx.quadraticCurveTo(ring.cx + 1.0, ring.cy + 1.0, ring.x1 + 1.0, ring.y1 + 1.0)
        ctx.stroke()

        ctx.strokeStyle = `rgba(110, 242, 202, ${0.28 * taper * alpha})`
        ctx.lineWidth = 1.75 * taper + 0.35
        ctx.beginPath()
        ctx.moveTo(ring.x0, ring.y0)
        ctx.quadraticCurveTo(ring.cx, ring.cy, ring.x1, ring.y1)
        ctx.stroke()

        const rCol = Math.round(lerp(232, 255, goldAmt))
        const gCol = Math.round(lerp(245, 224, goldAmt * 0.52))
        const bCol = Math.round(lerp(252, 148, goldAmt * 0.76))
        ctx.strokeStyle = `rgba(${rCol}, ${gCol}, ${bCol}, ${Math.min(1, (0.44 + 0.58 * ring.spec * headProx) * (taper + 0.12)) * alpha})`
        ctx.lineWidth = 0.95 * taper + 0.28
        ctx.beginPath()
        ctx.moveTo(ring.x0, ring.y0)
        ctx.quadraticCurveTo(ring.cx, ring.cy, ring.x1, ring.y1)
        ctx.stroke()
      }

      // 3. Branching radial stress cracks with tapering width, green edge tint & head-lit golden specular glints
      for (const seg of c.segments) {
        const taper = Math.pow(1 - seg.u0, 0.65) * (seg.depth === 0 ? 1 : seg.depth === 1 ? 0.74 : 0.5)
        const lx = hx - seg.mx
        const ly = hy - seg.my
        const ld = Math.hypot(lx, ly) || 1
        const ndotl = Math.abs((seg.nx * lx + seg.ny * ly) / ld)
        const headProx = clamp(1.36 - ld / lightRange, 0.25, 1.35)
        const goldGlow = Math.pow(ndotl, 1.35) * headProx

        const shOff = 1.15 * taper + 0.35
        ctx.strokeStyle = `rgba(0, 3, 9, ${(0.86 * taper + 0.14) * alpha})`
        ctx.lineWidth = 2.2 * taper + 0.45
        ctx.beginPath()
        ctx.moveTo(seg.x0 + shOff, seg.y0 + shOff)
        ctx.lineTo(seg.x1 + shOff, seg.y1 + shOff)
        ctx.stroke()

        if (taper > 0.2) {
          ctx.strokeStyle = `rgba(105, 240, 200, ${0.32 * taper * alpha})`
          ctx.lineWidth = 2.5 * taper + 0.42
          ctx.beginPath()
          ctx.moveTo(seg.x0 - seg.nx * 0.55, seg.y0 - seg.ny * 0.55)
          ctx.lineTo(seg.x1 - seg.nx * 0.55, seg.y1 - seg.ny * 0.55)
          ctx.stroke()
        }

        if (goldGlow > 0.22) {
          ctx.strokeStyle = `rgba(255, 195, 72, ${Math.min(0.95, goldGlow * 0.82 * taper) * alpha})`
          ctx.lineWidth = 2.0 * taper + 0.38
          ctx.beginPath()
          ctx.moveTo(seg.x0 + seg.nx * 0.4, seg.y0 + seg.ny * 0.4)
          ctx.lineTo(seg.x1 + seg.nx * 0.4, seg.y1 + seg.ny * 0.4)
          ctx.stroke()
        }

        ctx.strokeStyle = `rgba(255, 253, 246, ${Math.min(1, (0.46 + 0.56 * seg.spec * (0.5 + 0.5 * ndotl)) * (taper + 0.14)) * alpha})`
        ctx.lineWidth = 1.32 * taper + 0.3
        ctx.beginPath()
        ctx.moveTo(seg.x0, seg.y0)
        ctx.lineTo(seg.x1, seg.y1)
        ctx.stroke()
      }
      ctx.restore()

      // 4. Pulverized conchoidal impact crater (bright frosted silica rosette + dark central puncture)
      ctx.save()
      const rCore = 30 * c.power
      const halo = ctx.createRadialGradient(c.x, c.y, 1, c.x, c.y, rCore * 1.45)
      halo.addColorStop(0, `rgba(255, 253, 246, ${0.92 * alpha})`)
      halo.addColorStop(0.26, `rgba(242, 249, 255, ${0.56 * alpha})`)
      halo.addColorStop(0.58, `rgba(185, 236, 224, ${0.18 * alpha})`)
      halo.addColorStop(1, "rgba(120, 200, 190, 0)")
      ctx.fillStyle = halo
      ctx.beginPath()
      ctx.arc(c.x, c.y, rCore * 1.45, 0, Math.PI * 2)
      ctx.fill()

      for (const f of c.craterFacets) {
        ctx.beginPath()
        ctx.moveTo(f.pts[0][0], f.pts[0][1])
        ctx.lineTo(f.pts[1][0], f.pts[1][1])
        ctx.lineTo(f.pts[2][0], f.pts[2][1])
        ctx.closePath()
        ctx.fillStyle = f.dark
          ? `rgba(4, 7, 14, ${f.alpha * 0.9 * alpha})`
          : f.gold
            ? `rgba(255, 236, 180, ${f.alpha * 0.94 * alpha})`
            : `rgba(250, 254, 255, ${f.alpha * 0.96 * alpha})`
        ctx.fill()
      }

      // 5. Embedded crystalline micro-specks & star glints along the cracks & impact rosette
      for (const sp of c.specks) {
        ctx.save()
        ctx.translate(sp.x, sp.y)
        ctx.rotate(sp.ang)
        ctx.fillStyle = sp.gold
          ? `rgba(255, 224, 132, ${sp.alpha * alpha})`
          : `rgba(248, 253, 255, ${sp.alpha * alpha})`
        ctx.beginPath()
        ctx.ellipse(0, 0, sp.r * sp.stretch, sp.r * 0.7, 0, 0, Math.PI * 2)
        ctx.fill()
        if (sp.star) {
          ctx.strokeStyle = `rgba(255, 248, 220, ${sp.alpha * 0.85 * alpha})`
          ctx.lineWidth = 0.65
          const sl = sp.r * 2.8
          ctx.beginPath()
          ctx.moveTo(-sl, 0)
          ctx.lineTo(sl, 0)
          ctx.moveTo(0, -sl * 0.7)
          ctx.lineTo(0, sl * 0.7)
          ctx.stroke()
        }
        ctx.restore()
      }
      ctx.restore()
    }
  }

  // Draws one 3D extruded glass slab (thick beveled side walls + refracted face + golden Fresnel reflection + internal caustics)
  draw3DGlassShard(ctx, s, healMix, healFade, seamGlint, isGhost = false) {
    if (s.isSplinter && this.state === "rebuilding" && healMix > 0.35) return
    const { W, H } = this
    const hx = this.headX || W * 0.5
    const hy = this.headY || H * 0.38
    const lightRange = Math.min(W, H) * 0.88

    const tiltX = this.state === "rebuilding" ? lerp(0.45 + 0.55 * Math.cos(s.yaw), 1, healMix) : 0.42 + 0.58 * Math.cos(s.yaw)
    const tiltY = this.state === "rebuilding" ? lerp(0.58 + 0.42 * Math.cos(s.pitch), 1, healMix) : 0.56 + 0.44 * Math.cos(s.pitch)
    const zScale = this.state === "rebuilding" ? lerp(s.z ?? 1, 1, healMix) : clamp(s.z ?? 1, 0.78, 1.48)

    // 3D slab extrusion vector in local space (always visible while shattered, collapses to 0 when healed)
    const thickFactor = (1 - healMix) * (isGhost ? 0.6 : 1)
    const rawEx = Math.sin(s.yaw) * s.thick
    const rawEy = Math.sin(s.pitch) * s.thick
    const minBevel = s.thick * 0.45
    const ex = (Math.abs(rawEx) < minBevel ? (rawEx >= 0 ? minBevel : -minBevel) : rawEx) * thickFactor
    const ey = (Math.abs(rawEy) < minBevel ? (rawEy >= 0 ? minBevel : -minBevel) : rawEy) * thickFactor

    const lx = hx - s.x
    const ly = hy - s.y
    const ld = Math.hypot(lx, ly) || 1
    const lDirX = lx / ld
    const lDirY = ly / ld
    const headProx = clamp(1.42 - ld / lightRange, 0.32, 1.42)
    const faceSpec = (0.28 + 0.72 * Math.pow(0.5 + 0.5 * Math.sin(s.yaw * 1.7 + s.pitch * 1.4 + (s.specSeed ?? 0)), 3)) * (1 - healMix)

    const splinterFade = s.isSplinter && this.state === "rebuilding" ? 1 - healMix / 0.35 : 1

    ctx.save()
    ctx.globalAlpha = healFade * splinterFade * (isGhost ? 0.22 : 1)
    ctx.translate(s.x, s.y)
    ctx.scale(zScale, zScale)
    ctx.rotate(s.ang)
    ctx.scale(tiltX, tiltY)

    if (s.isDownload) {
      if (thickFactor > 0.05) {
        ctx.fillStyle = "rgba(185, 212, 222, 0.9)"
        ctx.beginPath()
        ctx.roundRect(-s.w / 2 + ex * 0.55, -s.h / 2 + ey * 0.55, s.w, s.h, s.h / 2)
        ctx.fill()
      }
      this.drawDownloadPill(ctx, -s.w / 2, -s.h / 2, s.w, s.h, s.label)
      ctx.strokeStyle = `rgba(255, 228, 148, ${Math.min(1, 0.55 + faceSpec * 0.45 + seamGlint)})`
      ctx.lineWidth = 2.2
      ctx.beginPath()
      ctx.roundRect(-s.w / 2, -s.h / 2, s.w, s.h, s.h / 2)
      ctx.stroke()
      ctx.restore()
      return
    }

    const poly = s.poly
    const n = poly.length

    // 1. Draw 3D extruded tempered-glass side walls (green borosilicate thickness + molten golden head-lit bevels)
    if (thickFactor > 0.04) {
      const cosA = Math.cos(s.ang)
      const sinA = Math.sin(s.ang)
      for (let k = 0; k < n; k++) {
        const p0 = poly[k]
        const p1 = poly[(k + 1) % n]
        const edx = p1[0] - p0[0]
        const edy = p1[1] - p0[1]
        const el = Math.hypot(edx, edy) || 1
        const lnx = edy / el
        const lny = -edx / el
        const wnx = lnx * cosA - lny * sinA
        const wny = lnx * sinA + lny * cosA
        const rimDot = Math.max(0, wnx * lDirX + wny * lDirY) * headProx

        ctx.beginPath()
        ctx.moveTo(p0[0], p0[1])
        ctx.lineTo(p1[0], p1[1])
        ctx.lineTo(p1[0] + ex, p1[1] + ey)
        ctx.lineTo(p0[0] + ex, p0[1] + ey)
        ctx.closePath()

        if (rimDot > 0.24) {
          const gIntensity = clamp(rimDot * 1.2, 0, 1)
          const rC = 255
          const gC = Math.round(lerp(182, 248, gIntensity))
          const bC = Math.round(lerp(58, 218, gIntensity * gIntensity))
          ctx.fillStyle = `rgba(${rC}, ${gC}, ${bC}, ${lerp(0.75, 0.98, gIntensity)})`
        } else {
          ctx.fillStyle = "rgba(18, 56, 48, 0.88)"
        }
        ctx.fill()

        ctx.strokeStyle =
          rimDot > 0.24
            ? `rgba(255, 248, 215, ${Math.min(1, rimDot * 0.96)})`
            : "rgba(115, 242, 204, 0.56)"
        ctx.lineWidth = 0.85
        ctx.stroke()
      }
    }

    // 2. Front glass face: refracted page content + glossy Fresnel reflection + internal caustic glints
    ctx.beginPath()
    ctx.moveTo(poly[0][0], poly[0][1])
    for (let k = 1; k < n; k++) ctx.lineTo(poly[k][0], poly[k][1])
    ctx.closePath()

    ctx.save()
    ctx.clip()

    if (!s.isSplinter) {
      const refX = Math.sin(s.yaw) * (s.refractShift ?? 9) * (1 - healMix)
      const refY = Math.sin(s.pitch) * (s.refractShift ?? 9) * (1 - healMix)

      if (healMix < 0.99) {
        ctx.drawImage(this.pageCanvas, 0, 0, this.pageCanvas.width, this.pageCanvas.height, -s.homeX + refX, -s.homeY + refY, W, H)
      }
      if (healMix > 0.01) {
        ctx.globalAlpha = healMix * healFade
        ctx.drawImage(this.cleanCanvas, 0, 0, this.cleanCanvas.width, this.cleanCanvas.height, -s.homeX, -s.homeY, W, H)
      }

      // Rich Fresnel glass surface reflection catching the warm golden Super Saiyan light & emerald glass tint
      if (!isGhost && healMix < 0.9) {
        const cosA = Math.cos(-s.ang)
        const sinA = Math.sin(-s.ang)
        const locLx = (lDirX * cosA - lDirY * sinA) * 75
        const locLy = (lDirX * sinA + lDirY * cosA) * 75
        const gloss = ctx.createLinearGradient(locLx, locLy, -locLx, -locLy)
        const gA = (0.18 + faceSpec * 0.42) * headProx * (1 - healMix)
        gloss.addColorStop(0, `rgba(255, 218, 118, ${Math.min(0.62, gA * 1.15)})`)
        gloss.addColorStop(0.38, `rgba(255, 252, 240, ${Math.min(0.54, gA * 0.95)})`)
        gloss.addColorStop(0.68, `rgba(120, 238, 206, ${Math.min(0.24, gA * 0.35)})`)
        gloss.addColorStop(1, "rgba(18, 42, 40, 0.12)")
        ctx.fillStyle = gloss
        ctx.fill()
      }

      // Internal stress-fracture caustics & star-glint inside the slab (matching target-shatter.png!)
      if (!isGhost && s.innerCracks && healMix < 0.88) {
        const icAlpha = (0.38 + faceSpec * 0.58) * (1 - healMix)
        for (const ic of s.innerCracks) {
          ctx.strokeStyle = `rgba(0, 6, 14, ${icAlpha * 0.7})`
          ctx.lineWidth = 1.4
          ctx.beginPath()
          ctx.moveTo(ic[0] + 1, ic[1] + 1)
          ctx.lineTo(ic[2] + 1, ic[3] + 1)
          ctx.lineTo(ic[4] + 1, ic[5] + 1)
          ctx.stroke()

          ctx.strokeStyle = `rgba(255, 238, 182, ${Math.min(0.95, icAlpha)})`
          ctx.lineWidth = 0.95
          ctx.beginPath()
          ctx.moveTo(ic[0], ic[1])
          ctx.lineTo(ic[2], ic[3])
          ctx.lineTo(ic[4], ic[5])
          ctx.stroke()
        }
        // Bright caustic star-hotspot at the internal fracture vertex
        const gv = s.innerCracks[0]
        if (gv && faceSpec > 0.35) {
          const sg = ctx.createRadialGradient(gv[0], gv[1], 0.5, gv[0], gv[1], 18)
          sg.addColorStop(0, `rgba(255, 253, 242, ${Math.min(0.92, faceSpec * (1 - healMix))})`)
          sg.addColorStop(0.4, `rgba(255, 205, 92, ${Math.min(0.55, faceSpec * 0.65 * (1 - healMix))})`)
          sg.addColorStop(1, "rgba(255, 190, 60, 0)")
          ctx.fillStyle = sg
          ctx.beginPath()
          ctx.arc(gv[0], gv[1], 18, 0, Math.PI * 2)
          ctx.fill()
        }
      }
    } else {
      // Secondary 3D crystalline splinter: warm golden-white specular prism
      const spGrad = ctx.createLinearGradient(-12, -8, 12, 8)
      spGrad.addColorStop(0, `rgba(255, 248, 222, ${0.75 + faceSpec * 0.25})`)
      spGrad.addColorStop(0.5, `rgba(255, 202, 88, ${0.58 + faceSpec * 0.32})`)
      spGrad.addColorStop(1, "rgba(120, 242, 208, 0.45)")
      ctx.fillStyle = spGrad
      ctx.fill()
    }
    ctx.restore()

    // 3. Per-edge directional specular bevel outline on the front face
    const disp = clamp(Math.hypot(s.x - s.homeX, s.y - s.homeY) / 36 + Math.abs(s.ang) * 1.4, 0, 1)
    const edgeVis = (this.state === "shattered" ? 1 : disp * (1 - healMix * 0.9)) + seamGlint
    if (edgeVis > 0.03) {
      const cosA = Math.cos(s.ang)
      const sinA = Math.sin(s.ang)
      for (let k = 0; k < n; k++) {
        const p0 = poly[k]
        const p1 = poly[(k + 1) % n]
        const edx = p1[0] - p0[0]
        const edy = p1[1] - p0[1]
        const el = Math.hypot(edx, edy) || 1
        const wnx = (edy / el) * cosA - (-edx / el) * sinA
        const wny = (edy / el) * sinA + (-edx / el) * cosA
        const rim = Math.max(0, wnx * lDirX + wny * lDirY) * headProx

        ctx.beginPath()
        ctx.moveTo(p0[0], p0[1])
        ctx.lineTo(p1[0], p1[1])
        if (seamGlint > 0.15) {
          ctx.strokeStyle = `rgba(255, 246, 205, ${Math.min(1, edgeVis)})`
          ctx.lineWidth = 1.9
        } else if (rim > 0.22) {
          ctx.strokeStyle = `rgba(255, 238, 175, ${Math.min(1, (0.52 + rim * 0.65) * edgeVis)})`
          ctx.lineWidth = 1.55 * clamp(edgeVis, 0.4, 1) + 0.35
        } else {
          ctx.strokeStyle = `rgba(130, 242, 208, ${Math.min(0.88, 0.55 * edgeVis)})`
          ctx.lineWidth = 1.05 * clamp(edgeVis, 0.4, 1) + 0.25
        }
        ctx.stroke()
      }
    }

    ctx.restore()
  }

  // Draws bold comic/cinema word-synced captions at the lower third so finale lines land even when muted.
  drawCaption(ctx, rtime) {
    const cap = this.caption
    if (!cap || rtime < cap.startRtime - 0.08 || rtime > cap.endRtime + 0.35) return
    const { W, H } = this
    const elapsed = rtime - cap.startRtime
    const fadeIn = smooth(-0.08, 0.08, elapsed)
    const fadeOut = 1 - smooth(cap.endRtime - cap.startRtime, cap.endRtime - cap.startRtime + 0.32, elapsed)
    const alpha = fadeIn * fadeOut
    if (alpha <= 0.01) return

    const fontSize = clamp(Math.round(W * 0.019), 17, 26)
    ctx.save()
    ctx.globalAlpha = alpha
    ctx.font = `760 ${fontSize}px ${SANS}`
    ctx.textBaseline = "middle"

    const spaceW = Math.max(10, ctx.measureText(" ").width * 1.65)
    const wordMetrics = cap.words.map((w, idx) => {
      const nextT = cap.words[idx + 1]?.t ?? cap.endRtime - cap.startRtime + 0.1
      const active = elapsed >= w.t - 0.04 && elapsed < nextT + 0.08
      const spoken = elapsed >= w.t - 0.04
      const width = ctx.measureText(w.text).width
      return { ...w, width, active, spoken }
    })
    const totalW = wordMetrics.reduce((acc, m) => acc + m.width, 0) + Math.max(0, wordMetrics.length - 1) * spaceW
    const padX = 26
    const padY = 13
    const boxW = Math.min(W - 32, totalW + padX * 2)
    const boxH = fontSize + padY * 2
    const boxX = (W - boxW) * 0.5
    const cy = H * 0.905
    const boxY = cy - boxH * 0.5

    ctx.save()
    ctx.shadowColor = "rgba(0, 0, 0, 0.82)"
    ctx.shadowBlur = 24
    ctx.shadowOffsetY = 6
    ctx.fillStyle = "rgba(8, 10, 14, 0.9)"
    ctx.strokeStyle = "rgba(255, 196, 72, 0.62)"
    ctx.lineWidth = 1.6
    ctx.beginPath()
    ctx.roundRect(boxX, boxY, boxW, boxH, 12)
    ctx.fill()
    ctx.stroke()
    ctx.restore()

    let curX = (W - totalW) * 0.5
    ctx.textAlign = "left"
    for (const m of wordMetrics) {
      ctx.save()
      const wcx = curX + m.width * 0.5
      ctx.translate(wcx, cy)
      if (m.active) {
        ctx.scale(1.08, 1.08)
        ctx.shadowColor = "rgba(255, 188, 46, 0.8)"
        ctx.shadowBlur = 12
        ctx.fillStyle = "#ffcb47"
      } else if (m.spoken) {
        ctx.fillStyle = "#f9fafb"
      } else {
        ctx.fillStyle = "rgba(205, 210, 220, 0.52)"
      }
      ctx.fillText(m.text, -m.width * 0.5, 0)
      ctx.restore()
      curX += m.width + spaceW
    }

    ctx.restore()
  }

  stepAndDraw(dt, rtime = 0) {
    if (this.state === "idle" && this.zWords.length === 0 && !this.caption) return
    const { ctx, W, H, dpr } = this
    if (!W || !H) return
    if (this.needsCleanCapture) {
      this.needsCleanCapture = false
      this.captureViewport(this.cleanCtx, { clean: true, skipDownload: true })
    }

    ctx.save()
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, W, H)

    this.flash = Math.max(0, this.flash - dt * 6.8)
    for (const c of this.cracks) c.age += dt

    // 1. If in "cracking" state, draw the ballistic fractures & page refraction
    if (this.state === "cracking" && this.cracks.length > 0) {
      this.drawCracksOnto(ctx, 1)
    }

    // 2. If in "shattered" or "rebuilding" state, simulate & draw the 3D tempered glass slabs + Download button
    if (this.state === "shattered" || this.state === "rebuilding") {
      let allDone = false
      let healFade = 1
      if (this.state === "rebuilding") {
        this.rebuildAge += dt
        const overallT = clamp(this.rebuildAge / this.rebuildDur, 0, 1)
        healFade = 1 - smooth(0.88, 1.0, overallT)
        if (overallT >= 1) allDone = true
      }

      for (const s of this.shards) {
        let healMix = 0
        let seamGlint = 0
        if (this.state === "shattered") {
          s.age += dt
          if (s.age > s.delay) {
            const effDt = dt
            // Explosive outward burst for the first 0.28s, then strong bullet-time damping so
            // 3D slabs and the Download button suspend & tumble across the viewport during the punchline!
            const drag = s.age < 0.28 ? 2.4 : 4.2
            s.vx = lerp(s.vx, s.driftX ?? 0, 1 - Math.exp(-drag * effDt))
            s.vy = lerp(s.vy, s.driftY ?? 22, 1 - Math.exp(-drag * effDt))
            s.vy += (s.isDownload ? 38 : 32) * effDt
            s.x += s.vx * effDt
            s.y += s.vy * effDt
            s.z = clamp((s.z ?? 1) + (s.vz ?? 0) * effDt, 0.78, 1.46)
            s.ang += s.spin * effDt
            s.pitch += s.vPitch * effDt
            s.yaw += s.vYaw * effDt
          }
        } else {
          // Time-reverse crystalline rebuild: outer shards lift first, center + Download button snap in last
          const startT = (1 - s.normD) * 0.24
          const span = 0.62
          const loc = clamp((this.rebuildAge / this.rebuildDur - startT) / span, 0, 1)
          const ease = loc < 0.5 ? 4 * loc * loc * loc : 1 - Math.pow(-2 * loc + 2, 3) / 2
          const arcLift = Math.sin(loc * Math.PI) * -42 * (0.4 + s.normD)
          s.x = lerp(s.fromX, s.homeX, ease)
          s.y = lerp(s.fromY, s.homeY, ease) + arcLift
          s.z = lerp(s.fromZ ?? 1, 1, ease)
          s.ang = lerp(s.fromAng, 0, ease)
          s.pitch = lerp(s.fromPitch, 0, ease)
          s.yaw = lerp(s.fromYaw, 0, ease)
          healMix = smooth(0.48, 0.92, loc)
          seamGlint = Math.exp(-Math.pow((loc - 0.92) / 0.055, 2))
        }

        if (s.y - 220 > H && this.state === "shattered") continue

        // Foreground depth-of-field / motion-blur ghost pass on outer panes
        if (this.state === "shattered" && !s.isDownload && !s.isSplinter && (s.normD > 0.58 || s.z > 1.14)) {
          const mbX = clamp(s.vx * 0.025 + (s.x - W * 0.5) * 0.012, -10, 10)
          const mbY = clamp(s.vy * 0.025 + (s.y - H * 0.5) * 0.012, -10, 10)
          ctx.save()
          ctx.translate(mbX, mbY)
          this.draw3DGlassShard(ctx, s, healMix, healFade, seamGlint, true)
          ctx.restore()
        }

        this.draw3DGlassShard(ctx, s, healMix, healFade, seamGlint, false)
      }

      if (allDone) {
        this.reset()
        ctx.restore()
        return
      }
    }

    // 3. Simulate & draw sparkling crystalline micro-splinters and golden glass dust (zero grey triangles!)
    for (let i = this.dust.length - 1; i >= 0; i--) {
      const d = this.dust[i]
      d.age += dt
      if (d.age >= d.life) {
        this.dust.splice(i, 1)
        continue
      }
      if (this.state === "rebuilding") {
        const cx = this.headX || W * 0.5
        const cy = this.headY || H * 0.45
        d.vx = lerp(d.vx, (cx - d.x) * 2.2, dt * 5)
        d.vy = lerp(d.vy, (cy - d.y) * 2.2 - 120, dt * 5)
      } else if (d.BulletTime) {
        d.vx *= Math.exp(-2.8 * dt)
        d.vy = lerp(d.vy, 18, dt * 3.2)
      } else {
        d.vx *= Math.exp(-1.1 * dt)
        d.vy += 320 * dt
      }
      d.x += d.vx * dt
      d.y += d.vy * dt
      d.rot += d.vrot * dt

      const lifeAlpha = (1 - d.age / d.life) * (this.state === "rebuilding" ? 1 - this.rebuildAge / this.rebuildDur : 1)
      const twinkle = 0.38 + 0.62 * Math.pow(0.5 + 0.5 * Math.sin(d.age * 24 + d.phase), 4)
      const a = clamp(lifeAlpha * twinkle, 0, 1)
      if (a <= 0.02) continue

      ctx.save()
      ctx.translate(d.x, d.y)
      ctx.rotate(d.rot)
      ctx.fillStyle = d.gold
        ? `rgba(255, 216, 102, ${a})`
        : d.green
          ? `rgba(135, 248, 212, ${a})`
          : `rgba(255, 253, 245, ${a})`
      ctx.beginPath()
      const rx = d.r
      const ry = d.r * d.aspect
      ctx.moveTo(0, -ry)
      ctx.lineTo(rx, 0)
      ctx.lineTo(0, ry)
      ctx.lineTo(-rx, 0)
      ctx.closePath()
      ctx.fill()
      ctx.restore()
    }

    // 4. Simulate & draw Z-axis words rocketing straight at the camera lens with zoom motion blur
    for (let i = this.zWords.length - 1; i >= 0; i--) {
      const zw = this.zWords[i]
      zw.age += dt
      const p = clamp(zw.age / zw.dur, 0, 1)
      const zCurve = Math.pow(p, 2.35)
      const x = lerp(zw.fromX, zw.targetX, zCurve)
      const y = lerp(zw.fromY, zw.targetY, zCurve)
      const scale = lerp(0.36, zw.final ? 8.2 : 4.8, Math.pow(p, 2.65))
      const rot = zw.rot + zw.vrot * zw.age

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
          this.addCrack(zw.targetX, zw.targetY, zw.hitsDownload ? 1.22 : 1.0, zw.hitsDownload)
        }
        zw.onImpact?.(zw)
      }
    }

    // 5. Tight localized impact flash (warm golden-white at the hit point, zero full-screen washout)
    if (this.flash > 0.01) {
      const fx = this.flashX ?? W * 0.5
      const fy = this.flashY ?? H * 0.5
      const fr = this.state === "shattered" ? Math.min(W, H) * 0.42 : 135
      const fg = ctx.createRadialGradient(fx, fy, 0, fx, fy, fr)
      fg.addColorStop(0, `rgba(255, 250, 232, ${Math.min(0.62, this.flash)})`)
      fg.addColorStop(0.35, `rgba(255, 205, 110, ${Math.min(0.24, this.flash * 0.42)})`)
      fg.addColorStop(1, "rgba(255, 180, 70, 0)")
      ctx.fillStyle = fg
      ctx.beginPath()
      ctx.arc(fx, fy, fr, 0, Math.PI * 2)
      ctx.fill()
    }

    // 6. Bold comic/cinema captions for the finale voice lines
    this.drawCaption(ctx, rtime)

    ctx.restore()
  }
}
