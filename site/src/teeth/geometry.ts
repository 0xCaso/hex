import { BufferGeometry, Color, Float32BufferAttribute, Vector3 } from "three"

export const smoothstep = (a: number, b: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)))
  return t * t * (3 - 2 * t)
}
const spow = (x: number, e: number) => Math.sign(x) * Math.pow(Math.abs(x), e)

// Jimenez's multi-bounce AO fit: saturated albedo keeps its colour in crevices
// instead of going grey (arcade kit shadeScene, lamplight rendering-tricks 1.9).
function multiBounce(ao: number, albedo: number) {
  const a = 2.0404 * albedo - 0.3324
  const b = -4.7951 * albedo + 0.6417
  const c = 2.7552 * albedo + 0.6903
  return Math.max(ao, ((ao * a + b) * ao + c) * ao)
}

export type Shade = { ao: number; tint?: [number, number, number] }

/**
 * A closed parametric surface with shared seam vertices and single poles, so
 * computed normals stay smooth everywhere. u wraps; v runs pole to pole.
 * Per-vertex AO is baked into vertex colours (the blog game's aOpenTone).
 */
export function surface(
  nu: number,
  nv: number,
  position: (u: number, v: number, out: Vector3) => void,
  albedo?: Color,
  shade?: (u: number, v: number, p: Vector3) => Shade,
): BufferGeometry {
  const positions: number[] = []
  const colors: number[] = []
  const p = new Vector3()
  const push = (u: number, v: number) => {
    position(u, v, p)
    positions.push(p.x, p.y, p.z)
    if (albedo && shade) {
      const { ao, tint = [1, 1, 1] } = shade(u, v, p)
      const clamped = Math.min(1, Math.max(0, ao))
      colors.push(
        multiBounce(clamped, albedo.r) * tint[0],
        multiBounce(clamped, albedo.g) * tint[1],
        multiBounce(clamped, albedo.b) * tint[2],
      )
    }
  }
  push(0, 0)
  for (let j = 1; j < nv; j++) for (let i = 0; i < nu; i++) push(i / nu, j / nv)
  push(0, 1)

  const index: number[] = []
  const ring = (j: number, i: number) => 1 + (j - 1) * nu + (i % nu)
  const last = 1 + (nv - 1) * nu
  for (let i = 0; i < nu; i++) index.push(0, ring(1, i + 1), ring(1, i))
  for (let j = 1; j < nv - 1; j++) {
    for (let i = 0; i < nu; i++) {
      const a = ring(j, i), b = ring(j, i + 1), c = ring(j + 1, i), d = ring(j + 1, i + 1)
      index.push(a, b, d, a, d, c)
    }
  }
  for (let i = 0; i < nu; i++) index.push(last, ring(nv - 1, i), ring(nv - 1, i + 1))

  // Face outward whatever the parameterisation's handedness.
  let signed = 0
  const e1 = new Vector3(), e2 = new Vector3(), centre = new Vector3()
  for (let i = 0; i < positions.length; i += 3) centre.x += positions[i], centre.y += positions[i + 1], centre.z += positions[i + 2]
  centre.divideScalar(positions.length / 3)
  for (let i = 0; i < index.length; i += 3) {
    const [a, b, c] = [index[i] * 3, index[i + 1] * 3, index[i + 2] * 3]
    e1.set(positions[b] - positions[a], positions[b + 1] - positions[a + 1], positions[b + 2] - positions[a + 2])
    e2.set(positions[c] - positions[a], positions[c + 1] - positions[a + 1], positions[c + 2] - positions[a + 2])
    e1.cross(e2)
    signed += e1.x * (positions[a] - centre.x) + e1.y * (positions[a + 1] - centre.y) + e1.z * (positions[a + 2] - centre.z)
  }
  if (signed < 0) for (let i = 0; i < index.length; i += 3) [index[i + 1], index[i + 2]] = [index[i + 2], index[i + 1]]

  const geometry = new BufferGeometry()
  geometry.setAttribute("position", new Float32BufferAttribute(positions, 3))
  if (colors.length) geometry.setAttribute("color", new Float32BufferAttribute(colors, 3))
  geometry.setIndex(index)
  geometry.computeVertexNormals()
  return geometry
}

