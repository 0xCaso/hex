// The swarm, four compute passes a frame (after common.wgsl). Adapted from the knobs MAGI hero.
//
//   splat    each agent adds itself (1, its velocity) bilinearly into a NEAR² grid (atomics)
//   resolve  the grid, tent-filtered, into the near field (density, momentum)
//   far      the near field blurred down to FAR²: the swarm at the scale of its clusters
//   advance  each agent flocks by the fields (cohesion, separation, alignment), keeps to its lane
//            in the voice's band round the ring, drifts clockwise, circles the lure and stays
//            inside the disc. Agents the write head passes while there's sound take its heat.
//
// The voice buffer holds the take's envelope by angle (BINS) and the recent input level.

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read_write> agents: array<vec4f>;
@group(0) @binding(2) var<storage, read_write> aux: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> grid: array<atomic<i32>>;
@group(0) @binding(4) var<storage, read> voice: array<f32>;
@group(0) @binding(5) var samp: sampler;
@group(1) @binding(0) var nearOut: texture_storage_2d<rgba16float, write>;
@group(1) @binding(1) var nearTex: texture_2d<f32>;
@group(1) @binding(2) var farOut: texture_storage_2d<rgba16float, write>;
@group(1) @binding(3) var farTex: texture_2d<f32>;

const FIX = 256.0;

fn agentCount() -> u32 {
  return u32(P.sim3.x);
}

fn lane(h: f32) -> f32 {
  let l = h * 2.0 - 1.0;
  return sign(l) * pow(abs(l), 1.25);
}

@compute @workgroup_size(64)
fn init(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= arrayLength(&agents)) {
    return;
  }
  // Already on the ring, so the first frame is the resting state.
  let u = hash11(i * 3u + 1u);
  let h = hash11(i * 7u + 5u);
  let r = P.shape.x + lane(h) * P.shape.y * 1.4;
  let a = u * TAU;
  let p = vec2f(sin(a), -cos(a)) * r;
  let t = vec2f(cos(a), sin(a));
  agents[i] = vec4f(p, t * 0.04);
  aux[i] = vec4f(0.0, h, 0.0, hash11(i * 11u + 9u));
}

fn splatAt(c: vec2i, w: f32, v: vec2f) {
  if (any(c < vec2i(0)) || any(c >= vec2i(i32(NEAR)))) {
    return;
  }
  let k = (u32(c.y) * NEAR + u32(c.x)) * 3u;
  atomicAdd(&grid[k], i32(round(w * FIX)));
  atomicAdd(&grid[k + 1u], i32(round(w * v.x * FIX)));
  atomicAdd(&grid[k + 2u], i32(round(w * v.y * FIX)));
}

@compute @workgroup_size(64)
fn splat(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= agentCount()) {
    return;
  }
  let a = agents[i];
  let g = fieldUv(a.xy) * f32(NEAR) - 0.5;
  let c = vec2i(floor(g));
  let f = fract(g);
  splatAt(c, (1.0 - f.x) * (1.0 - f.y), a.zw);
  splatAt(c + vec2i(1, 0), f.x * (1.0 - f.y), a.zw);
  splatAt(c + vec2i(0, 1), (1.0 - f.x) * f.y, a.zw);
  splatAt(c + vec2i(1, 1), f.x * f.y, a.zw);
}

fn cell(c: vec2i) -> vec3f {
  let q = clamp(c, vec2i(0), vec2i(i32(NEAR) - 1));
  let k = (u32(q.y) * NEAR + u32(q.x)) * 3u;
  return vec3f(f32(atomicLoad(&grid[k])), f32(atomicLoad(&grid[k + 1u])), f32(atomicLoad(&grid[k + 2u]))) / FIX;
}

@compute @workgroup_size(8, 8)
fn resolve(@builtin(global_invocation_id) id: vec3u) {
  if (any(id.xy >= vec2u(NEAR))) {
    return;
  }
  let c = vec2i(id.xy);
  var s = cell(c) * 4.0;
  s += (cell(c + vec2i(1, 0)) + cell(c - vec2i(1, 0)) + cell(c + vec2i(0, 1)) + cell(c - vec2i(0, 1))) * 2.0;
  s += cell(c + vec2i(1, 1)) + cell(c - vec2i(1, 1)) + cell(c + vec2i(1, -1)) + cell(c + vec2i(-1, 1));
  textureStore(nearOut, c, vec4f(s / 16.0, 0.0));
}

@compute @workgroup_size(8, 8)
fn far(@builtin(global_invocation_id) id: vec3u) {
  if (any(id.xy >= vec2u(FAR))) {
    return;
  }
  let uv = (vec2f(id.xy) + 0.5) / f32(FAR);
  let t = 2.0 / f32(NEAR);
  var s = vec3f(0.0);
  var wsum = 0.0;
  for (var y = -2; y <= 2; y++) {
    for (var x = -2; x <= 2; x++) {
      let w = exp(-f32(x * x + y * y) / 4.0);
      s += w * textureSampleLevel(nearTex, samp, uv + vec2f(f32(x), f32(y)) * t, 0.0).xyz;
      wsum += w;
    }
  }
  textureStore(farOut, vec2i(id.xy), vec4f(s / wsum, 0.0));
}

