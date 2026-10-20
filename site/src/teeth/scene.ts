import {
  AdditiveBlending,
  CanvasTexture,
  CatmullRomCurve3,
  Color,
  CylinderGeometry,
  DirectionalLight,
  ExtrudeGeometry,
  Float32BufferAttribute,
  Group,
  HemisphereLight,
  LatheGeometry,
  MathUtils,
  Mesh,
  MeshBasicMaterial,
  MeshPhysicalMaterial,
  MeshStandardMaterial,
  NeutralToneMapping,
  Path,
  PCFShadowMap,
  PerspectiveCamera,
  PlaneGeometry,
  PMREMGenerator,
  Scene,
  Shape,
  SphereGeometry,
  SpotLight,
  SRGBColorSpace,
  Timer,
  TubeGeometry,
  Vector2,
  Vector3,
  WebGLRenderer,
  type BufferGeometry,
  type Material,
} from "three"
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js"
import { Arch, coilPoint, shoe, sole, superellipse, sweep, smoothstep, tooth, type ToothKind } from "./geometry"

export type TeethMode = "live" | "og" | "poster"

const HINGE_Z = 0.12
const HOP = 2.8
const ATTACH = new Vector3(0.9, -1.06, 0.5)
const FOOT = new Vector3(0.95, -2.02, 0.42)
const COIL_BASE = 0.12
const SOLE_Y = FOOT.y - 0.38

type Spring = { x: number; v: number }
// Sub-stepped damped spring and impulse kick, after arcade's kit/rig.ts.
function stepSpring(s: Spring, k: number, z: number, dt: number, target = 0) {
  const c = 2 * Math.sqrt(k) * z
  const n = Math.max(1, Math.ceil(dt / (1 / 240)))
  const h = dt / n
  for (let i = 0; i < n; i++) {
    s.v += (-(s.x - target) * k - s.v * c) * h
    s.x += s.v * h
  }
}
const kick = (s: Spring, amount: number, k: number) => {
  s.x += amount * 0.55
  s.v += amount * Math.sqrt(k) * 0.9
}
const spring = (): Spring => ({ x: 0, v: 0 })

function seeded(seed: number) {
  let s = seed >>> 0
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 4294967296
  }
}

const TOOTH_KINDS: ToothKind[] = ["incisor", "lateral", "canine", "premolar", "premolar", "molar"]

function buildJaw(upper: boolean, mats: Materials, detail: number) {
  const sign = upper ? 1 : -1
  const pivot = new Group()
  pivot.position.z = HINGE_Z
  const jaw = new Group()
  jaw.position.z = -HINGE_Z
  pivot.add(jaw)

  const arch = upper ? new Arch(2.0, 1.04, -0.12) : new Arch(1.93, 0.98, -0.12)
  const widths = upper ? [0.42, 0.34, 0.33, 0.33, 0.36, 0.42] : [0.33, 0.32, 0.32, 0.33, 0.36, 0.42]
  const heights = upper ? [0.42, 0.38, 0.45, 0.36, 0.35, 0.33] : [0.38, 0.37, 0.43, 0.35, 0.34, 0.32]
  const depths = [0.28, 0.27, 0.32, 0.35, 0.37, 0.4]
  const gap = 0.024
  const edges = [0]
  for (const w of widths) edges.push(edges[edges.length - 1] + w + gap)
  const half = arch.theta(edges[edges.length - 1] + 0.1)

  const random = seeded(upper ? 7 : 19)
  const teeth: BufferGeometry[] = []
  const p = new Vector3(), o = new Vector3()
  for (const side of [-1, 1]) {
    widths.forEach((w, i) => {
      const center = (edges[i] + edges[i + 1] - gap) / 2
      const theta = side * arch.theta(center)
      const jitter = () => (random() - 0.5) * 2
      const g = tooth(TOOTH_KINDS[i], w * (1 + jitter() * 0.03), heights[i] * (1 + jitter() * 0.05), depths[i], mats.tooth.color, detail)
      if (upper) g.rotateZ(Math.PI)
      g.rotateZ(jitter() * 0.035)
      arch.point(theta, p)
      arch.outward(theta, o)
      g.rotateY(Math.atan2(o.x, o.z) + jitter() * 0.05)
      g.translate(p.x, sign * 0.45, p.z)
      teeth.push(g)
    })
  }
  const teethMesh = new Mesh(mergeGeometries(teeth), mats.tooth)
  teeth.forEach(g => g.dispose())

  const shell = new Mesh(
    sweep(
      arch, half,
      (phi) => {
        const s = superellipse(0.42, 0.35, 3.2, phi)
        return { across: s.across, up: s.up + sign * 0.8 }
      },
      Math.round(28 * detail), Math.round(72 * detail), 0.32,
      mats.shell.color,
      (phi) => {
        const towardTeeth = -sign * Math.sin(phi)
        const inside = Math.max(0, -Math.cos(phi))
        return { ao: (1 - 0.5 * smoothstep(0.2, 1, towardTeeth)) * (1 - 0.18 * inside) }
      },
    ),
    mats.shell,
  )

  // Gum ridge with papillae dipping between teeth.
  const papilla = (theta: number) => {
    const l = Math.abs(arch.length(theta))
    let d = Infinity
    for (const e of edges) d = Math.min(d, Math.abs(l - (e - gap / 2)))
    return Math.exp(-((d / 0.055) ** 2))
  }
  const gum = new Mesh(
    sweep(
      arch, half - 0.02,
      (phi, theta) => {
        const s = superellipse(0.25, 0.1, 2.4, phi)
        const towardTeeth = Math.max(0, -sign * Math.sin(phi))
        return { across: s.across, up: s.up + sign * 0.46 - sign * towardTeeth * papilla(theta) * 0.075 }
      },
      Math.round(20 * detail), Math.round(120 * detail), 0.2,
      mats.gum.color,
      (phi) => ({ ao: 0.72 + 0.28 * smoothstep(-0.2, 0.9, sign * Math.sin(phi)) }),
    ),
    mats.gum,
  )

  const palate = new Mesh(new SphereGeometry(1, 32, 16), upper ? mats.mouth : mats.tongue)
  if (upper) {
    palate.scale.set(1.62, 0.2, 0.62)
    palate.position.set(0, 0.56, 0.08)
  } else {
    palate.scale.set(1.1, 0.2, 0.6)
    palate.position.set(0, -0.36, 0.12)
    const floor = new Mesh(palate.geometry, mats.mouth)
    floor.scale.set(1.62, 0.2, 0.62)
    floor.position.set(0, -0.56, 0.08)
    jaw.add(floor)
  }
  jaw.add(shell, gum, teethMesh, palate)
  return { pivot, jaw, arch, half }
}

