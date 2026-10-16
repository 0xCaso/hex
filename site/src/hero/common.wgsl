// The hero's shared block: the swarm's parameters and the instrument's layout, written once a
// frame. World: the swarm's disc has radius 1 at the origin, y down (as the screen). Angles round
// the ring are fractions of a turn, 0 at twelve o'clock and growing clockwise. Screen lengths are
// CSS px unless named otherwise.
struct Params {
  sim0: vec4f, // cohesion, alignment, separation, speed (0–1 each)
  sim1: vec4f, // shape pull, swirl, dt (s), time (s)
  sim2: vec4f, // lure (world xy), lure strength, lure presence (0–1)
  sim3: vec4f, // agents, frame, mean near density, mean far density
  scr0: vec4f, // canvas (device px), dpr, the trail's decay this frame
  scr1: vec4f, // the ring's centre (px), its radius R (px), arrival (0–1)
  scr2: vec4f, // the scope: left, top, width, height (px; width 0: none)
  scr3: vec4f, // the trace's end (px), the paste's progress (0–1, < 0: none), recording (0–1)
  voice: vec4f, // the write head (turns), the take's extent (turns), the level now, its glow
  shape: vec4f, // ring radius, rest width, voice width, agitation (world)
  pal0: vec4f, // accent (linear), its brightness
  pal1: vec4f, // warm neutral (linear)
  page: vec4f, // the page (linear)
  cuts: array<vec4f, 2>, // up to eight pieces' cut points (turns; < 0 none)
  marks: array<vec4f, 6>, // brackets: centre (turns), half span (turns), lock (0–1), alpha
}

const NEAR = 192u;
const FAR = 48u;
const SPAN = 1.1;
const BINS = 256u;
const LEVELS_N = 160u;
const MARKS = 6u;
const TAU = 6.28318531;
const PI = 3.14159265;

fn sat(x: f32) -> f32 {
  return clamp(x, 0.0, 1.0);
}
fn hash11(n: u32) -> f32 {
  var x = n * 747796405u + 2891336453u;
  x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
  x = (x >> 22u) ^ x;
  return f32(x) / 4294967295.0;
}
/** The speed the swarm cruises at (world/s). */
fn cruise(p: Params) -> f32 {
  return mix(0.03, 0.3, p.sim0.w);
}
/** World to the near and far fields' uv. */
fn fieldUv(p: vec2f) -> vec2f {
  return (p + SPAN) / (2.0 * SPAN);
}
/** A world point's angle round the ring, in turns from twelve, clockwise. */
fn turns(p: vec2f) -> f32 {
  return fract(atan2(p.x, -p.y) / TAU + 1.0);
}
/** The shortest signed distance between two angles, in turns. */
fn turnDelta(a: f32, b: f32) -> f32 {
  let d = a - b;
  return d - round(d);
}
