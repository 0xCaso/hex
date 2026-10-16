// The instrument on screen (after common.wgsl). Two passes into half-float targets, then bloom
// and tone (post.wgsl, the knobs catalog's):
//
//   trail    last frame's agents fade (a blend by constant) and this frame's are added, each a
//            short streak along its velocity: warm neutral at rest, the accent where the voice
//            wrote. A phosphor that remembers where the swarm went.
//   hud      the instrument reading the take: the band's gauge round the ring (each tick as long
//            as the envelope there), cut marks where Hex transcribed a piece during a pause, the
//            write head, brackets on the pieces, the level scope and the trace the text takes to
//            the field. Then the trail over it.

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> agents: array<vec4f>;
@group(0) @binding(2) var<storage, read> aux: array<vec4f>;
@group(0) @binding(3) var<storage, read> voice: array<f32>;
@group(1) @binding(0) var trail: texture_2d<f32>;

struct FullOut {
  @builtin(position) pos: vec4f,
}
@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullOut {
  let uv = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
  return FullOut(vec4f(uv * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0), 0.0, 1.0));
}

@fragment
fn fs_fade() -> @location(0) vec4f {
  return vec4f(0.0);
}

// ── Agents ──

struct AgentOut {
  @builtin(position) pos: vec4f,
  @location(0) local: vec2f,
  @location(1) half: f32,
  @location(2) col: vec3f,
}

fn toClip(px: vec2f) -> vec4f {
  let d = px * P.scr0.z / P.scr0.xy;
  return vec4f(d.x * 2.0 - 1.0, 1.0 - d.y * 2.0, 0.0, 1.0);
}

@vertex
fn vs_agent(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> AgentOut {
  var o: AgentOut;
  let a = agents[ii];
  let R = P.scr1.z;
  let s = P.scr1.xy + a.xy * R;
  let vel = a.zw * R;
  let sp = length(vel);
  let dir = select(vec2f(1.0, 0.0), vel / sp, sp > 1e-4);
  let half = clamp(sp * 0.014, 0.4, 3.5);
  let corner = array(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0), vec2f(-1.0, 1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0))[vi];
  let local = corner * vec2f(half + 1.5, 2.0);
  o.pos = toClip(s + dir * local.x + vec2f(-dir.y, dir.x) * local.y);
  o.local = local;
  o.half = half;
  let heat = sat(aux[ii].z);
  let c = mix(P.pal1.rgb * 0.55, P.pal0.rgb * 1.25, heat * heat * (3.0 - 2.0 * heat));
  o.col = c * 0.3 * P.scr1.w * (1.0 - P.scr0.w) / (1.0 + half * 0.18);
  return o;
}

@fragment
fn fs_agent(in: AgentOut) -> @location(0) vec4f {
  let d = length(vec2f(max(abs(in.local.x) - in.half, 0.0), in.local.y));
  let cov = sat((0.7 - d) * P.scr0.z + 0.5);
  return vec4f(in.col * cov, 0.0);
}

// ── The instrument ──

var<private> aa: f32;

fn line(d: f32, w: f32) -> f32 {
  let ww = max(w, aa);
  return sat((ww * 0.5 - abs(d)) / aa + 0.5) * min(w / aa, 1.0);
}
fn fill(d: f32) -> f32 {
  return sat(0.5 - d / aa);
}
fn seg(p: vec2f, a: vec2f, b: vec2f) -> f32 {
  let pa = p - a;
  let ba = b - a;
  return length(pa - ba * sat(dot(pa, ba) / dot(ba, ba)));
}
/** A seven-segment digit, 4 × 7 px from its top left. */
fn digit(p: vec2f, n: u32) -> f32 {
  let bits = array(0x3Fu, 0x06u, 0x5Bu, 0x4Fu, 0x66u, 0x6Du, 0x7Du, 0x07u, 0x7Fu, 0x6Fu)[n % 10u];
  if (p.x < -1.0 || p.y < -1.0 || p.x > 5.0 || p.y > 8.0) {
    return 0.0;
  }
  let a = array(vec2f(0.6, 0.0), vec2f(4.0, 0.6), vec2f(4.0, 4.1), vec2f(0.6, 7.0), vec2f(0.0, 4.1), vec2f(0.0, 0.6), vec2f(0.6, 3.5));
  let b = array(vec2f(3.4, 0.0), vec2f(4.0, 2.9), vec2f(4.0, 6.4), vec2f(3.4, 7.0), vec2f(0.0, 6.4), vec2f(0.0, 2.9), vec2f(3.4, 3.5));
  var c = 0.0;
  for (var i = 0u; i < 7u; i++) {
    if ((bits & (1u << i)) != 0u) {
      c = max(c, line(seg(p, a[i], b[i]), 1.0));
    }
  }
  return c;
}
/** The registration mark: a cross in a ring. */
fn mark(q: vec2f) -> f32 {
  if (max(abs(q.x), abs(q.y)) > 9.0) {
    return 0.0;
  }
  let cross = max(line(q.x, 1.0) * step(abs(q.y), 6.0), line(q.y, 1.0) * step(abs(q.x), 6.0));
  return max(cross, line(length(q) - 2.8, 1.0));
}
fn envAt(u: f32) -> f32 {
  let x = u * f32(BINS) - 0.5;
  let i0 = u32(floor(x + f32(BINS))) % BINS;
  let i1 = (i0 + 1u) % BINS;
  return mix(voice[i0], voice[i1], fract(x));
}
fn levelAt(x: f32) -> f32 {
  let f = clamp(x, 0.0, 1.0) * f32(LEVELS_N - 1u);
  let i0 = u32(floor(f));
  let i1 = min(i0 + 1u, LEVELS_N - 1u);
  return mix(voice[BINS + i0], voice[BINS + i1], fract(f));
}
fn cutAt(i: u32) -> f32 {
  return P.cuts[i / 4u][i % 4u];
}