type Materials = ReturnType<typeof createMaterials>

function createMaterials() {
  // Red lacquer: a saturated base coat under a hard clear coat.
  const shell = new MeshPhysicalMaterial({
    color: 0xb30f26, roughness: 0.42, clearcoat: 1, clearcoatRoughness: 0.07,
    specularIntensity: 0.6, vertexColors: true, dithering: true,
  })
  const gum = new MeshPhysicalMaterial({
    color: 0xd9475f, roughness: 0.5, clearcoat: 0.55, clearcoatRoughness: 0.22,
    sheen: 0.6, sheenColor: new Color(0xff9aa6), sheenRoughness: 0.5, vertexColors: true, dithering: true,
  })
  // Enamel: ivory with a wet coat and a soft sheen standing in for scatter.
  const tooth = new MeshPhysicalMaterial({
    color: 0xf7f0e3, roughness: 0.3, clearcoat: 0.8, clearcoatRoughness: 0.12,
    sheen: 0.45, sheenColor: new Color(0xfff1dc), sheenRoughness: 0.35, vertexColors: true, dithering: true,
  })
  const eye = new MeshPhysicalMaterial({ color: 0xfffcf6, roughness: 0.25, clearcoat: 1, clearcoatRoughness: 0.04, dithering: true })
  const iris = new MeshPhysicalMaterial({ color: 0x2f6a6b, roughness: 0.35, clearcoat: 1, clearcoatRoughness: 0.03, vertexColors: true })
  const pupil = new MeshPhysicalMaterial({ color: 0x050608, roughness: 0.2, clearcoat: 1, clearcoatRoughness: 0.02 })
  const mouth = new MeshStandardMaterial({ color: 0x2a0710, roughness: 0.8 })
  const tongue = new MeshPhysicalMaterial({ color: 0xa8293c, roughness: 0.55, sheen: 0.5, sheenColor: new Color(0xff8090), clearcoat: 0.3 })
  // Brushed steel: anisotropy stretches highlights along the grain.
  const metal = new MeshPhysicalMaterial({
    color: 0xc4cad3, metalness: 1, roughness: 0.3, anisotropy: 0.75, clearcoat: 0.3, clearcoatRoughness: 0.2, dithering: true,
  })
  const rubber = new MeshStandardMaterial({ color: 0x0d0a0b, roughness: 0.9 })
  // The same lacquer for parts without baked AO.
  const lacquer = shell.clone()
  lacquer.vertexColors = false
  return { shell, lacquer, gum, tooth, eye, iris, pupil, mouth, tongue, metal, rubber }
}

function blobTexture(stops: [number, string][]) {
  const canvas = document.createElement("canvas")
  canvas.width = canvas.height = 128
  const context = canvas.getContext("2d")!
  const g = context.createRadialGradient(64, 64, 0, 64, 64, 64)
  for (const [at, color] of stops) g.addColorStop(at, color)
  context.fillStyle = g
  context.fillRect(0, 0, 128, 128)
  const texture = new CanvasTexture(canvas)
  texture.colorSpace = SRGBColorSpace
  return texture
}

