/// <reference types="@webgpu/types" />
// The hero's WebGPU engine: a swarm (sim.wgsl) shaped by the take's envelope, the instrument that
// reads it (draw.wgsl), then bloom and tone (post.wgsl). Adapted from the knobs MAGI hero. The
// caller owns layout, timing and the take; each frame it hands over a filled uniform block and
// the voice buffer.
import commonWgsl from "./common.wgsl?raw"
import drawWgsl from "./draw.wgsl?raw"
import postWgsl from "./post.wgsl?raw"
import simWgsl from "./sim.wgsl?raw"

const HDR: GPUTextureFormat = "rgba16float"
const LEVELS = 5
const NEAR = 192
const FAR = 48
export const BINS = 256
export const LEVELS_N = 160
export const UNIFORM_FLOATS = 21 * 4

export interface Engine {
  readonly agents: number
  /** Draws a frame. `params` must already hold everything but the canvas size. */
  frame(params: Float32Array, voice: Float32Array, look: { bloom: number; exposure: number }): void
  /** Device-pixel size the next frame renders at. */
  size(): { w: number; h: number; dpr: number }
  /** GPU milliseconds per stage, averaged since the last call (where timestamps exist). */
  timings(): Record<string, number> | null
  destroy(): void
}

export async function createEngine(
  stage: HTMLElement,
  options: { agents: number; maxDpr: number; page: readonly number[] },
): Promise<Engine | null> {
  if (!navigator.gpu) return null
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" })
  if (!adapter) return null
  const timing = adapter.features.has("timestamp-query")
  const device = await adapter.requestDevice({ requiredFeatures: timing ? ["timestamp-query"] : [] })
  let lost = false
  device.lost.then(() => {
    lost = true
  })
  device.addEventListener("uncapturederror", (e) =>
    console.error("hex hero:", (e as GPUUncapturedErrorEvent).error.message),
  )
  const AGENTS = options.agents
  const canvas = document.createElement("canvas")
  canvas.setAttribute("aria-hidden", "true")
  canvas.className = "hero-canvas"
  const ctx = canvas.getContext("webgpu")
  if (!ctx) {
    device.destroy()
    return null
  }
  const format = navigator.gpu.getPreferredCanvasFormat()
  ctx.configure({ device, format, alphaMode: "opaque" })

  const compile = (code: string, label: string) => {
    const m = device.createShaderModule({ code, label })
    m.getCompilationInfo().then((info) => {
      for (const x of info.messages)
        if (x.type === "error") console.error(`hex hero ${label} ${x.lineNum}:${x.linePos} ${x.message}`)
    })
    return m
  }
  const simMod = compile(commonWgsl + simWgsl, "hero-sim")
  const drawMod = compile(commonWgsl + drawWgsl, "hero-draw")
  const postMod = compile(postWgsl, "hero-post")

  const S = GPUBufferUsage.STORAGE
  const buf = (size: number, usage: number) => device.createBuffer({ size, usage })
  const uniform = buf(UNIFORM_FLOATS * 4, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST)
  const agents = buf(AGENTS * 16, S)
  const aux = buf(AGENTS * 16, S)
  const grid = buf(NEAR * NEAR * 3 * 4, S | GPUBufferUsage.COPY_DST)
  const voiceBuf = buf((BINS + LEVELS_N) * 4, S | GPUBufferUsage.COPY_DST)
  const fieldTex = (n: number) =>
    device.createTexture({
      size: [n, n],
      format: HDR,
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
    })
  const near = fieldTex(NEAR)
  const far = fieldTex(FAR)
  const sampler = device.createSampler({
    magFilter: "linear",
    minFilter: "linear",
    addressModeU: "clamp-to-edge",
    addressModeV: "clamp-to-edge",
  })

  // ── The simulation ──
  const C = GPUShaderStage.COMPUTE
  const g0 = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: C, buffer: {} },
      { binding: 1, visibility: C, buffer: { type: "storage" } },
      { binding: 2, visibility: C, buffer: { type: "storage" } },
      { binding: 3, visibility: C, buffer: { type: "storage" } },
      { binding: 4, visibility: C, buffer: { type: "read-only-storage" } },
      { binding: 5, visibility: C, sampler: {} },
    ],
  })
  const writeTex = (binding: number): GPUBindGroupLayoutEntry => ({
    binding,
    visibility: C,
    storageTexture: { access: "write-only", format: HDR },
  })
  const sampled = (binding: number, visibility = C): GPUBindGroupLayoutEntry => ({
    binding,
    visibility,
    texture: {},
  })
  const g1Resolve = device.createBindGroupLayout({ entries: [writeTex(0)] })
  const g1Far = device.createBindGroupLayout({ entries: [sampled(1), writeTex(2)] })
  const g1Read = device.createBindGroupLayout({ entries: [sampled(1), sampled(3)] })
  const computePipe = (entryPoint: string, g1: GPUBindGroupLayout) =>
    device.createComputePipeline({
      layout: device.createPipelineLayout({ bindGroupLayouts: [g0, g1] }),
      compute: { module: simMod, entryPoint },
    })
  const pInit = computePipe("init", g1Read)
  const pSplat = computePipe("splat", g1Read)
  const pResolve = computePipe("resolve", g1Resolve)
  const pFar = computePipe("far", g1Far)
  const pAdvance = computePipe("advance", g1Read)
  const b0 = device.createBindGroup({
    layout: g0,
    entries: [
      { binding: 0, resource: { buffer: uniform } },
      { binding: 1, resource: { buffer: agents } },
      { binding: 2, resource: { buffer: aux } },
      { binding: 3, resource: { buffer: grid } },
      { binding: 4, resource: { buffer: voiceBuf } },
      { binding: 5, resource: sampler },
    ],
  })
  const bResolve = device.createBindGroup({
    layout: g1Resolve,
    entries: [{ binding: 0, resource: near.createView() }],
  })
  const bFar = device.createBindGroup({
    layout: g1Far,
    entries: [
      { binding: 1, resource: near.createView() },
      { binding: 2, resource: far.createView() },
    ],
  })
  const bRead = device.createBindGroup({
    layout: g1Read,
    entries: [
      { binding: 1, resource: near.createView() },
      { binding: 3, resource: far.createView() },
    ],
  })

  // ── The instrument ──
  const VF = GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT
  const F = GPUShaderStage.FRAGMENT
  const r0 = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: VF, buffer: {} },
      ...[1, 2, 3].map((binding) => ({
        binding,
        visibility: VF,
        buffer: { type: "read-only-storage" as const },
      })),
    ],
  })
  const r1 = device.createBindGroupLayout({
    entries: [{ binding: 0, visibility: F, texture: { sampleType: "unfilterable-float" } }],
  })
  const rb0 = device.createBindGroup({
    layout: r0,
    entries: [
      { binding: 0, resource: { buffer: uniform } },
      { binding: 1, resource: { buffer: agents } },
      { binding: 2, resource: { buffer: aux } },
      { binding: 3, resource: { buffer: voiceBuf } },
    ],
  })
  const add: GPUBlendState = {
    color: { srcFactor: "one", dstFactor: "one", operation: "add" },
    alpha: { srcFactor: "one", dstFactor: "one", operation: "add" },
  }
  const keep: GPUBlendState = {
    color: { srcFactor: "zero", dstFactor: "constant", operation: "add" },
    alpha: { srcFactor: "zero", dstFactor: "constant", operation: "add" },
  }
  const drawPipe = (vs: string, fs: string, layouts: GPUBindGroupLayout[], blend?: GPUBlendState) =>
    device.createRenderPipeline({
      layout: device.createPipelineLayout({ bindGroupLayouts: layouts }),
      vertex: { module: drawMod, entryPoint: vs },
      fragment: { module: drawMod, entryPoint: fs, targets: [{ format: HDR, blend }] },
      primitive: { topology: "triangle-list" },
    })
  const pFade = drawPipe("vs_full", "fs_fade", [r0], keep)
  const pAgent = drawPipe("vs_agent", "fs_agent", [r0], add)
  const pHud = drawPipe("vs_full", "fs_hud", [r0, r1])

  // ── Bloom and tone ──
  const postLayout = device.createBindGroupLayout({
    entries: [
      sampled(0, F),
      { binding: 1, visibility: F, sampler: {} },
      { binding: 2, visibility: F, buffer: {} },
      sampled(3, F),
      { binding: 4, visibility: F, texture: { sampleType: "unfilterable-float" } },
    ],
  })
  const postPl = device.createPipelineLayout({ bindGroupLayouts: [postLayout] })
  const postPipe = (entryPoint: string, fmt: GPUTextureFormat, blend?: GPUBlendState) =>
    device.createRenderPipeline({
      layout: postPl,
      vertex: { module: postMod, entryPoint: "vs_full" },
      fragment: { module: postMod, entryPoint, targets: [{ format: fmt, blend }] },
      primitive: { topology: "triangle-list" },
    })
  const pDown = postPipe("fs_down", HDR)
  const pUp = postPipe("fs_up", HDR, add)
  const pComposite = postPipe("fs_composite", format)
  const noiseTex = device.createTexture({
    size: [64, 64],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  })
  device.queue.writeTexture({ texture: noiseTex }, blueNoise(64), { bytesPerRow: 64 * 4 }, [64, 64])
  const noise = noiseTex.createView()

  // ── Timing: the GPU's own clock where there is one ──
  const STAMPS = 5
  const query = timing ? device.createQuerySet({ type: "timestamp", count: STAMPS }) : null
  const queryBuf = query ? buf(STAMPS * 8, GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC) : null
  const queryRead = query ? buf(STAMPS * 8, GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST) : null
  let queryBusy = false
  const PARTS = ["sim", "trail", "instrument", "bloom"] as const
  const sums = PARTS.map(() => 0)
  let samples = 0

  interface Targets {
    w: number
    h: number
    dpr: number
    trail: GPUTexture
    hdr: GPUTexture
    levels: GPUTexture[]
    rb1: GPUBindGroup
    post: GPUBindGroup[]
    bufs: GPUBuffer[]
  }
  let tg: Targets | null = null
  let lookKey = ""
  const postData = new Float32Array(16)
  const writePost = (b: GPUBuffer, tex: GPUTexture, first: number, look: { bloom: number; exposure: number }) => {
    const u = postData
    u.fill(0)
    u[0] = 1 / tex.width
    u[1] = 1 / tex.height
    u[4] = first
    u[5] = 1
    u[6] = look.bloom
    u[7] = look.exposure
    u[8] = 1
    u[10] = 1
    u[11] = 1
    u.set(options.page, 12)
    u[15] = 0.05
    device.queue.writeBuffer(b, 0, u)
  }
  const measure = () => {
    const dpr = Math.min(options.maxDpr, devicePixelRatio || 1)
    const w = Math.max(2, Math.round(stage.clientWidth * dpr))
    const h = Math.max(2, Math.round(stage.clientHeight * dpr))
    return { w, h, dpr: w / Math.max(1, stage.clientWidth) }
  }
  const fitTargets = (look: { bloom: number; exposure: number }) => {
    const { w, h, dpr } = measure()
    const key = `${look.bloom}:${look.exposure}`
    if (tg && tg.w === w && tg.h === h && key === lookKey) return tg
    lookKey = key
    if (tg && tg.w === w && tg.h === h) {
      // Only the look changed: rewrite the post uniforms in place.
      writeAllPost(tg, look)
      return tg
    }
    if (tg) for (const t of [tg.trail, tg.hdr, ...tg.levels]) t.destroy()
    canvas.width = w
    canvas.height = h
    const tex = (tw: number, th: number) =>
      device.createTexture({
        size: [Math.max(1, tw), Math.max(1, th)],
        format: HDR,
        usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
      })
    const trail = tex(w, h)
    const hdr = tex(w, h)
    const levels = Array.from({ length: LEVELS }, (_, i) => tex(w >> (i + 1), h >> (i + 1)))
    const bufs =
      tg?.bufs ?? Array.from({ length: LEVELS * 2 }, () => buf(64, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST))
    const bind = (src: GPUTexture, b: GPUBuffer, bloom: GPUTexture) =>
      device.createBindGroup({
        layout: postLayout,
        entries: [
          { binding: 0, resource: src.createView() },
          { binding: 1, resource: sampler },
          { binding: 2, resource: { buffer: b } },
          { binding: 3, resource: bloom.createView() },
          { binding: 4, resource: noise },
        ],
      })
    const post: GPUBindGroup[] = []
    for (let i = 0; i < LEVELS; i++) post.push(bind(i ? levels[i - 1]! : hdr, bufs[i]!, hdr))
    for (let i = LEVELS - 1; i > 0; i--) post.push(bind(levels[i]!, bufs[LEVELS + (LEVELS - 1 - i)]!, hdr))
    post.push(bind(hdr, bufs[LEVELS * 2 - 1]!, levels[0]!))
    const rb1 = device.createBindGroup({ layout: r1, entries: [{ binding: 0, resource: trail.createView() }] })
    tg = { w, h, dpr, trail, hdr, levels, rb1, post, bufs }
    writeAllPost(tg, look)
    return tg
  }
  const writeAllPost = (t: Targets, look: { bloom: number; exposure: number }) => {
    for (let i = 0; i < LEVELS; i++) writePost(t.bufs[i]!, i ? t.levels[i - 1]! : t.hdr, i ? 0 : 1, look)
    for (let i = LEVELS - 1; i > 0; i--) writePost(t.bufs[LEVELS + (LEVELS - 1 - i)]!, t.levels[i]!, 0, look)
    writePost(t.bufs[LEVELS * 2 - 1]!, t.hdr, 0, look)
  }

  let seeded = false
  const ringBox = (t: Targets, cx: number, cy: number, R: number) => {
    const r = (R * 1.02 + 8) * t.dpr
    const x0 = Math.max(0, Math.floor(cx * t.dpr - r))
    const y0 = Math.max(0, Math.floor(cy * t.dpr - r))
    const x1 = Math.min(t.w, Math.ceil(cx * t.dpr + r))
    const y1 = Math.min(t.h, Math.ceil(cy * t.dpr + r))
    return [x0, y0, Math.max(1, x1 - x0), Math.max(1, y1 - y0)] as const
  }

  const frame = (params: Float32Array, voice: Float32Array, look: { bloom: number; exposure: number }) => {
    if (lost) return
    const t = fitTargets(look)
    params[16] = t.w
    params[17] = t.h
    params[18] = t.dpr
    device.queue.writeBuffer(uniform, 0, params as Float32Array<ArrayBuffer>)
    device.queue.writeBuffer(voiceBuf, 0, voice as Float32Array<ArrayBuffer>)
    const enc = device.createCommandEncoder()
    if (!seeded) {
      // Seeded from the first frame's ring, so the swarm starts at rest.
      seeded = true
      const pass = enc.beginComputePass()
      pass.setPipeline(pInit)
      pass.setBindGroup(0, b0)
      pass.setBindGroup(1, bRead)
      pass.dispatchWorkgroups(Math.ceil(AGENTS / 64))
      pass.end()
    }
    enc.clearBuffer(grid)
    const timed = query && !queryBusy
    const cp = enc.beginComputePass(
      timed ? { timestampWrites: { querySet: query!, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 } } : {},
    )
    cp.setBindGroup(0, b0)
    const run = (p: GPUComputePipeline, g1: GPUBindGroup, x: number, y = 1) => {
      cp.setPipeline(p)
      cp.setBindGroup(1, g1)
      cp.dispatchWorkgroups(x, y)
    }
    run(pSplat, bRead, Math.ceil(AGENTS / 64))
    run(pResolve, bResolve, NEAR / 8, NEAR / 8)
    run(pFar, bFar, FAR / 8, FAR / 8)
    run(pAdvance, bRead, Math.ceil(AGENTS / 64))
    cp.end()
    let rp = enc.beginRenderPass({
      colorAttachments: [{ view: t.trail.createView(), loadOp: "load", storeOp: "store" }],
      ...(timed ? { timestampWrites: { querySet: query!, endOfPassWriteIndex: 2 } } : {}),
    })
    rp.setBindGroup(0, rb0)
    const box = ringBox(t, params[20]!, params[21]!, params[22]!)
    rp.setScissorRect(box[0], box[1], box[2], box[3])
    const decay = params[19]!
    rp.setBlendConstant([decay, decay, decay, decay])
    rp.setPipeline(pFade)
    rp.draw(3)
    rp.setPipeline(pAgent)
    rp.draw(6, AGENTS)
    rp.end()
    const pass = (view: GPUTextureView, load: GPULoadOp, stamp = -1) =>
      enc.beginRenderPass({
        colorAttachments: [{ view, loadOp: load, storeOp: "store", clearValue: [0, 0, 0, 1] }],
        ...(stamp >= 0 && timed ? { timestampWrites: { querySet: query!, endOfPassWriteIndex: stamp } } : {}),
      })
    rp = pass(t.hdr.createView(), "clear", 3)
    rp.setPipeline(pHud)
    rp.setBindGroup(0, rb0)
    rp.setBindGroup(1, t.rb1)
    rp.draw(3)
    rp.end()
    for (let i = 0; i < LEVELS; i++) {
      rp = pass(t.levels[i]!.createView(), "clear")
      rp.setPipeline(pDown)
      rp.setBindGroup(0, t.post[i]!)
      rp.draw(3)
      rp.end()
    }
    for (let k = 0; k < LEVELS - 1; k++) {
      rp = pass(t.levels[LEVELS - 2 - k]!.createView(), "load", k === LEVELS - 2 ? 4 : -1)
      rp.setPipeline(pUp)
      rp.setBindGroup(0, t.post[LEVELS + k]!)
      rp.draw(3)
      rp.end()
    }
    rp = pass(ctx.getCurrentTexture().createView(), "clear")
    rp.setPipeline(pComposite)
    rp.setBindGroup(0, t.post[LEVELS * 2 - 1]!)
    rp.draw(3)
    rp.end()
    if (timed) {
      enc.resolveQuerySet(query!, 0, STAMPS, queryBuf!, 0)
      enc.copyBufferToBuffer(queryBuf!, 0, queryRead!, 0, STAMPS * 8)
    }
    device.queue.submit([enc.finish()])
    if (timed) {
      queryBusy = true
      queryRead!.mapAsync(GPUMapMode.READ).then(
        () => {
          const ts = new BigInt64Array(queryRead!.getMappedRange())
          const ms = (a: number, b: number) => Number(ts[b]! - ts[a]!) / 1e6
          const parts = [ms(0, 1), ms(1, 2), ms(2, 3), ms(3, 4)]
          queryRead!.unmap()
          queryBusy = false
          if (parts.every((x) => x >= 0 && x < 50)) {
            parts.forEach((x, i) => (sums[i]! += x))
            samples++
          }
        },
        () => {
          queryBusy = false
        },
      )
    }
  }

  stage.append(canvas)
  return {
    agents: AGENTS,
    frame,
    size: measure,
    timings() {
      if (!samples) return null
      const out: Record<string, number> = { samples }
      let total = 0
      PARTS.forEach((name, i) => {
        const v = sums[i]! / samples
        total += v
        out[name] = +v.toFixed(3)
        sums[i] = 0
      })
      out.total = +total.toFixed(3)
      samples = 0
      return out
    },
    destroy() {
      device.destroy()
      canvas.remove()
    },
  }
}

/** Blue noise for the dither: white noise high-passed three times, then ranked. */
function blueNoise(n: number) {
  let v = new Float32Array(n * n)
  let seed = 7
  const rnd = () => {
    seed = (seed * 16807) % 2147483647
    return seed / 2147483647
  }
  for (let i = 0; i < v.length; i++) v[i] = rnd()
  const K = [0.06, 0.24, 0.4, 0.24, 0.06]
  for (let pass = 0; pass < 3; pass++) {
    const a = new Float32Array(v.length)
    const b = new Float32Array(v.length)
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++) {
        let s = 0
        for (let k = -2; k <= 2; k++) s += K[k + 2]! * v[y * n + ((x + k + n) % n)]!
        a[y * n + x] = s
      }
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++) {
        let s = 0
        for (let k = -2; k <= 2; k++) s += K[k + 2]! * a[((y + k + n) % n) * n + x]!
        b[y * n + x] = v[y * n + x]! - s
      }
    v = b
  }
  const order = [...v.keys()].sort((i, j) => v[i]! - v[j]!)
  const out = new Float32Array(v.length)
  order.forEach((i, rank) => {
    out[i] = (rank + 0.5) / v.length
  })
  return out
}