fn hud(p: vec2f) -> vec3f {
  let c = P.scr1.xy;
  let R = P.scr1.z;
  let accent = P.pal0.rgb * P.pal0.w;
  let bone = P.pal1.rgb;
  let rec = P.scr3.w;
  let glow = P.voice.w;
  let q = p - c;
  let r = length(q);
  let rn = r / R;
  let u = fract(atan2(q.x, -q.y) / TAU + 1.0);
  var col = vec3f(0.0);

  if (rn < 1.3) {
    // The disc's edge, faint.
    col += bone * 0.07 * line(r - R, 1.0);
    // The gauge: 144 ticks, each as long as the take's envelope under it; lit across the take.
    let n = 144.0;
    let k = fract(u * n);
    let tu = (floor(u * n) + 0.5) / n;
    let env = envAt(tu);
    let len = 3.0 + env * R * 0.09;
    let r0 = R * 1.045;
    let tick = line((k - 0.5) * r * TAU / n, 1.1) * step(r0, r) * step(r, r0 + len);
    // Lit as the head passes, a tick's width at a time rather than a tick at once.
    let inTake = sat((P.voice.y - tu) * n + 0.5) * glow;
    col += mix(bone * 0.13, accent * 0.9, inTake * sat(0.25 + env * 2.0)) * tick;
    // Cut marks: where a pause let Hex transcribe the piece before it.
    for (var i = 0u; i < 8u; i++) {
      let cu = cutAt(i);
      let lit = P.cutLight[i / 4u][i % 4u];
      if (cu >= 0.0 && lit > 0.002) {
        // Drawn on from the disc outward, then cooling.
        let d = turnDelta(u, cu) * r * TAU;
        let reach = R * (0.99 + 0.18 * P.cutDraw[i / 4u][i % 4u]);
        col += bone * 0.75 * lit * line(d, 1.2) * step(R * 0.99, r) * sat(reach - r + 0.5);
      }
    }
    // The write head, a small notch pointing in, while recording.
    if (rec > 0.01) {
      let d = turnDelta(u, P.voice.x) * r * TAU;
      let y = r - R * 1.19;
      let tri = max(abs(d) - (y + 5.0) * 0.6, abs(y) - 5.0);
      col += accent * 1.6 * rec * fill(tri);
      col += accent * 0.35 * rec * line(d, 1.0) * step(R * 0.98, r) * step(r, R * 1.13);
    }
    // Brackets on the pieces, polar: a [ and a ] on the band at each piece's ends, closing in
    // as the piece locks, its number outside the band at its middle.
    let ringR = P.shape.x * R;
    for (var i = 0u; i < MARKS; i++) {
      let m = P.marks[i];
      if (m.w <= 0.01) {
        continue;
      }
      // The lock is a spring already; the uprights grow from the band outward as the bracket draws on, and
      // the arms follow once the uprights are nearly there.
      let lock = m.z;
      let info = P.markInfo[i];
      let reach = (P.shape.y + 0.035 + 0.07 * (1.0 - lock)) * R * info.x;
      let rin = ringR - reach;
      let rout = ringR + reach;
      if (r > rin - 4.0 && r < rout + 4.0) {
        let hue = mix(accent * 1.2, bone * 0.85, lock) * m.w;
        let arm = 7.0 * sat((info.x - 0.6) / 0.4);
        for (var side = 0u; side < 2u; side++) {
          let edgeU = select(m.x + m.y, m.x - m.y, side == 0u);
          let inward = select(-1.0, 1.0, side == 0u);
          let t = turnDelta(u, edgeU) * r * TAU * inward;
          let up = line(t, 1.3) * step(rin, r) * step(r, rout);
          let arms = (line(r - rin, 1.3) + line(r - rout, 1.3)) * step(0.0, t) * step(t, arm);
          col += hue * sat(up + arms);
        }
      }
      if (lock > 0.5) {
        let mid = m.x;
        let at = c + vec2f(sin(mid * TAU), -cos(mid * TAU)) * (R * 0.9);
        let dq = p - at + vec2f(5.5, 3.5);
        if (max(abs(dq.x - 5.5), abs(dq.y - 3.5)) < 9.0) {
          let no = u32(info.y + 0.5);
          let num = max(digit(dq, no / 10u), digit(dq - vec2f(6.0, 0.0), no));
          col += bone * 0.6 * m.w * (lock - 0.5) * 2.0 * num;
        }
      }
    }
    // The centre: a faint cross.
    col += bone * 0.12 * (line(q.x, 1.0) * step(abs(q.y), 5.0) + line(q.y, 1.0) * step(abs(q.x), 5.0));
  }

  // ── The trace: from the ring's port down to the field, the text's way out ──
  let end = P.scr3.xy;
  if (end.y > 0.0) {
    let port = c + vec2f(0.0, R * 1.24);
    let bend = vec2f(port.x, end.y);
    if (p.y > port.y - 8.0 && p.y < end.y + 8.0) {
      let d = min(seg(p, port, bend), seg(p, bend, end));
      col += bone * 0.16 * line(d, 1.0);
      col += bone * 0.5 * fill(max(abs(p.x - port.x), abs(p.y - port.y)) - 2.2);
      // The paste: one bead runs the trace and lands on the field as the words arrive, easing to a stop, then
      // hands its light to the field's edge (a DOM element) and cools where it landed.
      let prog = P.bead.x;
      if (prog >= 0.0) {
        let l1 = length(bend - port);
        let l2 = length(end - bend);
        let along = select(l1 + length(p - bend), length(p - port), seg(p, port, bend) <= seg(p, bend, end));
        let at = prog * (l1 + l2);
        let behind = at - along;
        let lit = select(exp(-behind / 9.0), exp(-(-behind) * 1.5), behind < 0.0);
        col += accent * 2.2 * P.bead.y * lit * line(d, 2.4);
      }
    }
  }

  // ── The scope: the input level, recent at the right ──
  let cl = P.scr2;
  if (cl.z > 0.0 && p.x > cl.x - 8.0 && p.x < cl.x + cl.z + 8.0 && p.y > cl.y - 8.0 && p.y < cl.y + cl.w + 8.0) {
    let sq = p - cl.xy;
    let b = max(abs(sq.x - cl.z * 0.5) - cl.z * 0.5, abs(sq.y - cl.w * 0.5) - cl.w * 0.5);
    let corner = step(cl.z * 0.5 - 8.0, abs(sq.x - cl.z * 0.5)) * step(cl.w * 0.5 - 8.0, abs(sq.y - cl.w * 0.5));
    col += bone * 0.45 * line(b, 1.1) * corner;
    col += bone * 0.06 * line(b, 1.0);
    if (b < 0.0) {
      let g = abs(fract(vec2f(sq.x / (cl.z / 8.0), sq.y / (cl.w / 4.0)) + 0.5) - 0.5) * vec2f(cl.z / 8.0, cl.w / 4.0);
      col += bone * 0.035 * (line(g.x, 1.0) + line(g.y, 1.0));
      // Mirrored about the middle, like a waveform.
      let x = sq.x / cl.z;
      let lv = levelAt(x);
      let amp = lv * cl.w * 0.42 + 0.6;
      let dy = abs(sq.y - cl.w * 0.5);
      col += mix(bone * 0.5, accent * 1.1, sat(lv * 3.0)) * (fill(dy - amp) * 0.35 + line(dy - amp, 1.0) * 0.8);
    }
  }

  // ── The frame: registration marks at the instrument's corners ──
  let fh = vec2f(R * 1.42);
  col += bone * 0.22 * mark(abs(q) - fh);
  return col;
}

@fragment
fn fs_hud(in: FullOut) -> @location(0) vec4f {
  let dpr = P.scr0.z;
  aa = 1.0 / dpr;
  let p = in.pos.xy / dpr;
  let page = P.page.rgb;
  var col = hud(p) * P.scr1.w;
  col += textureLoad(trail, vec2i(in.pos.xy), 0).rgb;
  let size = P.scr0.xy / dpr;
  let edge = smoothstep(0.0, 24.0, p.x) * smoothstep(0.0, 24.0, size.x - p.x) *
    smoothstep(0.0, 24.0, p.y) * smoothstep(0.0, 24.0, size.y - p.y);
  return vec4f(page + col * edge, 1.0);
}