export function mountTeeth(canvas: HTMLCanvasElement, mode: TeethMode): () => void {
  const still = mode !== "live"
  const coarse = window.matchMedia("(pointer: coarse)").matches || window.innerWidth < 640
  const detail = coarse ? 0.75 : 1

  const renderer = new WebGLRenderer({
    canvas,
    alpha: true,
    antialias: true,
    powerPreference: "high-performance",
    preserveDrawingBuffer: still,
  })
  renderer.outputColorSpace = SRGBColorSpace
  // Khronos PBR Neutral keeps the lacquer's hue and saturation where ACES
  // twists red toward orange (lamplight rendering-tricks 1.14).
  renderer.toneMapping = NeutralToneMapping
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = PCFShadowMap
  const scene = new Scene()

  // Authored softbox studio baked once into a PMREM: a broad warm key, a cool
  // strip fill, two rim strips for edge glints, and a dark warm floor bounce.
  const pmrem = new PMREMGenerator(renderer)
  const studio = new Scene()
  studio.background = new Color(0x07080b)
  const panel = (position: [number, number, number], size: [number, number], color: Color) => {
    const mesh = new Mesh(new PlaneGeometry(...size), new MeshBasicMaterial({ color }))
    mesh.position.set(...position)
    mesh.lookAt(0, 0, 0)
    studio.add(mesh)
  }
  panel([-4, 5, 5], [5, 4], new Color(7, 6.4, 5.6))
  panel([6, 0.5, 3], [1.2, 6], new Color(1.6, 1.9, 2.4))
  panel([0, 7, 0], [6, 1.2], new Color(2.4, 2.4, 2.5))
  panel([-5, 2, -5], [0.8, 6], new Color(3.4, 3.6, 4.2))
  panel([5, 2.5, -5], [0.8, 6], new Color(4.2, 3.6, 3.2))
  panel([0, -6, 0], [20, 20], new Color(0.07, 0.035, 0.03))
  const environment = pmrem.fromScene(studio, 0.035)
  scene.environment = environment.texture
  scene.environmentIntensity = 0.9
  studio.traverse(object => {
    if (object instanceof Mesh) {
      object.geometry.dispose()
      ;(object.material as Material).dispose()
    }
  })
  pmrem.dispose()

  const camera = new PerspectiveCamera(36, 1, 0.1, 40)
  camera.position.set(0, 0, 9)

  const mats = createMaterials()
  const root = new Group()
  const rig = new Group()
  const upper = buildJaw(true, mats, detail)
  const lower = buildJaw(false, mats, detail)

  // Eyes ride the upper jaw: lacquer sockets, sleepy lids, eyeballs that aim.
  const eyes: Group[] = []
  const lids: Mesh[] = []
  const eyeGeometry = new SphereGeometry(0.56, Math.round(40 * detail), Math.round(28 * detail))
  const socketGeometry = new SphereGeometry(0.61, 36, 18, 0, Math.PI * 2, Math.PI * 0.45, Math.PI * 0.55)
  const lidGeometry = new SphereGeometry(0.59, 40, 14, 0, Math.PI * 2, 0, Math.PI * 0.5)
  const irisGeometry = new SphereGeometry(0.563, 40, 12, 0, Math.PI * 2, 0, 0.5)
  {
    const colors: number[] = []
    const pos = irisGeometry.attributes.position
    for (let i = 0; i < pos.count; i++) {
      const r = Math.hypot(pos.getX(i), pos.getZ(i)) / (0.563 * Math.sin(0.5))
      const ring = 1 - 0.55 * smoothstep(0.7, 1, r)
      colors.push(ring * (0.75 + 0.35 * r), ring * (0.8 + 0.3 * r), ring * (0.85 + 0.2 * r))
    }
    irisGeometry.setAttribute("color", new Float32BufferAttribute(colors, 3))
  }
  const pupilGeometry = new SphereGeometry(0.566, 32, 8, 0, Math.PI * 2, 0, 0.25)
  for (const x of [-0.6, 0.6]) {
    const holder = new Group()
    holder.position.set(x, 1.6, 0.5)
    holder.rotation.z = -x * 0.1
    const socket = new Mesh(socketGeometry, mats.lacquer)
    socket.rotation.x = Math.PI / 2
    const eye = new Group()
    eye.add(new Mesh(eyeGeometry, mats.eye))
    const iris = new Mesh(irisGeometry, mats.iris)
    iris.rotation.x = Math.PI / 2
    const pupil = new Mesh(pupilGeometry, mats.pupil)
    pupil.rotation.x = Math.PI / 2
    eye.add(iris, pupil)
    const lid = new Mesh(lidGeometry, mats.lacquer)
    holder.add(socket, eye, lid)
    upper.jaw.add(holder)
    eyes.push(eye)
    lids.push(lid)
  }

  // Hinge bosses, screws, axle, and the wind-up key on the left cheek.
  const bossGeometry = new LatheGeometry(
    [new Vector2(0, -0.12), new Vector2(0.21, -0.12), new Vector2(0.26, -0.08), new Vector2(0.27, 0), new Vector2(0.26, 0.08), new Vector2(0.21, 0.12), new Vector2(0, 0.12)].map(v => v),
    40,
  )
  const screwGeometry = new LatheGeometry([new Vector2(0, 0.03), new Vector2(0.07, 0.025), new Vector2(0.1, 0.008), new Vector2(0.105, -0.01)], 24)
  const slotGeometry = new CylinderGeometry(0.012, 0.012, 0.15, 6)
  const hingeX = 1.99
  for (const side of [-1, 1]) {
    const boss = new Mesh(bossGeometry, mats.lacquer)
    boss.rotation.z = Math.PI / 2
    boss.position.set(side * hingeX, 0, HINGE_Z)
    const screw = new Mesh(screwGeometry, mats.metal)
    screw.rotation.z = -side * Math.PI / 2
    screw.position.set(side * (hingeX + 0.12), 0, HINGE_Z)
    rig.add(boss, screw)
    for (const angle of [0.4, 0.4 + Math.PI / 2]) {
      const slot = new Mesh(slotGeometry, mats.mouth)
      slot.position.set(side * (hingeX + 0.15), 0, HINGE_Z)
      slot.rotation.set(angle, 0, 0)
      rig.add(slot)
    }
  }
  const axle = new Mesh(new CylinderGeometry(0.05, 0.05, hingeX * 2, 16), mats.metal)
  axle.rotation.z = Math.PI / 2
  axle.position.z = HINGE_Z - 0.2
  // A dark throat behind the teeth so the open mouth never shows the page.
  const throat = new Mesh(new SphereGeometry(1, 32, 16), mats.mouth)
  throat.scale.set(1.75, 0.62, 0.22)
  throat.position.set(0, 0, -0.02)
  rig.add(axle, throat)

  const key = new Group()
  key.position.set(-hingeX - 0.14, 0, HINGE_Z)
  const shaft = new Mesh(new CylinderGeometry(0.045, 0.045, 0.42, 16), mats.metal)
  shaft.rotation.z = Math.PI / 2
  shaft.position.x = -0.2
  const collar = new Mesh(new LatheGeometry([new Vector2(0, -0.05), new Vector2(0.09, -0.05), new Vector2(0.1, 0), new Vector2(0.09, 0.05), new Vector2(0, 0.05)], 24), mats.metal)
  collar.rotation.z = Math.PI / 2
  collar.position.x = -0.06
  const bowShape = new Shape()
  for (let i = 0; i <= 96; i++) {
    const phi = (i / 96) * Math.PI * 2
    const r = 0.09 + 0.3 * Math.pow(Math.abs(Math.sin(phi)), 1.3)
    const x = Math.cos(phi) * r, y = Math.sin(phi) * r
    if (i === 0) bowShape.moveTo(x, y)
    else bowShape.lineTo(x, y)
  }
  for (const y of [-0.24, 0.24]) {
    const hole = new Path()
    hole.absarc(0, y, 0.075, 0, Math.PI * 2, true)
    bowShape.holes.push(hole)
  }
  const bowGeometry = new ExtrudeGeometry(bowShape, { depth: 0.05, bevelEnabled: true, bevelThickness: 0.025, bevelSize: 0.022, bevelSegments: 3, curveSegments: 16 })
  bowGeometry.translate(0, 0, -0.025)
  const bow = new Mesh(bowGeometry, mats.metal)
  bow.position.x = -0.5
  key.add(shaft, collar, bow)
  rig.add(key)

  rig.add(lower.pivot, upper.pivot)

  // Coil legs and shoes. Feet belong to the root so the body can crouch
  // without sliding the soles; each coil is re-laid along its axis per frame.
  type Leg = { foot: Group; geometry: TubeGeometry; centers: Float32Array; offsets: Float32Array }
  const legs: Leg[] = []
  const shoeGeometry = shoe(mats.shell.color, detail)
  const soleGeometry = sole(detail)
  const pegGeometry = new CylinderGeometry(0.07, 0.09, 0.12, 16)
  for (const side of [-1, 1]) {
    const foot = new Group()
    foot.position.set(side * FOOT.x, FOOT.y, FOOT.z)
    foot.rotation.y = side * 0.16
    const samples = Array.from({ length: 160 }, (_, i) => coilPoint(i / 159, 1, new Vector3()))
    const curve = new CatmullRomCurve3(samples)
    const segments = Math.round(150 * detail)
    const radial = 8
    const geometry = new TubeGeometry(curve, segments, 0.042, radial, false)
    const centers = new Float32Array((segments + 1) * 3)
    const offsets = new Float32Array(geometry.attributes.position.array.length)
    const c = new Vector3()
    for (let i = 0; i <= segments; i++) {
      curve.getPointAt(i / segments, c)
      centers.set([c.x, c.y, c.z], i * 3)
      for (let j = 0; j <= radial; j++) {
        const k = (i * (radial + 1) + j) * 3
        offsets[k] = geometry.attributes.position.array[k] - c.x
        offsets[k + 1] = geometry.attributes.position.array[k + 1] - c.y
        offsets[k + 2] = geometry.attributes.position.array[k + 2] - c.z
      }
    }
    const coil = new Mesh(geometry, mats.metal)
    coil.position.set(0, COIL_BASE, 0.12)
    const shoeMesh = new Mesh(shoeGeometry, mats.shell)
    const soleMesh = new Mesh(soleGeometry, mats.rubber)
    foot.add(coil, shoeMesh, soleMesh)
    const peg = new Mesh(pegGeometry, mats.metal)
    peg.position.set(side * ATTACH.x, ATTACH.y + 0.04, ATTACH.z)
    rig.add(peg)
    legs.push({ foot, geometry, centers, offsets })
    root.add(foot)
  }
  root.add(rig)
  scene.add(root)
  root.traverse(object => {
    if (object instanceof Mesh) {
      object.castShadow = true
      object.receiveShadow = true
    }
  })

  // Sky/ground hemisphere as a cheap two-term SH: cool from above, warm
  // lacquer bounce from below, so shadowed normals keep their form.
  scene.add(new HemisphereLight(0xb9c8e6, 0x3a1612, 0.45))
  const keyLight = new DirectionalLight(0xffeedd, 2.3)
  keyLight.castShadow = true
  keyLight.shadow.mapSize.setScalar(coarse ? 1024 : 2048)
  keyLight.shadow.radius = 4
  keyLight.shadow.bias = -0.0004
  keyLight.shadow.normalBias = 0.02
  scene.add(keyLight, keyLight.target)
  // A narrow cool rim separates the silhouette from the near-black page.
  const rimLight = new DirectionalLight(0xcfe0ff, 2.6)
  scene.add(rimLight, rimLight.target)
  const kickLight = new DirectionalLight(0xffb59a, 0.9)
  scene.add(kickLight, kickLight.target)

  // A visible pool of light announces the landing before the actor arrives.
  const poolTexture = blobTexture([[0, "rgba(255,226,177,0.8)"], [0.48, "rgba(255,221,162,0.5)"], [0.75, "rgba(255,216,150,0.12)"], [1, "rgba(255,216,150,0)"]])
  const additive = (map: CanvasTexture) => new MeshBasicMaterial({ map, transparent: true, depthWrite: false, blending: AdditiveBlending, toneMapped: false })
  const poolMaterial = additive(poolTexture)
  const pool = new Mesh(new PlaneGeometry(1, 1), poolMaterial)
  pool.renderOrder = -2
  scene.add(pool)
  const beamCanvas = document.createElement("canvas")
  beamCanvas.width = 256
  beamCanvas.height = 512
  const beamContext = beamCanvas.getContext("2d")!
  const beamGradient = beamContext.createLinearGradient(0, 0, 0, 512)
  beamGradient.addColorStop(0, "rgba(255, 229, 186, 0)")
  beamGradient.addColorStop(0.35, "rgba(255, 229, 186, 0.04)")
  beamGradient.addColorStop(1, "rgba(255, 229, 186, 0.13)")
  beamContext.fillStyle = beamGradient
  beamContext.filter = "blur(8px)"
  beamContext.beginPath()
  beamContext.moveTo(128, 0)
  beamContext.lineTo(244, 512)
  beamContext.lineTo(12, 512)
  beamContext.closePath()
  beamContext.fill()
  const beamTexture = new CanvasTexture(beamCanvas)
  beamTexture.colorSpace = SRGBColorSpace
  const beamMaterial = additive(beamTexture)
  const beam = new Mesh(new PlaneGeometry(1, 1), beamMaterial)
  beam.renderOrder = -2
  scene.add(beam)
  const spotlight = new SpotLight(0xffdfb2, 0, 15, Math.PI / 7, 0.8, 0)
  spotlight.position.set(0, 5, 4)
  scene.add(spotlight, spotlight.target)

  // Ground: a faint warm floor glow that follows the toy, and contact
  // shadows that tighten as each shoe meets it (capsule/blob grounding).
  const glowMaterial = additive(blobTexture([[0, "rgba(255,214,170,0.22)"], [0.6, "rgba(255,200,150,0.07)"], [1, "rgba(255,200,150,0)"]]))
  const glow = new Mesh(new PlaneGeometry(1, 1), glowMaterial)
  glow.renderOrder = -1
  scene.add(glow)
  const shadowTexture = blobTexture([[0, "rgba(0,0,0,0.9)"], [0.45, "rgba(0,0,0,0.55)"], [1, "rgba(0,0,0,0)"]])
  const contact = [-1, 1].map(() => {
    const mesh = new Mesh(new PlaneGeometry(1, 1), new MeshBasicMaterial({ map: shadowTexture, transparent: true, depthWrite: false, toneMapped: false }))
    mesh.renderOrder = -1
    scene.add(mesh)
    return mesh
  })

  const timer = new Timer()
  timer.connect(document)
  let frame = 0
  let stageWidth = 0
  let stageHeight = 0
  let pixelRatio = 1
  let maxPixelRatio = 1

  function resize() {
    const width = window.innerWidth
    const height = window.innerHeight
    camera.aspect = width / height
    camera.updateProjectionMatrix()
    maxPixelRatio = Math.min(window.devicePixelRatio || 1, coarse ? 1.5 : 2)
    pixelRatio = maxPixelRatio
    renderer.setPixelRatio(pixelRatio)
    renderer.setSize(width, height, false)
    stageHeight = 2 * Math.tan(MathUtils.degToRad(18)) * 9
    stageWidth = stageHeight * camera.aspect
    const scale = mode === "og" ? 0.82 : mode === "poster" ? 0.9 : Math.min(0.31, stageWidth / 18)
    root.scale.setScalar(scale)
    const s = keyLight.shadow.camera
    s.left = s.bottom = -scale * 3.4
    s.right = s.top = scale * 3.4
    s.near = 0.1
    s.far = 30
    s.updateProjectionMatrix()
  }

  // Motion state.
  const squash = spring(), lean = spring(), dip = spring(), recoil = spring(), jitter = spring()
  const gazeX = spring(), gazeY = spring(), yaw = spring()
  const pointer = { x: 0, y: 0, at: -Infinity }
  let blink = 0
  let nextBlink = 2.2
  let prevBeat = 0
  let prevClack = 0
  let slowFrames = 0
  let landed = false
  const onPointer = (event: PointerEvent) => {
    if (event.pointerType !== "mouse") return
    pointer.x = (event.clientX / window.innerWidth) * 2 - 1
    pointer.y = -(event.clientY / window.innerHeight) * 2 + 1
    pointer.at = timer.getElapsed()
  }
  window.addEventListener("pointermove", onPointer, { passive: true })

  const attach = new Vector3()
  function layCoils() {
    for (const [i, leg] of legs.entries()) {
      const side = i === 0 ? -1 : 1
      attach.set(side * ATTACH.x, ATTACH.y, ATTACH.z).multiply(rig.scale).add(rig.position)
      const length = MathUtils.clamp(attach.y - (leg.foot.position.y + COIL_BASE), 0.62, 1.25)
      const array = leg.geometry.attributes.position.array as Float32Array
      const radial = 9
      for (let s = 0; s < leg.centers.length / 3; s++) {
        const cx = leg.centers[s * 3], cy = leg.centers[s * 3 + 1] * length, cz = leg.centers[s * 3 + 2]
        for (let j = 0; j < radial; j++) {
          const k = (s * radial + j) * 3
          array[k] = cx + leg.offsets[k]
          array[k + 1] = cy + leg.offsets[k + 1]
          array[k + 2] = cz + leg.offsets[k + 2]
        }
      }
      leg.geometry.attributes.position.needsUpdate = true
      leg.geometry.computeBoundingSphere()
    }
  }

  function placeLights() {
    const target = root.position
    keyLight.position.copy(target).add(new Vector3(-3.5, 5, 6))
    keyLight.target.position.copy(target)
    rimLight.position.copy(target).add(new Vector3(2.5, 3, -5))
    rimLight.target.position.copy(target)
    kickLight.position.copy(target).add(new Vector3(-4, -1.5, -3))
    kickLight.target.position.copy(target)
  }

  function pose(elapsed: number, dt: number) {
    const s = root.scale.x
    const x = Math.max(0, stageWidth / 2 - s * 3.5)
    const y = stageHeight / 2 - s * 3.3 - 0.35
    const wide = window.innerWidth >= 1000
    const stops = wide
      ? [[0, y], [x, y], [x, -y], [0, -y], [-x, -y], [-x, y]]
      : [[0, y], [x * 0.45, y], [-x * 0.45, y]]

    root.visible = elapsed >= 3
    const motionTime = Math.max(0, elapsed - 4)
    const cycle = motionTime / HOP
    const beat = cycle % 1
    const index = Math.floor(cycle) % stops.length
    const from = stops[index]
    const to = stops[(index + 1) % stops.length]
    const direction = Math.sign(to[0] - from[0])

    // Hold (chatter) → crouch (anticipation) → flight → landing settle.
    let flight = MathUtils.clamp((beat - 0.38) / 0.32, 0, 1)
    let crouch = smoothstep(0.24, 0.36, beat) * (1 - smoothstep(0.36, 0.395, beat))
    let baseX = MathUtils.lerp(from[0], to[0], flight)
    let baseY = MathUtils.lerp(from[1], to[1], flight)
    let arc = Math.sin(flight * Math.PI) * 0.38
    let travel = direction
    if (elapsed < 4) {
      const entrance = MathUtils.clamp(elapsed - 3, 0, 1)
      flight = entrance < 1 ? entrance : 0
      crouch = 0
      baseX = MathUtils.lerp(-stageWidth / 2 - s * 4, stops[0][0], entrance)
      baseY = stops[0][1]
      arc = Math.sin(entrance * Math.PI) * 0.5
      travel = 1
    }

    const takeoff = elapsed >= 4 && prevBeat < 0.38 && beat >= 0.38
    const landing = (elapsed >= 4 && prevBeat < 0.7 && beat >= 0.7) || (!landed && elapsed >= 4)
    if (elapsed >= 4) landed = true
    prevBeat = elapsed >= 4 ? beat : 0
    if (takeoff) {
      kick(squash, 0.16, 160)
      kick(lean, -travel * 0.08, 90)
    }
    if (landing) {
      kick(squash, -0.2, 160)
      kick(dip, -0.22, 170)
      kick(lean, travel * 0.12, 90)
      kick(recoil, 0.16, 400)
      blink = Math.max(blink, 0.001)
    }

    // Chatter during the hold: a wide anticipatory gape, then clacks that
    // slow as the spring unwinds. Each closure is a contact with recoil.
    const hold = elapsed >= 4 && beat < 0.24 ? beat * HOP : -1
    let open = 0.05
    if (hold >= 0) {
      const duration = 0.24 * HOP
      const env = smoothstep(0, 0.08, hold) * (1 - smoothstep(duration - 0.1, duration, hold))
      const phase = Math.PI * (8.5 * hold - 1.4 * hold * hold)
      open = 0.05 + env * 0.36 * Math.pow(Math.abs(Math.cos(phase)), 0.75)
      const clack = Math.floor(phase / Math.PI + 0.5)
      if (clack !== prevClack && env > 0.3) {
        kick(recoil, 0.07, 400)
        kick(jitter, 0.025, 500)
      }
      prevClack = clack
    }
    open = open * (1 - crouch) + Math.sin(flight * Math.PI) * 0.26
    stepSpring(recoil, 400, 0.35, dt)
    stepSpring(jitter, 500, 0.3, dt)
    upper.pivot.rotation.x = -open - recoil.x
    lower.pivot.rotation.x = open * 0.12 + recoil.x * 0.3

    stepSpring(squash, 160, 0.32, dt)
    stepSpring(dip, 170, 0.28, dt)
    stepSpring(lean, 90, 0.5, dt)
    const breath = Math.sin(elapsed * 2.1) * 0.012
    const stretch = MathUtils.clamp(squash.x, -0.25, 0.25)
    rig.scale.set(1 - stretch * 0.45, 1 + stretch + breath, 1 - stretch * 0.45)
    rig.position.y = -crouch * 0.32 + dip.x * 1.1 + jitter.x

    root.position.set(baseX, baseY + arc, 0)
    for (const [i, leg] of legs.entries()) {
      // Feet trail the take-off and reach for the ground before landing.
      const trail = Math.sin(flight * Math.PI) * (i === (travel > 0 ? 0 : 1) ? 0.5 : -0.25)
      leg.foot.rotation.x = trail
      leg.foot.position.y = FOOT.y + Math.sin(flight * Math.PI) * 0.18 * (i === 0 ? 1 : 0.7)
    }
    layCoils()

    // Gaze: the pointer if it moved recently, else lead the travel while
    // crouching and flying, else the headline below.
    const sinceMouse = elapsed - pointer.at
    let tx = 0, ty = -0.2
    if (sinceMouse < 3) {
      tx = MathUtils.clamp((pointer.x * stageWidth / 2 - root.position.x) * 0.25, -0.5, 0.5)
      ty = MathUtils.clamp((pointer.y * stageHeight / 2 - root.position.y) * 0.25, -0.4, 0.4)
    } else if (beat > 0.2 && elapsed >= 4) {
      tx = direction * 0.45
      ty = (to[1] - from[1]) * 0.12
    } else {
      tx = -root.position.x * 0.12
      ty = -0.15 - root.position.y * 0.06
    }
    stepSpring(gazeX, 260, 0.72, dt, tx)
    stepSpring(gazeY, 260, 0.72, dt, ty)
    for (const eye of eyes) {
      eye.rotation.y = gazeX.x
      eye.rotation.x = -gazeY.x
    }

    nextBlink -= dt
    if (nextBlink <= 0) {
      blink = Math.max(blink, 0.001)
      nextBlink = 2.5 + Math.random() * 3.5
    }
    if (blink > 0) blink += dt / 0.18
    if (blink >= 1) blink = 0
    const closed = blink > 0 ? Math.sin(blink * Math.PI) : 0
    for (const lid of lids) lid.rotation.x = MathUtils.lerp(-0.62, 1.35, closed)

    key.rotation.x += dt * (hold >= 0 ? 5.5 : 1.4)

    stepSpring(yaw, 30, 0.9, dt, Math.sin(elapsed * 0.62) * 0.36 + (sinceMouse < 3 ? gazeX.x * 0.4 : 0))
    root.rotation.y = yaw.x
    root.rotation.x = -0.05 + Math.cos(elapsed * 0.38) * 0.05
    root.rotation.z = Math.sin(elapsed * 0.31) * 0.03 - lean.x - travel * Math.sin(flight * Math.PI) * 0.1

    // Stage light and ground.
    const lightIn = smoothstep(1.4, 2.4, elapsed)
    const lightOut = 1 - smoothstep(5, 6.2, elapsed)
    const lightLevel = lightIn * lightOut
    const groundY = stops[0][1] + SOLE_Y * s
    poolMaterial.opacity = lightLevel * 0.65
    pool.position.set(0, groundY + s * 0.15, -0.1)
    pool.scale.set(s * 8, s * 1.2, 1)
    const beamTop = stageHeight / 2 + 0.2
    beam.position.set(0, (beamTop + pool.position.y) / 2, -0.2)
    beam.scale.set(s * 8, beamTop - pool.position.y, 1)
    beamMaterial.opacity = lightLevel
    spotlight.target.position.set(0, stops[0][1], 0)
    spotlight.intensity = lightLevel * 1.2

    const height = root.position.y - baseY
    const floor = baseY + SOLE_Y * s
    glow.visible = root.visible
    glow.position.set(baseX, floor + s * 0.1, -0.6 * s)
    glow.scale.set(s * 7, s * 1.3, 1)
    glowMaterial.opacity = 1
    for (const [i, mesh] of contact.entries()) {
      const leg = legs[i]
      const lift = Math.max(0, height + (leg.foot.position.y - FOOT.y) * s)
      const spread = 1 + lift * 3
      mesh.visible = root.visible
      mesh.position.set(baseX + (i === 0 ? -1 : 1) * FOOT.x * s * Math.cos(yaw.x), floor + s * 0.02, -0.2 * s)
      mesh.scale.set(s * 1.25 * spread, s * 0.3 * spread, 1)
      ;(mesh.material as MeshBasicMaterial).opacity = 0.75 * Math.exp(-lift * 5)
    }
  }

  function staticPose() {
    root.visible = true
    if (mode === "og") {
      root.position.set(2.65, 0, 0)
      root.rotation.set(-0.08, -0.35, -0.08)
    } else {
      root.position.set(0, 0.25, 0)
      root.rotation.set(-0.04, -0.3, -0.02)
    }
    upper.pivot.rotation.x = -0.28
    lower.pivot.rotation.x = 0.02
    for (const lid of lids) lid.rotation.x = -0.62
    for (const eye of eyes) eye.rotation.set(0.08, 0.18, 0)
    key.rotation.x = 0.5
    for (const [i, leg] of legs.entries()) leg.foot.rotation.x = i === 0 ? 0.08 : -0.05
    layCoils()
    glow.visible = pool.visible = beam.visible = false
    for (const mesh of contact) mesh.visible = false
  }

  function render() {
    placeLights()
    renderer.render(scene, camera)
  }

  function animate() {
    timer.update()
    const elapsed = timer.getElapsed()
    const real = timer.getDelta()
    pose(elapsed, Math.min(0.05, real))
    render()
    // Dynamic resolution: step down when frames run long.
    if (real > 1 / 40 && real < 0.25) slowFrames++
    else slowFrames = Math.max(0, slowFrames - 1)
    if (slowFrames > 45 && pixelRatio > 1) {
      pixelRatio = Math.max(1, pixelRatio - 0.25)
      renderer.setPixelRatio(pixelRatio)
      renderer.setSize(window.innerWidth, window.innerHeight, false)
      slowFrames = 0
    }
    frame = requestAnimationFrame(animate)
  }

  const onResize = () => {
    resize()
    if (still) render()
  }
  const onVisibility = () => {
    cancelAnimationFrame(frame)
    if (!document.hidden && !still) frame = requestAnimationFrame(animate)
  }
  resize()
  window.addEventListener("resize", onResize)
  if (still) {
    staticPose()
    render()
  } else {
    document.addEventListener("visibilitychange", onVisibility)
    frame = requestAnimationFrame(animate)
  }

  return () => {
    cancelAnimationFrame(frame)
    window.removeEventListener("resize", onResize)
    window.removeEventListener("pointermove", onPointer)
    document.removeEventListener("visibilitychange", onVisibility)
    const disposed = new Set<unknown>()
    scene.traverse(object => {
      if (!(object instanceof Mesh)) return
      for (const item of [object.geometry, object.material]) {
        if (disposed.has(item)) continue
        disposed.add(item)
        item.dispose()
      }
    })
    for (const texture of [poolTexture, beamTexture, shadowTexture, glowMaterial.map]) texture?.dispose()
    environment.dispose()
    keyLight.shadow.map?.dispose()
    renderer.dispose()
    timer.dispose()
  }
}