fn farAt(p: vec2f) -> vec3f {
  return textureSampleLevel(farTex, samp, fieldUv(p), 0.0).xyz;
}
fn nearAt(p: vec2f) -> vec3f {
  return textureSampleLevel(nearTex, samp, fieldUv(p), 0.0).xyz;
}
fn farSlope(p: vec2f) -> vec2f {
  let e = 2.0 * SPAN / f32(FAR);
  let dx = farAt(p + vec2f(e, 0.0)).x - farAt(p - vec2f(e, 0.0)).x;
  let dy = farAt(p + vec2f(0.0, e)).x - farAt(p - vec2f(0.0, e)).x;
  return vec2f(dx, dy) / (2.0 * e * max(P.sim3.w, 1e-3));
}
/** The take's envelope at an angle (turns), linearly between bins. */
fn envAt(u: f32) -> f32 {
  let x = u * f32(BINS) - 0.5;
  let i0 = u32(floor(x + f32(BINS))) % BINS;
  let i1 = (i0 + 1u) % BINS;
  return mix(voice[i0], voice[i1], fract(x));
}
fn wall(p: vec2f, k: f32) -> vec2f {
  let r = length(p);
  let edge = 0.93;
  if (r < edge || r < 1e-4) {
    return vec2f(0.0);
  }
  return -p / r * ((r - edge) / 0.07) * k;
}

@compute @workgroup_size(64)
fn advance(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= agentCount()) {
    return;
  }
  let dt = P.sim1.z;
  var a = agents[i];
  var x = aux[i];
  let p = a.xy;
  var v = a.zw;
  let sp = cruise(P);
  let k = sp * 2.6;
  let speed = max(length(v), 1e-5);
  let dir = v / speed;
  let n0 = nearAt(p);
  let f0 = farAt(p);
  let e = 2.0 * SPAN / f32(NEAR);
  let gn = vec2f(nearAt(p + vec2f(e, 0.0)).x - nearAt(p - vec2f(e, 0.0)).x,
    nearAt(p + vec2f(0.0, e)).x - nearAt(p - vec2f(0.0, e)).x) / max(P.sim3.z, 1e-3);
  let gf = farSlope(p);
  var acc = vec2f(0.0);
  // The flock: up the swarm's slope, down the crowd's, toward the heading round it.
  acc += gf / (length(gf) + 2.0) * P.sim0.x * k * 2.2;
  acc -= gn / (length(gn) + 1.5) * P.sim0.z * k * 3.2;
  let m = n0.yz + f0.yz * 3.0;
  let ml = length(m);
  if (ml > 1e-4) {
    acc += (m / ml * speed - v) * P.sim0.y * 2.0;
  }
  let wob = hash11(i * 977u + u32(P.sim3.y) * 13u) - 0.5;
  acc += vec2f(-dir.y, dir.x) * wob * k * (1.4 - P.sim0.y) * 2.0;

  // The ring: each agent keeps to its lane of the band, which the voice widens where it spoke.
  let r = max(length(p), 1e-4);
  let nrm = p / r;
  let tang = vec2f(-nrm.y, nrm.x);
  let u = turns(p);
  let amp = envAt(u);
  let goalR = P.shape.x + lane(x.y) * (P.shape.y + amp * P.shape.z);
  let pull = P.sim1.x;
  acc += nrm * (goalR - r) * pull * 34.0;
  acc -= nrm * dot(v, nrm) * pull * 11.0;
  // A slow clockwise drift, each agent at its own pace.
  let drift = P.sim1.y * (0.55 + 0.9 * x.w);
  acc += tang * (drift - dot(v, tang)) * 1.6;

  // At the write head, sound shakes the band and leaves its heat.
  let dh = turnDelta(u, P.voice.x);
  let behind = select(1.0, exp(-max(dh, 0.0) * 160.0), dh > 0.0);
  let head = exp(-dh * dh / (0.018 * 0.018)) * P.scr3.w;
  let shake = hash11(i * 131u + u32(P.sim3.y) * 7u) - 0.5;
  acc += nrm * shake * P.voice.z * head * P.shape.w * 60.0;
  x.z = max(x.z * exp(-dt / 2.4), sat(head * behind * P.voice.z * 1.6));

  // The lure: a spring toward it and a swirl round it, fading with distance.
  let ld = P.sim2.xy - p;
  let lf = exp(-dot(ld, ld) / 0.06) * P.sim2.z * P.sim2.w;
  acc += (ld * 5.0 + vec2f(-ld.y, ld.x) * 6.0) * lf * k;
  acc += wall(p, k * 10.0 + 0.5);

  v += acc * dt;
  let s = length(v);
  let top = sp * 2.2 + 0.08;
  if (s > top) {
    v *= top / s;
  }
  if (s < 0.004) {
    v = tang * 0.004;
  }
  var q = p + v * dt;
  let rq = length(q);
  if (rq > 1.0) {
    let n = q / rq;
    q = n;
    v -= n * max(dot(v, n), 0.0) * 1.6;
  }
  agents[i] = vec4f(q, v);
  aux[i] = x;
}