/** The dental arch: a half ellipse in xz, opening toward -z (the hinge). */
export class Arch {
  private readonly table: number[] = []
  constructor(readonly a: number, readonly b: number, readonly zc: number) {
    let s = 0
    for (let i = 0; i <= 400; i++) {
      this.table.push(s)
      const t = (i / 400) * 1.6
      s += Math.hypot(a * Math.cos(t), b * Math.sin(t)) * (1.6 / 400)
    }
  }
  /** θ for an arc length from the midline. */
  theta(length: number) {
    const t = this.table
    let i = 0
    while (i < t.length - 2 && t[i + 1] < length) i++
    return ((i + (length - t[i]) / (t[i + 1] - t[i])) / 400) * 1.6
  }
  length(theta: number) {
    const x = (Math.abs(theta) / 1.6) * 400
    const i = Math.min(399, Math.floor(x))
    return Math.sign(theta) * (this.table[i] + (this.table[i + 1] - this.table[i]) * (x - i))
  }
  point(theta: number, out: Vector3) {
    return out.set(this.a * Math.sin(theta), 0, this.b * Math.cos(theta) + this.zc)
  }
  outward(theta: number, out: Vector3) {
    return out.set(this.b * Math.sin(theta), 0, this.a * Math.cos(theta)).normalize()
  }
  tangent(theta: number, out: Vector3) {
    return out.set(this.a * Math.cos(theta), 0, -this.b * Math.sin(theta)).normalize()
  }
}

type Profile = (phi: number, theta: number) => { across: number; up: number }

/**
 * Sweeps a closed profile along the arch with domed end caps.
 * The profile is in (outward, up) coordinates; u is the profile angle.
 */
export function sweep(
  arch: Arch,
  half: number,
  profile: Profile,
  nu: number,
  nv: number,
  capLength: number,
  albedo?: Color,
  shade?: (phi: number, theta: number, p: Vector3) => Shade,
) {
  const cap = 0.2
  const c = new Vector3(), o = new Vector3(), t = new Vector3()
  const frame = (v: number) => {
    const w = (v * 2 - 1) * (1 + cap)
    const k = Math.max(0, (Math.abs(w) - 1) / cap)
    const theta = Math.sign(w) * Math.min(1, Math.abs(w)) * half
    return { theta, k, scale: Math.sqrt(Math.max(0, 1 - k * k)), sign: Math.sign(w) || 1 }
  }
  return surface(
    nu,
    nv,
    (u, v, out) => {
      const f = frame(v)
      const phi = u * Math.PI * 2
      const { across, up } = profile(phi, f.theta)
      arch.point(f.theta, c)
      arch.outward(f.theta, o)
      arch.tangent(f.theta, t).multiplyScalar(f.sign * f.k * capLength)
      out.copy(c).add(t).addScaledVector(o, across * f.scale)
      out.y += up * f.scale
    },
    albedo,
    shade && ((u, v, p) => shade(u * Math.PI * 2, frame(v).theta, p)),
  )
}

export const superellipse = (a: number, b: number, n: number, phi: number) => ({
  across: a * spow(Math.cos(phi), 2 / n),
  up: b * spow(Math.sin(phi), 2 / n),
})

export type ToothKind = "incisor" | "lateral" | "canine" | "premolar" | "molar"

/** A tooth growing along +y from a root buried at y < 0, front face toward +z. */
export function tooth(kind: ToothKind, w: number, h: number, d: number, albedo: Color, detail: number) {
  const root = 0.18
  return surface(
    Math.round(18 * detail),
    Math.round(16 * detail),
    (u, v, out) => {
      const a = u * Math.PI * 2
      const s = Math.sin(v * Math.PI)
      const y = -root + (h + root) * (0.5 - 0.5 * spow(Math.cos(v * Math.PI), 0.42))
      const tip = smoothstep(0, h, y)
      let x = (w / 2) * spow(s, 0.3) * spow(Math.cos(a), 0.55)
      let z = (d / 2) * spow(s, 0.3) * spow(Math.sin(a), 0.7)
      // Necks narrow into the gum; crowns flare and thin to a chisel edge.
      x *= 0.84 + 0.16 * smoothstep(-0.05, 0.25, y)
      if (kind === "canine") x *= 1 - 0.22 * Math.pow(tip, 2.4)
      else if (kind === "incisor" || kind === "lateral") x *= 1 + 0.06 * tip
      z *= kind === "molar" || kind === "premolar" ? 1 - 0.2 * tip : 1 - 0.5 * Math.pow(tip, 1.4)
      // Labial convexity and a softly rounded biting edge.
      if (z > 0) z += 0.05 * d * (1 - Math.pow((2 * x) / w, 2))
      const cusp = kind === "canine" ? 0.04 * h * (1 - Math.abs((2 * x) / w)) : 0
      out.set(x, y + cusp * tip, z)
    },
    albedo,
    (u, _v, p) => {
      const tip = smoothstep(0, h, p.y)
      const side = Math.abs(Math.cos(u * Math.PI * 2))
      const ao = (0.55 + 0.45 * smoothstep(-0.04, 0.26, p.y)) * (1 - 0.12 * Math.pow(side, 6))
      // Enamel reads translucent at the edge (cool) and warmer over dentine.
      const tint: [number, number, number] = [1 - 0.07 * tip, 1 - 0.035 * tip + 0.01, 1 + 0.02 * tip - 0.06 * (1 - tip)]
      return { ao, tint }
    },
  )
}

/** Clown shoe with a domed toe; origin at the ankle, sole flat at y = -0.32. */
export function shoe(albedo: Color, detail: number) {
  return surface(
    Math.round(32 * detail),
    Math.round(20 * detail),
    (u, v, out) => {
      const a = u * Math.PI * 2
      const s = Math.sin(v * Math.PI)
      const py = -Math.cos(v * Math.PI)
      const pz = spow(s, 0.75) * Math.sin(a)
      const px = spow(s, 0.75) * Math.cos(a)
      const z = pz > 0 ? pz * 0.98 : pz * 0.5
      const toe = smoothstep(0, 1, pz)
      const x = px * (0.4 + 0.12 * toe)
      let y = py * (0.25 + 0.07 * toe)
      y = Math.max(y, -0.2) - 0.12
      out.set(x, y + 0.02, z + 0.22)
    },
    albedo,
    (_u, _v, p) => ({ ao: 0.6 + 0.4 * smoothstep(-0.32, -0.12, p.y) - 0.12 * smoothstep(0.12, 0.3, p.y) * smoothstep(0.3, -0.1, p.z - 0.22) }),
  )
}

export function sole(detail: number) {
  return surface(Math.round(32 * detail), Math.round(10 * detail), (u, v, out) => {
    const a = u * Math.PI * 2
    const s = Math.sin(v * Math.PI)
    const pz = spow(s, 0.25) * Math.sin(a)
    const px = spow(s, 0.25) * Math.cos(a)
    const z = pz > 0 ? pz * 1.04 : pz * 0.56
    out.set(px * (0.45 + 0.1 * smoothstep(0, 1, pz)), -Math.cos(v * Math.PI) * 0.045 - 0.335, z + 0.22)
  })
}

/** Conical coil spring centre line; `squeeze` scales its height. */
export function coilPoint(t: number, length: number, out: Vector3) {
  const turns = 6.5
  const r = 0.15 + 0.035 * (1 - t)
  const a = t * Math.PI * 2 * turns
  return out.set(Math.cos(a) * r, length * t, Math.sin(a) * r)
}
